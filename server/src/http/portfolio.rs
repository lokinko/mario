use super::context::PortfolioContext;
use crate::models::{
    market::quotes::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
    portfolio::{FinancialProfile, HoldingInput, Snapshot, VerifiedHoldingValuationInput},
};
use crate::ports::user_account::{FxAccount, PortfolioAccount, PortfolioState};
use crate::ports::user_records::LedgerAccount;
use crate::{
    application::prices,
    models::fx::{FxBook, FxOverrideInput},
    AppResult,
};
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/snapshot", get(snapshot))
        .route(
            "/api/financial-operations/resolve",
            post(resolve_financial_operation),
        )
        .route("/api/daily-assets/ensure", post(ensure_daily_assets))
        .route("/api/daily-assets", get(daily_assets))
        .route("/api/portfolio/allocation", get(allocation))
        .route("/api/portfolio/dividends", get(dividend_income))
        .route("/api/daily-assets/compare", get(compare_daily_assets))
        .route("/api/holdings/{id}/amount", put(update_holding_amount))
        .route("/api/holdings/{id}/option-price", put(update_option_price))
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

async fn update_option_price(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::options::OptionPriceInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().update_option_price(&id, &input)?))
}

async fn financial_history(
    state: PortfolioContext,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(state.account().financial_history(&id)?))
}

async fn preview_trade_repair(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::portfolio::TradeRepairInput>,
) -> AppResult<Json<crate::models::portfolio::HoldingTradeSummary>> {
    Ok(Json(state.account().preview_trade_repair(&id, &input)?))
}

async fn repair_trades(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::portfolio::TradeRepairInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().repair_trades(&id, &input)?))
}

async fn declare_corporate_action(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::portfolio::ManualCorporateActionInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().declare_corporate_action(&id, &input)?))
}

async fn review_corporate_action(
    state: PortfolioContext,
    Path((id, mode)): Path<(String, String)>,
    Json(input): Json<crate::models::portfolio::ConfirmCorporateActionInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(
        state
            .account()
            .review_corporate_action(&id, &input, &mode)?,
    ))
}

async fn snapshot(state: PortfolioContext) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let started = std::time::Instant::now();
    let snapshot = state.portfolio().snapshot()?;
    tracing::debug!(target: "mario::timing", stage = "snapshot", elapsed_us = started.elapsed().as_micros() as u64);
    let started = std::time::Instant::now();
    let response = Json(snapshot).into_response();
    tracing::debug!(target: "mario::timing", stage = "snapshot_serialize", elapsed_us = started.elapsed().as_micros() as u64, status = response.status().as_u16());
    Ok(response)
}

#[derive(serde::Deserialize)]
struct ResolveFinancialOperation {
    method: String,
    path: String,
    input: serde_json::Value,
}
async fn resolve_financial_operation(
    state: PortfolioContext,
    Json(input): Json<ResolveFinancialOperation>,
) -> AppResult<Json<serde_json::Value>> {
    let snapshot =
        state
            .account()
            .replay_financial_request(&input.method, &input.path, &input.input)?;
    Ok(Json(
        serde_json::json!({"committed":snapshot.is_some(),"snapshot":snapshot}),
    ))
}

async fn refresh_security_prices(
    state: PortfolioContext,
) -> AppResult<Json<prices::RefreshResult>> {
    Ok(Json(state.portfolio().refresh_prices().await?))
}

async fn fx_rate_quote(
    state: PortfolioContext,
    Query(query): Query<FxRateQuery>,
) -> AppResult<Json<FxRateQuote>> {
    Ok(Json(state.portfolio().fx_quote(&query).await?))
}

async fn security_price_quote(
    state: PortfolioContext,
    Query(query): Query<SecurityPriceQuery>,
) -> AppResult<Json<SecurityPriceQuote>> {
    Ok(Json(
        state.portfolio().preview_security_quote(&query).await?,
    ))
}

async fn apply_verified_holding_valuation(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<VerifiedHoldingValuationInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().verify_valuation(&id, input).await?))
}

async fn save_profile(
    state: PortfolioContext,
    Json(input): Json<FinancialProfile>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().save_profile(&input)?))
}

async fn add_holding(
    state: PortfolioContext,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.portfolio().save_holding(None, input).await?))
}

async fn update_holding(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(
        state.portfolio().save_holding(Some(&id), input).await?,
    ))
}

async fn delete_holding(
    state: PortfolioContext,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AppResult<Json<Snapshot>> {
    let input = if body.is_empty() {
        crate::models::portfolio::HoldingDeleteInput::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| crate::AppError::Validation("删除持仓请求格式无效".into()))?
    };
    Ok(Json(state.account().delete_holding(&id, &input)?))
}

async fn save_holding_trade(
    state: PortfolioContext,
    Json(input): Json<crate::models::portfolio::HoldingTradeInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().save_holding_trade(&input)?))
}

async fn update_holding_trade(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::portfolio::HoldingTradeUpdateInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().update_holding_trade(&id, &input)?))
}

async fn delete_holding_trade(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::portfolio::HoldingTradeDeleteInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().delete_holding_trade(&id, &input)?))
}

async fn ensure_daily_assets(
    state: PortfolioContext,
    Json(input): Json<crate::models::daily::EnsureInput>,
) -> AppResult<Json<crate::models::daily::DailyHistory>> {
    Ok(Json(state.portfolio().ensure_daily(&input).await?))
}

async fn daily_assets(
    state: PortfolioContext,
    Query(query): Query<crate::models::daily::HistoryQuery>,
) -> AppResult<Json<crate::models::daily::DailyHistory>> {
    Ok(Json(state.account().daily_history(&query)?))
}

async fn dividend_income(
    state: PortfolioContext,
) -> AppResult<Json<crate::models::accounting::DividendIncome>> {
    Ok(Json(
        crate::application::portfolio::dividend_income(state.account(), state.provider()).await?,
    ))
}

async fn allocation(
    state: PortfolioContext,
) -> AppResult<Json<crate::models::accounting::Allocation>> {
    Ok(Json(crate::application::portfolio::allocation(
        state.account(),
        state.catalog(),
    )?))
}

async fn compare_daily_assets(
    state: PortfolioContext,
    Query(query): Query<crate::models::daily::CompareQuery>,
) -> AppResult<Json<crate::models::daily::DailyComparison>> {
    Ok(Json(state.account().daily_compare(&query)?))
}

async fn update_holding_amount(
    state: PortfolioContext,
    Path(id): Path<String>,
    Json(input): Json<crate::models::daily::AmountInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.account().update_holding_amount(&id, &input)?))
}

async fn fx_settings(state: PortfolioContext) -> AppResult<Json<FxBook>> {
    Ok(Json(
        state.account().fx_book(&state.account().base_currency()?)?,
    ))
}

async fn refresh_fx_settings(state: PortfolioContext) -> AppResult<Json<FxBook>> {
    Ok(Json(state.portfolio().refresh_fx(true).await?))
}

async fn save_fx_override(
    state: PortfolioContext,
    Json(input): Json<FxOverrideInput>,
) -> AppResult<Json<FxBook>> {
    Ok(Json(state.portfolio().save_fx_override(&input).await?))
}
