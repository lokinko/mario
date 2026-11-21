//! HTTP adapters. Business workflows live in application; authentication selects a context.
mod analysis;
pub(crate) mod codex_bridge;
pub(crate) mod context;
mod daily_review;
mod error;
mod evidence;
mod memory;
mod methodology;
mod model_settings;
pub(crate) mod policy;
mod portfolio;
mod watch;
use axum::{routing::get, Json, Router};
pub(crate) use context::{RequestContext, RequestContextParts};
pub(crate) fn api_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/{*path}",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .merge(portfolio::router())
        .merge(analysis::router())
        .merge(daily_review::router())
        .merge(evidence::router())
        .merge(memory::router())
        .merge(methodology::router())
        .merge(model_settings::router())
        .merge(watch::router())
        .merge(codex_bridge::router())
}
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok", "version":env!("CARGO_PKG_VERSION")}))
}
