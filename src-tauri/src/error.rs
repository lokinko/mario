use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("本地数据库错误：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("无法访问本地文件：{0}")]
    Io(#[from] std::io::Error),
    #[error("模型服务请求失败：{0}")]
    Network(#[from] reqwest::Error),
    #[error("模型返回了无法识别的数据：{0}")]
    Json(#[from] serde_json::Error),
    #[error("系统钥匙串错误：{0}")]
    Keyring(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Model(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
