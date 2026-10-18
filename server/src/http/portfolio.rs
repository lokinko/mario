use super::RequestContext;
use crate::models::{
    FinancialProfile, FxRateQuery, FxRateQuote, HoldingInput, SecurityPriceConfig,
    SecurityPriceConfigInput, SecurityPriceQuery, SecurityPriceQuote, Snapshot,
    VerifiedHoldingValuationInput,
};
use crate::{
    application::prices,
    models,
    models::market::{FxBook, FxOverrideInput},
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
        .route("/api/snapshot", get(snapshot))
        .route(
            "/api/financial-operations/resolve",
            post(resolve_financial_operation),
        )
        .route("/api/daily-assets/ensure", post(ensure_daily_assets))
        .route("/api/daily-assets", get(daily_assets))
        .route("/api/daily-assets/compare", get(compare_daily_assets))
        .route("/api/holdings/{id}/amount", put(update_holding_amount))
        .route(
            "/api/holdings/{id}/financial-history",
            get(financial_history),
        )
        .route(
            "/api/holdings/{id}/trade-repair/preview",
            post(preview_trade_repair),
        )
        .route("/api/holdings/{id}/trade-repair", post(repair_trades))
        .route(
            "/api/holdings/{id}/corporate-actions",
            post(declare_corporate_action),
        )
        .route(
            "/api/corporate-actions/{id}/{mode}",
            post(review_corporate_action),
        )
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
}

async fn financial_history(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(state.db.financial_history(&id)?))
}

async fn preview_trade_repair(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::TradeRepairInput>,
) -> AppResult<Json<models::HoldingTradeSummary>> {
    Ok(Json(state.db.preview_trade_repair(&id, &input)?))
}

async fn repair_trades(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::TradeRepairInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.repair_trades(&id, &input)?))
}

async fn declare_corporate_action(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::ManualCorporateActionInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.declare_corporate_action(&id, &input)?))
}

async fn review_corporate_action(
    Extension(state): Extension<Arc<RequestContext>>,
    Path((id, mode)): Path<(String, String)>,
    Json(input): Json<models::ConfirmCorporateActionInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.review_corporate_action(&id, &input, &mode)?))
}

async fn snapshot(Extension(state): Extension<Arc<RequestContext>>) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.snapshot()?))
}

#[derive(serde::Deserialize)]
struct ResolveFinancialOperation {
    method: String,
    path: String,
    input: serde_json::Value,
}
async fn resolve_financial_operation(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<ResolveFinancialOperation>,
) -> AppResult<Json<serde_json::Value>> {
    let snapshot = state
        .db
        .replay_financial_request(&input.method, &input.path, &input.input)?;
    Ok(Json(
        serde_json::json!({"committed":snapshot.is_some(),"snapshot":snapshot}),
    ))
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
    Ok(Json(state.db.save_profile(&input)?))
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
    body: axum::body::Bytes,
) -> AppResult<Json<Snapshot>> {
    let input = if body.is_empty() {
        models::HoldingDeleteInput::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| crate::AppError::Validation("删除持仓请求格式无效".into()))?
    };
    Ok(Json(state.db.delete_holding(&id, &input)?))
}

async fn save_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<models::HoldingTradeInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.save_holding_trade(&input)?))
}

async fn update_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::HoldingTradeUpdateInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_holding_trade(&id, &input)?))
}

async fn delete_holding_trade(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<models::HoldingTradeDeleteInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_holding_trade(&id, &input)?))
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

async fn fx_settings(Extension(state): Extension<Arc<RequestContext>>) -> AppResult<Json<FxBook>> {
    Ok(Json(
        state
            .db
            .fx_book(&state.db.snapshot()?.profile.base_currency)?,
    ))
}

async fn refresh_fx_settings(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<FxBook>> {
    Ok(Json(state.portfolio().refresh_fx(true).await?))
}

async fn save_fx_override(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<FxOverrideInput>,
) -> AppResult<Json<FxBook>> {
    Ok(Json(state.portfolio().save_fx_override(&input).await?))
}
