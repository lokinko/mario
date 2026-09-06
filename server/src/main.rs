mod ai;
mod cloud_sync;
mod context;
mod db;
mod error;
mod event_import;
mod evidence;
mod market_data;
mod memory;
mod models;
mod performance;
mod planning;
mod risk;
mod secrets;
mod valuation;

use std::{
    collections::HashSet,
    net::SocketAddr,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use ai::{ChatMessage, InvestmentOrchestrator, ModelProvider, OpenAiCompatibleProvider};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, Method},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Local;
use cloud_sync::{
    AccountCredentials, AccountResult, CloudConfig, CloudStatus, PullInput, RecoveryKeyInput,
    SyncResult,
};
use db::Database;
use error::{AppError, AppResult};
use evidence::{EvidenceRetriever, LexicalEvidenceRetriever};
use market_data::{EcbFxRateProvider, FxRateProvider};
use memory::{HybridMemoryRetriever, MemoryRetriever};
use models::{
    AnalysisHistoryItem, AnalysisPreview, AnalysisRequest, AnalysisResult, DecisionEntry,
    DecisionRecord, DecisionReviewInput, FinancialProfile, FxRateQuery, FxRateQuote, GoalInput,
    HoldingInput, InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, ModelConfig,
    ModelConfigInput, ModelConnectionTest, PortfolioCheckInInput, PortfolioCheckInRecord,
    PortfolioEventImportCommitRequest, PortfolioEventImportPreview, PortfolioEventImportRequest,
    PortfolioEventImportResult, PortfolioEventInput, PortfolioEventRecord, ReminderSettings,
    ReminderSettingsInput, ResearchEvidence, ResearchEvidenceInput, ResearchEvidenceStatusInput,
    ReviewReminderAcknowledgeInput, ReviewReminderSummary, RuleEffectivenessSummary, Snapshot,
    StoredAnalysis, SystemReviewInput, SystemReviewRecord,
};
use tokio::sync::watch;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

struct AppState {
    db: Database,
    auth_token: Option<String>,
    fx_provider: Arc<dyn FxRateProvider>,
}

