use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Busy(String),
    #[error("{0}")]
    RateLimit(String),
    #[error("数据库错误：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("无法访问本地文件：{0}")]
    Io(#[from] std::io::Error),
    #[error("外部网络请求失败：{0}")]
    Network(#[from] reqwest::Error),
    #[error("模型返回了无法识别的数据：{0}")]
    Json(#[from] serde_json::Error),
    #[error("账户认证失败：{0}")]
    Auth(String),
    #[error("本地客户端认证失败：{0}")]
    LocalAuth(String),
    #[error("数据版本冲突：{0}")]
    Conflict(String),
    #[error("市场数据服务错误：{0}")]
    MarketData(String),
    #[error("{0}")]
    NotFound(String),
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
