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
        .route("/api/analysis/stream", post(stream_analysis))
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

async fn stream_analysis(
    Extension(state): Extension<Arc<RequestContext>>,
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
        AnalysisService::new(&state.db)
            .execute_stream(request, state.analysis_executor.as_ref(), sender)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai::{streaming::AnalysisEvent, AnalysisExecutor},
        context::BuiltContext,
        db::Database,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

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
            _: models::ModelConfig,
            _: Option<String>,
            _: AnalysisRequest,
            _: BuiltContext,
            _: Vec<MemoryItem>,
        ) -> AppResult<AnalysisResult> {
            unreachable!("stream route must use streaming execution")
        }
        async fn execute_stream(
            &self,
            config: models::ModelConfig,
            _: Option<String>,
            _: AnalysisRequest,
            context: BuiltContext,
            _: Vec<MemoryItem>,
            events: crate::ai::streaming::EventSender,
        ) -> AppResult<AnalysisResult> {
            let _guard = ExecutionGuard(self.dropped.clone());
            events
                .send(AnalysisEvent::Draft {
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
                "transparency":{"provider":config.provider,"model":config.model,"contextGroups":[],"payloadBytes":context.payload_bytes,"contextRevision":context.revision,"memoryItemsUsed":0,"externalDataUsed":false,"apiKeySent":false}
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
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        db.save_model_metadata("codex", "codex://local", "fixture")
            .unwrap();
        let state = Arc::new(RequestContext {
            db,
            market: Arc::new(crate::application::portfolio::MarketServices::new().unwrap()),
            analysis_executor: Arc::new(Executor {
                gate: gate.clone(),
                dropped: dropped.clone(),
                fail,
            }),
        });
        let mut request: AnalysisRequest = serde_json::from_value(serde_json::json!({"question":"流式问题","workflow":"quick","useMemory":false,"reflect":false,"exploreAlternatives":false,"webSearch":false})).unwrap();
        request.preview_revision = Some(
            AnalysisService::new(&state.db)
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
            assert!(state.db.analysis_history().unwrap().is_empty());
            gate.notify_one();
            let tail = response.text().await.unwrap();
            if fail {
                assert!(tail.contains("fixture interruption"));
                assert!(state.db.analysis_history().unwrap().is_empty());
            } else {
                assert!(tail.contains("\"type\":\"complete\""));
                assert_eq!(
                    state.db.analysis("streamed-result").unwrap().question,
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
        assert!(state.db.analysis_history().unwrap().is_empty());
        server.abort();
    }
}