struct ServerOptions {
    parent_pid: Option<u32>,
    port: u16,
    auth_token: Option<String>,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mario_server=info,tower_http=info".into()),
        )
        .init();

    let data_dir = data_directory()?;
    let options = server_options()?;
    let (parent_exit_tx, parent_exit_rx) = watch::channel(false);
    if let Some(pid) = options.parent_pid {
        tokio::spawn(watch_parent(pid, parent_exit_tx));
    }
    let state = Arc::new(AppState {
        db: Database::open(&data_dir.join("mario.db"))?,
        auth_token: options.auth_token.clone(),
        fx_provider: Arc::new(EcbFxRateProvider::new()?),
    });
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:1420".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<HeaderValue>().unwrap(),
            "tauri://localhost".parse::<HeaderValue>().unwrap(),
            "http://tauri.localhost".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/snapshot", get(snapshot))
        .route("/api/market-data/fx-rate", get(fx_rate_quote))
        .route("/api/profile", put(save_profile))
        .route("/api/holdings", post(add_holding))
        .route(
            "/api/holdings/{id}",
            put(update_holding).delete(delete_holding),
        )
        .route(
            "/api/portfolio-checkins",
            get(portfolio_checkins).post(save_portfolio_checkin),
        )
        .route(
            "/api/portfolio-events",
            get(portfolio_events).post(add_portfolio_event),
        )
        .route(
            "/api/portfolio-events/import/preview",
            post(preview_portfolio_event_import),
        )
        .route(
            "/api/portfolio-events/import/commit",
            post(commit_portfolio_event_import),
        )
        .route("/api/goals", post(add_goal))
        .route("/api/goals/{id}", put(update_goal).delete(delete_goal))
        .route("/api/decisions", get(decisions).post(save_decision))
        .route("/api/decisions/{id}/review", put(save_decision_review))
        .route(
            "/api/investment-rules",
            get(investment_rules).post(add_investment_rule),
        )
        .route("/api/investment-rules/{id}", put(update_investment_rule))
        .route(
            "/api/investment-rules/{id}/history",
            get(investment_rule_history),
        )
        .route("/api/rule-effectiveness", get(rule_effectiveness))
        .route(
            "/api/system-reviews",
            get(system_reviews).post(save_system_review),
        )
        .route(
            "/api/reminder-settings",
            get(reminder_settings).put(save_reminder_settings),
        )
        .route("/api/review-reminders", get(review_reminders))
        .route(
            "/api/review-reminders/acknowledge",
            post(acknowledge_review_reminder),
        )
        .route(
            "/api/research-evidence",
            get(research_evidence).post(add_research_evidence),
        )
        .route(
            "/api/research-evidence/{id}/status",
            put(set_research_evidence_status),
        )
        .route(
            "/api/model-config",
            get(model_config).put(save_model_config),
        )
        .route("/api/model-config/test", post(test_model_config))
        .route("/api/model-key", axum::routing::delete(delete_model_key))
        .route(
            "/api/cloud/config",
            get(cloud_config).put(save_cloud_config),
        )
        .route("/api/cloud/status", get(cloud_status))
        .route("/api/cloud/signup", post(cloud_signup))
        .route("/api/cloud/login", post(cloud_login))
        .route("/api/cloud/session", axum::routing::delete(cloud_logout))
        .route(
            "/api/cloud/recovery-key",
            get(export_cloud_recovery_key).put(import_cloud_recovery_key),
        )
        .route("/api/cloud/sync/push", post(cloud_push))
        .route("/api/cloud/sync/pull", post(cloud_pull))
        .route("/api/analysis/preview", post(preview_analysis))
        .route("/api/analysis", post(run_analysis))
        .route("/api/analyses", get(analyses))
        .route("/api/analyses/{id}", get(analysis))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_local_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let address = SocketAddr::from(([127, 0, 0, 1], options.port));
    tracing::info!(%address, data_dir = %data_dir.display(), authenticated = options.auth_token.is_some(), "local service started");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::Io)?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(parent_exit_rx))
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

async fn require_local_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.auth_token.as_deref() else {
        return next.run(request).await;
    };
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !provided.is_some_and(|value| token_matches(expected, value)) {
        return AppError::LocalAuth("请求缺少本次应用启动生成的访问令牌".into()).into_response();
    }
    next.run(request).await
}

fn token_matches(expected: &str, provided: &str) -> bool {
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;

    Sha256::digest(expected.as_bytes())
        .ct_eq(&Sha256::digest(provided.as_bytes()))
        .into()
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
async fn snapshot(State(state): State<Arc<AppState>>) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.snapshot()?))
}

