//! Runs on the user's computer. No inbound listener and no reads of auth.json.
use crate::{
    ai::{
        bridge::{Event, Job},
        CodexProvider,
    },
    models::connection::{BridgeMode, CodexModel},
    AppError, AppResult,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

fn api_url(site: &str) -> AppResult<String> {
    let url =
        reqwest::Url::parse(site).map_err(|_| AppError::Validation("Mario 网页地址无效".into()))?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "桥接要求 HTTPS 网页地址（本机测试可用 HTTP），且不能含凭据、查询或片段".into(),
        ));
    }
    Ok(format!(
        "{}/api/codex-bridge/worker",
        url.as_str().trim_end_matches('/')
    ))
}

#[derive(Clone)]
struct ChatGptSession {
    account: String,
    session: String,
    account_label: Option<String>,
    model: String,
    models: Vec<CodexModel>,
}

enum Backend {
    Codex {
        model: String,
        models: Vec<CodexModel>,
    },
    ChatGpt(Option<ChatGptSession>),
}

impl Backend {
    fn mode(&self) -> BridgeMode {
        match self {
            Self::Codex { .. } => BridgeMode::Codex,
            Self::ChatGpt(_) => BridgeMode::Chatgpt,
        }
    }
    fn auth_required(&self) -> bool {
        matches!(self, Self::ChatGpt(None))
    }
    fn model(&self) -> &str {
        match self {
            Self::Codex { model, .. } => model,
            Self::ChatGpt(Some(current)) => &current.model,
            Self::ChatGpt(None) => "",
        }
    }
    fn models(&self) -> &[CodexModel] {
        match self {
            Self::Codex { models, .. } => models,
            Self::ChatGpt(Some(current)) => &current.models,
            Self::ChatGpt(None) => &[],
        }
    }
    fn account_label(&self) -> Option<&str> {
        match self {
            Self::Codex { .. } | Self::ChatGpt(None) => None,
            Self::ChatGpt(Some(current)) => current.account_label.as_deref(),
        }
    }
    fn check(&self) -> AppResult<()> {
        if let Self::ChatGpt(Some(current)) = self {
            match crate::chatgpt_auth::is_authorized(&current.account, &current.session) {
                Ok(true) | Err(AppError::Busy(_)) => {}
                _ => {
                    return Err(AppError::Validation(
                        "ChatGPT 授权已退出、失效或在终端被替换；请重新授权".into(),
                    ))
                }
            }
        }
        Ok(())
    }
}

async fn load_chatgpt_session(account: String) -> AppResult<ChatGptSession> {
    let account_label = crate::chatgpt_auth::account_label(&account)?;
    let session = crate::chatgpt_auth::session(&account)?;
    let models = crate::ai::chatgpt::ChatGptProvider::models(&account, &session).await?;
    let model = models
        .iter()
        .find(|entry| entry.is_default)
        .unwrap_or(&models[0])
        .model
        .clone();
    Ok(ChatGptSession {
        account,
        session,
        account_label,
        model,
        models,
    })
}

pub(crate) async fn run(site: &str) -> AppResult<()> {
    api_url(site)?;
    eprintln!("正在检查本机 Codex 模型授权与模型列表…");
    let models = CodexProvider::models().await?;
    let model = models
        .iter()
        .find(|entry| entry.is_default)
        .unwrap_or(&models[0])
        .model
        .clone();
    eprintln!("本机模型已就绪：{model}");
    connect(site, Backend::Codex { model, models }).await
}

pub(crate) async fn run_chatgpt(site: &str) -> AppResult<()> {
    api_url(site)?;
    let initial = if crate::chatgpt_auth::has_active_account()? {
        let account = crate::chatgpt_auth::active_account()?;
        eprintln!("正在检查本机 ChatGPT 注册 {account} 的模型列表…");
        match load_chatgpt_session(account).await {
            Ok(session) => {
                eprintln!("本机 ChatGPT 模型已就绪：{}", session.model);
                Some(session)
            }
            Err(error) => {
                eprintln!("{error}；可在配对后于 Mario 网页设置点击“跳转 ChatGPT OAuth 登录”。");
                None
            }
        }
    } else {
        eprintln!(
            "本机尚无已授权的 ChatGPT 账号；请先粘贴配对码连接 Mario，随后直接在网页设置点击“跳转 ChatGPT OAuth 登录”。"
        );
        None
    };
    connect(site, Backend::ChatGpt(initial)).await
}

