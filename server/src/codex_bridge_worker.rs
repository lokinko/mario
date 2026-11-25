//! Runs on the user's computer. Only listens on 127.0.0.1 briefly for official OAuth callbacks.
use crate::{
    ai::bridge::{Event, Job},
    models::connection::{BridgeMode, CodexModel},
    AppError, AppResult,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

fn worker_url(address: &str, explicit_api: bool) -> AppResult<String> {
    let url =
        reqwest::Url::parse(address).map_err(|_| AppError::Validation("Mario 地址无效".into()))?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "桥接要求 HTTPS 地址（本机测试可用 HTTP），且不能含凭据、查询或片段".into(),
        ));
    }
    Ok(format!(
        "{}{}/codex-bridge/worker",
        url.as_str().trim_end_matches('/'),
        if explicit_api { "" } else { "/api" }
    ))
}

fn poll_error(status: reqwest::StatusCode, auth_required: bool) -> AppError {
    use reqwest::StatusCode;
    let detail = match status {
        status if status.is_redirection() => {
            "目标服务发生重定向。请使用生成配对码的 Mario 设置页所提供的启动命令。"
        }
        StatusCode::UNAUTHORIZED => {
            "配对码无效、已过期或被撤销。请在同一 Mario 站点重新生成配对码。"
        }
        StatusCode::BAD_REQUEST if auth_required => {
            "服务未接受待登录状态。请更新 Mario 站点，或先在本机执行 npm run chatgpt:auth -- login，再重新配对。"
        }
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => {
            "目标服务没有提供桥接接口。请核对 Mario 服务地址及部署版本。"
        }
        _ => "请核对 Mario 服务地址及部署版本，再重新配对。",
    };
    AppError::Model(format!(
        "桥接连接失败（HTTP {}）。{detail}",
        status.as_u16()
    ))
}

fn worker_client() -> AppResult<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

#[derive(Clone)]
struct ChatGptSession {
    account: String,
    session: String,
    account_label: Option<String>,
    model: String,
    models: Vec<CodexModel>,
}

struct SessionState(Option<ChatGptSession>);

