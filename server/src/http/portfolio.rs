use super::RequestContext;
use crate::models::{
    FinancialProfile, FxRateQuery, FxRateQuote, GoalInput, HoldingInput,
    PortfolioEventImportCommitRequest, PortfolioEventImportPreview, PortfolioEventImportRequest,
    PortfolioEventImportResult, PortfolioEventInput, PortfolioEventRecord,
    PortfolioEventReversalInput, SecurityPriceConfig, SecurityPriceConfigInput, SecurityPriceQuery,
    SecurityPriceQuote, Snapshot, VerifiedHoldingValuationInput,
};
use crate::{fx, models, prices, AppResult};
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/daily-assets/ensure", post(ensure_daily_assets))
        .route("/api/daily-assets", get(daily_assets))
        .route("/api/daily-assets/compare", get(compare_daily_assets))
        .route("/api/holdings/{id}/amount", put(update_holding_amount))
        .route("/api/market-data/fx-rate", get(fx_rate_quote))
        .route("/api/fx-settings", get(fx_settings).put(save_fx_override))
        .route("/api/fx-settings/refresh", post(refresh_fx_settings))
        .route("/api/market-data/security-price", get(security_price_quote))
        .route(
            "/api/market-data/security/config",
            get(security_price_config).put(save_security_price_config),
        )
        .route(
            "/api/market-data/security/key",
            axum::routing::delete(delete_security_price_key),
        )
        .route("/api/profile", put(save_profile))
        .route("/api/holdings", post(add_holding))
        .route("/api/holding-trades", post(save_holding_trade))
        .route(
            "/api/holding-trades/{id}",
            put(update_holding_trade).delete(delete_holding_trade),
        )
        .route(
            "/api/market-data/security/refresh",
            post(refresh_security_prices),
        )
        .route(
            "/api/holdings/{id}",
            put(update_holding).delete(delete_holding),
        )
        .route(
            "/api/holdings/{id}/verified-valuation",
            put(apply_verified_holding_valuation),
        )
        .route(
            "/api/portfolio-events",
            get(portfolio_events).post(add_portfolio_event),
        )
        .route(
            "/api/portfolio-events/import/preview",
            post(preview_portfolio_event_import),
        )
        .route(
            "/api/portfolio-events/import/commit",
            post(commit_portfolio_event_import),
        )
        .route(
            "/api/portfolio-events/{id}/reverse",
            post(reverse_portfolio_event),
        )
        .route("/api/goals", post(add_goal))
        .route("/api/goals/{id}", put(update_goal).delete(delete_goal))
}

async fn snapshot(Extension(state): Extension<Arc<RequestContext>>) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().snapshot().await?))
}

async fn refresh_security_prices(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<prices::RefreshResult>> {
    Ok(Json(state.portfolio().refresh_prices().await?))
}

async fn fx_rate_quote(
    Extension(state): Extension<Arc<RequestContext>>,
    Query(query): Query<FxRateQuery>,
) -> AppResult<Json<FxRateQuote>> {
    Ok(Json(state.portfolio().fx_quote(&query).await?))
}

async fn security_price_quote(
    Extension(state): Extension<Arc<RequestContext>>,
    Query(query): Query<SecurityPriceQuery>,
) -> AppResult<Json<SecurityPriceQuote>> {
    Ok(Json(state.portfolio().security_quote(&query).await?))
}

async fn security_price_config(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<SecurityPriceConfig>> {
    Ok(Json(state.portfolio().security_config()))
}

async fn save_security_price_config(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<SecurityPriceConfigInput>,
) -> AppResult<Json<SecurityPriceConfig>> {
    Ok(Json(
        state
            .portfolio()
            .save_security_config(input.api_key.as_deref())?,
    ))
}

async fn delete_security_price_key(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<SecurityPriceConfig>> {
    Ok(Json(state.portfolio().delete_security_key()?))
}

async fn apply_verified_holding_valuation(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<VerifiedHoldingValuationInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().verify_valuation(&id, input).await?))
}

async fn save_profile(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<FinancialProfile>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().save_profile(&input).await?))
}

async fn add_holding(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().save_holding(None, input).await?))
}

async fn update_holding(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(
        state.portfolio().save_holding(Some(&id), input).await?,
    ))
}

async fn delete_holding(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_holding(&id)?))
}

async fn save_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<models::HoldingTradeInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().save_trade(&input).await?))
}

async fn update_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::HoldingTradeUpdateInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().update_trade(&id, &input).await?))
}

async fn delete_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::HoldingTradeDeleteInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().delete_trade(&id, &input).await?))
}

async fn portfolio_events(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<PortfolioEventRecord>>> {
    Ok(Json(state.db.portfolio_events()?))
}

async fn add_portfolio_event(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<PortfolioEventInput>,
) -> AppResult<Json<PortfolioEventRecord>> {
    Ok(Json(state.portfolio().add_event(&input).await?))
}

async fn reverse_portfolio_event(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<PortfolioEventReversalInput>,
) -> AppResult<Json<PortfolioEventRecord>> {
    Ok(Json(state.db.reverse_portfolio_event(&id, &input)?))
}

async fn preview_portfolio_event_import(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<PortfolioEventImportRequest>,
) -> AppResult<Json<PortfolioEventImportPreview>> {
    Ok(Json(state.portfolio().preview_import(&input).await?))
}

async fn commit_portfolio_event_import(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<PortfolioEventImportCommitRequest>,
) -> AppResult<Json<PortfolioEventImportResult>> {
    Ok(Json(state.portfolio().commit_import(&input).await?))
}

async fn add_goal(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_goal(&input)?))
}

async fn update_goal(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_goal(&id, &input)?))
}

async fn delete_goal(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_goal(&id)?))
}

async fn ensure_daily_assets(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<models::daily::EnsureInput>,
) -> AppResult<Json<models::daily::DailyHistory>> {
    Ok(Json(state.portfolio().ensure_daily(&input).await?))
}

async fn daily_assets(
    Extension(state): Extension<Arc<RequestContext>>,
    Query(query): Query<models::daily::HistoryQuery>,
) -> AppResult<Json<models::daily::DailyHistory>> {
    Ok(Json(state.db.daily_history(&query)?))
}

async fn compare_daily_assets(
    Extension(state): Extension<Arc<RequestContext>>,
    Query(query): Query<models::daily::CompareQuery>,
) -> AppResult<Json<models::daily::DailyComparison>> {
    Ok(Json(state.db.daily_compare(&query)?))
}

async fn update_holding_amount(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::daily::AmountInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_holding_amount(&id, &input)?))
}

async fn fx_settings(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<fx::FxBook>> {
    Ok(Json(state.portfolio().refresh_fx(false).await?))
}

async fn refresh_fx_settings(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<fx::FxBook>> {
    Ok(Json(state.portfolio().refresh_fx(true).await?))
}

async fn save_fx_override(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<fx::FxOverrideInput>,
) -> AppResult<Json<fx::FxBook>> {
    Ok(Json(state.portfolio().save_fx_override(&input).await?))
}
