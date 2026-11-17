//! User-authored investment methodology.
use super::context::MethodologyContext;
use crate::ports::user_records::MethodologyAccount;
use crate::{
    models::methodology::{InvestmentMethodology, InvestmentMethodologyInput},
    AppResult,
};
use axum::{routing::get, Json, Router};
pub(super) fn router() -> Router {
    Router::new().route(
        "/api/investment-methodology",
        get(investment_methodology).put(save_investment_methodology),
    )
}
async fn investment_methodology(
    state: MethodologyContext,
) -> AppResult<Json<InvestmentMethodology>> {
    Ok(Json(state.account().investment_methodology()?))
}

async fn save_investment_methodology(
    state: MethodologyContext,
    Json(input): Json<InvestmentMethodologyInput>,
) -> AppResult<Json<InvestmentMethodology>> {
    Ok(Json(state.account().save_investment_methodology(input)?))
}
