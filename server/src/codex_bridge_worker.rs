//! Runs on the user's computer. No inbound listener and no reads of auth.json.
use crate::{
    ai::{
        bridge::{Event, Job},
        CodexProvider,
    },
    AppError, AppResult,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

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
enum Backend {
    Codex,
    ChatGpt { account: String, session: String },
}
impl Backend {
    fn check(&self) -> AppResult<()> {
        if let Self::ChatGpt { account, session } = self {
            match crate::chatgpt_auth::is_authorized(account, session) {
                Ok(true) | Err(AppError::Busy(_)) => {}
                _ => {
                    return Err(AppError::Validation(
                        "ChatGPT 授权已退出、失效或重新登录；已停止此桥接，请重新配对并启动".into(),
                    ))
                }
            }
        }
        Ok(())
    }
}
pub(crate) async fn run(site: &str) -> AppResult<()> {
    connect(site, Backend::Codex).await
}
pub(crate) async fn run_chatgpt(site: &str) -> AppResult<()> {
    api_url(site)?;
    // Explicit login errors are never disguised as a fresh-account sign-in.
    // The user authorizes locally; only the model catalog goes to Mario.
    if !crate::chatgpt_auth::has_active_account()? {
        crate::chatgpt_auth::command(&["login".into()]).await?;
    }
    let account = crate::chatgpt_auth::active_account()?;
    eprintln!("本机 ChatGPT 注册：{account}（此进程固定使用该注册；切换账号后请重启并重新配对）");
    let session = crate::chatgpt_auth::session(&account)?;
    connect(site, Backend::ChatGpt { account, session }).await
}
async fn connect(site: &str, backend: Backend) -> AppResult<()> {
    let base = api_url(site)?;
    eprintln!("正在检查本机模型授权与模型列表…");
    let models = match &backend {
        Backend::Codex => CodexProvider::models().await?,
        Backend::ChatGpt { account, session } => {
            crate::ai::chatgpt::ChatGptProvider::models(account, session).await?
        }
    };
    let model = &models
        .iter()
        .find(|entry| entry.is_default)
        .unwrap_or(&models[0])
        .model;
    eprintln!("本机模型已就绪：{model}");
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
        result = work(&client, &base, &token, &runner, model, &models, &backend) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}
async fn work(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    runner: &str,
    model: &str,
    models: &[crate::models::CodexModel],
    backend: &Backend,
) -> AppResult<()> {
    let mut connected = false;
    loop {
        backend.check()?;
        let response = client
            .post(format!("{base}/poll"))
            .bearer_auth(token)
            .json(&json!({"runner":runner,"model":model,"models":models}))
            .send()
            .await;
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
            eprintln!("桥接已在线，请回到 Mario 设置点击“启用本机模型”。");
            connected = true;
        }
        let job: Option<Job> = response.json().await?;
        if let Some(job) = job {
            if !models.iter().any(|entry| entry.model == job.model) {
                send(client, base, token, runner, &job.id, Event::Error).await?;
                eprintln!("请求的模型不在本机模型列表中，请刷新网页设置。");
                continue;
            }
            eprintln!("正在处理模型请求…");
            if execute(client, base, token, runner, job, backend)
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
            Backend::Codex => {
                provider
                    .generate(job.messages, job.search, Some(callback))
                    .await
            }
            Backend::ChatGpt { account, session } => {
                crate::ai::chatgpt::ChatGptProvider::new(
                    job.model,
                    account.clone(),
                    session.clone(),
                )
                .generate(job.messages, job.search, Some(callback))
                .await
            }
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
