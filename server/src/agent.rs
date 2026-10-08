//! Stateless AI service. Receives only the backend's frozen, authorized context.
//! It has no database connection, session store, or mutation API.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Job {
    request: AnalysisRequest,
    context: context::BuiltContext,
    memories: Vec<models::MemoryItem>,
    config: ModelConfig,
    api_key: String,
}

pub fn validate_endpoint(base_url: &str) -> AppResult<()> {
    let url =
        reqwest::Url::parse(base_url).map_err(|_| AppError::Validation("模型地址无效".into()))?;
    let allowed = std::env::var("MARIO_MODEL_HOSTS")
        .unwrap_or_else(|_| "api.openai.com,api.anthropic.com".into());
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|h| allowed.split(',').any(|v| v.trim() == h))
        || url.port().is_some_and(|p| p != 443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "该模型地址未获服务器允许，请联系管理员配置 MARIO_MODEL_HOSTS".into(),
        ));
    }
    Ok(())
}
pub async fn execute(
    config: ModelConfig,
    api_key: Option<String>,
    request: AnalysisRequest,
    context: context::BuiltContext,
    memories: Vec<models::MemoryItem>,
) -> AppResult<AnalysisResult> {
    if let Ok(endpoint) = std::env::var("MARIO_AGENT_URL") {
        let token = std::env::var("MARIO_AGENT_TOKEN")
            .map_err(|_| AppError::Validation("缺少 Agent 服务令牌".into()))?;
        let job = Job {
            api_key: api_key
                .ok_or_else(|| AppError::Validation("独立 Agent 需要模型 API Key".into()))?,
            config,
            request,
            context,
            memories,
        };
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(280))
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .post(format!("{}/execute", endpoint.trim_end_matches('/')))
            .bearer_auth(token)
            .json(&job)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AppError::Model(
                "AI Agent 执行失败，请检查 Agent 服务日志及模型配置".into(),
            ));
        }
        return Ok(response.json().await?);
    }
    let provider: Box<dyn ModelProvider> = if config.provider == "codex" {
        Box::new(CodexProvider::new(config.model))
    } else {
        Box::new(NativeModelProvider::new(
            config.provider,
            config.base_url,
            config.model,
            api_key.ok_or_else(|| AppError::Validation("请配置 API Key".into()))?,
        )?)
    };
    InvestmentOrchestrator::new(provider.as_ref(), &HybridMemoryRetriever::default())
        .run(&request, &context, &memories)
        .await
}
pub async fn serve() -> AppResult<()> {
    let token = std::env::var("MARIO_AGENT_TOKEN").unwrap_or_default();
    if token.len() < 32 {
        return Err(AppError::Validation(
            "MARIO_AGENT_TOKEN 至少需要 32 字符".into(),
        ));
    }
    let slots = Arc::new(tokio::sync::Semaphore::new(4));
    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/execute", post(run))
        .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let token = token.clone();
            let slots = slots.clone();
            async move {
                if !request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|h| h.strip_prefix("Bearer "))
                    .is_some_and(|v| token_matches(&token, v))
                {
                    return AppError::Auth("Agent 认证失败".into()).into_response();
                }
                let _permit = if request.uri().path() == "/execute" {
                    match slots.try_acquire() {
                        Ok(permit) => Some(permit),
                        Err(_) => {
                            return AppError::Busy("Agent 繁忙，请稍后重试".into()).into_response()
                        }
                    }
                } else {
                    None
                };
                next.run(request).await
            }
        }));
    let address = std::env::var("MARIO_AGENT_BIND").unwrap_or_else(|_| "127.0.0.1:4218".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(%address,"stateless AI Agent started");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
async fn run(Json(job): Json<Job>) -> AppResult<Json<AnalysisResult>> {
    validate_endpoint(&job.config.base_url)?;
    validate_analysis_request(&job.request)?;
    let provider = NativeModelProvider::new(
        job.config.provider,
        job.config.base_url,
        job.config.model,
        job.api_key,
    )?;
    Ok(Json(
        InvestmentOrchestrator::new(&provider, &HybridMemoryRetriever::default())
            .run(&job.request, &job.context, &job.memories)
            .await?,
    ))
}
