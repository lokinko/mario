mod accounts;
mod agent;
mod ai;
mod hosted;
mod storage;
#[cfg(test)]
fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
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
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use ai::{ChatMessage, CodexProvider, InvestmentOrchestrator, ModelProvider, NativeModelProvider};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, Method},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Extension, Json, Router,
};
use chrono::Local;
use db::Database;
pub use error::{AppError, AppResult};
use evidence::{EvidenceRetriever, LexicalEvidenceRetriever};
use market_data::{
    EcbFxRateProvider, FxRateProvider, SecurityPriceProvider, TwelveDataSecurityPriceProvider,
};
use memory::{HybridMemoryRetriever, MemoryRetriever};
use models::{
    AnalysisHistoryItem, AnalysisPreview, AnalysisRequest, AnalysisResult, DecisionEntry,
    DecisionRecord, DecisionReviewInput, FinancialProfile, FxRateQuery, FxRateQuote, GoalInput,
    HoldingInput, InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, MemoryItem,
    MemoryPreferenceInput, ModelConfig, ModelConfigInput, ModelConnectionTest,
    PortfolioCheckInInput, PortfolioCheckInRecord, PortfolioEventImportCommitRequest,
    PortfolioEventImportPreview, PortfolioEventImportRequest, PortfolioEventImportResult,
    PortfolioEventInput, PortfolioEventRecord, PortfolioEventReversalInput, ReminderSettings,
    ReminderSettingsInput, ResearchEvidence, ResearchEvidenceInput, ResearchEvidenceStatusInput,
    ReviewReminderAcknowledgeInput, ReviewReminderSummary, RuleEffectivenessSummary,
    SecurityPriceConfig, SecurityPriceConfigInput, SecurityPriceQuery, SecurityPriceQuote,
    Snapshot, StoredAnalysis, SystemReviewInput, SystemReviewRecord, VerifiedHoldingValuationInput,
};
use tokio::sync::watch;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

struct AppState {
    mutation_lock: tokio::sync::Mutex<()>,
    db: Database,
    auth_token: Option<String>,
    fx_provider: Arc<dyn FxRateProvider>,
    security_price_provider: Arc<dyn SecurityPriceProvider>,
}

struct ServerOptions {
    parent_pid: Option<u32>,
    port: u16,
    auth_token: Option<String>,
}

pub async fn run_from_env() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mario_server=info,tower_http=info".into()),
        )
        .init();

    if std::env::var("MARIO_ROLE").as_deref() == Ok("agent") {
        return agent::serve().await;
    }
    if let Some(path) = std::env::args()
        .skip_while(|arg| arg != "--export-data")
        .nth(1)
    {
        let db = Database::open(&data_directory()?.join("mario.db"))?;
        let data = serde_json::to_vec_pretty(&db.export_sync_data()?)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        options.open(path)?.write_all(&data)?;
        return Ok(());
    }
    if let Some(email) = std::env::args()
        .skip_while(|arg| arg != "--reset-password")
        .nth(1)
    {
        let url = std::env::var("DATABASE_URL")
            .map_err(|_| AppError::Validation("缺少 DATABASE_URL".into()))?;
        let password = std::env::var("MARIO_RESET_PASSWORD")
            .map_err(|_| AppError::Validation("缺少 MARIO_RESET_PASSWORD".into()))?;
        accounts::Accounts::open(url)?.reset_password(&email, &password)?;
        return Ok(());
    }
    let options = server_options()?;
    let (parent_exit_tx, parent_exit_rx) = watch::channel(false);
    if let Some(pid) = options.parent_pid {
        tokio::spawn(watch_parent(pid, parent_exit_tx));
    }
    let web_dir = std::env::var_os("MARIO_WEB_DIR").map(PathBuf::from);
    let host: IpAddr = std::env::var("MARIO_HOST")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse()
        .map_err(|_| AppError::Validation("MARIO_HOST 必须是有效 IP 地址".into()))?;
    let database_url = std::env::var("DATABASE_URL").ok();
    if database_url.is_none()
        && (web_dir.is_some() || !host.is_loopback())
        && options
            .auth_token
            .as_ref()
            .is_none_or(|token| !(32..=256).contains(&token.len()))
    {
        return Err(AppError::Validation(
            "Web 服务必须设置 32–256 字符的 MARIO_AUTH_TOKEN".into(),
        ));
    }
    if let Some(dir) = &web_dir {
        if !dir.join("index.html").is_file() {
            return Err(AppError::Validation(
                "MARIO_WEB_DIR 中没有 index.html，请先构建 Web 页面".into(),
            ));
        }
    }
    if let Some(url) = database_url {
        return hosted::serve(
            url,
            SocketAddr::new(host, options.port),
            web_dir,
            shutdown_signal(parent_exit_rx),
        )
        .await;
    }
    let data_dir = data_directory()?;
    serve_configured(
        data_dir,
        SocketAddr::new(host, options.port),
        options.auth_token,
        web_dir,
        shutdown_signal(parent_exit_rx),
    )
    .await
}

