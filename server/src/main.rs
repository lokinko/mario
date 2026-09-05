mod ai;
mod db;
mod error;
mod memory;
mod models;
mod risk;
mod secrets;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use ai::{InvestmentOrchestrator, OpenAiCompatibleProvider};
use axum::{
    extract::State,
    http::{HeaderValue, Method},
    routing::{get, post, put},
    Json, Router,
};
use db::Database;
use error::{AppError, AppResult};
use memory::LexicalMemoryRetriever;
use models::{
    AnalysisRequest, AnalysisResult, DecisionEntry, FinancialProfile, GoalInput, HoldingInput,
    ModelConfig, ModelConfigInput, Snapshot,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

struct AppState {
    db: Database,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "compass_server=info,tower_http=info".into()),
        )
        .init();

    let data_dir = data_directory()?;
    let state = Arc::new(AppState {
        db: Database::open(&data_dir.join("compass.db"))?,
    });
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:1420".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<HeaderValue>().unwrap(),
            "tauri://localhost".parse::<HeaderValue>().unwrap(),
            "http://tauri.localhost".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/snapshot", get(snapshot))
        .route("/api/profile", put(save_profile))
        .route("/api/holdings", post(add_holding))
        .route("/api/goals", post(add_goal))
        .route("/api/decisions", post(save_decision))
        .route(
            "/api/model-config",
            get(model_config).put(save_model_config),
        )
        .route("/api/analysis", post(run_analysis))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let address = SocketAddr::from(([127, 0, 0, 1], 4217));
    tracing::info!(%address, data_dir = %data_dir.display(), "local service started");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::Io)?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
async fn snapshot(State(state): State<Arc<AppState>>) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.snapshot()?))
}
async fn save_profile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<FinancialProfile>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.save_profile(&input)?))
}
async fn add_holding(
    State(state): State<Arc<AppState>>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_holding(&input)?))
}
async fn add_goal(
    State(state): State<Arc<AppState>>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_goal(&input)?))
}
async fn save_decision(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DecisionEntry>,
) -> AppResult<axum::http::StatusCode> {
    state.db.save_decision(&input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn model_config(State(state): State<Arc<AppState>>) -> AppResult<Json<ModelConfig>> {
    Ok(Json(state.db.model_config()?))
}

async fn save_model_config(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ModelConfigInput>,
) -> AppResult<Json<ModelConfig>> {
    state
        .db
        .save_model_metadata(&input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input.api_key.as_deref() {
        secrets::set_api_key(key)?;
    }
    Ok(Json(state.db.model_config()?))
}

async fn run_analysis(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisResult>> {
    if request.question.trim().is_empty() {
        return Err(AppError::Validation("分析问题不能为空".into()));
    }
    let config = state.db.model_config()?;
    let snapshot = state.db.snapshot()?;
    let memories = if request.use_memory {
        state.db.memories()?
    } else {
        Vec::new()
    };
    let provider =
        OpenAiCompatibleProvider::new(config.base_url, config.model, secrets::get_api_key()?)?;
    let retriever = LexicalMemoryRetriever;
    let orchestrator = InvestmentOrchestrator::new(&provider, &retriever);
    let result = orchestrator.run(&request, &snapshot, &memories).await?;
    state.db.save_analysis(
        &result.id,
        &request.question,
        &result.answer,
        &result.created_at,
    )?;
    Ok(Json(result))
}

fn data_directory() -> AppResult<PathBuf> {
    if let Ok(value) = std::env::var("COMPASS_DATA_DIR") {
        return Ok(PathBuf::from(value));
    }
    dirs::data_local_dir()
        .map(|path| path.join("com.compassinvest.desktop"))
        .ok_or_else(|| AppError::Validation("无法确定本地数据目录".into()))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
