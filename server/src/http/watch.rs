use super::RequestContext;
use crate::{
    models::watch::{grid::GridPlan, rules::WatchRules, WatchBoard, WatchEntry, WatchInput},
    AppResult,
};
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/watch", get(board).put(save))
        .route("/api/watch/catalog", get(catalog).put(save_catalog))
        .route(
            "/api/watch/catalog/{symbol}/observation",
            get(catalog_detail),
        )
        .route("/api/watch/{symbol}/observation", get(detail))
        .route("/api/watch/catalog/refresh", post(refresh_catalog))
        .route("/api/watch/catalog/resolve", post(resolve_company))
        .route("/api/watch/dividend-calendar", get(dividend_calendar))
        .route("/api/watch/refresh", post(refresh))
        .route("/api/watch/rules", put(save_rules))
        .route("/api/watch/{symbol}", axum::routing::delete(remove))
        .route("/api/watch/{symbol}/read", post(read))
        .route("/api/watch/{symbol}/pin", put(pin))
        .route("/api/watch/{symbol}/grid", put(save_grid))
}
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CalendarQuery {
    #[serde(default)]
    scope: crate::models::watch::calendar::CalendarScope,
    from: Option<chrono::NaiveDate>,
}
fn calendar_inputs(state: &RequestContext) -> AppResult<Vec<WatchInput>> {
    Ok(state
        .db
        .watch_entries()?
        .into_iter()
        .map(|entry| entry.input)
        .collect())
}
async fn dividend_calendar(
    Extension(state): Extension<Arc<RequestContext>>,
    Query(query): Query<CalendarQuery>,
) -> AppResult<Json<crate::models::watch::calendar::CalendarBoard>> {
    Ok(Json(
        state.dividend_calendar.board(
            &calendar_inputs(&state)?,
            query.scope,
            query
                .from
                .unwrap_or_else(crate::application::dividend_calendar::today),
        )?,
    ))
}
async fn catalog(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    let mut board = state.watch_catalog.board()?;
    board.can_edit = state.is_superuser;
    board.apply_rules(state.db.watch_rules()?);
    Ok(Json(board))
}
async fn detail(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
) -> AppResult<Json<crate::models::watch::WatchRow>> {
    Ok(Json(state.db.watch_row(&symbol)?))
}
async fn catalog_detail(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(symbol): Path<String>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogRow>> {
    let mut row = state.watch_catalog.detail(&symbol)?;
    row.apply_rules(&state.db.watch_rules()?);
    Ok(Json(row))
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
    board.apply_rules(state.db.watch_rules()?);
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
    board.apply_rules(state.db.watch_rules()?);
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatchRulesSave {
    rules: WatchRules,
    expected_rules: WatchRules,
}
async fn save_rules(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<WatchRulesSave>,
) -> AppResult<Json<WatchBoard>> {
    state
        .db
        .save_watch_rules(input.rules, input.expected_rules)?;
    current_board(&state)
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
            model_services: crate::composition::model_services(),
            user_id: "test-user".into(),
            is_superuser: false,
            db: crate::db::Database::test(),
            watch_provider: runtime.watch_provider.clone(),
            watch_catalog: crate::application::watch_catalog::tests::fixture_service(),
            dividend_calendar: crate::application::dividend_calendar::tests::fixture_service(),
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
        let public_before = serde_json::to_value(state.watch_catalog.board().unwrap()).unwrap();
        let custom = WatchRules {
            min_yield_bps: 725,
            ..WatchRules::default()
        };
        state
            .db
            .save_watch_rules(custom, WatchRules::default())
            .unwrap();
        let custom_revision = state.db.revision().unwrap();
        let projected: serde_json::Value = client
            .get(format!("{base}/api/watch/catalog"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(projected["rules"]["minYieldBps"], 725);
        assert_eq!(projected["rows"][0]["latest"]["rules"]["minYieldBps"], 725);
        assert_eq!(projected["rows"][0]["latest"]["yieldMatchesRule"], false);
        assert_eq!(
            serde_json::to_value(state.watch_catalog.board().unwrap()).unwrap(),
            public_before,
            "account projection never writes shared snapshots"
        );
        assert_eq!(state.db.revision().unwrap(), custom_revision);
        state
            .db
            .save_watch(WatchInput {
                symbol: "600000.SS".into(),
                name: "本账号自选".into(),
                thesis: "私有理由".into(),
            })
            .unwrap();
        let calendar_revision = state.db.revision().unwrap();
        let unread = client
            .get(format!(
                "{base}/api/watch/dividend-calendar?scope=favorites"
            ))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert!(unread["entries"].as_array().unwrap().is_empty());
        assert_eq!(unread["coverage"][0]["symbol"], "600000.SS");
        state
            .dividend_calendar
            .refresh(&["600000.SS".into()])
            .await
            .unwrap();
        let calendar: serde_json::Value = client
            .get(format!(
                "{base}/api/watch/dividend-calendar?scope=favorites"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(calendar["entries"].as_array().unwrap().len(), 1);
        assert_eq!(calendar["entries"][0]["plan"]["stage"], "scheduled");
        assert!(calendar["entries"][0]["plan"]["paymentDate"].is_null());
        assert_eq!(state.db.revision().unwrap(), calendar_revision);
        assert!(
            state
                .db
                .watch_board(crate::application::dividend_calendar::today())
                .unwrap()
                .rows[0]
                .latest
                .is_none(),
            "future plans must not create implemented observations or a TTM yield"
        );
        server.abort();
    }
}
