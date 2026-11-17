//! Memory use cases: overview, user edits and dreaming with a frozen input.
use super::{
    analysis::frozen_credentials,
    model_settings::{configuration, ServerOpenAi},
};
#[cfg(test)]
use crate::db::Database;
use crate::{
    models::memory::{screen_operations, DreamInput, MemoryNote, MemoryOverview, DREAM_DUE_AFTER},
    ports::ai_account::MemoryAccount,
    ports::analysis::AnalysisExecutor,
    AppError, AppResult,
};

pub(crate) struct MemoryService<'a, A: MemoryAccount> {
    db: &'a A,
    server: Option<&'a ServerOpenAi>,
}

impl<'a, A: MemoryAccount> MemoryService<'a, A> {
    pub fn new(db: &'a A, server: Option<&'a ServerOpenAi>) -> Self {
        Self { db, server }
    }

    pub fn overview(&self) -> AppResult<MemoryOverview> {
        let enabled = self
            .db
            .analysis_settings()?
            .context_selection
            .include_memory;
        let (_, pending_analyses) = self.db.undreamed_analyses()?;
        Ok(MemoryOverview {
            items: self.db.memory_notes()?,
            dream: self.db.dream_state()?,
            pending_analyses,
            dream_due: enabled && pending_analyses >= DREAM_DUE_AFTER,
            enabled,
        })
    }

    pub fn set_status(&self, id: &str, active: bool) -> AppResult<MemoryNote> {
        self.db.set_memory_status(id, active)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.db.delete_memory(id)
    }

    /// Runs with the caller's account lock held; the lock is released only while the model runs.
    pub async fn dream(&self, executor: &dyn AnalysisExecutor) -> AppResult<MemoryOverview> {
        if !self
            .db
            .analysis_settings()?
            .context_selection
            .include_memory
        {
            return Err(AppError::Validation(
                "分析默认设置已关闭长期记忆，不会整理新的记忆".into(),
            ));
        }
        let (analyses, _) = self.db.undreamed_analyses()?;
        if analyses.is_empty() {
            return Err(AppError::Validation("暂无新的分析需要整理".into()));
        }
        let input = DreamInput {
            analyses,
            memories: self.db.active_memory_notes()?,
        };
        let watermark = self.db.dream_state()?.dreamed_through;
        let config = configuration(self.db, self.server)?;
        let api_key = frozen_credentials(self.db, self.server, &config)?;
        let outcome = self
            .db
            .without_account_lock(tokio::time::timeout(
                crate::models::inference::DREAM_TIMEOUT,
                executor.dream(config, api_key, input.clone()),
            ))
            .await?
            .unwrap_or_else(|_| {
                Err(AppError::Model(
                    crate::models::inference::DREAM_TIMEOUT_MESSAGE.into(),
                ))
            })?;
        // New analyses may arrive meanwhile and wait for the next batch; edited memories
        // or a concurrent dream would make the proposed operations stale.
        let current = DreamInput {
            analyses: input.analyses.clone(),
            memories: self.db.active_memory_notes()?,
        };
        if current.revision() != input.revision()
            || self.db.dream_state()?.dreamed_through != watermark
        {
            return Err(AppError::Conflict(
                "整理期间记忆已被修改或已有其他整理完成，请刷新后重试".into(),
            ));
        }
        let (accepted, rejected) = screen_operations(&input, &outcome.proposal);
        self.db.apply_dream(&input, &accepted, rejected)?;
        self.overview()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        analysis::OutputValidationTrace,
        connection::ModelConfig,
        memory::{DreamOperation, DreamOutcome, DreamProposal, MemoryKind, MemoryOrigin},
        provider::ModelProviderKind,
    };

    struct Dreamer {
        operations: Vec<DreamOperation>,
        edit_during_run: Option<std::sync::Arc<Database>>,
    }
    #[async_trait::async_trait]
    impl AnalysisExecutor for Dreamer {
        async fn execute(
            &self,
            _: ModelConfig,
            _: Option<String>,
            _: crate::models::analysis::AnalysisRequest,
            _: crate::context::BuiltContext,
        ) -> AppResult<crate::models::analysis::AnalysisResult> {
            unreachable!("dreaming never runs an analysis")
        }
        async fn dream(
            &self,
            config: ModelConfig,
            api_key: Option<String>,
            input: DreamInput,
        ) -> AppResult<DreamOutcome> {
            assert_eq!(config.provider, ModelProviderKind::CodexBridge);
            assert_eq!(api_key.as_deref(), Some("bridge-fixture"));
            assert_eq!(input.analyses.len(), 3);
            if let Some(db) = &self.edit_during_run {
                let id = db.memory_notes().unwrap()[0].id.clone();
                db.set_memory_status(&id, false).unwrap();
            }
            Ok(DreamOutcome {
                proposal: DreamProposal {
                    operations: self.operations.clone(),
                },
                calls: Vec::new(),
                validation: OutputValidationTrace {
                    status: "valid".into(),
                    attempts: 1,
                    errors: Vec::new(),
                    rejected_output: None,
                },
            })
        }
    }

