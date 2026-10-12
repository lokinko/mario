use super::RequestContext;
use crate::application::analysis::AnalysisService;
use crate::models::{
    AnalysisHistoryItem, AnalysisPreview, AnalysisRequest, AnalysisResult, MemoryItem,
    MemoryPreferenceInput, ResearchEvidence, ResearchEvidenceInput, ResearchEvidenceStatusInput,
    StoredAnalysis,
};
use crate::{models, AppResult};
use axum::{
    extract::Path,
    routing::{get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route("/api/memories", get(memories))
        .route("/api/memories/{id}/preference", put(save_memory_preference))
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
        .route("/api/analysis/preview", post(preview_analysis))
        .route("/api/analysis", post(run_analysis))
        .route("/api/analyses", get(analyses))
        .route("/api/analyses/{id}", get(analysis))
}

async fn memories(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<MemoryItem>>> {
    Ok(Json(state.db.memories()?))
}

async fn save_memory_preference(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
    Json(input): Json<MemoryPreferenceInput>,
) -> AppResult<Json<MemoryItem>> {
    Ok(Json(state.db.save_memory_preference(&id, &input)?))
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

async fn research_attachment(
    Extension(state): Extension<Arc<RequestContext>>,
    Path((id, attachment_id)): Path<(String, String)>,
) -> AppResult<Json<models::ResearchAttachmentDownload>> {
    Ok(Json(state.db.research_attachment(&id, &attachment_id)?))
}

async fn run_analysis(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisResult>> {
    Ok(Json(
        AnalysisService::new(&state.db)
            .execute(request, state.analysis_executor.as_ref())
            .await?,
    ))
}

async fn preview_analysis(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisPreview>> {
    Ok(Json(AnalysisService::new(&state.db).preview(&request)?))
}

async fn analyses(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<Vec<AnalysisHistoryItem>>> {
    Ok(Json(state.db.analysis_history()?))
}

async fn analysis(
    Extension(state): Extension<Arc<RequestContext>>,
    Path(id): Path<String>,
) -> AppResult<Json<StoredAnalysis>> {
    Ok(Json(state.db.analysis(&id)?))
}
