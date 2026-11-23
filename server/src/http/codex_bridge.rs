use super::context::BridgeContext;
use crate::{ai::bridge, application::codex_bridge, AppError, AppResult};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderMap},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/codex-bridge", get(status).post(pair).delete(revoke))
        .route("/api/codex-bridge/enable", post(enable))
}
// Separate from account middleware: long polling must never hold a database lock.
// The capability token grants only this bridge's job/result channel, no account APIs.
pub(crate) fn worker_router() -> Router {
    Router::new()
        .route("/api/codex-bridge/worker/poll", post(poll))
        .route("/api/codex-bridge/worker/event", post(event))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
}
async fn status(
    state: BridgeContext,
) -> AppResult<Json<crate::models::connection::CodexBridgeStatus>> {
    Ok(Json(codex_bridge::status(
        state.account(),
        state.services().bridges.as_ref(),
    )?))
}
async fn pair(state: BridgeContext) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"pairingCode":codex_bridge::pair(state.account(), state.services().bridges.as_ref())?, "expiresInSeconds":600}),
    ))
}
async fn revoke(state: BridgeContext) -> AppResult<Json<Value>> {
    codex_bridge::revoke(state.account(), state.services().bridges.as_ref())?;
    Ok(Json(json!({"ok":true})))
}
async fn enable(state: BridgeContext) -> AppResult<Json<crate::models::connection::ModelConfig>> {
    codex_bridge::enable(state.account(), state.services().bridges.as_ref())?;
    Ok(Json(crate::application::model_settings::configuration(
        state.account(),
        state.services().server.as_ref(),
    )?))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Poll {
    runner: String,
    model: String,
    #[serde(default)]
    models: Vec<crate::models::connection::CodexModel>,
    #[serde(default)]
    mode: Option<crate::models::connection::BridgeMode>,
    #[serde(default)]
    account_label: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    runner: String,
    job_id: String,
    event: bridge::Event,
}
fn authenticate(headers: &HeaderMap) -> AppResult<Arc<bridge::Connection>> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("缺少桥接配对码".into()))?;
    bridge::authenticate(token)
}
async fn poll(headers: HeaderMap, Json(input): Json<Poll>) -> AppResult<Json<Option<bridge::Job>>> {
    let connection = authenticate(&headers)?;
    connection.advertise(
        &input.runner,
        &input.model,
        &input.models,
        input.mode,
        input.account_label.as_deref(),
    )?;
    Ok(Json(connection.poll(&input.runner, &input.model).await?))
}
async fn event(headers: HeaderMap, Json(input): Json<Output>) -> AppResult<Json<Value>> {
    let active = authenticate(&headers)?.event(&input.runner, &input.job_id, input.event)?;
    Ok(Json(json!({"active":active})))
}
