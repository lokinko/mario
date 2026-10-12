use super::RequestContext;
use crate::application::model_settings;
use crate::models::{AnalysisSettings, ModelConfig, ModelConfigInput, ModelConnectionTest};
use crate::AppResult;
use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router {
    Router::new()
        .route(
            "/api/model-config",
            get(model_config).put(save_model_config),
        )
        .route("/api/model-config/test", post(test_model_config))
        .route("/api/model-config/codex", post(read_codex_credentials))
        .route("/api/model-key", axum::routing::delete(delete_model_key))
        .route(
            "/api/analysis-settings",
            get(analysis_settings).put(save_analysis_settings),
        )
}

async fn model_config(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<ModelConfig>> {
    Ok(Json(state.db.model_config()?))
}

async fn read_codex_credentials(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<ModelConfig>> {
    Ok(Json(model_settings::read_codex(&state.db).await?))
}

async fn save_model_config(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(input): Json<ModelConfigInput>,
) -> AppResult<Json<ModelConfig>> {
    Ok(Json(model_settings::save(&state.db, input).await?))
}

async fn test_model_config(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<ModelConnectionTest>> {
    Ok(Json(model_settings::test_connection(&state.db).await?))
}

async fn delete_model_key(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<ModelConfig>> {
    state.db.remove_secret("model")?;
    Ok(Json(state.db.model_config()?))
}

async fn analysis_settings(
    Extension(state): Extension<Arc<RequestContext>>,
) -> AppResult<Json<AnalysisSettings>> {
    Ok(Json(state.db.analysis_settings()?))
}

async fn save_analysis_settings(
    Extension(state): Extension<Arc<RequestContext>>,
    Json(settings): Json<AnalysisSettings>,
) -> AppResult<Json<AnalysisSettings>> {
    Ok(Json(state.db.save_analysis_settings(settings)?))
}
