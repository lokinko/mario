//! Process configuration, maintenance commands, and graceful shutdown.
use crate::{accounts, agent, hosted, AppError, AppResult};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};
pub async fn run_from_env() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mario_server=info,tower_http=info".into()),
        )
        .init();

    if let Some(site) = std::env::args()
        .skip_while(|arg| arg != "--codex-bridge")
        .nth(1)
    {
        return crate::codex_bridge_worker::run(&site).await;
    }
    if std::env::var("MARIO_ROLE").as_deref() == Ok("agent") {
        return agent::serve().await;
    }
    if let Some(email) = std::env::args()
        .skip_while(|arg| arg != "--reset-password")
        .nth(1)
    {
        let url = database_url()?;
        let password = std::env::var("MARIO_RESET_PASSWORD")
            .map_err(|_| AppError::Validation("缺少 MARIO_RESET_PASSWORD".into()))?;
        accounts::Accounts::open(url)?.reset_password(&email, &password)?;
        return Ok(());
    }
    let database_url = database_url()?;
    let port = server_port()?;
    let web_dir = std::env::var_os("MARIO_WEB_DIR").map(PathBuf::from);
    let host: IpAddr = std::env::var("MARIO_HOST")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse()
        .map_err(|_| AppError::Validation("MARIO_HOST 必须是有效 IP 地址".into()))?;
    if let Some(dir) = &web_dir {
        if !dir.join("index.html").is_file() {
            return Err(AppError::Validation(
                "MARIO_WEB_DIR 中没有 index.html，请先构建 Web 页面".into(),
            ));
        }
    }
    hosted::serve(
        database_url,
        SocketAddr::new(host, port),
        web_dir,
        shutdown_signal(),
    )
    .await
}

fn database_url() -> AppResult<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::Validation(
                "缺少 DATABASE_URL；本地开发请使用 npm run dev 启动临时 PostgreSQL".into(),
            )
        })
}

fn server_port() -> AppResult<u16> {
    let mut arguments = std::env::args();
    let mut port = 4217_u16;
    while let Some(argument) = arguments.next() {
        if argument == "--port" {
            port = arguments
                .next()
                .ok_or_else(|| AppError::Validation("--port 缺少参数".into()))?
                .parse::<u16>()
                .map_err(|_| AppError::Validation("--port 参数无效".into()))?;
            if port == 0 {
                return Err(AppError::Validation("--port 不能为 0".into()));
            }
        }
    }
    Ok(port)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
