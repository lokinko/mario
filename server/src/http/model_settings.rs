//! AI model connections and analysis presets.
use super::context::ModelSettingsContext;
use crate::application::model_settings;
use crate::models::{
    analysis::AnalysisSettings,
    connection::{ModelConfig, ModelConfigInput, ModelConnectionTest},
};
use crate::ports::ai_records::AnalysisPresets;
use crate::AppResult;
use axum::{
    routing::{get, post},
    Json, Router,
};

pub(super) fn router() -> Router {
    Router::new()
        .route(
            "/api/model-config",
            get(model_config).put(save_model_config),
        )
        .route("/api/model-config/test", post(test_model_config))
        .route("/api/model-config/models", get(server_models))
        .route("/api/model-key", axum::routing::delete(delete_model_key))
        .route(
            "/api/analysis-settings",
            get(analysis_settings).put(save_analysis_settings),
        )
}

async fn model_config(state: ModelSettingsContext) -> AppResult<Json<ModelConfig>> {
    Ok(Json(model_settings::configuration(
        state.account(),
        state.services().server.as_ref(),
    )?))
}

async fn save_model_config(
    state: ModelSettingsContext,
    Json(input): Json<ModelConfigInput>,
) -> AppResult<Json<ModelConfig>> {
    Ok(Json(
        model_settings::save(state.account(), input, state.services()).await?,
    ))
}

async fn server_models(
    state: ModelSettingsContext,
) -> AppResult<Json<crate::models::connection::ServerModelCatalog>> {
    Ok(Json(
        model_settings::server_models(state.account(), state.services()).await?,
    ))
}

async fn test_model_config(state: ModelSettingsContext) -> AppResult<Json<ModelConnectionTest>> {
    Ok(Json(
        model_settings::test_connection(
            state.account(),
            state.services().connector.as_ref(),
            state.services().server.as_ref(),
        )
        .await?,
    ))
}

async fn delete_model_key(state: ModelSettingsContext) -> AppResult<Json<ModelConfig>> {
    Ok(Json(model_settings::delete_key(
        state.account(),
        state.services().server.as_ref(),
    )?))
}

async fn analysis_settings(state: ModelSettingsContext) -> AppResult<Json<AnalysisSettings>> {
    Ok(Json(state.presets().analysis_settings()?))
}

async fn save_analysis_settings(
    state: ModelSettingsContext,
    Json(settings): Json<AnalysisSettings>,
) -> AppResult<Json<AnalysisSettings>> {
    Ok(Json(state.presets().save_analysis_settings(settings)?))
}
