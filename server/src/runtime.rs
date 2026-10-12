//! Process configuration, maintenance commands, and graceful shutdown.
use crate::{accounts, agent, db::Database, hosted, AppError, AppResult};
use std::{
    net::{IpAddr, SocketAddr},
    path::{Path as FilePath, PathBuf},
};
struct ServerOptions {
    port: u16,
    auth_token: Option<String>,
}
pub async fn run_from_env() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mario_server=info,tower_http=info".into()),
        )
        .init();

    if std::env::var("MARIO_ROLE").as_deref() == Ok("agent") {
        return agent::serve().await;
    }
    if let Some(path) = std::env::args()
        .skip_while(|arg| arg != "--export-data")
        .nth(1)
    {
        let db = Database::open(&data_directory()?.join("mario.db"))?;
        let data = serde_json::to_vec_pretty(&db.export_sync_data()?)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        options.open(path)?.write_all(&data)?;
        return Ok(());
    }
    if let Some(email) = std::env::args()
        .skip_while(|arg| arg != "--reset-password")
        .nth(1)
    {
        let url = std::env::var("DATABASE_URL")
            .map_err(|_| AppError::Validation("缺少 DATABASE_URL".into()))?;
        let password = std::env::var("MARIO_RESET_PASSWORD")
            .map_err(|_| AppError::Validation("缺少 MARIO_RESET_PASSWORD".into()))?;
        accounts::Accounts::open(url)?.reset_password(&email, &password)?;
        return Ok(());
    }
    let options = server_options()?;
    let web_dir = std::env::var_os("MARIO_WEB_DIR").map(PathBuf::from);
    let host: IpAddr = std::env::var("MARIO_HOST")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse()
        .map_err(|_| AppError::Validation("MARIO_HOST 必须是有效 IP 地址".into()))?;
    let database_url = std::env::var("DATABASE_URL").ok();
    if database_url.is_none()
        && (web_dir.is_some() || !host.is_loopback())
        && options
            .auth_token
            .as_ref()
            .is_none_or(|token| !(32..=256).contains(&token.len()))
    {
        return Err(AppError::Validation(
            "Web 服务必须设置 32–256 字符的 MARIO_AUTH_TOKEN".into(),
        ));
    }
    if let Some(dir) = &web_dir {
        if !dir.join("index.html").is_file() {
            return Err(AppError::Validation(
                "MARIO_WEB_DIR 中没有 index.html，请先构建 Web 页面".into(),
            ));
        }
    }
    if let Some(url) = database_url {
        return hosted::serve(
            url,
            SocketAddr::new(host, options.port),
            web_dir,
            shutdown_signal(),
        )
        .await;
    }
    let data_dir = data_directory()?;
    crate::http::local::serve(
        data_dir,
        SocketAddr::new(host, options.port),
        options.auth_token,
        web_dir,
        shutdown_signal(),
    )
    .await
}

fn data_directory() -> AppResult<PathBuf> {
    if let Ok(value) = std::env::var("MARIO_DATA_DIR") {
        let directory = PathBuf::from(value);
        migrate_legacy_database_filename(&directory)?;
        return Ok(directory);
    }
    if let Ok(value) = std::env::var("COMPASS_DATA_DIR") {
        tracing::warn!("COMPASS_DATA_DIR 已弃用，请改用 MARIO_DATA_DIR");
        let directory = PathBuf::from(value);
        migrate_legacy_database_filename(&directory)?;
        return Ok(directory);
    }
    let root = dirs::data_local_dir()
        .ok_or_else(|| AppError::Validation("无法确定本地数据目录".into()))?;
    migrate_legacy_data_directory(&root)
}

fn migrate_legacy_data_directory(root: &FilePath) -> AppResult<PathBuf> {
    let current = root.join("com.lokinko.mario");
    let legacy = root.join("com.compassinvest.desktop");
    let selected = if !current.exists() && legacy.exists() {
        match std::fs::rename(&legacy, &current) {
            Ok(()) => current.clone(),
            Err(error) => {
                tracing::warn!(%error, "无法移动旧版数据目录，将继续从原位置读取");
                legacy.clone()
            }
        }
    } else {
        current.clone()
    };
    if selected == current && legacy.exists() {
        migrate_legacy_database_between_directories(&legacy, &current)?;
    }
    migrate_legacy_database_filename(&selected)?;
    Ok(selected)
}

