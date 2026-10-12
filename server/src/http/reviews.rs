use super::RequestContext;
use crate::models::{
    InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, ReviewReminderSummary,
    SystemReviewInput, SystemReviewRecord,
};
use crate::AppResult;
use axum::{
    extract::Path,
    routing::{get, put},
    Extension, Json, Router,
};
use chrono::Local;
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route(
            "/api/investment-rules",
            get(investment_rules).post(add_investment_rule),
        )
        .route("/api/investment-rules/{id}", put(update_investment_rule))
        .route(
            "/api/investment-rules/{id}/history",
            get(investment_rule_history),
        )
        .route(
            "/api/system-reviews",
            get(system_reviews).post(save_system_review),
        )
        .route("/api/review-reminders", get(review_reminders))
}

async fn investment_rules(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<InvestmentRule>>> {
    Ok(Json(state.db.investment_rules()?))
}

async fn add_investment_rule(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.add_investment_rule(&input)?))
}

async fn update_investment_rule(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.update_investment_rule(&id, &input)?))
}

async fn investment_rule_history(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<InvestmentRuleRevision>>> {
    Ok(Json(state.db.investment_rule_history(&id)?))
}

async fn system_reviews(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<SystemReviewRecord>>> {
    Ok(Json(state.db.system_reviews()?))
}

async fn save_system_review(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<SystemReviewInput>,
) -> AppResult<Json<SystemReviewRecord>> {
    Ok(Json(state.db.save_system_review(&input)?))
}

async fn review_reminders(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(
        state
            .db
            .review_reminder_summary(Local::now().date_naive())?,
    ))
}
