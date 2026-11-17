//! User-owned research originals, tags and attachments.
use super::RequestContext;
use crate::models::research::{
    ResearchEvidence, ResearchEvidenceInput, ResearchEvidenceStatusInput, ResearchEvidenceTagsInput,
};
use crate::AppResult;
use axum::{
    extract::Path,
    routing::{get, put},
    Extension, Json, Router,
};
use std::sync::Arc;
pub(super) fn router() -> Router {
    Router::new()
        .route(
            "/api/research-evidence",
            get(research_evidence)
                .post(add_research_evidence)
                .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route(
            "/api/research-evidence/{id}/attachments/{attachment_id}",
            get(research_attachment),
        )
        .route(
            "/api/research-evidence/{id}/status",
            put(set_research_evidence_status),
        )
        .route(
            "/api/research-evidence/{id}/tags",
            put(set_research_evidence_tags),
        )
}
async fn research_evidence(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<ResearchEvidence>>> {
    Ok(Json(state.db.research_evidence()?))
}

async fn add_research_evidence(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<ResearchEvidenceInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(state.db.add_research_evidence(&input)?))
}

async fn set_research_evidence_status(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceStatusInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(
        state.db.set_research_evidence_status(&id, input.active)?,
    ))
}

async fn set_research_evidence_tags(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceTagsInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(state.db.set_research_evidence_tags(&id, &input)?))
}

async fn research_attachment(
    Extension(state): Extension<Arc<RequestContext>>,
    Path((id, attachment_id)): Path<(String, String)>,
) -> AppResult<Json<crate::models::research::ResearchAttachmentDownload>> {
    Ok(Json(state.db.research_attachment(&id, &attachment_id)?))
}
