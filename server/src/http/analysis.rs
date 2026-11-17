//! AI analysis execution, streaming and history.
use super::context::AnalysisContext;
use crate::models::analysis::{
    AnalysisHistoryItem, AnalysisPreview, AnalysisRequest, AnalysisResult, StoredAnalysis,
};
use crate::ports::ai_records::AnalysisArchive;
use crate::{application::analysis::AnalysisService, AppResult};
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
pub(super) fn router() -> Router {
    Router::new()
        .route("/api/analysis/preview", post(preview_analysis))
        .route("/api/analysis", post(run_analysis))
        .route("/api/analysis/stream", post(stream_analysis))
        .route("/api/analyses", get(analyses))
        .route("/api/analyses/{id}", get(analysis))
}
async fn run_analysis(
    state: AnalysisContext,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisResult>> {
    Ok(Json(
        AnalysisService::new(state.account(), state.server())
            .execute(request, state.executor())
            .await?,
    ))
}

async fn preview_analysis(
    state: AnalysisContext,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<Json<AnalysisPreview>> {
    Ok(Json(
        AnalysisService::new(state.account(), state.server()).preview(&request)?,
    ))
}

async fn stream_analysis(
    state: AnalysisContext,
    Json(request): Json<AnalysisRequest>,
) -> AppResult<axum::response::Response> {
    use axum::response::{sse::KeepAlive, IntoResponse, Sse};
    static SLOTS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
        std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(4)));
    let permit = SLOTS
        .clone()
        .try_acquire_owned()
        .map_err(|_| crate::AppError::Busy("问答繁忙，请稍后重试".into()))?;
    request.validate()?;
    let (sender, receiver) = tokio::sync::mpsc::channel(32);
    let future = Box::pin(async move {
        let _permit = permit;
        AnalysisService::new(state.account(), state.server())
            .execute_stream(request, state.executor(), sender)
            .await
    });
    let mut response = Sse::new(crate::streaming::response_stream(future, receiver))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
        .into_response();
    response.headers_mut().insert(
        "x-accel-buffering",
        axum::http::HeaderValue::from_static("no"),
    );
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

async fn analyses(state: AnalysisContext) -> AppResult<Json<Vec<AnalysisHistoryItem>>> {
    Ok(Json(state.archive().analysis_history()?))
}

async fn analysis(
    state: AnalysisContext,
    Path(id): Path<String>,
) -> AppResult<Json<StoredAnalysis>> {
    Ok(Json(state.archive().analysis(&id)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{RequestContext, RequestContextParts};
    use crate::{
        context::BuiltContext, db::Database, models::inference::AnalysisEvent,
        ports::analysis::AnalysisExecutor,
    };
    use axum::Extension;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct Executor {
        gate: Arc<tokio::sync::Notify>,
        dropped: Arc<AtomicBool>,
        fail: bool,
    }
    struct ExecutionGuard(Arc<AtomicBool>);
    impl Drop for ExecutionGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    #[async_trait::async_trait]
    impl AnalysisExecutor for Executor {
        async fn execute(
            &self,
            _: crate::models::connection::ModelConfig,
            _: Option<String>,
            _: AnalysisRequest,
            _: BuiltContext,
        ) -> AppResult<AnalysisResult> {
            unreachable!("stream route must use streaming execution")
        }
        async fn execute_stream(
            &self,
            config: crate::models::connection::ModelConfig,
            _: Option<String>,
            _: AnalysisRequest,
            context: BuiltContext,
            events: crate::ports::analysis::EventSender,
        ) -> AppResult<AnalysisResult> {
            let _guard = ExecutionGuard(self.dropped.clone());
            events
                .send(AnalysisEvent::Draft {
                    proposal: "P1".into(),
                    answer: "正在逐步回复".into(),
                })
                .await
                .unwrap();
            self.gate.notified().await;
            if self.fail {
                return Err(crate::AppError::Model("fixture interruption".into()));
            }
            Ok(serde_json::from_value(serde_json::json!({
                "id":"streamed-result", "answer":"最终报告", "stages":[],
                "createdAt":chrono::Utc::now().to_rfc3339(), "disclaimer":"test",
                "transparency":{"provider":config.provider,"model":config.model,"contextGroups":[],"payloadBytes":context.payload_bytes,"contextRevision":context.revision,"externalDataUsed":false,"apiKeySent":false}
            }))?)
        }
    }
    async fn fixture(
        fail: bool,
    ) -> (
        Arc<RequestContext>,
        reqwest::Response,
        Arc<tokio::sync::Notify>,
        Arc<AtomicBool>,
        tokio::task::JoinHandle<()>,
    ) {
        let gate = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let db = Database::test();
        db.save_model_configuration(
            crate::models::provider::ModelProviderKind::CodexBridge,
            "codex://bridge",
            "fixture",
            None,
        )
        .unwrap();
        db.set_setting("model.codex_bridge_id", "bridge-fixture")
            .unwrap();
        let market_runtime = crate::composition::MarketRuntime::new(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
        )
        .unwrap();
        let state = Arc::new(RequestContext::from(RequestContextParts {
            model_services: crate::composition::model_services(),
            user_id: "test-user".into(),
            is_superuser: false,
            watch_catalog: market_runtime.watch_catalog.clone(),
            dividend_calendar: market_runtime.dividend_calendar.clone(),
            watch_provider: Arc::new(
                crate::market_data::watch::PublicWatchProvider::new().unwrap(),
            ),
            db,
            market: market_runtime.services.clone(),
            analysis_executor: Arc::new(Executor {
                gate: gate.clone(),
                dropped: dropped.clone(),
                fail,
            }),
        }));
        let mut request: AnalysisRequest = serde_json::from_value(
            serde_json::json!({"question":"流式问题","workflow":"council","webSearch":false}),
        )
        .unwrap();
        request.preview_revision = Some(
            AnalysisService::new(
                state.database(),
                state.test_model_services().server.as_ref(),
            )
            .preview(&request)
            .unwrap()
            .context_revision,
        );
        let app = router().layer(Extension(state.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/analysis/stream"))
            .json(&request)
            .send()
            .await
            .unwrap();
        (state, response, gate, dropped, server)
    }
    async fn read_draft(response: &mut reqwest::Response) {
        let mut decoder = crate::ai::streaming::SseDecoder::default();
        loop {
            let chunk = tokio::time::timeout(std::time::Duration::from_secs(5), response.chunk())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            for data in decoder.feed(&chunk).unwrap() {
                if matches!(
                    serde_json::from_str::<AnalysisEvent>(&data).unwrap(),
                    AnalysisEvent::Draft { .. }
                ) {
                    return;
                }
            }
        }
    }
    #[tokio::test]
    async fn stream_emits_draft_before_completion_and_persists_only_success() {
        for fail in [false, true] {
            let (state, mut response, gate, _, server) = fixture(fail).await;
            assert_eq!(response.headers()["x-accel-buffering"], "no");
            assert!(response.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains("text/event-stream"));
            read_draft(&mut response).await;
            assert!(state.database().analysis_history().unwrap().is_empty());
            gate.notify_one();
            let tail = response.text().await.unwrap();
            if fail {
                assert!(tail.contains("fixture interruption"));
                assert!(state.database().analysis_history().unwrap().is_empty());
            } else {
                assert!(tail.contains("\"type\":\"complete\""));
                assert_eq!(
                    state
                        .database()
                        .analysis("streamed-result")
                        .unwrap()
                        .question,
                    "流式问题"
                );
            }
            server.abort();
        }
    }
    #[tokio::test]
    async fn disconnect_cancels_execution_without_saving_history() {
        let (state, mut response, _gate, dropped, server) = fixture(false).await;
        read_draft(&mut response).await;
        drop(response);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(state.database().analysis_history().unwrap().is_empty());
        server.abort();
    }
}
