//! Framework-specific error presentation; domain errors do not depend on Axum.
use crate::AppError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Busy(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::RateLimit(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Auth(_) | AppError::LocalAuth(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Model(_) | AppError::Network(_) | AppError::Codex(_) => {
                StatusCode::BAD_GATEWAY
            }
            AppError::MarketData(_) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            // PostgreSQL's Display only says "db error"; Debug retains its
            // SQLSTATE and underlying cause. Keep details in server logs only.
            tracing::error!(error=%self, cause=?self,"request failed");
            "服务暂时不可用，请联系管理员查看日志".into()
        } else {
            self.to_string()
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn error_mapping_retains_conflicts_and_hides_internal_diagnostics() {
        assert_eq!(
            AppError::Conflict("changed".into())
                .into_response()
                .status(),
            StatusCode::CONFLICT
        );
        let response = AppError::Database(rusqlite::Error::InvalidQuery).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let message: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(message["error"], "服务暂时不可用，请联系管理员查看日志");
    }
}
