//! Official Codex App Server adapter. Credentials remain owned and refreshed by
//! Codex; no OAuth token or auth.json content is returned to the frontend.
use super::provider::{
    ChatMessage, ModelCompletion, ModelProvider, SearchCompletion, SearchSource,
};
use crate::error::{AppError, AppResult};
use crate::models::inference::{CodexFailure, MODEL_CALL_TIMEOUT};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct CodexProvider {
    model: String,
    status: Option<StatusCallback>,
}
type StatusCallback = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

async fn retry_turn<T, F, Fut>(
    mut operation: F,
    emitted: &std::sync::atomic::AtomicBool,
    status: Option<&StatusCallback>,
) -> AppResult<(T, usize)>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = AppResult<T>>,
{
    tokio::time::timeout(MODEL_CALL_TIMEOUT, async {
        for attempt in 0..3 {
            match operation().await {
                Ok(output) => return Ok((output, attempt + 1)),
                Err(AppError::Codex(reason))
                    if reason.retryable()
                        && !emitted.load(std::sync::atomic::Ordering::Relaxed)
                        && attempt < 2 =>
                {
                    if let Some(status) = status {
                        status(&format!(
                            "Codex 暂时连接失败，正在自动重试（{}/2）…",
                            attempt + 1
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(
                        1000 * (1 << attempt) as u64 + rand::random::<u8>() as u64,
                    ))
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    })
    .await
    .map_err(|_| AppError::Codex(CodexFailure::Timeout))?
}
impl CodexProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            status: None,
        }
    }
    pub(crate) fn with_status(mut self, status: StatusCallback) -> Self {
        self.status = Some(status);
        self
    }
    pub(crate) fn with_events(self, events: Option<super::streaming::EventSender>) -> Self {
        self.with_status(std::sync::Arc::new(move |message| {
            super::streaming::status(&events, message)
        }))
    }
    pub async fn detect() -> AppResult<String> {
        let models = Self::models().await?;
        Ok(models
            .iter()
            .find(|model| model.is_default)
            .unwrap_or(&models[0])
            .model
            .clone())
    }
    pub async fn models() -> AppResult<Vec<crate::models::CodexModel>> {
        tokio::time::timeout(Duration::from_secs(25), async {
            let mut rpc = Rpc::start().await?;
            rpc.account().await?;
            let mut catalog = Vec::<crate::models::CodexModel>::new();
            let mut cursor = Value::Null;
            for _ in 0..10 {
                let page = rpc
                    .call(
                        "model/list",
                        json!({"limit":100,"includeHidden":false,"cursor":cursor}),
                    )
                    .await?;
                for model in parse_models(&page)? {
                    if !catalog.iter().any(|entry| entry.model == model.model) {
                        catalog.push(model);
                    }
                }
                let next = page["nextCursor"].clone();
                if next.is_null() {
                    break;
                }
                if next == cursor || !next.is_string() {
                    return Err(AppError::Model(
                        "Codex 模型列表分页无效，请更新 Codex 后重试".into(),
                    ));
                }
                cursor = next;
            }
            if catalog.is_empty() || catalog.len() > 100 {
                return Err(AppError::Model("Codex 没有返回有效的可选模型列表".into()));
            }
            Ok(catalog)
        })
        .await
        .map_err(|_| {
            AppError::Model("读取 Codex 凭证超时，请打开 Codex 检查登录状态后重试".into())
        })?
    }
    pub(crate) async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
        on_delta: Option<super::streaming::DeltaCallback>,
    ) -> AppResult<SearchCompletion> {
        // One deadline covers all attempts. Never restart a turn after any assistant
        // output, even when this caller did not request streaming callbacks.
        let emitted = std::sync::atomic::AtomicBool::new(false);
        let (mut output, attempts) = retry_turn(
            || self.generate_once(&messages, search, on_delta.as_ref(), &emitted),
            &emitted,
            self.status.as_ref(),
        )
        .await?;
        output.api_calls = attempts;
        Ok(output)
    }
    async fn generate_once(
        &self,
        messages: &[ChatMessage],
        search: bool,
        on_delta: Option<&super::streaming::DeltaCallback>,
        emitted: &std::sync::atomic::AtomicBool,
    ) -> AppResult<SearchCompletion> {
        let mut rpc = Rpc::start().await?;
        rpc.account().await?;
        let work = WorkDir::new()?;
        let instructions = messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let prompt = messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let thread = rpc.call("thread/start", json!({
                "model":self.model, "modelProvider":"openai", "ephemeral":true,
                "cwd":work.0, "approvalPolicy":"never", "sandbox":"read-only",
                "environments":[], "selectedCapabilityRoots":[],
                "baseInstructions":format!("你是 mario 的问答模型。仅依据本次输入回答。禁止调用文件、终端、应用、MCP 或代理工具。{}\n{}", if search { "只允许内置网页搜索；来源必须提供链接。" } else { "禁止调用工具。" }, instructions),
                "config": {"web_search":if search { "live" } else { "disabled" },
                    "features.shell_tool":false, "features.multi_agent":false,
                    "features.apps":false, "features.skills":false, "features.hooks":false,
                    "mcp_servers":{}}
            })).await?;
        let id = thread["thread"]["id"]
            .as_str()
            .ok_or_else(|| AppError::Model("Codex 未创建临时会话".into()))?;
        rpc.call("turn/start", json!({"threadId":id,"input":[{"type":"text","text":prompt}],"effort":"low","environments":[]})).await?;
        let mut output = SearchCompletion {
            api_calls: 1,
            ..Default::default()
        };
        loop {
            let event = rpc.read().await?;
            if event.get("id").is_some() && event.get("method").is_some() {
                // No interactive tool requests are permitted in this model adapter.
                return Err(AppError::Model(
                    "Codex 请求了问答以外的工具操作，已停止本次调用".into(),
                ));
            }
            match event["method"].as_str() {
                Some("item/agentMessage/delta") => {
                    if let Some(delta) = event["params"]["delta"]
                        .as_str()
                        .filter(|delta| !delta.is_empty())
                    {
                        emitted.store(true, std::sync::atomic::Ordering::Relaxed);
                        if let Some(callback) = on_delta {
                            callback(delta);
                        }
                    }
                }
                Some("item/completed") => {
                    let item = &event["params"]["item"];
                    if item["type"] == "agentMessage"
                        && item["text"].as_str().is_some_and(|text| !text.is_empty())
                    {
                        emitted.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    ingest_item(item, &mut output)?;
                }
                Some("error") => {
                    if event["params"]["willRetry"] == true {
                        if let Some(status) = &self.status {
                            status("Codex 正在恢复上游连接，请稍候…");
                        }
                    } else {
                        return Err(classify_failure(&event["params"]["error"]).into());
                    }
                }
                Some("thread/tokenUsage/updated") => {
                    let usage = &event["params"]["tokenUsage"]["total"];
                    output.usage.input_tokens = usage["inputTokens"].as_u64();
                    output.usage.output_tokens = usage["outputTokens"].as_u64();
                }
                Some("turn/completed") => {
                    if event["params"]["turn"]["status"] != "completed" {
                        return Err(classify_failure(&event["params"]["turn"]["error"]).into());
                    }
                    if output.content.trim().is_empty() {
                        return Err(AppError::Model("Codex 没有返回有效回答".into()));
                    }
                    return Ok(output);
                }
                _ => {}
            }
        }
    }
}

fn classify_failure(error: &Value) -> CodexFailure {
    let info = if error["codexErrorInfo"].is_null() {
        &error["data"]["codexErrorInfo"]
    } else {
        &error["codexErrorInfo"]
    };
    // Current App Server uses externally tagged objects for HTTP/stream errors;
    // older builds may use a type field or string enum variant.
    let tagged = info
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.iter().next());
    let kind = info
        .as_str()
        .or_else(|| info["type"].as_str())
        .or_else(|| tagged.map(|(kind, _)| kind.as_str()))
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match kind.as_str() {
        "usagelimitexceeded" => CodexFailure::UsageLimit,
        "sessionbudgetexceeded" => CodexFailure::SessionBudget,
        "unauthorized" => CodexFailure::Unauthorized,
        "contextwindowexceeded" => CodexFailure::ContextLimit,
        "badrequest" | "cyberpolicy" | "misalignmentpolicyviolation" | "toomanydenials" => {
            CodexFailure::Rejected
        }
        "ratelimitexceeded" | "serveroverloaded" | "flexunavailable" => CodexFailure::Busy,
        // App Server already exhausted its own retries; do not multiply them.
        "responsetoomanyfailedattempts" => CodexFailure::Other,
        _ => match info["httpStatusCode"]
            .as_u64()
            .or_else(|| tagged.and_then(|(_, details)| details["httpStatusCode"].as_u64()))
            .or_else(|| error["httpStatusCode"].as_u64())
        {
            Some(401 | 403) => CodexFailure::Unauthorized,
            Some(429) => CodexFailure::Busy,
            Some(500 | 502 | 503 | 504) => CodexFailure::Unavailable,
            Some(400 | 404 | 422) => CodexFailure::Rejected,
            _ if error["code"] == -32001 => CodexFailure::Busy,
            _ if matches!(
                kind.as_str(),
                "responsestreamconnectionfailed" | "responsestreamdisconnected"
            ) =>
            {
                CodexFailure::Disconnected
            }
            _ if kind == "internalservererror" => CodexFailure::Unavailable,
            _ => CodexFailure::Other,
        },
    }
}

fn parse_models(page: &Value) -> AppResult<Vec<crate::models::CodexModel>> {
    let items = page["data"]
        .as_array()
        .ok_or_else(|| AppError::Model("Codex 模型列表格式无效".into()))?;
    items
        .iter()
        .filter(|item| item["hidden"] != true)
        .map(|item| {
            let model = item["model"]
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 200)
                .ok_or_else(|| AppError::Model("Codex 模型 ID 无效".into()))?;
            Ok(crate::models::CodexModel {
                model: model.into(),
                display_name: item["displayName"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(model)
                    .chars()
                    .take(200)
                    .collect(),
                description: item["description"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(2000)
                    .collect(),
                is_default: item["isDefault"] == true,
            })
        })
        .collect()
}
#[async_trait]
impl ModelProvider for CodexProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false, None).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
            request_count: result.api_calls.max(1),
        })
    }
    async fn search(&self, messages: Vec<ChatMessage>) -> AppResult<SearchCompletion> {
        self.generate(messages, true, None).await
    }
    async fn complete_stream(
        &self,
        messages: Vec<ChatMessage>,
        on_delta: super::streaming::DeltaCallback,
    ) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false, Some(on_delta)).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
            request_count: result.api_calls.max(1),
        })
    }
    fn provider_name(&self) -> &str {
        "codex"
    }
    fn model_name(&self) -> &str {
        &self.model
    }
}

