//! Freeze authorized sources, validate the preview, execute, and persist the result.
use super::model_settings::configured_model_key;
use crate::{
    context,
    db::Database,
    evidence::search_evidence,
    models::{
        AnalysisPreview, AnalysisRequest, AnalysisResult, ModelConfig, ModelProviderKind,
        ResearchEvidence, Snapshot,
    },
    ports::analysis::AnalysisExecutor,
    AppError, AppResult,
};
use std::collections::HashSet;
pub(crate) struct AnalysisService<'a> {
    db: &'a Database,
}
struct PreparedAnalysis {
    config: ModelConfig,
    context: context::BuiltContext,
    evidence_candidates: Vec<ResearchEvidence>,
}
impl<'a> AnalysisService<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
    fn prepare(&self, request: &AnalysisRequest) -> AppResult<PreparedAnalysis> {
        request.validate()?;
        let config = self.db.model_config()?;
        let snapshot = self.db.snapshot()?;
        let rules = self.db.investment_rules()?;
        let system_reviews = self
            .db
            .system_reviews()?
            .into_iter()
            .take(8)
            .collect::<Vec<_>>();
        let evidence_candidates = relevant_evidence(self.db, request, &snapshot)?;
        let daily_assets = self.db.daily_analysis_context(request)?;
        let built_context = context::ContextBuilder::build(
            request,
            &snapshot,
            &context::ContextSources {
                daily_assets: Some(&daily_assets),
                rules: &rules,
                system_reviews: &system_reviews,
                evidence_candidates: &evidence_candidates,
            },
        );
        Ok(PreparedAnalysis {
            config,
            context: built_context,
            evidence_candidates,
        })
    }
    pub fn preview(&self, request: &AnalysisRequest) -> AppResult<AnalysisPreview> {
        let prepared = self.prepare(request)?;
        Ok(context::ContextBuilder::preview(
            request,
            prepared.context,
            prepared.config.provider,
            prepared.config.model,
            prepared.evidence_candidates,
        ))
    }
    pub async fn execute(
        &self,
        request: AnalysisRequest,
        executor: &dyn AnalysisExecutor,
    ) -> AppResult<AnalysisResult> {
        self.execute_with_events(request, executor, None).await
    }

    pub async fn execute_stream(
        &self,
        request: AnalysisRequest,
        executor: &dyn AnalysisExecutor,
        events: crate::ports::analysis::EventSender,
    ) -> AppResult<AnalysisResult> {
        // Streaming body starts after authentication middleware releases its lock.
        let _lock = self.db.lock_account()?;
        self.execute_with_events(request, executor, Some(events))
            .await
    }

    async fn execute_with_events(
        &self,
        request: AnalysisRequest,
        executor: &dyn AnalysisExecutor,
        events: Option<crate::ports::analysis::EventSender>,
    ) -> AppResult<AnalysisResult> {
        let prepared = self.prepare(&request)?;
        let config = prepared.config;
        if request.preview_revision.as_deref() != Some(prepared.context.revision.as_str()) {
            return Err(AppError::Validation(
                "本地数据或上下文选择已变化，请重新预览后再确认分析".into(),
            ));
        }
        // Freeze credentials and their destination under the same account lock;
        // another device may change its model settings while this analysis runs.
        if config.provider != ModelProviderKind::CodexBridge {
            crate::models::validate_endpoint(&config.base_url)?;
        }
        let api_key = match config.provider {
            ModelProviderKind::CodexBridge => Some(super::codex_bridge::id(self.db)?),
            _ => Some(configured_model_key(self.db, &config)?),
        };
        let execution = async {
            if let Some(events) = events {
                executor
                    .execute_stream(config, api_key, request.clone(), prepared.context, events)
                    .await
            } else {
                executor
                    .execute(config, api_key, request.clone(), prepared.context)
                    .await
            }
        };
        // The immutable authorized context is now frozen. Release the account lock
        // while the remote Agent runs so other devices can continue editing.
        let result = self
            .db
            .without_account_lock(tokio::time::timeout(
                crate::models::inference::ANALYSIS_TIMEOUT,
                execution,
            ))
            .await?
            .unwrap_or_else(|_| {
                Err(AppError::Model(
                    crate::models::inference::ANALYSIS_TIMEOUT_MESSAGE.into(),
                ))
            })?;
        self.db.save_analysis(
            &result,
            request.user_message.as_deref().unwrap_or(&request.question),
        )?;
        Ok(result)
    }
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
    Ok(search_evidence(&query, &items, 30))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContextSelection;
    use serde_json::json;
    fn request() -> AnalysisRequest {
        AnalysisRequest {
            daily_asset_range: None,
            web_search: false,
            user_message: None,
            question: "复盘指数集中风险".into(),
            workflow: "deep".into(),
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        }
    }

    struct Executor {
        captured: std::sync::Mutex<Vec<serde_json::Value>>,
        fail: bool,
    }
    #[async_trait::async_trait]
    impl AnalysisExecutor for Executor {
        async fn execute(
            &self,
            config: ModelConfig,
            api_key: Option<String>,
            request: AnalysisRequest,
            context: context::BuiltContext,
        ) -> AppResult<AnalysisResult> {
            assert_eq!(config.provider, ModelProviderKind::CodexBridge);
            assert_eq!(api_key.as_deref(), Some(BRIDGE_ID));
            assert_eq!(
                request.preview_revision.as_deref(),
                Some(context.revision.as_str())
            );
            self.captured.lock().unwrap().push(context.payload);
            if self.fail {
                return Err(AppError::Model("executor failure".into()));
            }
            Ok(serde_json::from_value(json!({
                "id": uuid::Uuid::new_v4().to_string(), "answer":"frozen answer", "stages":[],
                "createdAt":chrono::Utc::now().to_rfc3339(), "disclaimer":"test",
                "transparency":{"provider":config.provider, "model":config.model, "contextGroups":[], "payloadBytes":context.payload_bytes, "contextRevision":context.revision, "externalDataUsed":false, "apiKeySent":false}
            }))?)
        }
    }
    const BRIDGE_ID: &str = "bridge-fixture";
    fn database() -> Database {
        let db = Database::test();
        db.save_model_configuration(
            ModelProviderKind::CodexBridge,
            "codex://bridge",
            "fixture",
            None,
        )
        .unwrap();
        db.set_setting("model.codex_bridge_id", BRIDGE_ID).unwrap();
        db
    }
    struct DelayedExecutor {
        delay: std::time::Duration,
        dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }
    #[async_trait::async_trait]
    impl AnalysisExecutor for DelayedExecutor {
        async fn execute(
            &self,
            config: ModelConfig,
            key: Option<String>,
            request: AnalysisRequest,
            context: context::BuiltContext,
        ) -> AppResult<AnalysisResult> {
            struct Guard(std::sync::Arc<std::sync::atomic::AtomicBool>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let _guard = Guard(self.dropped.clone());
            tokio::time::sleep(self.delay).await;
            Executor {
                captured: Default::default(),
                fail: false,
            }
            .execute(config, key, request, context)
            .await
        }
    }
    #[tokio::test(start_paused = true)]
    async fn long_workflow_survives_five_minutes_but_deadline_cancels_without_saving() {
        for (seconds, success) in [(360, true), (26 * 60, false)] {
            let db = database();
            let service = AnalysisService::new(&db);
            let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let executor = DelayedExecutor {
                delay: std::time::Duration::from_secs(seconds),
                dropped: dropped.clone(),
            };
            let mut request = request();
            request.preview_revision = Some(service.preview(&request).unwrap().context_revision);
            let result = service.execute(request, &executor).await;
            assert_eq!(result.is_ok(), success);
            if !success {
                assert!(result.unwrap_err().to_string().contains("25 分钟"));
            }
            assert!(dropped.load(std::sync::atomic::Ordering::Relaxed));
            assert_eq!(db.analysis_history().unwrap().len(), usize::from(success));
            db.save_profile(&db.snapshot().unwrap().profile).unwrap();
        }
    }
    #[tokio::test]
    async fn stored_answers_never_enter_a_new_analysis_context() {
        let db = database();
        let service = AnalysisService::new(&db);
        let request = request();
        let before = service.preview(&request).unwrap();
        db.conn().unwrap().execute_batch("INSERT INTO analyses(id,question,answer,created_at) VALUES ('previous','复盘指数集中风险','private-history-marker','2026-10-17T00:00:00Z');").unwrap();
        let after = service.preview(&request).unwrap();
        assert_eq!(before.context_revision, after.context_revision);
        assert_eq!(before.payload, after.payload);
        let preview = serde_json::to_value(after).unwrap();
        assert!(preview.get("memoryCandidates").is_none());
        assert!(preview.get("memoryPolicy").is_none());
        assert!(!preview.to_string().contains("private-history-marker"));
        assert_eq!(
            db.analysis("previous").unwrap().answer,
            "private-history-marker"
        );
    }

    #[tokio::test]
    async fn execution_uses_the_preview_payload_and_persists_the_original_user_message() {
        let db = database();
        let service = AnalysisService::new(&db);
        let executor = Executor {
            captured: Default::default(),
            fail: false,
        };
        let mut request = request();
        request.user_message = Some("本轮原始问题".into());
        let preview = service.preview(&request).unwrap();
        request.preview_revision = Some(preview.context_revision);
        let result = service.execute(request, &executor).await.unwrap();
        assert_eq!(executor.captured.lock().unwrap()[0], preview.payload);
        assert_eq!(db.analysis(&result.id).unwrap().question, "本轮原始问题");
        assert_eq!(db.analysis_history().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn data_or_authorization_changes_reject_execution_before_contacting_the_executor() {
        let db = database();
        let service = AnalysisService::new(&db);
        let executor = Executor {
            captured: Default::default(),
            fail: false,
        };
        let mut request = request();
        request.preview_revision = Some(service.preview(&request).unwrap().context_revision);
        let mut changed = request.clone();
        changed.context_selection.include_profile = false;
        assert!(service.execute(changed, &executor).await.is_err());
        let mut profile = db.snapshot().unwrap().profile;
        profile.monthly_income += 100.0;
        db.save_profile(&profile).unwrap();
        assert!(service.execute(request, &executor).await.is_err());
        assert!(executor.captured.lock().unwrap().is_empty());
        assert!(db.analysis_history().unwrap().is_empty());
    }
    #[tokio::test]
    async fn failed_executor_creates_no_history_and_leaves_the_repository_usable() {
        let db = database();
        let service = AnalysisService::new(&db);
        let executor = Executor {
            captured: Default::default(),
            fail: true,
        };
        let mut request = request();
        request.preview_revision = Some(service.preview(&request).unwrap().context_revision);
        assert!(service
            .execute(request, &executor)
            .await
            .unwrap_err()
            .to_string()
            .contains("executor failure"));
        assert!(db.analysis_history().unwrap().is_empty());
        db.save_profile(&db.snapshot().unwrap().profile).unwrap();
    }
}
