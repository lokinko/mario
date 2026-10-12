//! Stateless AI service. Receives only the backend's frozen, authorized context.
//! It has no database connection, session store, or mutation API.
use crate::models::validate_endpoint;
use crate::{
    ai::{CodexProvider, InvestmentOrchestrator, ModelProvider, NativeModelProvider},
    authentication::token_matches,
    context,
    memory::HybridMemoryRetriever,
    models::{self, AnalysisRequest, AnalysisResult, ModelConfig},
    AppError, AppResult,
};
use axum::{
    extract::Request,
    http::header,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct Job {
    request: AnalysisRequest,
    context: context::BuiltContext,
    memories: Vec<models::MemoryItem>,
    config: ModelConfig,
    api_key: String,
}

pub(crate) struct AgentExecutor;

#[async_trait::async_trait]
impl crate::ai::AnalysisExecutor for AgentExecutor {
    async fn execute(
        &self,
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
    job.request.validate()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agent_runs_analysis_against_custom_chat_completions_endpoint() {
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = requests.clone();
        let app = Router::new().route(
            "/gateway/v1/chat/completions",
            post(move |headers: axum::http::HeaderMap, Json(body): Json<serde_json::Value>| {
                let captured = captured.clone();
                async move {
                    captured.lock().unwrap().push((headers, body));
                    let report = serde_json::json!({
                        "verdict":"先核对集中风险",
                        "facts":[{"statement":"组合集中度为100%","basis":"user_data","evidenceIds":[]}],
                        "inferences":[],
                        "unknowns":["资金使用期限"],
                        "options":[{"name":"先核对资料","suitableWhen":"资料不足","tradeoffs":["延后决策"],"risks":["信息仍不完整"]}],
                        "actions":[{"supportingFactIndices":[0],"evidenceLimits":["资金使用期限未知"],"action":"核对资金期限","rationale":"集中风险需要结合期限判断","reversible":true,"reviewTrigger":"明确期限后复核"}],
                        "reviewTriggers":["资金期限变化"]
                    });
                    Json(serde_json::json!({
                        "choices":[{"message":{"role":"assistant","content":report.to_string()},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":10,"completion_tokens":5}
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let request: AnalysisRequest = serde_json::from_value(serde_json::json!({
            "question":"如何检查集中风险？","workflow":"quick","webSearch":false,
            "useMemory":false,"reflect":false,"exploreAlternatives":false
        }))
        .unwrap();
        let result = run(Json(Job {
            request,
            context: context::BuiltContext {
                payload: serde_json::json!({"portfolio":{"concentrationPct":100.0}}),
                groups: Vec::new(),
                payload_bytes: 40,
                revision: "custom-model-test".into(),
                evidence_items: 0,
            },
            memories: Vec::new(),
            config: ModelConfig {
                provider: "openai-chat".into(),
                base_url: format!("http://{address}/gateway/v1/"),
                model: "third-party-model".into(),
                has_api_key: true,
            },
            api_key: "custom-model-test-key".into(),
        }))
        .await;
        server.abort();
        let Json(result) = result.unwrap();
        assert!(result.answer.contains("先核对集中风险"));
        assert_eq!(result.transparency.provider, "openai-chat");
        assert_eq!(result.transparency.model, "third-party-model");
        let requests = requests.lock().unwrap();
        assert!(!requests.is_empty());
        for (headers, body) in requests.iter() {
            assert_eq!(headers["authorization"], "Bearer custom-model-test-key");
            assert_eq!(body["model"], "third-party-model");
            assert!(body["messages"].is_array());
            assert!(body.get("tools").is_none());
        }
    }
}
