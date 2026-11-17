//! User-authored investment methodology.
use super::RequestContext;
use crate::{
    models::methodology::{InvestmentMethodology, InvestmentMethodologyInput},
    AppResult,
};
use axum::{routing::get, Extension, Json, Router};
use std::sync::Arc;
pub(super) fn router() -> Router {
    Router::new().route(
        "/api/investment-methodology",
        get(investment_methodology).put(save_investment_methodology),
    )
}
async fn investment_methodology(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<InvestmentMethodology>> {
    Ok(Json(state.db.investment_methodology()?))
}

async fn save_investment_methodology(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<InvestmentMethodologyInput>,
) -> AppResult<Json<InvestmentMethodology>> {
    Ok(Json(state.db.save_investment_methodology(input)?))
}