pub async fn serve_embedded<F>(
    data_dir: PathBuf,
    port: u16,
    auth_token: String,
    shutdown: F,
) -> AppResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if port == 0 {
        return Err(AppError::Validation("嵌入式本地服务端口不能为 0".into()));
    }
    if !(32..=256).contains(&auth_token.len()) {
        return Err(AppError::Validation(
            "嵌入式本地服务必须使用 32–256 字符的随机认证令牌".into(),
        ));
    }
    serve_local(data_dir, port, Some(auth_token), shutdown).await
}

async fn serve_local<F>(
    data_dir: PathBuf,
    port: u16,
    auth_token: Option<String>,
    shutdown: F,
) -> AppResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    serve_configured(
        data_dir,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        auth_token,
        None,
        shutdown,
    )
    .await
}

async fn serve_configured<F>(
    data_dir: PathBuf,
    address: SocketAddr,
    auth_token: Option<String>,
    web_dir: Option<PathBuf>,
    shutdown: F,
) -> AppResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let state = Arc::new(AppState {
        mutation_lock: tokio::sync::Mutex::new(()),
        db: Database::open(&data_dir.join("mario.db"))?,
        auth_token: auth_token.clone(),
        fx_provider: Arc::new(EcbFxRateProvider::new()?),
        security_price_provider: Arc::new(TwelveDataSecurityPriceProvider::new()?),
    });
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:1420".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<HeaderValue>().unwrap(),
            "tauri://localhost".parse::<HeaderValue>().unwrap(),
            "http://tauri.localhost".parse::<HeaderValue>().unwrap(),
            "https://tauri.localhost".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = api_router()
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_local_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // Only compiled public assets are served outside the authenticated API routes.
    let app = if let Some(dir) = web_dir {
        app.fallback_service(tower_http::services::ServeDir::new(dir))
    } else {
        app
    };
    tracing::info!(%address, data_dir = %data_dir.display(), authenticated = auth_token.is_some(), "local service started");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::Io)?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

fn api_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/snapshot", get(snapshot))
        .route("/api/daily-assets/ensure", post(ensure_daily_assets))
        .route("/api/daily-assets", get(daily_assets))
        .route("/api/daily-assets/compare", get(compare_daily_assets))
        .route("/api/holdings/{id}/amount", put(update_holding_amount))
        .route("/api/market-data/fx-rate", get(fx_rate_quote))
        .route("/api/market-data/security-price", get(security_price_quote))
        .route(
            "/api/market-data/security/config",
            get(security_price_config).put(save_security_price_config),
        )
        .route(
            "/api/market-data/security/key",
            axum::routing::delete(delete_security_price_key),
        )
        .route("/api/profile", put(save_profile))
        .route("/api/holdings", post(add_holding))
        .route(
            "/api/holdings/{id}",
            put(update_holding).delete(delete_holding),
        )
        .route(
            "/api/holdings/{id}/verified-valuation",
            put(apply_verified_holding_valuation),
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
        .route(
            "/api/portfolio-events/{id}/reverse",
            post(reverse_portfolio_event),
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
        .route("/api/memories", get(memories))
        .route("/api/memories/{id}/preference", put(save_memory_preference))
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
        .route("/api/model-config/codex", post(read_codex_credentials))
        .route("/api/model-key", axum::routing::delete(delete_model_key))
        .route("/api/analysis/preview", post(preview_analysis))
        .route("/api/analysis", post(run_analysis))
        .route("/api/analyses", get(analyses))
        .route("/api/analyses/{id}", get(analysis))
}

async fn require_local_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(expected) = state.auth_token.as_deref() {
        let provided = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !provided.is_some_and(|value| token_matches(expected, value)) {
            return AppError::LocalAuth("请求缺少本次应用启动生成的访问令牌".into())
                .into_response();
        }
    }
    let _guard = if request.method() != Method::GET {
        Some(state.mutation_lock.lock().await)
    } else {
        None
    };
    request.extensions_mut().insert(state.clone());
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
async fn snapshot(Extension(state): Extension<Arc<AppState>>) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.snapshot()?))
}