async fn connect(site: &str, backend: Backend) -> AppResult<()> {
    let base = api_url(site)?;
    eprintln!("目标网站：{site}");
    eprintln!("请粘贴 Mario 设置页面生成的配对码，然后回车（不需要 ChatGPT 密码或 API Key）：");
    let mut token = String::new();
    std::io::stdin().read_line(&mut token)?;
    let token = token.trim().to_owned();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Validation("配对码格式无效".into()));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let runner = uuid::Uuid::new_v4().to_string();
    eprintln!("正在连接；保持此进程运行，关闭窗口或按 Ctrl+C 即停止。配对最长有效 12 小时。");
    tokio::select! {
        result = work(&client, &base, &token, &runner, backend) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

async fn work(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    runner: &str,
    mut backend: Backend,
) -> AppResult<()> {
    let mut connected = false;
    let (oauth_tx, mut oauth_rx) = mpsc::channel::<ChatGptSession>(2);
    let mut oauth_task: Option<JoinHandle<()>> = None;
    loop {
        if matches!(backend, Backend::ChatGpt(Some(_))) && backend.check().is_err() {
            eprintln!("本机 ChatGPT 授权已退出或失效，已切换为待登录状态；可在 Mario 网页设置点击“跳转 ChatGPT OAuth 登录”。");
            backend = Backend::ChatGpt(None);
        }
        let poll_future = client
            .post(format!("{base}/poll"))
            .bearer_auth(token)
            .json(&json!({
                "runner": runner,
                "model": backend.model(),
                "models": backend.models(),
                "mode": backend.mode(),
                "accountLabel": backend.account_label(),
                "authRequired": backend.auth_required(),
            }))
            .send();
        let response = tokio::select! {
            Some(session) = oauth_rx.recv() => {
                eprintln!("网页 ChatGPT OAuth 登录已完成，模型已就绪：{}", session.model);
                backend = Backend::ChatGpt(Some(session));
                continue;
            }
            response = poll_future => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(_) => {
                eprintln!("连接中断，5 秒后重连；不会重放已开始的模型请求。");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        if !response.status().is_success() {
            if matches!(
                response.status().as_u16(),
                408 | 429 | 500 | 502 | 503 | 504
            ) {
                eprintln!(
                    "桥接服务暂时不可用（HTTP {}），5 秒后自动重连。",
                    response.status().as_u16()
                );
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            return Err(AppError::Model(format!(
                "桥接连接失败（HTTP {}）。请确认服务器已部署新版，并在网页重新配对。",
                response.status().as_u16()
            )));
        }
        if !connected {
            if backend.auth_required() {
                eprintln!("桥接已在线，请回到 Mario 设置点击“跳转 ChatGPT OAuth 登录”。");
            } else {
                eprintln!("桥接已在线，请回到 Mario 设置点击“启用本机模型”。");
            }
            connected = true;
        }
        let job: Option<Job> = response.json().await?;
        if let Some(job) = job {
            if let Some(new_account) = job.oauth_new_account {
                match crate::chatgpt_auth::begin_browser_login(new_account).await {
                    Ok(login) => {
                        let url = login.url.clone();
                        send(
                            client,
                            base,
                            token,
                            runner,
                            &job.id,
                            Event::OAuthReady { url },
                        )
                        .await?;
                        if let Some(previous) = oauth_task.take() {
                            previous.abort();
                        }
                        let tx = oauth_tx.clone();
                        oauth_task = Some(tokio::spawn(async move {
                            eprintln!("已在网页发起 ChatGPT OAuth 授权，等待本机 127.0.0.1 回调…");
                            match login.complete().await {
                                Ok(account) => match load_chatgpt_session(account).await {
                                    Ok(session) => {
                                        let _ = tx.send(session).await;
                                    }
                                    Err(error) => eprintln!("读取 ChatGPT 模型目录失败：{error}"),
                                },
                                Err(error) => eprintln!("ChatGPT OAuth 登录未完成：{error}"),
                            }
                        }));
                    }
                    Err(error) => {
                        eprintln!("无法初始化本机 ChatGPT OAuth 回环监听：{error}");
                        let _ = send(client, base, token, runner, &job.id, Event::Error).await;
                    }
                }
                continue;
            }
            if !backend
                .models()
                .iter()
                .any(|entry| entry.model == job.model)
            {
                send(client, base, token, runner, &job.id, Event::Error).await?;
                eprintln!("请求的模型不在本机模型列表中，请刷新网页设置。");
                continue;
            }
            eprintln!("正在处理模型请求…");
            if execute(client, base, token, runner, job, &backend)
                .await
                .is_err()
            {
                eprintln!("本次请求已中止或连接失败；不会自动重试推理。请在网页查看状态后重试。");
            } else {
                eprintln!("请求结束，等待下一次请求。");
            }
        }
    }
}

async fn send(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    runner: &str,
    id: &str,
    event: Event,
) -> AppResult<bool> {
    let response = client
        .post(format!("{base}/event"))
        .bearer_auth(token)
        .json(&json!({"runner":runner,"jobId":id,"event":event}))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::Model("桥接结果通道已断开".into()));
    }
    Ok(response.json::<Value>().await?["active"] == true)
}

async fn execute(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    runner: &str,
    job: Job,
    backend: &Backend,
) -> AppResult<()> {
    let statuses = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let captured_statuses = statuses.clone();
    let provider = CodexProvider::new(job.model.clone()).with_status(Arc::new(move |message| {
        eprintln!("{message}");
        let mut statuses = captured_statuses.lock().unwrap();
        if statuses.len() >= 8 {
            statuses.pop_front();
        }
        statuses.push_back(message.to_owned());
    }));
    let buffer = Arc::new(Mutex::new(String::new()));
    let captured = buffer.clone();
    let overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let too_large = overflow.clone();
    let callback = Box::new(move |delta: &str| {
        let mut output = captured.lock().unwrap();
        if output.len() + delta.len() <= 2 * 1024 * 1024 {
            output.push_str(delta);
        } else {
            too_large.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    let future = async {
        match backend {
            Backend::Codex { .. } => {
                provider
                    .generate(job.messages, job.search, Some(callback))
                    .await
            }
            Backend::ChatGpt(Some(current)) => {
                crate::ai::chatgpt::ChatGptProvider::new(
                    job.model,
                    current.account.clone(),
                    current.session.clone(),
                )
                .generate(job.messages, job.search, Some(callback))
                .await
            }
            Backend::ChatGpt(None) => Err(AppError::Codex(
                crate::models::inference::CodexFailure::Unauthorized,
            )),
        }
    };
    tokio::pin!(future);
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            result = &mut future => {
                let progress = std::mem::take(&mut *statuses.lock().unwrap());
                for message in progress {
                    if !send(client, base, token, runner, &job.id, Event::Status { message }).await? { return Ok(()); }
                }
                let pending = std::mem::take(&mut *buffer.lock().unwrap());
                if !pending.is_empty() && !send(client, base, token, runner, &job.id, Event::Delta { text: pending }).await? { return Ok(()); }
                let result = result.and_then(|result| { backend.check()?; Ok(result) });
                let event = match result {
                    Ok(result) if !overflow.load(std::sync::atomic::Ordering::Relaxed) => Event::Complete { result },
                    Ok(_) => Event::Error,
                    Err(AppError::Codex(reason)) => { eprintln!("{reason}"); Event::Failure { reason } },
                    Err(error) => { eprintln!("{error}"); Event::Error },
                };
                send(client, base, token, runner, &job.id, event).await?;
                return Ok(());
            }
            _ = interval.tick() => {
                if let Err(error) = backend.check() {
                    send(client, base, token, runner, &job.id, Event::Failure { reason: crate::models::inference::CodexFailure::Unauthorized }).await?;
                    return Err(error);
                }
                let progress = std::mem::take(&mut *statuses.lock().unwrap());
                for message in progress {
                    if !send(client, base, token, runner, &job.id, Event::Status { message }).await? { return Ok(()); }
                }
                if overflow.load(std::sync::atomic::Ordering::Relaxed) {
                    send(client, base, token, runner, &job.id, Event::Error).await?;
                    return Err(AppError::Model("Codex 输出超出桥接缓冲区".into()));
                }
                let text = std::mem::take(&mut *buffer.lock().unwrap());
                let event = if text.is_empty() { Event::Heartbeat } else { Event::Delta { text } };
                if !send(client, base, token, runner, &job.id, event).await? { return Ok(()); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_site_prefix_and_rejects_insecure_remote_or_embedded_credentials() {
        assert_eq!(
            api_url("https://www.madeagents.ai/mario/").unwrap(),
            "https://www.madeagents.ai/mario/api/codex-bridge/worker"
        );
        assert!(api_url("http://127.0.0.1:4217/").is_ok());
        for site in [
            "http://example.org",
            "https://user:secret@example.org",
            "https://example.org/?token=x",
            "file:///tmp/test",
        ] {
            assert!(api_url(site).is_err());
        }
    }
}
