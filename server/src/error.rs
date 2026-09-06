use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("本地数据库错误：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("无法访问本地文件：{0}")]
    Io(#[from] std::io::Error),
    #[error("外部网络请求失败：{0}")]
    Network(#[from] reqwest::Error),
    #[error("模型返回了无法识别的数据：{0}")]
    Json(#[from] serde_json::Error),
    #[error("系统钥匙串错误：{0}")]
    Keyring(String),
    #[error("账户认证失败：{0}")]
    Auth(String),
    #[error("本地客户端认证失败：{0}")]
    LocalAuth(String),
    #[error("云同步冲突：{0}")]
    Conflict(String),
    #[error("云同步服务错误：{0}")]
    Cloud(String),
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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Auth(_) | AppError::LocalAuth(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Model(_) | AppError::Network(_) => StatusCode::BAD_GATEWAY,
            AppError::Cloud(_) | AppError::MarketData(_) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = self.to_string();
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