fn binary() -> AppResult<PathBuf> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Err(AppError::Validation(
            "此设备无法读取桌面 Codex 凭证，请使用 OpenAI 或 Anthropic API Key".into(),
        ));
    }
    let mut candidates = Vec::new();
    for name in ["MARIO_CODEX_BIN", "CODEX_CLI_PATH"] {
        if let Some(path) = std::env::var_os(name) {
            candidates.push(PathBuf::from(path));
        }
    }
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        // Desktop installs keep versioned native binaries here. Prefer the newest
        // installed executable when a standalone terminal has no CODEX_CLI_PATH.
        let root = PathBuf::from(local).join("OpenAI/Codex/bin");
        if let Ok(entries) = std::fs::read_dir(root) {
            let mut installed = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("codex.exe"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            installed.sort_by_key(|path| {
                std::cmp::Reverse(path.metadata().and_then(|meta| meta.modified()).ok())
            });
            candidates.extend(installed);
        }
    }
    candidates.extend([
        PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
        PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
    ]);
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/ChatGPT.app/Contents/Resources/codex"));
        candidates.push(home.join("Applications/Codex.app/Contents/Resources/codex"));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ]);
    if let Some(path) = std::env::var_os("PATH") {
        // npm installs a .cmd launcher on Windows; spawn the native executable
        // directly so prompts and paths never pass through a shell.
        #[cfg(windows)]
        for dir in std::env::split_paths(&path) {
            for arch in ["x64", "arm64"] {
                let triple = if arch == "x64" { "x86_64" } else { "aarch64" };
                candidates.push(dir.join(format!("node_modules/@openai/codex/node_modules/@openai/codex-win32-{arch}/vendor/{triple}-pc-windows-msvc/bin/codex.exe")));
                candidates.push(dir.join(format!(
                    "node_modules/@openai/codex/vendor/{triple}-pc-windows-msvc/codex/codex.exe"
                )));
            }
        }
        candidates.extend(
            std::env::split_paths(&path)
                .map(|dir| dir.join(if cfg!(windows) { "codex.exe" } else { "codex" })),
        );
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        AppError::Validation("读取失败：没有找到 Codex，请先安装并登录 Codex".into())
    })
}
struct WorkDir(PathBuf);
impl WorkDir {
    fn new() -> AppResult<Self> {
        let path = std::env::temp_dir().join(format!("mario-codex-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Rpc {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
    pending: std::collections::VecDeque<Value>,
}
impl Rpc {
    async fn start() -> AppResult<Self> {
        let mut child = Command::new(binary()?)
            .args(["app-server", "-c", "model_provider=\"openai\""])
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| AppError::Model("无法启动 Codex，请检查 Codex 安装是否完整".into()))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Model("Codex 输入通道不可用".into()))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| AppError::Model("Codex 输出通道不可用".into()))?,
        );
        let mut rpc = Self {
            _child: child,
            input,
            output,
            sequence: 0,
            pending: Default::default(),
        };
        rpc.call("initialize",json!({"clientInfo":{"name":"mario","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        rpc.send(json!({"method":"initialized","params":{}}))
            .await?;
        Ok(rpc)
    }
    async fn send(&mut self, value: Value) -> AppResult<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| AppError::Codex(CodexFailure::Disconnected))?;
        self.input
            .flush()
            .await
            .map_err(|_| AppError::Codex(CodexFailure::Disconnected))?;
        Ok(())
    }
    async fn wire_read(&mut self) -> AppResult<Value> {
        let mut bytes = Vec::new();
        // Bound each event before parsing; never log responses or credentials.
        loop {
            let available = self
                .output
                .fill_buf()
                .await
                .map_err(|_| AppError::Codex(CodexFailure::Disconnected))?;
            if available.is_empty() {
                return Err(CodexFailure::Disconnected.into());
            }
            let end = available
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(available.len());
            if bytes.len() + end > 4 * 1024 * 1024 {
                return Err(AppError::Model("Codex 返回的数据过大".into()));
            }
            let finished = available[end - 1] == b'\n';
            bytes.extend_from_slice(&available[..end]);
            self.output.consume(end);
            if finished {
                break;
            }
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Model("Codex 协议响应无效，请更新 Codex 后重试".into()))
    }
    async fn read(&mut self) -> AppResult<Value> {
        if let Some(value) = self.pending.pop_front() {
            Ok(value)
        } else {
            self.wire_read().await
        }
    }
    async fn call(&mut self, method: &str, params: Value) -> AppResult<Value> {
        self.sequence += 1;
        let id = self.sequence;
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let event = self.wire_read().await?;
            if event["id"].as_u64() == Some(id) && event.get("method").is_none() {
                if event.get("error").is_some() {
                    return Err(classify_failure(&event["error"]).into());
                }
                return Ok(event["result"].clone());
            }
            if event.get("id").is_some() && event.get("method").is_some() {
                return Err(AppError::Model(
                    "Codex 需要交互确认，请先在 Codex 中完成登录".into(),
                ));
            }
            if self.pending.len() >= 2048 {
                return Err(AppError::Model("Codex 返回事件过多".into()));
            }
            self.pending.push_back(event);
        }
    }
    async fn account(&mut self) -> AppResult<()> {
        let result = self
            .call("account/read", json!({"refreshToken":false}))
            .await?;
        check_account(&result)
    }
}
fn check_account(value: &Value) -> AppResult<()> {
    match value["account"]["type"].as_str() {
        Some("chatgpt" | "apiKey") => Ok(()),
        _ => Err(AppError::Validation(
            "读取失败：未找到已登录的 Codex 凭证，请先在 Codex 中登录".into(),
        )),
    }
}
fn ingest_item(item: &Value, output: &mut SearchCompletion) -> AppResult<()> {
    match item["type"].as_str() {
        Some("agentMessage") => {
            if item["phase"].as_str() != Some("commentary") {
                output.content = item["text"].as_str().unwrap_or_default().to_string();
            }
        }
        Some("webSearch") => {
            output.search_calls += 1;
            // Only structured provider results qualify as evidence, never links
            // invented in an assistant's plain-text answer.
            if let Some(results) = item["results"].as_array() {
                for source in results {
                    collect_source(source, output);
                }
            }
        }
        Some(
            "commandExecution"
            | "fileChange"
            | "mcpToolCall"
            | "dynamicToolCall"
            | "collabAgentToolCall",
        ) => {
            return Err(AppError::Model(
                "Codex 尝试调用问答以外的工具，已停止本次调用".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}
fn collect_source(source: &Value, output: &mut SearchCompletion) {
    let url = source["url"].as_str().unwrap_or_default();
    let title = source["title"].as_str().unwrap_or_default();
    let claim = source["snippet"]
        .as_str()
        .or_else(|| source["text"].as_str())
        .unwrap_or_default();
    if reqwest::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https") && u.username().is_empty() && u.password().is_none()
    }) && !claim.is_empty()
        && !output
            .sources
            .iter()
            .any(|s| s.url == url && s.claim == claim)
    {
        output.sources.push(SearchSource {
            url: url.into(),
            title: title.into(),
            claim: claim.into(),
            original: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    };

    #[test]
    fn classifies_native_error_protocol_without_echoing_provider_messages() {
        for (info, expected) in [
            (
                json!({"httpConnectionFailed":{"httpStatusCode":503}}),
                CodexFailure::Unavailable,
            ),
            (
                json!({"responseStreamDisconnected":{"httpStatusCode":401}}),
                CodexFailure::Unauthorized,
            ),
            (
                json!({"responseStreamConnectionFailed":{"httpStatusCode":null}}),
                CodexFailure::Disconnected,
            ),
            (
                json!({"responseTooManyFailedAttempts":{"httpStatusCode":503}}),
                CodexFailure::Other,
            ),
            (json!("usageLimitExceeded"), CodexFailure::UsageLimit),
            (json!("sessionBudgetExceeded"), CodexFailure::SessionBudget),
            (json!("rateLimitExceeded"), CodexFailure::Busy),
            (json!("contextWindowExceeded"), CodexFailure::ContextLimit),
            (json!("badRequest"), CodexFailure::Rejected),
        ] {
            let error = json!({"codexErrorInfo":info,"message":"secret-provider-payload"});
            assert_eq!(classify_failure(&error), expected);
            assert!(!classify_failure(&error).to_string().contains("secret"));
        }
        assert_eq!(
            classify_failure(&json!({"code":-32001})),
            CodexFailure::Busy
        );
        assert_eq!(
            classify_failure(&json!({"message":"insufficient balance"})),
            CodexFailure::Other
        );
    }

    #[tokio::test(start_paused = true)]
    async fn transient_turn_failures_retry_twice_with_progress_and_count_actual_attempts() {
        let calls = AtomicUsize::new(0);
        let emitted = AtomicBool::new(false);
        let progress = Arc::new(Mutex::new(Vec::new()));
        let captured = progress.clone();
        let status: StatusCallback =
            Arc::new(move |message| captured.lock().unwrap().push(message.to_string()));
        let (output, count) = retry_turn(
            || {
                let attempt = calls.fetch_add(1, Ordering::Relaxed);
                async move {
                    if attempt < 2 {
                        Err(CodexFailure::Unavailable.into())
                    } else {
                        Ok("answer")
                    }
                }
            },
            &emitted,
            Some(&status),
        )
        .await
        .unwrap();
        assert_eq!(output, "answer");
        assert_eq!(count, 3);
        assert_eq!(progress.lock().unwrap().len(), 2);
        let failed = retry_turn(
            || async { Err::<(), _>(CodexFailure::Busy.into()) },
            &emitted,
            None,
        )
        .await;
        assert!(matches!(failed, Err(AppError::Codex(CodexFailure::Busy))));
    }

    #[tokio::test(start_paused = true)]
    async fn permanent_errors_or_partial_output_are_never_replayed() {
        for (reason, partial) in [
            (CodexFailure::UsageLimit, false),
            (CodexFailure::SessionBudget, false),
            (CodexFailure::Unauthorized, false),
            (CodexFailure::Rejected, false),
            (CodexFailure::ContextLimit, false),
            (CodexFailure::Other, false),
            (CodexFailure::Disconnected, true),
        ] {
            let calls = AtomicUsize::new(0);
            let emitted = AtomicBool::new(partial);
            let result = retry_turn(
                || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    async { Err::<(), _>(reason.into()) }
                },
                &emitted,
                None,
            )
            .await;
            assert!(matches!(result,Err(AppError::Codex(actual)) if actual==reason));
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_during_backoff_and_call_deadline_stop_work() {
        let calls = Arc::new(AtomicUsize::new(0));
        let captured = calls.clone();
        let task = tokio::spawn(async move {
            retry_turn(
                || {
                    captured.fetch_add(1, Ordering::Relaxed);
                    async { Err::<(), _>(CodexFailure::Busy.into()) }
                },
                &AtomicBool::new(false),
                None,
            )
            .await
        });
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        task.abort();
        let _ = task.await;
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        let result = retry_turn(
            std::future::pending::<AppResult<()>>,
            &AtomicBool::new(false),
            None,
        )
        .await;
        assert!(matches!(
            result,
            Err(AppError::Codex(CodexFailure::Timeout))
        ));
    }

    #[test]
    fn catalog_uses_native_labels_and_defaults_and_filters_hidden_models() {
        let models = parse_models(&json!({"data":[
            {"model":"a","displayName":"Model A","description":"Tasks","isDefault":true},
            {"model":"hidden","hidden":true}, {"model":"b"}
        ]}))
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].display_name, "Model A");
        assert!(models[0].is_default);
        assert_eq!(models[1].display_name, "b");
        assert!(parse_models(&json!({"data":[{"model":""}]})).is_err());
        assert!(parse_models(&json!({})).is_err());
    }

    #[tokio::test]
    #[ignore = "Reads the user's local Codex model catalog; no inference"]
    async fn live_codex_model_catalog() {
        let models = CodexProvider::models().await.unwrap();
        assert!(!models.is_empty());
        for model in models {
            println!(
                "{} | {} | default={}",
                model.model, model.display_name, model.is_default
            );
        }
    }

    #[test]
    fn missing_or_unknown_account_is_a_visible_failure() {
        for account in [json!(null), json!({}), json!({"type":"unknown"})] {
            assert!(check_account(&json!({"account":account}))
                .unwrap_err()
                .to_string()
                .contains("未找到"));
        }
        assert!(check_account(&json!({"account":{"type":"chatgpt"}})).is_ok());
        assert!(check_account(&json!({"account":{"type":"apiKey"}})).is_ok());
    }

    #[test]
    fn only_native_search_results_become_sources() {
        let mut output = SearchCompletion::default();
        ingest_item(
            &json!({"type":"agentMessage","text":"https://invented.example/"}),
            &mut output,
        )
        .unwrap();
        assert!(output.sources.is_empty());
        let result = json!({"type":"webSearch","results":[
            {"url":"https://example.com/docs","title":"Docs","snippet":"Native search excerpt"},
            {"url":"https://example.com/docs","title":"Docs","snippet":"Native search excerpt"},
            {"url":"file:///private/data","snippet":"Invalid scheme"},
            {"url":"https://user:password@example.com","snippet":"Credentials"},
            {"title":"Missing URL","snippet":"Not evidence"}
        ]});
        ingest_item(&result, &mut output).unwrap();
        assert_eq!(output.search_calls, 1);
        assert_eq!(output.sources.len(), 1);
        assert_eq!(output.sources[0].claim, "Native search excerpt");
        assert!(!output.sources[0].original);
        ingest_item(
            &json!({"type":"agentMessage","phase":"commentary","text":"Searching"}),
            &mut output,
        )
        .unwrap();
        assert_eq!(output.content, "https://invented.example/");
        assert!(ingest_item(&json!({"type":"commandExecution"}), &mut output).is_err());
    }

    #[tokio::test]
    #[ignore = "Uses the user's local Codex login and model quota; run explicitly"]
    async fn live_codex_completion_and_native_search() {
        let model = CodexProvider::detect().await.unwrap();
        let provider = CodexProvider::new(model);
        let reply = provider
            .complete(vec![ChatMessage::user("Reply only OK")])
            .await
            .unwrap();
        assert!(reply.content.contains("OK"));
        let search = provider.search(vec![ChatMessage::user("Use native web search to find the official OpenAI Responses API documentation. Search site:platform.openai.com/docs Responses API. Briefly identify the page in one sentence.")]).await.unwrap();
        assert!(search.search_calls > 0, "Expected native web search");
        assert!(
            !search.sources.is_empty(),
            "Expected structured search sources"
        );
        assert!(!search.content.is_empty());
    }
}
