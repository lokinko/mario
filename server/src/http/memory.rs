use super::RequestContext;
use crate::application::memory::MemoryService;
use crate::models::memory::{MemoryNote, MemoryOverview, MemoryStatusInput};
use crate::AppResult;
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/memory", get(overview))
        .route("/api/memory/dream", post(dream))
        .route("/api/memory/{id}/status", put(set_status))
        .route("/api/memory/{id}", delete(remove))
}

fn service(state: &RequestContext) -> MemoryService<'_> {
    MemoryService::new(&state.db, state.model_services.server.as_ref())
}

async fn overview(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<MemoryOverview>> {
    Ok(Json(service(&state).overview()?))
}

async fn dream(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<MemoryOverview>> {
    Ok(Json(
        service(&state)
            .dream(state.analysis_executor.as_ref())
            .await?,
    ))
}

async fn set_status(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<MemoryStatusInput>,
) -> AppResult<Json<MemoryNote>> {
    Ok(Json(service(&state).set_status(&id, input.active)?))
}

async fn remove(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    service(&state).delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}
