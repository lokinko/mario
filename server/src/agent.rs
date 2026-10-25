//! Stateless AI service. Receives only the backend's frozen, authorized context.
//! It has no database connection, session store, or mutation API.
use crate::models::validate_endpoint;
use crate::{
    ai::{InvestmentOrchestrator, ModelProvider, NativeModelProvider},
    authentication::token_matches,
    context,
    models::{AnalysisRequest, AnalysisResult, ModelConfig, ModelProviderKind},
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
    config: ModelConfig,
    api_key: String,
}

pub(crate) struct AgentExecutor;

#[async_trait::async_trait]
impl crate::ports::analysis::AnalysisExecutor for AgentExecutor {
    async fn execute(
        &self,
        config: ModelConfig,
        api_key: Option<String>,
        request: AnalysisRequest,
        context: context::BuiltContext,
    ) -> AppResult<AnalysisResult> {
        self.execute_with_events(config, api_key, request, context, None)
            .await
    }
    async fn execute_stream(
        &self,
        config: ModelConfig,
        api_key: Option<String>,
        request: AnalysisRequest,
        context: context::BuiltContext,
        events: crate::ports::analysis::EventSender,
    ) -> AppResult<AnalysisResult> {
        self.execute_with_events(config, api_key, request, context, Some(events))
            .await
    }
}
impl AgentExecutor {
    async fn execute_with_events(
        &self,
        config: ModelConfig,
        api_key: Option<String>,
        request: AnalysisRequest,
        context: context::BuiltContext,
        events: Option<crate::ports::analysis::EventSender>,
    ) -> AppResult<AnalysisResult> {
        if let Some(endpoint) = std::env::var("MARIO_AGENT_URL")
            .ok()
            .filter(|_| config.provider != ModelProviderKind::CodexBridge)
        {
            let token = std::env::var("MARIO_AGENT_TOKEN")
                .map_err(|_| AppError::Validation("缺少 Agent 服务令牌".into()))?;
            let job = Job {
                api_key: api_key
                    .ok_or_else(|| AppError::Validation("独立 Agent 需要模型 API Key".into()))?,
                config,
                request,
                context,
            };
            let mut response = reqwest::Client::builder()
                .timeout(crate::models::inference::AGENT_HTTP_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()?
                .post(format!(
                    "{}/execute{}",
                    endpoint.trim_end_matches('/'),
                    if events.is_some() { "/stream" } else { "" }
                ))
                .bearer_auth(token)
                .json(&job)
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(AppError::Model(
                    "AI Agent 执行失败，请检查 Agent 服务日志及模型配置".into(),
                ));
            }
            if let Some(events) = events {
                let mut decoder = crate::ai::streaming::SseDecoder::default();
                while let Some(bytes) = response.chunk().await? {
                    for data in decoder.feed(&bytes)? {
                        let event: crate::models::inference::AnalysisEvent =
                            serde_json::from_str(&data)
                                .map_err(|_| AppError::Model("Agent 流式事件无效".into()))?;
                        match event {
                            crate::models::inference::AnalysisEvent::Complete { result } => {
                                return Ok(*result)
                            }
                            crate::models::inference::AnalysisEvent::Error { message } => {
                                return Err(AppError::Model(message))
                            }
                            event => {
                                let _ = events.send(event).await;
                            }
                        }
                    }
                }
                return Err(AppError::Model("Agent 流连接中断，回答尚未完成".into()));
            }
            return Ok(response.json().await?);
        }
        let provider: Box<dyn ModelProvider> = if config.provider == ModelProviderKind::CodexBridge
        {
            Box::new(
                crate::ai::bridge::BridgeProvider::new(
                    api_key.ok_or_else(|| AppError::Validation("请先配对本机 Codex".into()))?,
                    config.model,
                )
                .with_events(events.clone()),
            )
        } else {
            Box::new(
                NativeModelProvider::new(
                    config.provider,
                    config.base_url,
                    config.model,
                    api_key.ok_or_else(|| AppError::Validation("请配置 API Key".into()))?,
                )?
                .with_events(events.clone()),
            )
        };
        let mut orchestrator = InvestmentOrchestrator::new(provider.as_ref());
        if let Some(events) = events {
            orchestrator = orchestrator.with_events(events);
        }
        orchestrator.run(&request, &context).await
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
        .route("/execute/stream", post(run_stream))
        .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(middleware::from_fn(
            move |mut request: Request, next: Next| {
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
                    let _permit = if matches!(request.uri().path(), "/execute" | "/execute/stream")
                    {
                        match slots.try_acquire_owned() {
                            Ok(permit) => Some(Arc::new(permit)),
                            Err(_) => {
                                return AppError::Busy("Agent 繁忙，请稍后重试".into())
                                    .into_response()
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(permit) = &_permit {
                        request.extensions_mut().insert(permit.clone());
                    }
                    next.run(request).await
                }
            },
        ));
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
    let orchestrator = InvestmentOrchestrator::new(&provider);
    Ok(Json(
        tokio::time::timeout(
            crate::models::inference::ANALYSIS_TIMEOUT,
            orchestrator.run(&job.request, &job.context),
        )
        .await
        .map_err(|_| {
            AppError::Model(crate::models::inference::ANALYSIS_TIMEOUT_MESSAGE.into())
        })??,
    ))
}

async fn run_stream(
    axum::Extension(permit): axum::Extension<Arc<tokio::sync::OwnedSemaphorePermit>>,
    Json(job): Json<Job>,
) -> AppResult<axum::response::Response> {
    validate_endpoint(&job.config.base_url)?;
    job.request.validate()?;
    let provider = NativeModelProvider::new(
        job.config.provider,
        job.config.base_url,
        job.config.model,
        job.api_key,
    )?;
    let (sender, receiver) = tokio::sync::mpsc::channel(32);
    let provider = provider.with_events(Some(sender.clone()));
    let future = Box::pin(async move {
        let _permit = permit;
        let orchestrator = InvestmentOrchestrator::new(&provider).with_events(sender);
        tokio::time::timeout(
            crate::models::inference::ANALYSIS_TIMEOUT,
            orchestrator.run(&job.request, &job.context),
        )
        .await
        .map_err(|_| AppError::Model(crate::models::inference::ANALYSIS_TIMEOUT_MESSAGE.into()))?
    });
    let mut response =
        axum::response::Sse::new(crate::streaming::response_stream(future, receiver))
            .keep_alive(axum::response::sse::KeepAlive::new())
            .into_response();
    response.headers_mut().insert(
        "x-accel-buffering",
        axum::http::HeaderValue::from_static("no"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agent_runs_json_and_streamed_analysis_against_custom_chat_completions_endpoint() {
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = requests.clone();
        let app = Router::new().route(
            "/gateway/v1/chat/completions",
            post(move |headers: axum::http::HeaderMap, Json(body): Json<serde_json::Value>| {
                let captured = captured.clone();
                async move {
                    let streaming = body["stream"] == true;
                    captured.lock().unwrap().push((headers, body));
                    let report = serde_json::json!({
                        "answer":"先核对集中风险",
                        "facts":[{"statement":"组合集中度为100%","basis":"user_data","evidenceIds":[]}],
                        "suggestions":[{"factIndices":[0],"action":"先保持现有仓位","why":"集中度100%，暂不加仓","unlessWhen":"资金期限明确后重评"}],
                        "question":"资金什么时候要用？"
                    });
                    if streaming {
                        let content = report.to_string();
                        let stream = async_stream::stream! {
                            yield Ok::<_, std::convert::Infallible>(bytes::Bytes::from(format!("data: {}\n\n", serde_json::json!({"choices":[{"index":0,"delta":{"content":content},"finish_reason":null}]}))));
                            tokio::task::yield_now().await;
                            yield Ok(bytes::Bytes::from("data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"));
                        };
                        return axum::response::Response::builder().header("content-type", "text/event-stream").body(axum::body::Body::from_stream(stream)).unwrap();
                    }
                    Json(serde_json::json!({
                        "choices":[{"message":{"role":"assistant","content":report.to_string()},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":10,"completion_tokens":5}
                    })).into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let request: AnalysisRequest = serde_json::from_value(serde_json::json!({
            "question":"如何检查集中风险？","workflow":"quick","webSearch":false,
            "reflect":false,"exploreAlternatives":false
        }))
        .unwrap();
        let make_job = || Job {
            request: request.clone(),
            context: context::BuiltContext {
                payload: serde_json::json!({"portfolio":{"concentrationPct":100.0}}),
                groups: Vec::new(),
                payload_bytes: 40,
                revision: "custom-model-test".into(),
                evidence_items: 0,
                valuation_evidence: Vec::new(),
                evidence_catalog: Vec::new(),
            },
            config: ModelConfig {
                provider: ModelProviderKind::OpenaiChat,
                base_url: format!("http://{address}/gateway/v1/"),
                model: "third-party-model".into(),
                has_api_key: true,
            },
            api_key: "custom-model-test-key".into(),
        };
        let Json(result) = run(Json(make_job())).await.unwrap();
        assert!(result.answer.contains("先核对集中风险"));
        assert_eq!(result.transparency.provider, "openai-chat");
        assert_eq!(result.transparency.model, "third-party-model");
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = Arc::new(slots.clone().try_acquire_owned().unwrap());
        let response = run_stream(axum::Extension(permit), Json(make_job()))
            .await
            .unwrap();
        assert_eq!(slots.available_permits(), 0);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let events = crate::ai::streaming::SseDecoder::default()
            .feed(&body)
            .unwrap();
        assert!(events
            .iter()
            .any(|data| data.contains("\"type\":\"draft\"") && data.contains("先核对集中风险")));
        let completed = events.last().unwrap();
        assert!(completed.contains("\"type\":\"complete\""));
        assert!(completed.contains("先核对集中风险"));
        assert_eq!(slots.available_permits(), 1);
        server.abort();
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
