use super::{bearer, HostedState};
use crate::{
    accounts::activity::{Page, UsageDashboard},
    AppError, AppResult,
};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActivityInput {
    page: Page,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DashboardQuery {
    #[serde(default = "default_days")]
    days: i64,
}
fn default_days() -> i64 {
    30
}

pub(super) async fn record(
    State(host): State<Arc<HostedState>>,
    headers: HeaderMap,
    Json(input): Json<ActivityInput>,
) -> AppResult<Json<serde_json::Value>> {
    let _permit = host
        .slots
        .try_acquire()
        .map_err(|_| AppError::Busy("服务繁忙，请稍后重试".into()))?;
    let user = host.accounts.authenticate(bearer(&headers)?)?;
    let accounts = host.accounts.clone();
    tokio::task::spawn_blocking(move || accounts.record_activity(&user, input.page))
        .await
        .map_err(|_| AppError::Busy("活跃度记录暂时不可用".into()))??;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Dashboard {
    #[serde(flatten)]
    usage: UsageDashboard,
    market_prices: crate::models::market::monitor::PriceCollectionDashboard,
}
pub(super) async fn dashboard(
    State(host): State<Arc<HostedState>>,
    headers: HeaderMap,
    Query(query): Query<DashboardQuery>,
) -> AppResult<Json<Dashboard>> {
    let _permit = host
        .slots
        .try_acquire()
        .map_err(|_| AppError::Busy("服务繁忙，请稍后重试".into()))?;
    let user = host.accounts.authenticate(bearer(&headers)?)?;
    let accounts = host.accounts.clone();
    let prices = host.public_prices.clone();
    Ok(Json(
        tokio::task::spawn_blocking(move || {
            // Account authority checks the current database role before public operational data is read.
            let usage = accounts.usage_dashboard(&user, query.days)?;
            let market_prices = prices.collection_dashboard(query.days)?;
            Ok::<_, AppError>(Dashboard {
                usage,
                market_prices,
            })
        })
        .await
        .map_err(|_| AppError::Busy("看板统计暂时不可用".into()))??,
    ))
}
