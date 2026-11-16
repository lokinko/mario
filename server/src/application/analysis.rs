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
        let methodology = self.db.investment_methodology()?;
        let snapshot = self.db.snapshot()?;
        let evidence_candidates = relevant_evidence(self.db, request, &snapshot)?;
        let built_context = context::ContextBuilder::build(
            request,
            &snapshot,
            &context::ContextSources {
                methodology: Some(&methodology.prompt),
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
        self.db.save_analysis(&result, &request.question)?;
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
    let board = db.watch_board(
        chrono::Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive(),
    )?;
    // Resolve the subject from user questions only. A newly named company wins
    // over older subjects, and unrelated holdings cannot consume its evidence slots.
    let mut companies = board
        .rows
        .iter()
        .map(|row| (row.entry.input.name.clone(), row.entry.input.symbol.clone()))
        .collect::<Vec<_>>();
    if request.context_selection.include_holdings {
        companies.extend(
            snapshot
                .holdings
                .iter()
                .map(|h| (h.name.clone(), h.symbol.clone())),
        );
    }
    companies.extend(
        items
            .iter()
            .filter(|item| !item.asset_name.trim().is_empty())
            .map(|item| (item.asset_name.clone(), String::new())),
    );
    let mut questions = context::discussion::retrieval_questions(request);
    if context::discussion::is_general_finance_question(&request.question) {
        questions.truncate(1);
    }
    let subject_question = questions.iter().find(|question| {
        !context::discussion::explicit_ashare_symbols(question).is_empty()
            || companies
                .iter()
                .any(|(name, symbol)| context::discussion::mentions_company(question, name, symbol))
    });
    if let Some(question) = subject_question {
        companies.extend(
            context::discussion::explicit_ashare_symbols(question)
                .into_iter()
                .map(|symbol| (String::new(), symbol)),
        );
    }
    let subjects = companies
        .into_iter()
        .filter(|(name, symbol)| {
            subject_question.is_some_and(|question| {
                context::discussion::mentions_company(question, name, symbol)
            })
        })
        .collect::<Vec<_>>();
    let query = if let Some(subject) = subject_question {
        format!("{} {}", request.question, subject)
    } else {
        let holdings = snapshot
            .holdings
            .iter()
            .filter(|_| request.context_selection.include_holdings)
            .map(|h| format!("{} {}", h.name, h.symbol))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {}", request.question, holdings)
    };
    if !subjects.is_empty() {
        items.retain(|item| {
            let attachments = item
                .attachments
                .iter()
                .map(|file| format!("{} {}", file.name, file.extracted_text))
                .collect::<Vec<_>>()
                .join(" ");
            let identity = format!(
                "{} {} {} {} {}",
                item.asset_name, item.title, item.claim, item.notes, attachments
            );
            subjects.iter().any(|(name, symbol)| {
                context::discussion::mentions_company(&identity, name, symbol)
            })
        });
    }
    let mut watch = Vec::new();
    for row in board.rows {
        if !subjects.iter().any(|(name, symbol)| {
            (!symbol.is_empty() && *symbol == row.entry.input.symbol)
                || *name == row.entry.input.name
        }) {
            continue;
        }
        let Some(observation) = row.latest else {
            continue;
        };
        let Some(source_url) = observation.sources.first() else {
            continue;
        };
        let week = board
            .weekly
            .iter()
            .filter(|v| v.symbol == observation.symbol)
            .collect::<Vec<_>>();
        let mut saved_rules = Vec::new();
        let week_rows = week
            .iter()
            .map(|v| {
                let index = saved_rules
                    .iter()
                    .position(|rules| *rules == v.rules)
                    .unwrap_or_else(|| {
                        saved_rules.push(v.rules);
                        saved_rules.len() - 1
                    });
                serde_json::json!([
                    v.observed_on,
                    v.status,
                    v.price,
                    v.yield_pct,
                    v.daily.as_ref().and_then(|b| b.distance_pct),
                    v.weekly.as_ref().and_then(|b| b.distance_pct),
                    index
                ])
            })
            .collect::<Vec<_>>();
        let rule_rows = saved_rules
            .iter()
            .map(|r| {
                serde_json::json!([
                    r.min_yield_bps,
                    r.daily.reference,
                    r.daily.mode,
                    r.daily.offset_bps,
                    r.weekly.reference,
                    r.weekly.mode,
                    r.weekly.offset_bps
                ])
            })
            .collect::<Vec<_>>();
        let model_facts = serde_json::json!({
            "symbol":observation.symbol,"observedOn":observation.observed_on,"rules":observation.rules,
            "yieldMatchesRule":observation.yield_matches_rule,"price":observation.price,
            "dividendPerShare":observation.dividend_per_share,"yieldPct":observation.yield_pct,
            "dividendBasis":observation.dividend_basis,"dividendEvents":observation.dividend_events,
            "daily":observation.daily,"weekly":observation.weekly,"status":observation.status,"issues":observation.issues,
            "thisWeek":{
                "columns":["date","status","price","yieldPct","dailyDistancePct","weeklyDistancePct","ruleIndex"],
                "rows":week_rows,
                "ruleColumns":["minYieldBps","dailyReference","dailyMode","dailyOffsetBps","weeklyReference","weeklyMode","weeklyOffsetBps"],
                "rules":rule_rows
            }
        });
        let facts = serde_json::json!({
            "kind":"mario_watch_observation_v1",
            "modelFacts":model_facts,
            "originalFacts":{"latest":observation,"thisWeek":week}
        });
        watch.push(ResearchEvidence {
            id: format!("watch-{}-{}",observation.symbol,observation.observed_on),
            asset_name: row.entry.input.name.clone(), title: format!("{} 收盘买点观察",row.entry.input.name),
            publisher: "Yahoo Finance / 东方财富；Mario 确定性计算".into(), source_url: source_url.clone(),
            source_tier: "二手来源".into(), evidence_type: "watch_observation".into(), stance: "neutral".into(),
            as_of_date: observation.observed_on.clone(),
            claim: serde_json::to_string(&facts)?,
            notes: format!("当前规则：{}。历史记录按各自保存的规则解释。技术价格为供应商调整收盘归一至当前价格，周线含未结束周。信号不证明低估、基本面稳定或分红持续性；公司分红不等于个人到账。原始来源：{}",observation.rules.description(),observation.sources.join("\n")), active: true, captured_at: observation.checked_at, attachments: Vec::new(),
        });
    }
    watch.truncate(crate::evidence::ANALYSIS_EVIDENCE_LIMIT);
    watch.extend(search_evidence(
        &query,
        &items,
        crate::evidence::ANALYSIS_EVIDENCE_LIMIT - watch.len(),
    ));
    Ok(watch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContextSelection;
    use serde_json::json;
    #[test]
    fn watch_evidence_freezes_the_week_and_respects_research_authorization() {
        use crate::models::watch::{evaluate, tests::market_fixture, WatchInput};
        let db = database();
        let entry = db
            .save_watch(WatchInput {
                symbol: "600036.SS".into(),
                name: "观察公司".into(),
                thesis: String::new(),
            })
            .unwrap();
        let today = chrono::Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let (data, day) = market_fixture();
        let mut observation = evaluate(
            &Default::default(),
            "600036.SS",
            &data,
            day,
            chrono::Utc::now().to_rfc3339(),
        );
        observation.observed_on = today.to_string();
        db.save_watch_observation(
            &entry,
            Some(&observation),
            &observation.checked_at,
            today,
            None,
        )
        .unwrap();
        let mut request = request();
        request.question = "观察公司 600036.SS 本周复盘".into();
        let service = AnalysisService::new(&db);
        let preview = service.prepare(&request).unwrap();
        assert_eq!(preview.evidence_candidates.len(), 1);
        let evidence = &preview.evidence_candidates[0];
        assert!(evidence.claim.contains("thisWeek"));
        assert!(evidence
            .notes
            .contains(&crate::models::watch::rules::WatchRules::default().description()));
        let facts: serde_json::Value = serde_json::from_str(&evidence.claim).unwrap();
        assert_eq!(facts["modelFacts"]["weekly"]["reference"], "middle");
        assert!(evidence.claim.contains(&today.to_string()));
        let excerpt = observation_excerpt(evidence);
        assert!(excerpt.chars().count() <= 2_000);
        let excerpt: serde_json::Value = serde_json::from_str(&excerpt).unwrap();
        assert_eq!(excerpt["thisWeek"]["rows"][0][0], today.to_string());
        request.context_selection.include_evidence = false;
        assert!(service
            .prepare(&request)
            .unwrap()
            .evidence_candidates
            .is_empty());
    }
    fn observation_excerpt(evidence: &ResearchEvidence) -> String {
        let reference = serde_json::from_value(serde_json::to_value(evidence).unwrap()).unwrap();
        context::observation_excerpt(&reference).unwrap()
    }

    fn save_watch_fixture(db: &Database, symbol: &str, name: &str, many_dividends: bool) {
        use crate::models::watch::{evaluate, tests::market_fixture, DividendEvent, WatchInput};
        let entry = db
            .save_watch(WatchInput {
                symbol: symbol.into(),
                name: name.into(),
                thesis: String::new(),
            })
            .unwrap();
        let today = chrono::Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let (mut data, day) = market_fixture();
        if many_dividends {
            data.dividends = Some(
                (0..50)
                    .map(|index| DividendEvent {
                        report_date: Some(day - chrono::Duration::days(index * 3 + 1)),
                        ex_date: day - chrono::Duration::days(index * 3),
                        cash_per_ten: "0.123456789".into(),
                        bonus_per_ten: "0".into(),
                        transfer_per_ten: "0".into(),
                    })
                    .collect(),
            );
        }
        let mut observation = evaluate(
            &Default::default(),
            symbol,
            &data,
            day,
            chrono::Utc::now().to_rfc3339(),
        );
        observation.observed_on = today.to_string();
        db.save_watch_observation(
            &entry,
            Some(&observation),
            &observation.checked_at,
            today,
            None,
        )
        .unwrap();
    }

    #[test]
    fn follow_ups_retrieve_the_latest_user_subject_and_ignore_model_subjects() {
        let db = database();
        save_watch_fixture(&db, "600036.SS", "观察甲公司", false);
        save_watch_fixture(&db, "600519.SS", "观察乙公司", false);
        let service = AnalysisService::new(&db);
        let mut request = request();
        request.question = "分红能持续吗？".into();
        request.previous_turn = Some(crate::models::PreviousTurn {
            question: "再说下风险".into(),
            answer: "观察乙公司值得考虑".into(),
            suggestions: Vec::new(),
            earlier_questions: vec!["请讨论观察甲公司600036".into()],
            pending_question: None,
        });
        let preview = service.preview(&request).unwrap();
        assert_eq!(preview.evidence_candidates.len(), 1);
        assert_eq!(preview.evidence_candidates[0].asset_name, "观察甲公司");
        assert!(preview.payload["researchEvidence"][0]["claim"]
            .as_str()
            .unwrap()
            .contains("600036.SS"));
        request
            .previous_turn
            .as_mut()
            .unwrap()
            .earlier_questions
            .push("换成观察乙公司".into());
        assert_eq!(
            service.preview(&request).unwrap().evidence_candidates[0].asset_name,
            "观察乙公司"
        );
        request.question = "观察甲公司600036.SH现在如何？".into();
        assert_eq!(
            service.preview(&request).unwrap().evidence_candidates[0].asset_name,
            "观察甲公司"
        );
        request.question = "改看000001分红".into();
        let unknown_company = service.preview(&request).unwrap();
        assert!(unknown_company.evidence_candidates.is_empty());
        assert_eq!(unknown_company.payload["researchEvidence"], json!([]));
        request.question = "应急金怎么补？".into();
        assert!(service
            .preview(&request)
            .unwrap()
            .evidence_candidates
            .is_empty());
        request.context_selection.include_evidence = false;
        let hidden = service.preview(&request).unwrap();
        assert!(hidden.evidence_candidates.is_empty());
        assert!(hidden.payload.get("researchEvidence").is_none());
    }

    #[test]
    fn explicit_company_evidence_is_not_displaced_by_holdings_background() {
        let db = database();
        save_watch_fixture(&db, "600036.SS", "观察甲公司", false);
        let target = db
            .add_research_evidence(&crate::models::ResearchEvidenceInput {
                content: "观察甲公司600036 年报：分红持续性取决于现金流。".into(),
                attachments: Vec::new(),
            })
            .unwrap();
        for index in 0..8 {
            db.add_research_evidence(&crate::models::ResearchEvidenceInput {
                content: format!("虚构成长基金 FUND-A 虚构债券基金 BOND-B 虚构海外基金 USD-C 分红持续性 资料{index}"),
                attachments: Vec::new(),
            }).unwrap();
        }
        let snapshot =
            serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/snapshot.json"))
                .unwrap();
        let mut request = request();
        request.question = "观察甲公司600036分红持续性如何？".into();
        let selected = relevant_evidence(&db, &request, &snapshot).unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|item| item.id == target.id));
        assert!(!selected
            .iter()
            .any(|item| item.claim.contains("虚构成长基金")));
    }

    #[test]
    fn long_dividend_sources_keep_complete_audit_and_latest_boll_and_week_in_payload() {
        let db = database();
        save_watch_fixture(&db, "600036.SS", "观察甲公司", true);
        let service = AnalysisService::new(&db);
        let mut request = request();
        request.question = "600036本周复盘与分红持续性".into();
        let prepared = service.prepare(&request).unwrap();
        let full = &prepared.context.evidence_catalog[0].claim;
        let audit: serde_json::Value = serde_json::from_str(full).unwrap();
        assert_eq!(
            audit["originalFacts"]["latest"]["dividendEvents"]
                .as_array()
                .unwrap()
                .len(),
            50
        );
        assert!(full.chars().count() > 4_000);
        let excerpt = prepared.context.payload["researchEvidence"][0]["claim"]
            .as_str()
            .unwrap();
        assert!(excerpt.chars().count() <= 2_000);
        let compact: serde_json::Value = serde_json::from_str(excerpt).unwrap();
        assert_eq!(compact["daily"], audit["originalFacts"]["latest"]["daily"]);
        assert_eq!(
            compact["weekly"],
            audit["originalFacts"]["latest"]["weekly"]
        );
        assert_eq!(compact["rules"], audit["originalFacts"]["latest"]["rules"]);
        assert_eq!(compact["thisWeek"]["rows"].as_array().unwrap().len(), 1);
        assert!(compact["dividendEventsOmitted"].as_u64().unwrap() > 0);
        assert_eq!(compact["dividendEventCount"], 50);
        assert!(full.contains("cashPerTen"));
    }

    fn request() -> AnalysisRequest {
        AnalysisRequest {
            web_search: false,
            previous_turn: None,
            question: "复盘指数集中风险".into(),
            workflow: "quick".into(),

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
    #[tokio::test]
    async fn methodology_is_frozen_and_changes_invalidate_the_preview() {
        use crate::models::methodology::InvestmentMethodologyInput;
        let db = database();
        let service = AnalysisService::new(&db);
        let mut request = request();
        let before = service.preview(&request).unwrap();
        db.save_investment_methodology(InvestmentMethodologyInput {
            prompt: "优先比较管理成本，使用表格解释".into(),
        })
        .unwrap();
        request.preview_revision = Some(before.context_revision);
        let executor = Executor {
            captured: Default::default(),
            fail: false,
        };
        assert!(matches!(
            service.execute(request.clone(), &executor).await,
            Err(AppError::Validation(_))
        ));
        assert!(executor.captured.lock().unwrap().is_empty());
        let after = service.preview(&request).unwrap();
        assert_eq!(
            after.payload["investmentMethodology"],
            "优先比较管理成本，使用表格解释"
        );
        assert!(after
            .system_policy
            .contains("优先比较管理成本，使用表格解释"));
        request.preview_revision = Some(after.context_revision);
        service.execute(request, &executor).await.unwrap();
        assert_eq!(
            executor.captured.lock().unwrap()[0]["investmentMethodology"],
            "优先比较管理成本，使用表格解释"
        );
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
    async fn execution_uses_the_preview_payload_and_persists_only_the_current_question() {
        let db = database();
        let service = AnalysisService::new(&db);
        let executor = Executor {
            captured: Default::default(),
            fail: false,
        };
        let mut request = request();
        request.question = "本轮原始问题".into();
        request.previous_turn = Some(crate::models::PreviousTurn {
            question: "上一轮问题".into(),
            answer: "上一轮结论".into(),
            suggestions: vec!["上一轮动作".into()],
            earlier_questions: Vec::new(),
            pending_question: None,
        });
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
