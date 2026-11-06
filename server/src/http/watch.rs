use super::RequestContext;
use crate::{
    models::watch::{grid::GridPlan, WatchBoard, WatchEntry, WatchInput},
    AppResult,
};
use axum::{
    extract::Path,
    routing::{get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/watch", get(board).put(save))
        .route("/api/watch/refresh", post(refresh))
        .route("/api/watch/{symbol}", axum::routing::delete(remove))
        .route("/api/watch/{symbol}/read", post(read))
        .route("/api/watch/{symbol}/pin", put(pin))
        .route("/api/watch/{symbol}/grid", put(save_grid))
}
async fn board(Extension(state): Extension<Arc<RequestContext>>) -> AppResult<Json<WatchBoard>> {
    current_board(&state)
}
fn current_board(state: &RequestContext) -> AppResult<Json<WatchBoard>> {
    let today = chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive();
    Ok(Json(state.db.watch_board(today)?))
}
async fn save_grid(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
    Json(plan): Json<Option<GridPlan>>,
) -> AppResult<Json<WatchBoard>> {
    state.db.set_watch_grid(&symbol, plan)?;
    current_board(&state)
}
async fn save(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<WatchInput>,
) -> AppResult<Json<WatchEntry>> {
    Ok(Json(state.db.save_watch(input)?))
}
async fn remove(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.db.delete_watch(&symbol)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
async fn read(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.db.read_watch_alerts(&symbol)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchPinInput {
    pinned: bool,
}
async fn pin(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
    Json(input): Json<WatchPinInput>,
) -> AppResult<Json<WatchEntry>> {
    Ok(Json(state.db.set_watch_pinned(&symbol, input.pinned)?))
}
async fn refresh(Extension(state): Extension<Arc<RequestContext>>) -> AppResult<Json<WatchBoard>> {
    Ok(Json(
        crate::application::watch::refresh(&state.db, state.watch_provider.clone(), true).await?,
    ))
}
