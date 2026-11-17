use super::context::ReviewContext;
use crate::models::reviews::{
    InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, ReviewReminderSummary,
    SystemReviewInput, SystemReviewRecord,
};
use crate::ports::user_records::ReviewAccount;
use crate::AppResult;
use axum::{
    extract::Path,
    routing::{get, put},
    Json, Router,
};
use chrono::Local;

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

async fn investment_rules(state: ReviewContext) -> AppResult<Json<Vec<InvestmentRule>>> {
    Ok(Json(state.account().investment_rules()?))
}

async fn add_investment_rule(
    state: ReviewContext,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.account().add_investment_rule(&input)?))
}

async fn update_investment_rule(
    state: ReviewContext,
    Path(id): Path<String>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.account().update_investment_rule(&id, &input)?))
}

async fn investment_rule_history(
    state: ReviewContext,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<InvestmentRuleRevision>>> {
    Ok(Json(state.account().investment_rule_history(&id)?))
}

async fn system_reviews(state: ReviewContext) -> AppResult<Json<Vec<SystemReviewRecord>>> {
    Ok(Json(state.account().system_reviews()?))
}

async fn save_system_review(
    state: ReviewContext,
    Json(input): Json<SystemReviewInput>,
) -> AppResult<Json<SystemReviewRecord>> {
    Ok(Json(state.account().save_system_review(&input)?))
}

async fn review_reminders(state: ReviewContext) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(
        state
            .account()
            .review_reminder_summary(Local::now().date_naive())?,
    ))
}
