//! HTTP adapters. Business workflows live in application; authentication selects a context.
mod analysis;
pub(crate) mod codex_bridge;
mod error;
mod evidence;
mod memory;
mod methodology;
mod model_settings;
pub(crate) mod policy;
mod portfolio;
mod reviews;
mod watch;
use crate::{
    application::portfolio::{MarketServices, PortfolioService},
    db::Database,
};
use axum::{routing::get, Json, Router};
use std::sync::Arc;

pub(crate) struct RequestContext {
    pub model_services: Arc<crate::application::model_settings::ModelServices>,
    pub db: Database,
    pub user_id: String,
    pub is_superuser: bool,
    pub watch_provider: Arc<dyn crate::ports::market::WatchMarketProvider>,
    pub watch_catalog: Arc<crate::application::watch_catalog::WatchCatalogService>,
    pub dividend_calendar: Arc<crate::application::dividend_calendar::DividendCalendarService>,
    pub market: Arc<MarketServices>,
    pub analysis_executor: Arc<dyn crate::ports::analysis::AnalysisExecutor>,
}
impl RequestContext {
    pub fn portfolio(
        &self,
    ) -> PortfolioService<'_, impl crate::ports::user_account::PortfolioAccount> {
        self.market.for_account(&self.db)
    }
}
pub(crate) fn api_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/{*path}",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .merge(portfolio::router())
        .merge(analysis::router())
        .merge(evidence::router())
        .merge(memory::router())
        .merge(reviews::router())
        .merge(methodology::router())
        .merge(model_settings::router())
        .merge(watch::router())
        .merge(codex_bridge::router())
}
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok", "version":env!("CARGO_PKG_VERSION")}))
}
