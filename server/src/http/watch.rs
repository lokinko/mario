use super::context::WatchContext;
use crate::ports::user_records::WatchManagement;
use crate::ports::{account::AccountSession, user_account::WatchAccount};
use crate::{
    models::watch::{
        grid::{GridPlan, GridProjection},
        rules::WatchRules,
        WatchBoard, WatchEntry, WatchInput,
    },
    AppResult,
};
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};

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
        .route("/api/watch/grid-preview", post(preview_grid))
}
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CalendarQuery {
    #[serde(default)]
    scope: crate::models::watch::calendar::CalendarScope,
    from: Option<chrono::NaiveDate>,
}
fn calendar_inputs(state: &WatchContext) -> AppResult<Vec<WatchInput>> {
    Ok(state
        .account()
        .watch_entries()?
        .into_iter()
        .map(|entry| entry.input)
        .collect())
}
async fn dividend_calendar(
    state: WatchContext,
    Query(query): Query<CalendarQuery>,
) -> AppResult<Json<crate::models::watch::calendar::CalendarBoard>> {
    Ok(Json(
        state.calendar().board(
            &calendar_inputs(&state)?,
            query.scope,
            query
                .from
                .unwrap_or_else(crate::application::dividend_calendar::today),
        )?,
    ))
}
async fn catalog(
    state: WatchContext,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    let mut board = state.catalog().board()?;
    board.can_edit = state.is_superuser();
    board.apply_rules(state.account().watch_rules()?);
    Ok(Json(board))
}
async fn detail(
    state: WatchContext,
    Path(symbol): Path<String>,
) -> AppResult<Json<crate::models::watch::WatchRow>> {
    Ok(Json(state.account().watch_row(&symbol)?))
}
async fn catalog_detail(
    state: WatchContext,
    Path(symbol): Path<String>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogRow>> {
    let mut row = state.catalog().detail(&symbol)?;
    row.apply_rules(&state.account().watch_rules()?);
    Ok(Json(row))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CompanyQuery {
    query: String,
}
async fn resolve_company(
    state: WatchContext,
    Json(input): Json<CompanyQuery>,
) -> AppResult<Json<crate::models::watch::catalog::Company>> {
    if !state.is_superuser() {
        return Err(crate::AppError::Forbidden(
            "仅超级账户可维护公共公司目录".into(),
        ));
    }
    let company = state
        .account()
        .without_account_lock(state.catalog().resolve(&input.query))
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
    state: WatchContext,
    Json(input): Json<CatalogSaveInput>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    if !state.is_superuser() {
        return Err(crate::AppError::Forbidden(
            "仅超级账户可维护公共公司目录".into(),
        ));
    }
    let mut board = state
        .catalog()
        .save(&input.catalog, input.revision, state.user_id())?;
    board.can_edit = true;
    board.apply_rules(state.account().watch_rules()?);
    Ok(Json(board))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogRefreshInput {
    industry_id: String,
}
async fn refresh_catalog(
    state: WatchContext,
    Json(input): Json<CatalogRefreshInput>,
) -> AppResult<Json<crate::models::watch::catalog::CatalogBoard>> {
    let board = state
        .account()
        .without_account_lock(state.catalog().refresh(Some(&input.industry_id), true))
        .await?;
    let mut board = board?;
    board.can_edit = state.is_superuser();
    board.apply_rules(state.account().watch_rules()?);
    Ok(Json(board))
}
async fn board(state: WatchContext) -> AppResult<Json<WatchBoard>> {
    current_board(&state)
}
fn current_board(state: &WatchContext) -> AppResult<Json<WatchBoard>> {
    let today = chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive();
    Ok(Json(state.account().watch_board(today)?))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatchRulesSave {
    rules: WatchRules,
    expected_rules: WatchRules,
}
async fn save_rules(
    state: WatchContext,
    Json(input): Json<WatchRulesSave>,
) -> AppResult<Json<WatchBoard>> {
    state
        .account()
        .save_watch_rules(input.rules, input.expected_rules)?;
    current_board(&state)
}
async fn save_grid(
    state: WatchContext,
    Path(symbol): Path<String>,
    Json(plan): Json<Option<GridPlan>>,
) -> AppResult<Json<WatchBoard>> {
    state.account().set_watch_grid(&symbol, plan)?;
    current_board(&state)
}
async fn preview_grid(Json(plan): Json<GridPlan>) -> AppResult<Json<GridProjection>> {
    plan.validate()?;
    Ok(Json(plan.project()))
}
async fn save(
    state: WatchContext,
    Json(input): Json<WatchSaveInput>,
) -> AppResult<Json<WatchEntry>> {
    Ok(Json(if input.if_absent {
        state.account().add_watch_if_missing(input.company)?
    } else {
        state.account().save_watch(input.company)?
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
    state: WatchContext,
    Path(symbol): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.account().delete_watch(&symbol)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
async fn read(
    state: WatchContext,
    Path(symbol): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.account().read_watch_alerts(&symbol)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchPinInput {
    pinned: bool,
}
async fn pin(
    state: WatchContext,
    Path(symbol): Path<String>,
    Json(input): Json<WatchPinInput>,
) -> AppResult<Json<WatchEntry>> {
    Ok(Json(
        state.account().set_watch_pinned(&symbol, input.pinned)?,
    ))
}
async fn refresh(state: WatchContext) -> AppResult<Json<WatchBoard>> {
    Ok(Json(
        crate::application::watch::refresh(state.account(), state.provider(), true).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{RequestContext, RequestContextParts};
    use axum::Extension;
    use std::sync::Arc;

    #[tokio::test]
    async fn public_refresh_returns_public_results_without_advancing_the_account_or_copying_history(
    ) {
        let runtime = crate::composition::MarketRuntime::new(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
        )
        .unwrap();
        let (calendar, plan_service) =
            crate::application::dividend_calendar::tests::fixture_services();
        let state = Arc::new(RequestContext::from(RequestContextParts {
            model_services: crate::composition::model_services(),
            user_id: "test-user".into(),
            is_superuser: false,
            db: crate::db::Database::test(),
            watch_provider: runtime.watch_provider.clone(),
            watch_catalog: crate::application::watch_catalog::tests::fixture_service(),
            dividend_calendar: calendar,
            dividend_plans: runtime.dividend_plans.clone(),
            announcements: runtime.announcements.clone(),
            market: runtime.services.clone(),
            analysis_executor: Arc::new(crate::agent::AgentExecutor),
        }));
        let revision = state.database().revision().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = router().layer(Extension(state.clone()));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        let _lock = state.database().lock_account().unwrap();
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
                .test_catalog()
                .board()
                .unwrap()
                .catalog
                .industry("banks")
                .unwrap()
                .companies
                .len()
        );
        assert_eq!(state.database().revision().unwrap(), revision);
        assert!(state.database().watch_entries().unwrap().is_empty());
        assert!(state
            .database()
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
        let public_before = serde_json::to_value(state.test_catalog().board().unwrap()).unwrap();
        let custom = WatchRules {
            min_yield_bps: 725,
            ..WatchRules::default()
        };
        state
            .database()
            .save_watch_rules(custom, WatchRules::default())
            .unwrap();
        let custom_revision = state.database().revision().unwrap();
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
            serde_json::to_value(state.test_catalog().board().unwrap()).unwrap(),
            public_before,
            "account projection never writes shared snapshots"
        );
        assert_eq!(state.database().revision().unwrap(), custom_revision);
        state
            .database()
            .save_watch(WatchInput {
                symbol: "600000.SS".into(),
                name: "本账号自选".into(),
                thesis: "私有理由".into(),
            })
            .unwrap();
        let calendar_revision = state.database().revision().unwrap();
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
        plan_service.refresh(&["600000.SS".into()]).await.unwrap();
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
        assert_eq!(state.database().revision().unwrap(), calendar_revision);
        assert!(
            state
                .database()
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