async fn fx_rate_quote(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FxRateQuery>,
) -> AppResult<Json<FxRateQuote>> {
    Ok(Json(state.fx_provider.quote(&query).await?))
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
async fn update_holding(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_holding(&id, &input)?))
}
async fn delete_holding(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_holding(&id)?))
}
async fn portfolio_checkins(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<PortfolioCheckInRecord>>> {
    Ok(Json(state.db.portfolio_checkins()?))
}
async fn save_portfolio_checkin(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PortfolioCheckInInput>,
) -> AppResult<Json<PortfolioCheckInRecord>> {
    Ok(Json(state.db.save_portfolio_checkin(&input)?))
}
async fn portfolio_events(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<PortfolioEventRecord>>> {
    Ok(Json(state.db.portfolio_events()?))
}
async fn add_portfolio_event(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PortfolioEventInput>,
) -> AppResult<Json<PortfolioEventRecord>> {
    Ok(Json(state.db.add_portfolio_event(&input)?))
}
async fn preview_portfolio_event_import(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PortfolioEventImportRequest>,
) -> AppResult<Json<PortfolioEventImportPreview>> {
    Ok(Json(state.db.preview_portfolio_event_import(&input)?))
}
async fn commit_portfolio_event_import(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PortfolioEventImportCommitRequest>,
) -> AppResult<Json<PortfolioEventImportResult>> {
    Ok(Json(state.db.commit_portfolio_event_import(&input)?))
}
async fn add_goal(
    State(state): State<Arc<AppState>>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_goal(&input)?))
}
async fn update_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_goal(&id, &input)?))
}
async fn delete_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_goal(&id)?))
}
async fn save_decision(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DecisionEntry>,
) -> AppResult<axum::http::StatusCode> {
    state.db.save_decision(&input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn decisions(State(state): State<Arc<AppState>>) -> AppResult<Json<Vec<DecisionRecord>>> {
    Ok(Json(state.db.decisions()?))
}
async fn save_decision_review(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<DecisionReviewInput>,
) -> AppResult<axum::http::StatusCode> {
    state.db.save_decision_review(&id, &input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn investment_rules(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<InvestmentRule>>> {
    Ok(Json(state.db.investment_rules()?))
}
async fn add_investment_rule(
    State(state): State<Arc<AppState>>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.add_investment_rule(&input)?))
}
async fn update_investment_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.update_investment_rule(&id, &input)?))
}
async fn investment_rule_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<InvestmentRuleRevision>>> {
    Ok(Json(state.db.investment_rule_history(&id)?))
}
async fn rule_effectiveness(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<RuleEffectivenessSummary>> {
    Ok(Json(state.db.rule_effectiveness()?))
}
async fn system_reviews(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<SystemReviewRecord>>> {
    Ok(Json(state.db.system_reviews()?))
}
async fn save_system_review(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SystemReviewInput>,
) -> AppResult<Json<SystemReviewRecord>> {
    Ok(Json(state.db.save_system_review(&input)?))
}
async fn reminder_settings(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<ReminderSettings>> {
    Ok(Json(state.db.reminder_settings()?))
}
async fn save_reminder_settings(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ReminderSettingsInput>,
) -> AppResult<Json<ReminderSettings>> {
    Ok(Json(state.db.save_reminder_settings(input.enabled)?))
}
async fn review_reminders(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(
        state
            .db
            .review_reminder_summary(Local::now().date_naive())?,
    ))
}
async fn acknowledge_review_reminder(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ReviewReminderAcknowledgeInput>,
) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(state.db.acknowledge_review_reminder(
        Local::now().date_naive(),
        &input.fingerprint,
    )?))
}
async fn research_evidence(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ResearchEvidence>>> {
    Ok(Json(state.db.research_evidence()?))
}
async fn add_research_evidence(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ResearchEvidenceInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(state.db.add_research_evidence(&input)?))
}
async fn set_research_evidence_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceStatusInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(
        state.db.set_research_evidence_status(&id, input.active)?,
    ))
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

async fn test_model_config(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<ModelConnectionTest>> {
    let config = state.db.model_config()?;
    let provider = OpenAiCompatibleProvider::new(
        config.base_url,
        config.model.clone(),
        secrets::get_api_key()?,
    )?;
    let started = std::time::Instant::now();
    provider
        .complete(vec![
            ChatMessage::system("这是连接测试。"),
            ChatMessage::user("请只回复 OK"),
        ])
        .await?;
    Ok(Json(ModelConnectionTest {
        ok: true,
        model: config.model,
        latency_ms: started.elapsed().as_millis(),
    }))
}

async fn delete_model_key(State(state): State<Arc<AppState>>) -> AppResult<Json<ModelConfig>> {
    secrets::delete_api_key()?;
    Ok(Json(state.db.model_config()?))
}

async fn cloud_config(State(state): State<Arc<AppState>>) -> AppResult<Json<Option<CloudConfig>>> {
    Ok(Json(cloud_sync::config(&state.db)?))
}

async fn save_cloud_config(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CloudConfig>,
) -> AppResult<Json<CloudStatus>> {
    Ok(Json(cloud_sync::save_config(&state.db, &input)?))
}

async fn cloud_status(State(state): State<Arc<AppState>>) -> AppResult<Json<CloudStatus>> {
    Ok(Json(cloud_sync::status(&state.db)?))
}

async fn cloud_signup(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AccountCredentials>,
) -> AppResult<Json<AccountResult>> {
    Ok(Json(cloud_sync::sign_up(&state.db, &input).await?))
}

async fn cloud_login(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AccountCredentials>,
) -> AppResult<Json<AccountResult>> {
    Ok(Json(cloud_sync::sign_in(&state.db, &input).await?))
}

async fn cloud_logout(State(state): State<Arc<AppState>>) -> AppResult<Json<CloudStatus>> {
    Ok(Json(cloud_sync::sign_out(&state.db).await?))
}

async fn export_cloud_recovery_key(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "recoveryKey": cloud_sync::export_recovery_key(&state.db)?,
        "warning": "任何获得此密钥的人都可能解密你的云端投资数据，请离线保管。"
    })))
}

