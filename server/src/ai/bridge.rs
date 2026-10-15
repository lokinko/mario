//! Account-selected model relay. Only short-lived pairing secrets live here;
//! OpenAI credentials never cross this boundary. The worker makes outbound HTTPS calls.
use super::{
    provider::{ModelCompletion, SearchCompletion},
    ChatMessage, ModelProvider,
};
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

const PAIR_TTL: Duration = Duration::from_secs(600);
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
const ONLINE_TTL: Duration = Duration::from_secs(35);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Job {
    pub id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub search: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum Event {
    Delta {
        text: String,
    },
    Complete {
        result: SearchCompletion,
    },
    Error,
    Heartbeat,
    Status {
        message: String,
    },
    Failure {
        reason: crate::models::inference::CodexFailure,
    },
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub paired: bool,
    pub online: bool,
    pub model: Option<String>,
    pub models: Vec<crate::models::CodexModel>,
}
struct Active {
    id: String,
    events: mpsc::Sender<Event>,
}
struct State {
    closed: bool,
    runner: Option<String>,
    seen: Option<Instant>,
    model: Option<String>,
    models: Vec<crate::models::CodexModel>,
    active: Option<Active>,
}
pub(crate) struct Connection {
    token_hash: String,
    created: Instant,
    state: Mutex<State>,
    jobs: mpsc::Sender<Job>,
    receiver: AsyncMutex<mpsc::Receiver<Job>>,
}
fn registry() -> &'static Mutex<HashMap<String, Arc<Connection>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Connection>>>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}
fn hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn offline() -> AppError {
    AppError::Model("本机 Codex 桥接未连接或已过期，请在设置中重新配对并启动本机桥接".into())
}
impl Connection {
    fn expired(&self, state: &State) -> bool {
        state.closed
            || self.created.elapsed() > SESSION_TTL
            || (state.runner.is_none() && self.created.elapsed() > PAIR_TTL)
    }
    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.active.take();
    }
    fn live(&self) -> bool {
        let state = self.state.lock().unwrap();
        !self.expired(&state) && state.seen.is_some_and(|seen| seen.elapsed() < ONLINE_TTL)
    }
    fn claim(&self, runner: &str, model: &str) -> AppResult<()> {
        if runner.len() != 36 || model.is_empty() || model.len() > 200 {
            return Err(AppError::Validation("桥接身份或模型无效".into()));
        }
        let mut state = self.state.lock().unwrap();
        if self.expired(&state) {
            return Err(offline());
        }
        if state.runner.as_deref().is_some_and(|value| value != runner) {
            return Err(AppError::Conflict(
                "此配对已被另一桥接进程使用，请重新配对".into(),
            ));
        }
        state.runner = Some(runner.into());
        state.model = Some(model.into());
        state.seen = Some(Instant::now());
        Ok(())
    }
    pub async fn poll(&self, runner: &str, model: &str) -> AppResult<Option<Job>> {
        self.claim(runner, model)?;
        let mut rx = self
            .receiver
            .try_lock()
            .map_err(|_| AppError::Busy("已有桥接等待请求".into()))?;
        let future = async {
            while let Some(job) = rx.recv().await {
                let state = self.state.lock().unwrap();
                if state.closed {
                    return None;
                }
                if state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.id == job.id)
                {
                    return Some(job);
                }
            }
            None
        };
        Ok(tokio::time::timeout(Duration::from_secs(15), future)
            .await
            .unwrap_or(None))
    }
    pub fn advertise(
        &self,
        runner: &str,
        model: &str,
        models: &[crate::models::CodexModel],
    ) -> AppResult<()> {
        if models.len() > 100
            || models.iter().any(|entry| {
                entry.model.is_empty()
                    || entry.model.len() > 200
                    || entry.display_name.len() > 800
                    || entry.description.len() > 8000
            })
            || (!models.is_empty() && !models.iter().any(|entry| entry.model == model))
        {
            return Err(AppError::Validation("桥接模型列表无效".into()));
        }
        self.claim(runner, model)?;
        let mut state = self.state.lock().unwrap();
        state.models = if models.is_empty() {
            // Older workers advertise only their default model.
            vec![crate::models::CodexModel {
                model: model.into(),
                display_name: model.into(),
                description: String::new(),
                is_default: true,
            }]
        } else {
            models.to_vec()
        };
        Ok(())
    }
    pub fn event(&self, runner: &str, id: &str, event: Event) -> AppResult<bool> {
        let mut state = self.state.lock().unwrap();
        if self.expired(&state) || state.runner.as_deref() != Some(runner) {
            return Err(offline());
        }
        state.seen = Some(Instant::now());
        let Some(active) = state.active.as_ref().filter(|active| active.id == id) else {
            return Ok(false);
        };
        active
            .events
            .try_send(event)
            .map_err(|_| AppError::Busy("桥接输出过快或请求已停止".into()))?;
        Ok(true)
    }
}
pub(crate) fn create() -> AppResult<(String, String)> {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let id = uuid::Uuid::new_v4().to_string();
    let (jobs, receiver) = mpsc::channel(4);
    let mut registry = registry().lock().unwrap();
    registry.retain(|_, connection| {
        let expired = connection.expired(&connection.state.lock().unwrap());
        if expired {
            connection.close();
        }
        !expired
    });
    if registry.len() >= 1024 {
        return Err(AppError::Busy("桥接连接数已达上限".into()));
    }
    registry.insert(
        id.clone(),
        Arc::new(Connection {
            token_hash: hash(&token),
            created: Instant::now(),
            jobs,
            receiver: AsyncMutex::new(receiver),
            state: Mutex::new(State {
                closed: false,
                runner: None,
                seen: None,
                model: None,
                models: Vec::new(),
                active: None,
            }),
        }),
    );
    Ok((id, token))
}
pub(crate) fn revoke(id: &str) {
    if let Some(connection) = registry().lock().unwrap().remove(id) {
        connection.close();
    }
}
pub(crate) fn authenticate(token: &str) -> AppResult<Arc<Connection>> {
    if token.len() != 64 {
        return Err(AppError::Auth("桥接配对码无效".into()));
    }
    let token_hash = hash(token);
    registry()
        .lock()
        .unwrap()
        .values()
        .find(|connection| {
            connection.token_hash == token_hash
                && !connection.expired(&connection.state.lock().unwrap())
        })
        .cloned()
        .ok_or_else(|| AppError::Auth("桥接配对已过期或被撤销，请重新配对".into()))
}
pub(crate) fn status(id: &str) -> Status {
    let registry = registry().lock().unwrap();
    let Some(connection) = registry.get(id) else {
        return Status::default();
    };
    let state = connection.state.lock().unwrap();
    Status {
        paired: !connection.expired(&state),
        online: !connection.expired(&state)
            && state.seen.is_some_and(|seen| seen.elapsed() < ONLINE_TTL),
        model: state.model.clone(),
        models: state.models.clone(),
    }
}
pub(crate) struct BridgeProvider {
    id: String,
    model: String,
    events: Option<super::streaming::EventSender>,
}
impl BridgeProvider {
    pub fn new(id: String, model: String) -> Self {
        Self {
            id,
            model,
            events: None,
        }
    }
    pub(crate) fn with_events(mut self, events: Option<super::streaming::EventSender>) -> Self {
        self.events = events;
        self
    }
    async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
        delta: Option<super::streaming::DeltaCallback>,
    ) -> AppResult<SearchCompletion> {
        let connection = registry()
            .lock()
            .unwrap()
            .get(&self.id)
            .cloned()
            .ok_or_else(offline)?;
        if !connection.live() {
            return Err(offline());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, mut receiver) = mpsc::channel(64);
        {
            let mut state = connection.state.lock().unwrap();
            if state.active.is_some() {
                return Err(AppError::Busy(
                    "本机 Codex 正在处理另一请求，请稍后重试".into(),
                ));
            }
            state.active = Some(Active {
                id: id.clone(),
                events: sender,
            });
        }
        // Dropping the browser's execution future cancels this job; the next worker
        // heartbeat observes cancellation and drops its Codex child process.
        let _guard = ActiveGuard {
            connection: connection.clone(),
            id: id.clone(),
        };
        connection
            .jobs
            .try_send(Job {
                id,
                model: self.model.clone(),
                messages,
                search,
            })
            .map_err(|_| AppError::Busy("本机桥接队列繁忙".into()))?;
        tokio::time::timeout(Duration::from_secs(240), async {
            loop {
                let event = tokio::time::timeout(Duration::from_secs(35), receiver.recv())
                    .await
                    .map_err(|_| offline())?;
                if connection.expired(&connection.state.lock().unwrap()) {
                    return Err(offline());
                }
                match event {
                    Some(Event::Delta { text }) => {
                        if let Some(callback) = &delta {
                            callback(&text);
                        }
                    }
                    Some(Event::Complete { result }) if !result.content.trim().is_empty() => {
                        return Ok(result)
                    }
                    Some(Event::Heartbeat) => {}
                    Some(Event::Status { message }) => super::streaming::status(
                        &self.events,
                        message.chars().take(300).collect::<String>(),
                    ),
                    Some(Event::Failure { reason }) => return Err(reason.into()),
                    _ => {
                        return Err(AppError::Model(
                            "本机 Codex 调用失败或桥接已断开，请查看本机桥接窗口".into(),
                        ))
                    }
                }
            }
        })
        .await
        .map_err(|_| AppError::Model("本机 Codex 桥接请求超时".into()))?
    }
}
struct ActiveGuard {
    connection: Arc<Connection>,
    id: String,
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let mut state = self.connection.state.lock().unwrap();
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.id == self.id)
        {
            state.active.take();
        }
    }
}
#[async_trait::async_trait]
impl ModelProvider for BridgeProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false, None).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
            request_count: result.api_calls,
        })
    }
    async fn complete_stream(
        &self,
        messages: Vec<ChatMessage>,
        delta: super::streaming::DeltaCallback,
    ) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false, Some(delta)).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
            request_count: result.api_calls,
        })
    }
    async fn search(&self, messages: Vec<ChatMessage>) -> AppResult<SearchCompletion> {
        self.generate(messages, true, None).await
    }
    fn provider_name(&self) -> &str {
        "codex-bridge"
    }
    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (String, Arc<Connection>, String) {
        let (id, token) = create().unwrap();
        let connection = authenticate(&token).unwrap();
        let runner = uuid::Uuid::new_v4().to_string();
        connection.claim(&runner, "fixture").unwrap();
        (id, connection, runner)
    }
    #[tokio::test]
    async fn bridge_forwards_retry_progress_and_preserves_safe_failure_category() {
        let (id, connection, runner) = setup();
        let (sender, mut receiver) = mpsc::channel(8);
        let provider = BridgeProvider::new(id.clone(), "fixture".into()).with_events(Some(sender));
        let task = tokio::spawn(async move { provider.complete(vec![]).await });
        let job = connection.poll(&runner, "fixture").await.unwrap().unwrap();
        connection
            .event(
                &runner,
                &job.id,
                Event::Status {
                    message: "Codex 正在自动重试（1/2）…".into(),
                },
            )
            .unwrap();
        let event = receiver.recv().await.unwrap();
        assert!(
            matches!(event,super::super::streaming::AnalysisEvent::Status {message} if message.contains("自动重试"))
        );
        connection
            .event(
                &runner,
                &job.id,
                Event::Failure {
                    reason: crate::models::inference::CodexFailure::UsageLimit,
                },
            )
            .unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(AppError::Codex(
                crate::models::inference::CodexFailure::UsageLimit
            ))
        ));
        revoke(&id);
    }
    #[tokio::test]
    async fn bridge_streams_only_matching_job_and_preserves_search_provenance() {
        let (id, connection, runner) = setup();
        let captured = Arc::new(Mutex::new(String::new()));
        let output = captured.clone();
        let provider = BridgeProvider::new(id.clone(), "fixture".into());
        let task = tokio::spawn(async move {
            provider
                .generate(
                    vec![ChatMessage::user("test")],
                    true,
                    Some(Box::new(move |text| output.lock().unwrap().push_str(text))),
                )
                .await
        });
        let job = connection.poll(&runner, "fixture").await.unwrap().unwrap();
        assert!(job.search);
        assert!(!connection
            .event(
                &runner,
                "other-job",
                Event::Delta {
                    text: "wrong".into()
                }
            )
            .unwrap());
        assert!(connection
            .event(&uuid::Uuid::new_v4().to_string(), &job.id, Event::Heartbeat)
            .is_err());
        connection
            .event(
                &runner,
                &job.id,
                Event::Delta {
                    text: "answer".into(),
                },
            )
            .unwrap();
        connection
            .event(
                &runner,
                &job.id,
                Event::Complete {
                    result: SearchCompletion {
                        content: "answer".into(),
                        api_calls: 1,
                        search_calls: 1,
                        sources: vec![super::super::provider::SearchSource {
                            url: "https://example.org".into(),
                            title: "source".into(),
                            claim: "evidence".into(),
                            original: false,
                        }],
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        let result = task.await.unwrap().unwrap();
        assert_eq!(result.sources.len(), 1);
        assert_eq!(result.search_calls, 1);
        assert_eq!(*captured.lock().unwrap(), "answer");
        assert!(!connection
            .event(&runner, &job.id, Event::Heartbeat)
            .unwrap());
        revoke(&id);
    }
    #[tokio::test]
    async fn bridge_cancellation_and_revocation_release_jobs_and_reject_late_results() {
        let (id, connection, runner) = setup();
        let provider = BridgeProvider::new(id.clone(), "fixture".into());
        let task = tokio::spawn(async move { provider.complete(vec![]).await });
        let job = connection.poll(&runner, "fixture").await.unwrap().unwrap();
        let busy = BridgeProvider::new(id.clone(), "fixture".into())
            .complete(vec![])
            .await;
        assert!(matches!(busy, Err(AppError::Busy(_))));
        task.abort();
        let _ = task.await;
        assert!(!connection
            .event(&runner, &job.id, Event::Heartbeat)
            .unwrap());
        let provider = BridgeProvider::new(id.clone(), "fixture".into());
        let next = tokio::spawn(async move { provider.complete(vec![]).await });
        let job = connection.poll(&runner, "fixture").await.unwrap().unwrap();
        revoke(&id);
        assert!(next.await.unwrap().is_err());
        assert!(connection
            .event(&runner, &job.id, Event::Heartbeat)
            .is_err());
        assert!(!status(&id).paired);
    }
    #[test]
    fn bridge_expiry_and_single_worker_claim_are_enforced() {
        let (id, connection, runner) = setup();
        assert!(connection.claim(&runner, "fixture").is_ok());
        assert!(connection
            .claim(&uuid::Uuid::new_v4().to_string(), "fixture")
            .is_err());
        assert!(authenticate(&"a".repeat(64)).is_err());
        {
            let mut state = connection.state.lock().unwrap();
            state.seen = Some(Instant::now() - ONLINE_TTL);
        }
        assert!(!status(&id).online);
        revoke(&id);
        let mut connection = Arc::try_unwrap(connection).ok().unwrap();
        connection.created = Instant::now() - SESSION_TTL;
        let mut state = connection.state.lock().unwrap();
        state.closed = false;
        assert!(connection.expired(&state));
        state.runner = None;
        connection.created = Instant::now() - PAIR_TTL;
        assert!(connection.expired(&state));
    }
}