async fn fx_rate_quote(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<FxRateQuery>,
) -> AppResult<Json<FxRateQuote>> {
    Ok(Json(state.fx_provider.quote(&query).await?))
}

async fn security_price_quote(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<SecurityPriceQuery>,
) -> AppResult<Json<SecurityPriceQuote>> {
    let key = state.db.secret("security-price")?;
    Ok(Json(
        state.security_price_provider.quote(&query, &key).await?,
    ))
}

async fn security_price_config(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<SecurityPriceConfig>> {
    Ok(Json(SecurityPriceConfig {
        provider: "twelve-data".into(),
        has_api_key: state.db.secret("security-price").is_ok(),
    }))
}

async fn save_security_price_config(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<SecurityPriceConfigInput>,
) -> AppResult<Json<SecurityPriceConfig>> {
    if let Some(key) = input.api_key.as_deref() {
        state.db.save_secret("security-price", key)?;
    }
    security_price_config(Extension(state)).await
}

async fn delete_security_price_key(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<SecurityPriceConfig>> {
    state.db.remove_secret("security-price")?;
    security_price_config(Extension(state)).await
}

async fn apply_verified_holding_valuation(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<VerifiedHoldingValuationInput>,
) -> AppResult<Json<Snapshot>> {
    let query = SecurityPriceQuery {
        symbol: input.symbol,
        on_date: input.on_date,
    };
    let key = state.db.secret("security-price")?;
    let quote = state.security_price_provider.quote(&query, &key).await?;
    Ok(Json(state.db.apply_verified_holding_valuation(
        &id,
        input.quantity,
        &quote,
    )?))
}
async fn save_profile(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<FinancialProfile>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.save_profile(&input)?))
}
async fn add_holding(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_holding(&input)?))
}
async fn update_holding(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<HoldingInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_holding(&id, &input)?))
}
async fn delete_holding(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_holding(&id)?))
}
async fn portfolio_checkins(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<PortfolioCheckInRecord>>> {
    Ok(Json(state.db.portfolio_checkins()?))
}
async fn save_portfolio_checkin(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<PortfolioCheckInInput>,
) -> AppResult<Json<PortfolioCheckInRecord>> {
    Ok(Json(state.db.save_portfolio_checkin(&input)?))
}
async fn portfolio_events(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<PortfolioEventRecord>>> {
    Ok(Json(state.db.portfolio_events()?))
}
async fn add_portfolio_event(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<PortfolioEventInput>,
) -> AppResult<Json<PortfolioEventRecord>> {
    Ok(Json(state.db.add_portfolio_event(&input)?))
}
async fn reverse_portfolio_event(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<PortfolioEventReversalInput>,
) -> AppResult<Json<PortfolioEventRecord>> {
    Ok(Json(state.db.reverse_portfolio_event(&id, &input)?))
}
async fn preview_portfolio_event_import(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<PortfolioEventImportRequest>,
) -> AppResult<Json<PortfolioEventImportPreview>> {
    Ok(Json(state.db.preview_portfolio_event_import(&input)?))
}
async fn commit_portfolio_event_import(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<PortfolioEventImportCommitRequest>,
) -> AppResult<Json<PortfolioEventImportResult>> {
    Ok(Json(state.db.commit_portfolio_event_import(&input)?))
}
async fn memories(Extension(state): Extension<Arc<AppState>>) -> AppResult<Json<Vec<MemoryItem>>> {
    Ok(Json(state.db.memories()?))
}
async fn save_memory_preference(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<MemoryPreferenceInput>,
) -> AppResult<Json<MemoryItem>> {
    Ok(Json(state.db.save_memory_preference(&id, &input)?))
}
async fn add_goal(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.add_goal(&input)?))
}
async fn update_goal(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<GoalInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_goal(&id, &input)?))
}
async fn delete_goal(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.delete_goal(&id)?))
}
async fn save_decision(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<DecisionEntry>,
) -> AppResult<axum::http::StatusCode> {
    state.db.save_decision(&input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn decisions(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<DecisionRecord>>> {
    Ok(Json(state.db.decisions()?))
}
async fn save_decision_review(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<DecisionReviewInput>,
) -> AppResult<axum::http::StatusCode> {
    state.db.save_decision_review(&id, &input)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn investment_rules(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<InvestmentRule>>> {
    Ok(Json(state.db.investment_rules()?))
}
async fn add_investment_rule(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.add_investment_rule(&input)?))
}
async fn update_investment_rule(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<InvestmentRuleInput>,
) -> AppResult<Json<InvestmentRule>> {
    Ok(Json(state.db.update_investment_rule(&id, &input)?))
}
async fn investment_rule_history(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<InvestmentRuleRevision>>> {
    Ok(Json(state.db.investment_rule_history(&id)?))
}
async fn rule_effectiveness(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<RuleEffectivenessSummary>> {
    Ok(Json(state.db.rule_effectiveness()?))
}
async fn system_reviews(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<SystemReviewRecord>>> {
    Ok(Json(state.db.system_reviews()?))
}
async fn save_system_review(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<SystemReviewInput>,
) -> AppResult<Json<SystemReviewRecord>> {
    Ok(Json(state.db.save_system_review(&input)?))
}
async fn reminder_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<ReminderSettings>> {
    Ok(Json(state.db.reminder_settings()?))
}
async fn save_reminder_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<ReminderSettingsInput>,
) -> AppResult<Json<ReminderSettings>> {
    Ok(Json(state.db.save_reminder_settings(input.enabled)?))
}
async fn review_reminders(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(
        state
            .db
            .review_reminder_summary(Local::now().date_naive())?,
    ))
}
async fn acknowledge_review_reminder(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<ReviewReminderAcknowledgeInput>,
) -> AppResult<Json<ReviewReminderSummary>> {
    Ok(Json(state.db.acknowledge_review_reminder(
        Local::now().date_naive(),
        &input.fingerprint,
    )?))
}
async fn research_evidence(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<ResearchEvidence>>> {
    Ok(Json(state.db.research_evidence()?))
}
async fn add_research_evidence(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<ResearchEvidenceInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(state.db.add_research_evidence(&input)?))
}
async fn set_research_evidence_status(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ResearchEvidenceStatusInput>,
) -> AppResult<Json<ResearchEvidence>> {
    Ok(Json(
        state.db.set_research_evidence_status(&id, input.active)?,
    ))
}
async fn model_config(Extension(state): Extension<Arc<AppState>>) -> AppResult<Json<ModelConfig>> {
    Ok(Json(state.db.model_config()?))
}

async fn read_codex_credentials(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<ModelConfig>> {
    if state.db.hosted() {
        return Err(AppError::Validation(
            "托管服务不允许使用服务器的个人 Codex 登录".into(),
        ));
    }
    let model = CodexProvider::detect().await?;
    save_codex_config(&state.db, &model)?;
    Ok(Json(state.db.model_config()?))
}

fn save_codex_config(db: &Database, model: &str) -> AppResult<()> {
    let previous = db.model_config()?;
    if previous.provider != "codex" && db.setting("model.key_binding")?.is_none() {
        db.set_setting(
            "model.key_binding",
            &model_key_binding(&previous.provider, &previous.base_url),
        )?;
    }
    db.save_model_metadata("codex", "codex://local", model)?;
    db.set_setting("model.codex_ready", "true")
}

fn configured_model_provider(
    db: &Database,
    config: &ModelConfig,
) -> AppResult<Box<dyn ModelProvider>> {
    if db.hosted() {
        agent::validate_endpoint(&config.base_url)?;
    }
    if config.provider == "codex" {
        if db.hosted() {
            return Err(AppError::Validation(
                "托管服务不允许使用 Codex 本机登录".into(),
            ));
        }
        return Ok(Box::new(CodexProvider::new(config.model.clone())));
    }
    Ok(Box::new(NativeModelProvider::new(
        config.provider.clone(),
        config.base_url.clone(),
        config.model.clone(),
        configured_model_key(db, config)?,
    )?))
}

async fn save_model_config(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<ModelConfigInput>,
) -> AppResult<Json<ModelConfig>> {
    if input.provider == "codex" {
        if state.db.hosted() {
            return Err(AppError::Validation(
                "托管服务不允许使用 Codex 本机登录".into(),
            ));
        }
        CodexProvider::detect().await?;
        save_codex_config(&state.db, &input.model)?;
        return Ok(Json(state.db.model_config()?));
    }
    let previous = state.db.model_config()?;
    let protocol = if input.provider == "openai-compatible" {
        "openai-responses"
    } else {
        &input.provider
    };
    if previous.has_api_key
        && (protocol != previous.provider
            || input.base_url.trim_end_matches('/') != previous.base_url.trim_end_matches('/'))
        && input
            .api_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
    {
        return Err(AppError::Validation(
            "切换模型接口或地址时，请填写对应 API Key".into(),
        ));
    }
    // Bind legacy credentials before changing metadata. A failed keychain write must
    // never leave an old provider's key usable at the newly selected destination.
    if state.db.setting("model.key_binding")?.is_none() {
        state.db.set_setting(
            "model.key_binding",
            &model_key_binding(&previous.provider, &previous.base_url),
        )?;
    }
    state
        .db
        .save_model_metadata(&input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input.api_key.as_deref() {
        state.db.save_secret("model", key)?;
        state.db.set_setting(
            "model.key_binding",
            &model_key_binding(protocol, &input.base_url),
        )?;
    }
    Ok(Json(state.db.model_config()?))
}

fn model_key_binding(provider: &str, base_url: &str) -> String {
    let protocol = if provider == "openai-compatible" {
        "openai-responses"
    } else {
        provider
    };
    format!("{} {}", protocol, base_url.trim_end_matches('/'))
}
fn configured_model_key(db: &Database, config: &ModelConfig) -> AppResult<String> {
    if db
        .setting("model.key_binding")?
        .is_some_and(|binding| binding != model_key_binding(&config.provider, &config.base_url))
    {
        return Err(AppError::Validation(
            "当前密钥不属于这个模型接口，请重新保存对应 API Key".into(),
        ));
    }
    db.secret("model")
}

async fn test_model_config(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<ModelConnectionTest>> {
    let config = state.db.model_config()?;
    let provider = configured_model_provider(&state.db, &config)?;
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

async fn delete_model_key(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<ModelConfig>> {
    state.db.remove_secret("model")?;
    Ok(Json(state.db.model_config()?))
}

async fn run_analysis(
    Extension(state): Extension<Arc<AppState>>,
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
    let daily_assets = state.db.daily_analysis_context(&request)?;
    let built_context = context::ContextBuilder::build(
        &request,
        &snapshot,
        &context::ContextSources {
            daily_assets: Some(&daily_assets),
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
    // Freeze credentials and their destination under the same account lock;
    // another device may change its model settings while this analysis runs.
    if state.db.hosted() {
        agent::validate_endpoint(&config.base_url)?;
    }
    let api_key = if config.provider == "codex" {
        None
    } else {
        Some(configured_model_key(&state.db, &config)?)
    };
    // The immutable authorized context is now frozen. Release the account lock
    // while the remote Agent runs so other devices can continue editing.
    state.db.unlock_account()?;
    let result = agent::execute(config, api_key, request.clone(), built_context, memories).await;
    state.db.lock_account()?;
    let result = result?;
    state.db.save_analysis(&result, &request.question)?;
    Ok(Json(result))
}

async fn preview_analysis(
    Extension(state): Extension<Arc<AppState>>,
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
    let daily_assets = state.db.daily_analysis_context(&request)?;
    let built_context = context::ContextBuilder::build(
        &request,
        &snapshot,
        &context::ContextSources {
            daily_assets: Some(&daily_assets),
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

async fn analyses(
    Extension(state): Extension<Arc<AppState>>,
) -> AppResult<Json<Vec<AnalysisHistoryItem>>> {
    Ok(Json(state.db.analysis_history()?))
}

async fn analysis(
    Extension(state): Extension<Arc<AppState>>,
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
        .filter(|_| request.context_selection.include_holdings)
        .map(|holding| format!("{} {}", holding.name, holding.symbol))
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!("{} {}", request.question, holdings);
    let mut items = db.research_evidence()?;
    let existing = items
        .iter()
        .map(|item| (item.source_url.clone(), item.claim.clone()))
        .collect::<HashSet<_>>();
    items.extend(
        db.automatic_research_evidence()?
            .into_iter()
            .filter(|item| !existing.contains(&(item.source_url.clone(), item.claim.clone()))),
    );
    Ok(LexicalEvidenceRetriever.search(&query, &items, 12))
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
    if std::env::var("DATABASE_URL").is_err()
        && !allow_unauthenticated_dev
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

async fn ensure_daily_assets(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<db::daily::EnsureInput>,
) -> AppResult<Json<db::daily::DailyHistory>> {
    Ok(Json(state.db.ensure_daily(&input)?))
}
async fn daily_assets(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<db::daily::HistoryQuery>,
) -> AppResult<Json<db::daily::DailyHistory>> {
    Ok(Json(state.db.daily_history(&query)?))
}
async fn compare_daily_assets(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<db::daily::CompareQuery>,
) -> AppResult<Json<db::daily::DailyComparison>> {
    Ok(Json(state.db.daily_compare(&query)?))
}
async fn update_holding_amount(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<db::daily::AmountInput>,
) -> AppResult<Json<Snapshot>> {
    Ok(Json(state.db.update_holding_amount(&id, &input)?))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use models::{ContextSelection, MemoryItem};

    #[test]
    fn mismatched_key_destination_is_rejected_before_reading_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("key-binding.db")).unwrap();
        db.set_setting(
            "model.key_binding",
            &model_key_binding("openai-compatible", "https://api.openai.com/v1/"),
        )
        .unwrap();
        let config = ModelConfig {
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            model: "fixture".into(),
            has_api_key: true,
        };
        assert!(configured_model_key(&db, &config)
            .unwrap_err()
            .to_string()
            .contains("当前密钥不属于"));
        assert_eq!(
            model_key_binding("openai-compatible", "https://api.openai.com/v1/"),
            model_key_binding("openai-responses", "https://api.openai.com/v1")
        );
    }

    fn request(excluded_memory_ids: Vec<String>) -> AnalysisRequest {
        AnalysisRequest {
            daily_asset_range: None,
            web_search: false,
            user_message: None,
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
            preference: "default".into(),
            preference_note: String::new(),
            preference_updated_at: None,
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

    #[tokio::test]
    async fn embedded_server_uses_the_same_authenticated_health_api() {
        let data_dir = tempfile::tempdir().unwrap();
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_embedded(
            data_dir.path().to_path_buf(),
            port,
            token.into(),
            async move {
                let _ = shutdown_rx.await;
            },
        ));
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/api/health");
        let mut authenticated_status = None;
        for _ in 0..40 {
            if let Ok(response) = client.get(&url).bearer_auth(token).send().await {
                authenticated_status = Some(response.status());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert_eq!(authenticated_status, Some(reqwest::StatusCode::OK));
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        shutdown_tx.send(()).unwrap();
        server.await.unwrap().unwrap();
        assert!(data_dir.path().join("mario.db").exists());
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