fn migrate_legacy_database_between_directories(
    legacy_directory: &FilePath,
    current_directory: &FilePath,
) -> AppResult<()> {
    let legacy = legacy_directory.join("compass.db");
    let current = current_directory.join("mario.db");
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(current_directory)?;
    std::fs::rename(&legacy, &current)?;
    for suffix in ["-wal", "-shm"] {
        let legacy_sidecar = legacy_directory.join(format!("compass.db{suffix}"));
        if legacy_sidecar.exists() {
            std::fs::rename(
                legacy_sidecar,
                current_directory.join(format!("mario.db{suffix}")),
            )?;
        }
    }
    tracing::info!(data_dir = %current_directory.display(), "已迁移旧版 mario 本地数据库");
    Ok(())
}

fn migrate_legacy_database_filename(directory: &FilePath) -> AppResult<()> {
    let current = directory.join("mario.db");
    let legacy = directory.join("compass.db");
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(directory)?;
    std::fs::rename(&legacy, &current)?;
    for suffix in ["-wal", "-shm"] {
        let legacy_sidecar = directory.join(format!("compass.db{suffix}"));
        if legacy_sidecar.exists() {
            std::fs::rename(legacy_sidecar, directory.join(format!("mario.db{suffix}")))?;
        }
    }
    tracing::info!(data_dir = %directory.display(), "已迁移旧版 mario 本地数据库");
    Ok(())
}

fn server_options() -> AppResult<ServerOptions> {
    let mut arguments = std::env::args();
    let mut port = 4217_u16;
    let mut allow_unauthenticated_dev = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--port" => {
                port = arguments
                    .next()
                    .ok_or_else(|| AppError::Validation("--port 缺少参数".into()))?
                    .parse::<u16>()
                    .map_err(|_| AppError::Validation("--port 参数无效".into()))?;
                if port == 0 {
                    return Err(AppError::Validation("--port 不能为 0".into()));
                }
            }
            "--allow-unauthenticated-dev" => allow_unauthenticated_dev = true,
            _ => {}
        }
    }
    let auth_token = std::env::var("MARIO_AUTH_TOKEN")
        .or_else(|_| std::env::var("COMPASS_AUTH_TOKEN"))
        .ok()
        .filter(|value| !value.trim().is_empty());
    if std::env::var("DATABASE_URL").is_err()
        && !allow_unauthenticated_dev
        && auth_token
            .as_deref()
            .is_none_or(|value| value.len() < 32 || value.len() > 256)
    {
        return Err(AppError::Validation(
            "本地服务必须配置 32–256 字符的 MARIO_AUTH_TOKEN；开发模式请显式使用 --allow-unauthenticated-dev"
                .into(),
        ));
    }
    Ok(ServerOptions { port, auth_token })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_brand_data_directory_and_database_name() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("com.compassinvest.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("compass.db"), b"legacy database").unwrap();
        std::fs::write(legacy.join("compass.db-wal"), b"legacy wal").unwrap();

        let selected = migrate_legacy_data_directory(root.path()).unwrap();
        assert_eq!(selected, root.path().join("com.lokinko.mario"));
        assert_eq!(
            std::fs::read(selected.join("mario.db")).unwrap(),
            b"legacy database"
        );
        assert_eq!(
            std::fs::read(selected.join("mario.db-wal")).unwrap(),
            b"legacy wal"
        );
        assert!(!root.path().join("com.compassinvest.desktop").exists());
    }

    #[test]
    fn migrates_legacy_database_when_new_brand_directory_already_exists() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("com.compassinvest.desktop");
        let current = root.path().join("com.lokinko.mario");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(legacy.join("compass.db"), b"legacy database").unwrap();

        let selected = migrate_legacy_data_directory(root.path()).unwrap();
        assert_eq!(selected, current);
        assert_eq!(
            std::fs::read(selected.join("mario.db")).unwrap(),
            b"legacy database"
        );
        assert!(!legacy.join("compass.db").exists());
    }
}