async fn import_cloud_recovery_key(
    State(state): State<Arc<AppState>>,
    Json(input): Json<RecoveryKeyInput>,
) -> AppResult<axum::http::StatusCode> {
    cloud_sync::import_recovery_key(&state.db, &input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn cloud_push(State(state): State<Arc<AppState>>) -> AppResult<Json<SyncResult>> {
    Ok(Json(cloud_sync::push(&state.db).await?))
}

async fn cloud_pull(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PullInput>,
) -> AppResult<Json<SyncResult>> {
    Ok(Json(cloud_sync::pull(&state.db, &input).await?))
}

async fn run_analysis(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisResult>> {
    validate_analysis_request(&request)?;
    let config = state.db.model_config()?;
    let snapshot = state.db.snapshot()?;
    let retriever = HybridMemoryRetriever::default();
    let memory_candidates = if request.workflow == "deep" && request.use_memory {
        initial_memory_candidates(
            &retriever,
            &request.question,
            &state.db.memories()?,
            &request.excluded_memory_ids,
        )
    } else {
        Vec::new()
    };
    let memories = selected_memories(&memory_candidates);
    let rules = state.db.investment_rules()?;
    let system_reviews = state
        .db
        .system_reviews()?
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    let portfolio_checkins = state
        .db
        .portfolio_checkins()?
        .into_iter()
        .take(12)
        .collect::<Vec<_>>();
    let portfolio_events = state
        .db
        .portfolio_events()?
        .into_iter()
        .take(100)
        .collect::<Vec<_>>();
    let evidence_candidates = relevant_evidence(&state.db, &request, &snapshot)?;
    let built_context = context::ContextBuilder::build(
        &request,
        &snapshot,
        &context::ContextSources {
            rules: &rules,
            system_reviews: &system_reviews,
            portfolio_checkins: &portfolio_checkins,
            portfolio_events: &portfolio_events,
            evidence_candidates: &evidence_candidates,
            memory_candidates: &memories,
        },
    );
    if request.preview_revision.as_deref() != Some(built_context.revision.as_str()) {
        return Err(AppError::Validation(
            "本地数据或上下文选择已变化，请重新预览后再确认分析".into(),
        ));
    }
    let provider =
        OpenAiCompatibleProvider::new(config.base_url, config.model, secrets::get_api_key()?)?;
    let orchestrator = InvestmentOrchestrator::new(&provider, &retriever);
    let result = orchestrator
        .run(&request, &built_context, &memories)
        .await?;
    state.db.save_analysis(&result, &request.question)?;
    Ok(Json(result))
}

async fn preview_analysis(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisPreview>> {
    validate_analysis_request(&request)?;
    let config = state.db.model_config()?;
    let snapshot = state.db.snapshot()?;
    let retriever = HybridMemoryRetriever::default();
    let memory_candidates = if request.workflow == "deep" && request.use_memory {
        initial_memory_candidates(
            &retriever,
            &request.question,
            &state.db.memories()?,
            &request.excluded_memory_ids,
        )
    } else {
        Vec::new()
    };
    let memories = selected_memories(&memory_candidates);
    let rules = state.db.investment_rules()?;
    let system_reviews = state
        .db
        .system_reviews()?
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    let portfolio_checkins = state
        .db
        .portfolio_checkins()?
        .into_iter()
        .take(12)
        .collect::<Vec<_>>();
    let portfolio_events = state
        .db
        .portfolio_events()?
        .into_iter()
        .take(100)
        .collect::<Vec<_>>();
    let evidence_candidates = relevant_evidence(&state.db, &request, &snapshot)?;
    let built_context = context::ContextBuilder::build(
        &request,
        &snapshot,
        &context::ContextSources {
            rules: &rules,
            system_reviews: &system_reviews,
            portfolio_checkins: &portfolio_checkins,
            portfolio_events: &portfolio_events,
            evidence_candidates: &evidence_candidates,
            memory_candidates: &memories,
        },
    );
    Ok(Json(context::ContextBuilder::preview(
        &request,
        built_context,
        config.provider,
        config.model,
        memory_candidates,
        evidence_candidates,
    )))
}

async fn analyses(State(state): State<Arc<AppState>>) -> AppResult<Json<Vec<AnalysisHistoryItem>>> {
    Ok(Json(state.db.analysis_history()?))
}

async fn analysis(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<StoredAnalysis>> {
    Ok(Json(state.db.analysis(&id)?))
}

fn validate_analysis_request(request: &AnalysisRequest) -> AppResult<()> {
    if request.question.trim().is_empty() {
        return Err(AppError::Validation("分析问题不能为空".into()));
    }
    if !matches!(request.workflow.as_str(), "quick" | "deep") {
        return Err(AppError::Validation("未知的分析工作流".into()));
    }
    if request.excluded_memory_ids.len() > 64 {
        return Err(AppError::Validation("单次最多排除 64 条候选记忆".into()));
    }
    let mut unique_ids = HashSet::new();
    for id in &request.excluded_memory_ids {
        if id.trim().is_empty() || id.chars().count() > 128 || !unique_ids.insert(id) {
            return Err(AppError::Validation(
                "排除记忆 ID 必须非空、不重复且不超过 128 个字符".into(),
            ));
        }
    }
    Ok(())
}

fn relevant_evidence(
    db: &Database,
    request: &AnalysisRequest,
    snapshot: &Snapshot,
) -> AppResult<Vec<ResearchEvidence>> {
    if !request.context_selection.include_evidence {
        return Ok(Vec::new());
    }
    let holdings = snapshot
        .holdings
        .iter()
        .map(|holding| format!("{} {}", holding.name, holding.symbol))
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!("{} {}", request.question, holdings);
    Ok(LexicalEvidenceRetriever.search(&query, &db.research_evidence()?, 12))
}

fn initial_memory_candidates(
    retriever: &dyn MemoryRetriever,
    question: &str,
    pool: &[models::MemoryItem],
    excluded_memory_ids: &[String],
) -> Vec<models::MemoryItem> {
    let mut candidates = retriever.search(question, pool, 16);
    for item in &mut candidates {
        item.selected = !excluded_memory_ids
            .iter()
            .any(|excluded| excluded == &item.id);
        if let Some(retrieval) = &mut item.retrieval {
            retrieval.passes.push("发送前问题初筛".into());
        }
    }
    candidates
}

fn selected_memories(candidates: &[models::MemoryItem]) -> Vec<models::MemoryItem> {
    candidates
        .iter()
        .filter(|item| item.selected)
        .cloned()
        .collect()
}

fn data_directory() -> AppResult<PathBuf> {
    if let Ok(value) = std::env::var("MARIO_DATA_DIR") {
        let directory = PathBuf::from(value);
        migrate_legacy_database_filename(&directory)?;
        return Ok(directory);
    }
    if let Ok(value) = std::env::var("COMPASS_DATA_DIR") {
        tracing::warn!("COMPASS_DATA_DIR 已弃用，请改用 MARIO_DATA_DIR");
        let directory = PathBuf::from(value);
        migrate_legacy_database_filename(&directory)?;
        return Ok(directory);
    }
    let root = dirs::data_local_dir()
        .ok_or_else(|| AppError::Validation("无法确定本地数据目录".into()))?;
    migrate_legacy_data_directory(&root)
}

fn migrate_legacy_data_directory(root: &FilePath) -> AppResult<PathBuf> {
    let current = root.join("com.lokinko.mario");
    let legacy = root.join("com.compassinvest.desktop");
    let selected = if !current.exists() && legacy.exists() {
        match std::fs::rename(&legacy, &current) {
            Ok(()) => current.clone(),
            Err(error) => {
                tracing::warn!(%error, "无法移动旧版数据目录，将继续从原位置读取");
                legacy.clone()
            }
        }
    } else {
        current.clone()
    };
    if selected == current && legacy.exists() {
        migrate_legacy_database_between_directories(&legacy, &current)?;
    }
    migrate_legacy_database_filename(&selected)?;
    Ok(selected)
}

fn migrate_legacy_database_between_directories(
    legacy_directory: &FilePath,
    current_directory: &FilePath,
) -> AppResult<()> {
    let legacy = legacy_directory.join("compass.db");
    let current = current_directory.join("mario.db");
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(current_directory)?;
    std::fs::rename(&legacy, &current)?;
    for suffix in ["-wal", "-shm"] {
        let legacy_sidecar = legacy_directory.join(format!("compass.db{suffix}"));
        if legacy_sidecar.exists() {
            std::fs::rename(
                legacy_sidecar,
                current_directory.join(format!("mario.db{suffix}")),
            )?;
        }
    }
    tracing::info!(data_dir = %current_directory.display(), "已迁移旧版 mario 本地数据库");
    Ok(())
}

fn migrate_legacy_database_filename(directory: &FilePath) -> AppResult<()> {
    let current = directory.join("mario.db");
    let legacy = directory.join("compass.db");
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(directory)?;
    std::fs::rename(&legacy, &current)?;
    for suffix in ["-wal", "-shm"] {
        let legacy_sidecar = directory.join(format!("compass.db{suffix}"));
        if legacy_sidecar.exists() {
            std::fs::rename(legacy_sidecar, directory.join(format!("mario.db{suffix}")))?;
        }
    }
    tracing::info!(data_dir = %directory.display(), "已迁移旧版 mario 本地数据库");
    Ok(())
}

fn server_options() -> AppResult<ServerOptions> {
    let mut arguments = std::env::args();
    let mut parent_pid = None;
    let mut port = 4217_u16;
    let mut allow_unauthenticated_dev = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--parent-pid" => {
                parent_pid = Some(
                    arguments
                        .next()
                        .ok_or_else(|| AppError::Validation("--parent-pid 缺少参数".into()))?
                        .parse::<u32>()
                        .map_err(|_| AppError::Validation("--parent-pid 参数无效".into()))?,
                );
            }
            "--port" => {
                port = arguments
                    .next()
                    .ok_or_else(|| AppError::Validation("--port 缺少参数".into()))?
                    .parse::<u16>()
                    .map_err(|_| AppError::Validation("--port 参数无效".into()))?;
                if port == 0 {
                    return Err(AppError::Validation("--port 不能为 0".into()));
                }
            }
            "--allow-unauthenticated-dev" => allow_unauthenticated_dev = true,
            _ => {}
        }
    }
    let auth_token = std::env::var("MARIO_AUTH_TOKEN")
        .or_else(|_| std::env::var("COMPASS_AUTH_TOKEN"))
        .ok()
        .filter(|value| !value.trim().is_empty());
    if !allow_unauthenticated_dev
        && auth_token
            .as_deref()
            .is_none_or(|value| value.len() < 32 || value.len() > 256)
    {
        return Err(AppError::Validation(
            "本地服务必须由桌面客户端携带随机认证令牌启动；开发模式请显式使用 --allow-unauthenticated-dev"
                .into(),
        ));
    }
    Ok(ServerOptions {
        parent_pid,
        port,
        auth_token,
    })
}

async fn watch_parent(parent_pid: u32, exit: watch::Sender<bool>) {
    let pid = sysinfo::Pid::from_u32(parent_pid);
    let mut system = sysinfo::System::new();
    loop {
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        if system.process(pid).is_none() {
            let _ = exit.send(true);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn shutdown_signal(mut parent_exit: watch::Receiver<bool>) {
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
    let parent_stopped = async {
        while parent_exit.changed().await.is_ok() {
            if *parent_exit.borrow() {
                break;
            }
        }
    };
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {}, _ = parent_stopped => {} }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use models::{ContextSelection, MemoryItem};

    fn request(excluded_memory_ids: Vec<String>) -> AnalysisRequest {
        AnalysisRequest {
            question: "复盘指数集中风险".into(),
            workflow: "deep".into(),
            use_memory: true,
            reflect: true,
            explore_alternatives: true,
            excluded_memory_ids,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        }
    }

    fn memory() -> MemoryItem {
        MemoryItem {
            id: "memory-1".into(),
            kind: "decision".into(),
            title: "指数".into(),
            summary: "集中风险".into(),
            content: json!({ "lesson": "降低集中度" }),
            created_at: "2026-01-01".into(),
            occurred_at: "2026-01-01".into(),
            status: "已复盘".into(),
            reviewed: true,
            contradiction: false,
            tags: vec!["指数".into()],
            selected: true,
            retrieval: None,
        }
    }

    #[test]
    fn excluded_memory_remains_visible_but_is_not_authorized() {
        let retriever = HybridMemoryRetriever::default();
        let candidates = initial_memory_candidates(
            &retriever,
            "复盘指数集中风险",
            &[memory()],
            &["memory-1".into()],
        );
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].selected);
        assert!(selected_memories(&candidates).is_empty());
    }

    #[test]
    fn rejects_duplicate_or_oversized_memory_exclusions() {
        assert!(validate_analysis_request(&request(vec!["same".into(), "same".into()])).is_err());
        assert!(validate_analysis_request(&request(vec!["x".repeat(129)])).is_err());
        assert!(validate_analysis_request(&request(vec!["memory-1".into()])).is_ok());
    }

    #[test]
    fn local_auth_token_comparison_rejects_missing_or_different_values() {
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(token_matches(token, token));
        assert!(!token_matches(token, "different"));
    }

    #[test]
    fn migrates_legacy_brand_data_directory_and_database_name() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("com.compassinvest.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("compass.db"), b"legacy database").unwrap();
        std::fs::write(legacy.join("compass.db-wal"), b"legacy wal").unwrap();

        let selected = migrate_legacy_data_directory(root.path()).unwrap();
        assert_eq!(selected, root.path().join("com.lokinko.mario"));
        assert_eq!(
            std::fs::read(selected.join("mario.db")).unwrap(),
            b"legacy database"
        );
        assert_eq!(
            std::fs::read(selected.join("mario.db-wal")).unwrap(),
            b"legacy wal"
        );
        assert!(!root.path().join("com.compassinvest.desktop").exists());
    }

    #[test]
    fn migrates_legacy_database_when_new_brand_directory_already_exists() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("com.compassinvest.desktop");
        let current = root.path().join("com.lokinko.mario");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(legacy.join("compass.db"), b"legacy database").unwrap();

        let selected = migrate_legacy_data_directory(root.path()).unwrap();
        assert_eq!(selected, current);
        assert_eq!(
            std::fs::read(selected.join("mario.db")).unwrap(),
            b"legacy database"
        );
        assert!(!legacy.join("compass.db").exists());
    }
}
