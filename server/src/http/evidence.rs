//! User-owned research originals, tags and attachments.
use super::context::EvidenceContext;
use crate::models::research::{
    ResearchEvidence, ResearchEvidenceInput, ResearchEvidenceStatusInput, ResearchEvidenceTagsInput,
};
use crate::ports::user_records::EvidenceAccount;
use crate::AppResult;
use axum::{
    extract::Path,
    routing::{get, put},
    Json, Router,
};
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
async fn research_evidence(state: EvidenceContext) -> AppResult<Json<Vec<ResearchEvidence>>> {
    Ok(Json(state.account().research_evidence()?))
}

async fn add_research_evidence(
    state: EvidenceContext,
    Json(input): Json<ResearchEvidenceInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(state.account().add_research_evidence(&input)?))
}

async fn set_research_evidence_status(
    state: EvidenceContext,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceStatusInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(
        state
            .account()
            .set_research_evidence_status(&id, input.active)?,
    ))
}

async fn set_research_evidence_tags(
    state: EvidenceContext,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceTagsInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(
        state.account().set_research_evidence_tags(&id, &input)?,
    ))
}

async fn research_attachment(
    state: EvidenceContext,
    Path((id, attachment_id)): Path<(String, String)>,
) -> AppResult<Json<crate::models::research::ResearchAttachmentDownload>> {
    Ok(Json(
        state.account().research_attachment(&id, &attachment_id)?,
    ))
}
