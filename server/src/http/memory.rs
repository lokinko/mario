use super::context::MemoryContext;
use crate::application::memory::MemoryService;
use crate::models::memory::{MemoryNote, MemoryOverview, MemoryStatusInput};
use crate::AppResult;
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/memory", get(overview))
        .route("/api/memory/dream", post(dream))
        .route("/api/memory/{id}/status", put(set_status))
        .route("/api/memory/{id}", delete(remove))
}

fn service(
    state: &MemoryContext,
) -> MemoryService<'_, impl crate::ports::ai_account::MemoryAccount> {
    MemoryService::new(state.account(), state.server())
}

async fn overview(state: MemoryContext) -> AppResult<Json<MemoryOverview>> {
    Ok(Json(service(&state).overview()?))
}

async fn dream(state: MemoryContext) -> AppResult<Json<MemoryOverview>> {
    Ok(Json(service(&state).dream(state.executor()).await?))
}

async fn set_status(
    state: MemoryContext,
    Path(id): Path<String>,
    Json(input): Json<MemoryStatusInput>,
) -> AppResult<Json<MemoryNote>> {
    Ok(Json(service(&state).set_status(&id, input.active)?))
}

async fn remove(state: MemoryContext, Path(id): Path<String>) -> AppResult<StatusCode> {
    service(&state).delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}
