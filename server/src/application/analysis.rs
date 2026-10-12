//! Freeze authorized sources, validate the preview, execute, and persist the result.
use super::model_settings::configured_model_key;
use crate::{
    ai::AnalysisExecutor,
    context,
    db::Database,
    evidence::{EvidenceRetriever, LexicalEvidenceRetriever},
    memory::{HybridMemoryRetriever, MemoryRetriever},
    models::{
        AnalysisPreview, AnalysisRequest, AnalysisResult, MemoryItem, ModelConfig,
        ResearchEvidence, Snapshot,
    },
    AppError, AppResult,
};
use std::collections::HashSet;
pub(crate) struct AnalysisService<'a> {
    db: &'a Database,
}
struct PreparedAnalysis {
    config: ModelConfig,
    context: context::BuiltContext,
    memories: Vec<MemoryItem>,
    memory_candidates: Vec<MemoryItem>,
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
        let retriever = HybridMemoryRetriever::default();
        let memory_candidates = if request.use_memory {
            initial_memory_candidates(
                &retriever,
                &request.question,
                &self.db.memories()?,
                &request.excluded_memory_ids,
            )
        } else {
            Vec::new()
        };
        let memories = selected_memories(&memory_candidates);
        let rules = self.db.investment_rules()?;
        let system_reviews = self
            .db
            .system_reviews()?
            .into_iter()
            .take(8)
            .collect::<Vec<_>>();
        let portfolio_events = self
            .db
            .portfolio_events()?
            .into_iter()
            .take(100)
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
                portfolio_events: &portfolio_events,
                evidence_candidates: &evidence_candidates,
                memory_candidates: &memories,
            },
        );
        Ok(PreparedAnalysis {
            config,
            context: built_context,
            memories,
            memory_candidates,
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
            prepared.memory_candidates,
            prepared.evidence_candidates,
        ))
    }
    pub async fn execute(
        &self,
        request: AnalysisRequest,
        executor: &dyn AnalysisExecutor,
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
        if self.db.hosted() {
            crate::models::validate_endpoint(&config.base_url)?;
        }
        let api_key = if config.provider == "codex" {
            None
        } else {
            Some(configured_model_key(self.db, &config)?)
        };
        // The immutable authorized context is now frozen. Release the account lock
        // while the remote Agent runs so other devices can continue editing.
        self.db.unlock_account()?;
        let result = executor
            .execute(
                config,
                api_key,
                request.clone(),
                prepared.context,
                prepared.memories,
            )
            .await;
        self.db.lock_account()?;
        let result = result?;
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
    Ok(LexicalEvidenceRetriever.search(&query, &items, 30))
}

fn initial_memory_candidates(
    retriever: &dyn MemoryRetriever,
    question: &str,
    pool: &[MemoryItem],
    excluded_memory_ids: &[String],
) -> Vec<MemoryItem> {
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

fn selected_memories(candidates: &[MemoryItem]) -> Vec<MemoryItem> {
    candidates
        .iter()
        .filter(|item| item.selected)
        .cloned()
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContextSelection;
    use serde_json::json;
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
            kind: "analysis".into(),
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
        assert!(request(vec!["same".into(), "same".into()])
            .validate()
            .is_err());
        assert!(request(vec!["x".repeat(129)]).validate().is_err());
        assert!(request(vec!["memory-1".into()]).validate().is_ok());
    }

    struct Executor {
        captured: std::sync::Mutex<Vec<(serde_json::Value, Vec<MemoryItem>)>>,
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
            memories: Vec<MemoryItem>,
        ) -> AppResult<AnalysisResult> {
            assert_eq!(config.provider, "codex");
            assert!(api_key.is_none());
            assert_eq!(
                request.preview_revision.as_deref(),
                Some(context.revision.as_str())
            );
            self.captured
                .lock()
                .unwrap()
                .push((context.payload, memories));
            if self.fail {
                return Err(AppError::Model("executor failure".into()));
            }
            Ok(serde_json::from_value(json!({
                "id": uuid::Uuid::new_v4().to_string(), "answer":"frozen answer", "stages":[],
                "createdAt":chrono::Utc::now().to_rfc3339(), "disclaimer":"test",
                "transparency":{"provider":config.provider, "model":config.model, "contextGroups":[], "payloadBytes":context.payload_bytes, "contextRevision":context.revision, "memoryItemsUsed":0, "externalDataUsed":false, "apiKeySent":false}
            }))?)
        }
    }
    fn database() -> Database {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        db.save_model_metadata("codex", "codex://local", "fixture")
            .unwrap();
        db
    }
    #[tokio::test]
    async fn execution_uses_the_preview_payload_and_persists_the_original_user_message() {
        let db = database();
        let service = AnalysisService::new(&db);
        let executor = Executor {
            captured: Default::default(),
            fail: false,
        };
        let mut request = request(Vec::new());
        request.user_message = Some("本轮原始问题".into());
        let preview = service.preview(&request).unwrap();
        request.preview_revision = Some(preview.context_revision);
        let result = service.execute(request, &executor).await.unwrap();
        assert_eq!(executor.captured.lock().unwrap()[0].0, preview.payload);
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
        let mut request = request(Vec::new());
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
        let mut request = request(Vec::new());
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
