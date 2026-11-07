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
        .route("/api/watch/catalog", get(catalog).put(save_catalog))
        .route("/api/watch/catalog/refresh", post(refresh_catalog))
        .route("/api/watch/catalog/resolve", post(resolve_company))
        .route("/api/watch/refresh", post(refresh))
        .route("/api/watch/{symbol}", axum::routing::delete(remove))
        .route("/api/watch/{symbol}/read", post(read))
        .route("/api/watch/{symbol}/pin", put(pin))
        .route("/api/watch/{symbol}/grid", put(save_grid))
}
async fn catalog(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    let mut board = state.watch_catalog.board()?;
    board.can_edit = state.is_superuser;
    Ok(Json(board))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CompanyQuery {
    query: String,
}
async fn resolve_company(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<CompanyQuery>,
) -> AppResult<Json<crate::models::watch::catalog::Company>> {
    if !state.is_superuser {
        return Err(crate::AppError::Forbidden(
            "仅超级账户可维护公共公司目录".into(),
        ));
    }
    let company = state
        .db
        .without_account_lock(state.watch_catalog.resolve(&input.query))
        .await??;
    Ok(Json(company))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogSaveInput {
    catalog: crate::models::watch::catalog::Catalog,
    revision: i64,
}
async fn save_catalog(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<CatalogSaveInput>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    if !state.is_superuser {
        return Err(crate::AppError::Forbidden(
            "仅超级账户可维护公共公司目录".into(),
        ));
    }
    let mut board = state
        .watch_catalog
        .save(&input.catalog, input.revision, &state.user_id)?;
    board.can_edit = true;
    Ok(Json(board))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogRefreshInput {
    industry_id: String,
}
async fn refresh_catalog(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<CatalogRefreshInput>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    let board = state
        .db
        .without_account_lock(state.watch_catalog.refresh(Some(&input.industry_id), true))
        .await?;
    let mut board = board?;
    board.can_edit = state.is_superuser;
    Ok(Json(board))
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
    Json(input): Json<WatchSaveInput>,
) -> AppResult<Json<WatchEntry>> {
    Ok(Json(if input.if_absent {
        state.db.add_watch_if_missing(input.company)?
    } else {
        state.db.save_watch(input.company)?
    }))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchSaveInput {
    #[serde(flatten)]
    company: WatchInput,
    #[serde(default)]
    if_absent: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn public_refresh_returns_public_results_without_advancing_the_account_or_copying_history(
    ) {
        let runtime = crate::composition::MarketRuntime::new(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
        )
        .unwrap();
        let state = Arc::new(RequestContext {
            user_id: "test-user".into(),
            is_superuser: false,
            db: crate::db::Database::test(),
            watch_provider: runtime.watch_provider.clone(),
            watch_catalog: crate::application::watch_catalog::tests::fixture_service(),
            market: runtime.services.clone(),
            analysis_executor: Arc::new(crate::agent::AgentExecutor),
        });
        let revision = state.db.revision().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = router().layer(Extension(state.clone()));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        let _lock = state.db.lock_account().unwrap();
        let response = client
            .post(format!("{base}/api/watch/catalog/refresh"))
            .json(&serde_json::json!({"industryId":"banks"}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let board: serde_json::Value = response.json().await.unwrap();
        assert!(board["rows"][0]["latest"]["price"].is_number());
        assert_eq!(
            board["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| !r["latest"].is_null())
                .count(),
            state
                .watch_catalog
                .board()
                .unwrap()
                .catalog
                .industry("banks")
                .unwrap()
                .companies
                .len()
        );
        assert_eq!(state.db.revision().unwrap(), revision);
        assert!(state.db.watch_entries().unwrap().is_empty());
        assert!(state
            .db
            .watch_board(super::super::super::application::watch::cutoff())
            .unwrap()
            .weekly
            .is_empty());
        let response = client
            .get(format!("{base}/api/watch/catalog"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.json::<serde_json::Value>().await.unwrap(), board);
        server.abort();
    }
}