impl SessionState {
    fn auth_required(&self) -> bool {
        self.0.is_none()
    }
    fn model(&self) -> &str {
        self.0.as_ref().map_or("", |current| &current.model)
    }
    fn models(&self) -> &[CodexModel] {
        self.0.as_ref().map_or(&[], |current| &current.models)
    }
    fn account_label(&self) -> Option<&str> {
        self.0
            .as_ref()
            .and_then(|current| current.account_label.as_deref())
    }
    fn check(&self) -> AppResult<()> {
        if let Some(current) = &self.0 {
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

pub(crate) async fn run_chatgpt(address: &str, explicit_api: bool) -> AppResult<()> {
    let base = worker_url(address, explicit_api)?;
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
    eprintln!("Mario 桥接接口：{base}");
    connect(&base, SessionState(initial)).await
}

async fn connect(base: &str, session: SessionState) -> AppResult<()> {
    eprintln!("请粘贴 Mario 设置页面生成的配对码，然后回车（不需要 ChatGPT 密码或 API Key）：");
    let mut token = String::new();
    std::io::stdin().read_line(&mut token)?;
    let token = token.trim().to_owned();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Validation("配对码格式无效".into()));
    }
    let client = worker_client()?;
    let runner = uuid::Uuid::new_v4().to_string();
    eprintln!("正在连接；保持此进程运行，关闭窗口或按 Ctrl+C 即停止。配对最长有效 12 小时。");
    tokio::select! {
        result = work(&client, base, &token, &runner, session) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

async fn work(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    runner: &str,
    mut session: SessionState,
) -> AppResult<()> {
    let mut connected = false;
    let (oauth_tx, mut oauth_rx) = mpsc::channel::<ChatGptSession>(2);
    let mut oauth_task: Option<JoinHandle<()>> = None;
    loop {
        if !session.auth_required() && session.check().is_err() {
            eprintln!("本机 ChatGPT 授权已退出或失效，已切换为待登录状态；可在 Mario 网页设置点击“跳转 ChatGPT OAuth 登录”。");
            session = SessionState(None);
        }
        let poll_future = client
            .post(format!("{base}/poll"))
            .bearer_auth(token)
            .json(&json!({
                "runner": runner,
                "model": session.model(),
                "models": session.models(),
                "mode": BridgeMode::Chatgpt,
                "accountLabel": session.account_label(),
                "authRequired": session.auth_required(),
            }))
            .send();
        let response = tokio::select! {
            Some(next) = oauth_rx.recv() => {
                eprintln!("网页 ChatGPT OAuth 登录已完成，模型已就绪：{}", next.model);
                session = SessionState(Some(next));
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
            return Err(poll_error(response.status(), session.auth_required()));
        }
        let job: Option<Job> = response.json().await.map_err(|_| {
            AppError::Model(
                "目标服务未返回有效的桥接响应。请核对 Mario 服务地址及部署版本。".into(),
            )
        })?;
        if !connected {
            if session.auth_required() {
                eprintln!("桥接已在线，请回到 Mario 设置点击“跳转 ChatGPT OAuth 登录”。");
            } else {
                eprintln!("桥接已在线，请回到 Mario 设置点击“启用本机模型”。");
            }
            connected = true;
        }
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
            if !session
                .models()
                .iter()
                .any(|entry| entry.model == job.model)
            {
                send(client, base, token, runner, &job.id, Event::Error).await?;
                eprintln!("请求的模型不在本机模型列表中，请刷新网页设置。");
                continue;
            }
            eprintln!("正在处理模型请求…");
            if execute(client, base, token, runner, job, &session)
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
    session: &SessionState,
) -> AppResult<()> {
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
        match &session.0 {
            Some(current) => {
                crate::ai::chatgpt::ChatGptProvider::new(
                    job.model,
                    current.account.clone(),
                    current.session.clone(),
                )
                .generate(job.messages, job.search, Some(callback))
                .await
            }
            None => Err(AppError::Codex(
                crate::models::inference::CodexFailure::Unauthorized,
            )),
        }
    };
    tokio::pin!(future);
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            result = &mut future => {
                let pending = std::mem::take(&mut *buffer.lock().unwrap());
                if !pending.is_empty() && !send(client, base, token, runner, &job.id, Event::Delta { text: pending }).await? { return Ok(()); }
                let result = result.and_then(|result| { session.check()?; Ok(result) });
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
                if let Err(error) = session.check() {
                    send(client, base, token, runner, &job.id, Event::Failure { reason: crate::models::inference::CodexFailure::Unauthorized }).await?;
                    return Err(error);
                }
                if overflow.load(std::sync::atomic::Ordering::Relaxed) {
                    send(client, base, token, runner, &job.id, Event::Error).await?;
                    return Err(AppError::Model("ChatGPT 输出超出桥接缓冲区".into()));
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
    use axum::{http::StatusCode, response::Redirect, routing::post, Router};

    #[test]
    fn preserves_site_prefix_and_rejects_insecure_remote_or_embedded_credentials() {
        assert_eq!(
            worker_url("https://mario.example.org/mario/", false).unwrap(),
            "https://mario.example.org/mario/api/codex-bridge/worker"
        );
        assert_eq!(
            worker_url("https://mario.example.org/custom/gateway/", true).unwrap(),
            "https://mario.example.org/custom/gateway/codex-bridge/worker"
        );
        assert!(worker_url("http://127.0.0.1:4217/", false).is_ok());
        for site in [
            "http://example.org",
            "https://user:secret@example.org",
            "https://example.org/?token=x",
            "file:///tmp/test",
        ] {
            assert!(worker_url(site, false).is_err());
            assert!(worker_url(site, true).is_err());
        }
    }

    #[tokio::test]
    async fn worker_pairs_through_the_configured_api_and_rejects_redirects() {
        let (id, token) = crate::ai::bridge::create().unwrap();
        let router = Router::new()
            .nest("/gateway", crate::http::codex_bridge::worker_router())
            .route(
                "/redirect/codex-bridge/worker/poll",
                post(|| async { Redirect::temporary("/gateway/api/codex-bridge/worker/poll") }),
            )
            .route(
                "/legacy-api/codex-bridge/worker/poll",
                post(|| async { StatusCode::BAD_REQUEST }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = worker_client().unwrap();
        let runner = uuid::Uuid::new_v4().to_string();
        let redirect = worker_url(&format!("http://{address}/redirect"), true).unwrap();
        let error = work(&client, &redirect, &token, &runner, SessionState(None))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("HTTP 307"));
        assert!(error.contains("重定向"));
        assert!(!error.contains(&token));
        assert!(!crate::ai::bridge::status(&id).online);

        let legacy = worker_url(&format!("http://{address}/legacy-api"), true).unwrap();
        let error = work(&client, &legacy, &token, &runner, SessionState(None))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("npm run chatgpt:auth -- login"));

        let base = worker_url(&format!("http://{address}/gateway/api"), true).unwrap();
        tokio::select! {
            result = work(&client, &base, &token, &runner, SessionState(None)) => {
                panic!("worker exited before pairing: {result:?}");
            }
            online = tokio::time::timeout(Duration::from_secs(5), async {
                while !crate::ai::bridge::status(&id).online {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }) => {
                online.expect("worker did not pair");
                assert!(crate::ai::bridge::status(&id).auth_required);
            }
        }
        crate::ai::bridge::revoke(&id);
        server.abort();
    }
}