    fn database() -> Database {
        let db = Database::test();
        db.save_model_configuration(
            ModelProviderKind::CodexBridge,
            "codex://bridge",
            "fixture",
            None,
        )
        .unwrap();
        db.set_setting("model.codex_bridge_id", "bridge-fixture")
            .unwrap();
        for (index, question) in ["我希望先看结论再看依据", "分红能持续吗", "现金流够吗"]
            .iter()
            .enumerate()
        {
            db.conn()
                .unwrap()
                .execute(
                    "INSERT INTO analyses(id,question,answer,created_at) VALUES ($1,$2,'回答',$3)",
                    crate::storage::params![
                        format!("a{index}"),
                        *question,
                        format!("2026-11-16T0{index}:00:00+00:00")
                    ],
                )
                .unwrap();
        }
        db
    }

    fn operations() -> Vec<DreamOperation> {
        vec![
            DreamOperation::Add {
                kind: MemoryKind::Preference,
                origin: MemoryOrigin::UserStated,
                statement: "用户倾向先看结论再看依据".into(),
                rationale: "用户原话".into(),
                source_analysis_ids: vec!["a0".into()],
                quote: "先看结论再看依据".into(),
            },
            DreamOperation::Add {
                kind: MemoryKind::Preference,
                origin: MemoryOrigin::ModelInferred,
                statement: "用户喜欢被肯定".into(),
                rationale: "模型猜测".into(),
                source_analysis_ids: vec!["a1".into()],
                quote: "分红能持续吗".into(),
            },
        ]
    }

    #[tokio::test]
    async fn dreaming_screens_operations_and_advances_once() {
        let db = database();
        let service = MemoryService::new(&db, None);
        let before = service.overview().unwrap();
        assert!(before.dream_due && before.pending_analyses == 3);
        let executor = Dreamer {
            operations: operations(),
            edit_during_run: None,
        };
        let after = service.dream(&executor).await.unwrap();
        assert_eq!(after.items.len(), 1);
        assert_eq!(after.items[0].statement, "用户倾向先看结论再看依据");
        let summary = after.dream.last_result.unwrap();
        assert_eq!((summary.added, summary.rejected.len()), (1, 1));
        assert!(!after.dream_due && after.pending_analyses == 0);
        assert!(matches!(
            service.dream(&executor).await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn disabled_memory_never_calls_the_model() {
        let db = database();
        let mut settings = db.analysis_settings().unwrap();
        settings.context_selection.include_memory = false;
        db.save_analysis_settings(settings).unwrap();
        let service = MemoryService::new(&db, None);
        assert!(!service.overview().unwrap().dream_due);
        let executor = Dreamer {
            operations: operations(),
            edit_during_run: None,
        };
        assert!(matches!(
            service.dream(&executor).await,
            Err(AppError::Validation(_))
        ));
        assert!(db.memory_notes().unwrap().is_empty());
    }

    #[tokio::test]
    async fn memories_edited_while_the_model_runs_reject_the_stale_proposal() {
        let db = std::sync::Arc::new(database());
        db.conn().unwrap().execute_batch(
            "INSERT INTO analysis_memories VALUES ('m1','preference','userStated','先讲结论','理由','[\"a0\"]','先看结论',1,'2026-11-15T00:00:00+00:00','2026-11-15T00:00:00+00:00')",
        ).unwrap();
        let service = MemoryService::new(db.as_ref(), None);
        let executor = Dreamer {
            operations: operations(),
            edit_during_run: Some(db.clone()),
        };
        assert!(matches!(
            service.dream(&executor).await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(db.memory_notes().unwrap().len(), 1);
        assert_eq!(db.undreamed_analyses().unwrap().1, 3);
    }
}
