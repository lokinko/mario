use super::context::DailyReviewContext;
use crate::{
    application::daily_review::DailyReviewService,
    models::daily_review::{
        DailyReviewFeedQuery, DailyReviewFeedResponse, DailyReviewReadInput, DailyReviewReadResult,
        DailyReviewRunInput, DailyReviewRunResult, DailyReviewSettings, DailyReviewTrigger,
    },
    ports::account::AccountSession,
    AppError, AppResult,
};
use axum::{
    extract::Query,
    routing::{get, post, put},
    Json, Router,
};
use chrono::NaiveDate;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/daily-review", get(feed))
        .route("/api/daily-review/settings", put(save_settings))
        .route("/api/daily-review/run", post(run_now))
        .route("/api/daily-review/read", post(mark_read))
}

async fn feed(
    state: DailyReviewContext,
    Query(query): Query<DailyReviewFeedQuery>,
) -> AppResult<Json<DailyReviewFeedResponse>> {
    Ok(Json(
        DailyReviewService::new(state.account(), state.server()).feed(&query)?,
    ))
}

async fn save_settings(
    state: DailyReviewContext,
    Json(input): Json<DailyReviewSettings>,
) -> AppResult<Json<DailyReviewSettings>> {
    Ok(Json(
        DailyReviewService::new(state.account(), state.server()).save_settings(&input)?,
    ))
}

async fn run_now(
    state: DailyReviewContext,
    input: Option<Json<DailyReviewRunInput>>,
) -> AppResult<Json<DailyReviewRunResult>> {
    let requested_day = input
        .and_then(|Json(body)| body.review_day)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|_| AppError::Validation("复盘日期必须为 YYYY-MM-DD".into()))
        })
        .transpose()?;
    let service = DailyReviewService::new(state.account(), state.server());
    let symbols = service.tracked_symbols()?;
    if !symbols.is_empty() {
        let announcements = state.announcements().clone();
        if let Err(error) = state
            .account()
            .without_account_lock(announcements.refresh(&symbols, false))
            .await?
        {
            tracing::warn!(%error, "manual daily review announcement refresh failed; using cache");
        }
    }
    let result = service
        .run(
            requested_day,
            DailyReviewTrigger::Manual,
            state.announcements().as_ref(),
            state.dividend_plans().as_ref(),
            state.executor(),
        )
        .await?;
    Ok(Json(result))
}

async fn mark_read(
    state: DailyReviewContext,
    Json(input): Json<DailyReviewReadInput>,
) -> AppResult<Json<DailyReviewReadResult>> {
    Ok(Json(
        DailyReviewService::new(state.account(), state.server())
            .mark_read(input.id.as_deref(), input.all.unwrap_or(false))?,
    ))
}
