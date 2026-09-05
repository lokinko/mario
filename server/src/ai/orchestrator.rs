use std::time::Instant;

use super::{
    workflow::{AnalysisWorkflow, CandidateContext, InvestmentWorkflowV3, StagePrompt},
    ModelProvider,
};
use crate::{
    context::BuiltContext,
    error::AppResult,
    memory::MemoryRetriever,
    models::{
        AnalysisAlternative, AnalysisRequest, AnalysisResult, AnalysisTransparency,
        AnalysisWorkflowTrace, MemoryItem, ModelCallTrace,
    },
};

pub struct InvestmentOrchestrator<'a> {
    provider: &'a dyn ModelProvider,
    retriever: &'a dyn MemoryRetriever,
    workflow: &'a dyn AnalysisWorkflow,
}

static DEFAULT_WORKFLOW: InvestmentWorkflowV3 = InvestmentWorkflowV3;

impl<'a> InvestmentOrchestrator<'a> {
    pub fn new(provider: &'a dyn ModelProvider, retriever: &'a dyn MemoryRetriever) -> Self {
        Self::with_workflow(provider, retriever, &DEFAULT_WORKFLOW)
    }

    pub fn with_workflow(
        provider: &'a dyn ModelProvider,
        retriever: &'a dyn MemoryRetriever,
        workflow: &'a dyn AnalysisWorkflow,
    ) -> Self {
        Self {
            provider,
            retriever,
            workflow,
        }
    }

    pub async fn run(
        &self,
        request: &AnalysisRequest,
        built_context: &BuiltContext,
        memory_pool: &[MemoryItem],
    ) -> AppResult<AnalysisResult> {
        let context = serde_json::to_string_pretty(&built_context.payload)?;
        let mut stages = vec!["确定性风险检查".into(), "构建最小必要上下文".into()];
        let mut trace = AnalysisWorkflowTrace {
            version: self.workflow.version().into(),
            ..AnalysisWorkflowTrace::default()
        };
        if built_context.evidence_items > 0 {
            stages.push("冻结带来源研究证据".into());
        }

        if request.workflow == "quick" {
            let (answer, call) = self
                .call_stage(self.workflow.quick(&request.question, &context))
                .await?;
            trace.calls.push(call);
            stages.push(format!("{} 快速分析", self.provider.model_name()));
            return Ok(result(
                self.provider,
                answer,
                stages,
                built_context,
                &[],
                trace,
            ));
        }

        let (plan, call) = self
            .call_stage(
                self.workflow
                    .research_plan(&request.question, &built_context.payload),
            )
            .await?;
        trace.research_plan = Some(plan.clone());
        trace.calls.push(call);
        stages.push("生成研究计划".into());

        let memories = if request.use_memory {
            let mut first = self.retriever.search(&request.question, memory_pool, 4);
            mark_retrieval_pass(&mut first, "用户问题复核");
            let mut second = self.retriever.search(&plan, memory_pool, 6);
            mark_retrieval_pass(&mut second, "研究计划扩展");
            merge_memories(first, second, 8)
        } else {
            Vec::new()
        };
        if request.use_memory {
            stages.push("多轮本地记忆检索".into());
        }
        let memory_context = if memories.is_empty() {
            "没有检索到相关历史记录".into()
        } else {
            serde_json::to_string_pretty(&memories)?
        };
        trace.memory_items = memories.clone();

        if request.explore_alternatives {
            stages.push("探索多个可行方案".into());
        }
        let alternative_specs = self
            .workflow
            .alternative_specs(request.explore_alternatives);
        let candidate_context = CandidateContext {
            question: &request.question,
            local_context: &context,
            research_plan: &plan,
            memory_context: &memory_context,
        };

        for spec in alternative_specs {
            let (content, call) = self
                .call_stage(self.workflow.alternative(&candidate_context, &spec))
                .await?;
            trace.alternatives.push(AnalysisAlternative {
                id: spec.id.into(),
                label: spec.label.into(),
                lens: spec.lens.into(),
                content,
            });
            trace.calls.push(call);
        }

        let alternatives_context = serde_json::to_string_pretty(&trace.alternatives)?;

        if request.reflect {
            stages.push("独立纠错反思".into());
            let (critique, call) = self
                .call_stage(
                    self.workflow
                        .critique(&request.question, &alternatives_context),
                )
                .await?;
            trace.critique = Some(critique);
            trace.calls.push(call);
        }

        let critique_context = trace
            .critique
            .as_deref()
            .unwrap_or("用户未启用独立风险审查。");
        let (answer, call) = self
            .call_stage(self.workflow.synthesis(
                &request.question,
                &context,
                &plan,
                &alternatives_context,
                critique_context,
            ))
            .await?;
        trace.calls.push(call);
        stages.push("综合结论与行动清单".into());
        Ok(result(
            self.provider,
            answer,
            stages,
            built_context,
            &memories,
            trace,
        ))
    }

    async fn call_stage(&self, prompt: StagePrompt) -> AppResult<(String, ModelCallTrace)> {
        let started = Instant::now();
        let completion = self.provider.complete(prompt.messages).await?;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok((
            completion.content,
            ModelCallTrace {
                stage: prompt.key.into(),
                label: prompt.label.into(),
                latency_ms,
                input_tokens: completion.usage.input_tokens,
                output_tokens: completion.usage.output_tokens,
            },
        ))
    }
}

fn merge_memories(
    first: Vec<MemoryItem>,
    second: Vec<MemoryItem>,
    limit: usize,
) -> Vec<MemoryItem> {
    let mut result: Vec<MemoryItem> = Vec::new();
    for item in first.into_iter().chain(second) {
        if let Some(existing) = result.iter_mut().find(|existing| existing.id == item.id) {
            merge_retrieval(existing, &item);
        } else {
            result.push(item);
        }
        if result.len() == limit {
            break;
        }
    }
    result
}

fn mark_retrieval_pass(items: &mut [MemoryItem], pass: &str) {
    for item in items {
        if let Some(retrieval) = &mut item.retrieval {
            if !retrieval.passes.iter().any(|existing| existing == pass) {
                retrieval.passes.push(pass.into());
            }
        }
    }
}

fn merge_retrieval(existing: &mut MemoryItem, incoming: &MemoryItem) {
    let (Some(current), Some(next)) = (&mut existing.retrieval, &incoming.retrieval) else {
        return;
    };
    current.score = current.score.max(next.score);
    current.age_days = current.age_days.min(next.age_days);
    for reason in &next.reasons {
        if !current.reasons.contains(reason) {
            current.reasons.push(reason.clone());
        }
    }
    for pass in &next.passes {
        if !current.passes.contains(pass) {
            current.passes.push(pass.clone());
        }
    }
    if current.passes.len() > 1
        && !current
            .reasons
            .iter()
            .any(|reason| reason == "原问题与研究计划均命中")
    {
        current.reasons.push("原问题与研究计划均命中".into());
    }
}

fn result(
    provider: &dyn ModelProvider,
    answer: String,
    stages: Vec<String>,
    built_context: &BuiltContext,
    memory_items: &[MemoryItem],
    workflow_trace: AnalysisWorkflowTrace,
) -> AnalysisResult {
    let mut context_groups: Vec<_> = built_context
        .groups
        .iter()
        .filter(|group| group.included)
        .map(|group| group.label.clone())
        .collect();
    if !memory_items.is_empty() {
        context_groups.push("相关历史记忆".into());
    }
    let total_latency_ms = workflow_trace
        .calls
        .iter()
        .map(|call| call.latency_ms)
        .sum();
    let input_tokens = sum_known_tokens(workflow_trace.calls.iter().map(|call| call.input_tokens));
    let output_tokens =
        sum_known_tokens(workflow_trace.calls.iter().map(|call| call.output_tokens));
    let model_calls = workflow_trace.calls.len();
    AnalysisResult {
        id: uuid::Uuid::new_v4().to_string(),
        answer,
        stages,
        transparency: AnalysisTransparency {
            provider: provider.provider_name().into(),
            model: provider.model_name().into(),
            context_groups,
            payload_bytes: built_context.payload_bytes
                + serde_json::to_vec(memory_items).map_or(0, |bytes| bytes.len()),
            context_revision: built_context.revision.clone(),
            memory_items_used: memory_items.len(),
            reviewed_memory_items_used: memory_items.iter().filter(|item| item.reviewed).count(),
            conflicting_memory_items_used: memory_items
                .iter()
                .filter(|item| item.contradiction)
                .count(),
            evidence_items_used: built_context.evidence_items,
            citations_required: built_context.evidence_items > 0,
            model_calls,
            total_latency_ms,
            input_tokens,
            output_tokens,
            external_data_used: built_context.evidence_items > 0,
            api_key_sent: false,
        },
        workflow_trace,
        created_at: chrono::Utc::now().to_rfc3339(),
        disclaimer: "本分析用于投资教育与决策支持，不构成收益保证或针对具体证券的买卖建议。".into(),
    }
}

fn sum_known_tokens(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    let mut count = 0;
    let mut total = 0;
    for value in values {
        total += value?;
        count += 1;
    }
    (count > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        ai::ChatMessage,
        context::ContextBuilder,
        error::AppResult,
        memory::HybridMemoryRetriever,
        models::{
            ContextSelection, FinancialProfile, Holding, PortfolioPlan, ResearchEvidence,
            RiskFinding, Snapshot,
        },
    };

    struct MockProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn complete(
            &self,
            _messages: Vec<ChatMessage>,
        ) -> AppResult<super::super::ModelCompletion> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let content = match call {
                0 => "研究计划：检索指数和集中度".into(),
                1 => "稳健基准：先不行动".into(),
                2 => "目标方案：逐步降低集中度".into(),
                3 => "审查：需要说明未知信息".into(),
                _ => "最终：先处理集中风险并设置复盘条件".into(),
            };
            Ok(super::super::ModelCompletion {
                content,
                usage: super::super::ModelUsage {
                    input_tokens: Some(100),
                    output_tokens: Some(20),
                },
            })
        }

        fn model_name(&self) -> &str {
            "mock"
        }

        fn provider_name(&self) -> &str {
            "mock-provider"
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            profile: FinancialProfile::default(),
            goals: Vec::new(),
            holdings: vec![Holding {
                id: "h1".into(),
                symbol: "IDX".into(),
                name: "指数".into(),
                asset_class: "基金".into(),
                market_value: 100.0,
                cost_basis: 90.0,
                target_pct: 100.0,
                currency: "CNY".into(),
            }],
            findings: vec![RiskFinding {
                level: "medium".into(),
                title: "集中度".into(),
                detail: "测试".into(),
                action: "复核".into(),
            }],
            total_value: 100.0,
            emergency_months: 6.0,
            concentration_pct: 100.0,
            plan: PortfolioPlan {
                monthly_surplus: 0.0,
                committed_monthly: 0.0,
                modeled_annual_return_pct: 5.5,
                modeled_annual_volatility_pct: 14.0,
                stress_loss_pct: 28.0,
                risk_capacity_pct: 15.0,
                risk_status: "over".into(),
                goal_projections: Vec::new(),
                rebalancing: Vec::new(),
                assumptions: "测试".into(),
            },
            updated_at: "2026-01-01".into(),
        }
    }

    #[tokio::test]
    async fn deep_workflow_runs_plan_retrieval_exploration_reflection_and_synthesis() {
        let provider = MockProvider {
            calls: AtomicUsize::new(0),
        };
        let retriever = HybridMemoryRetriever::default();
        let memory = vec![MemoryItem {
            id: "m1".into(),
            kind: "decision".into(),
            title: "指数决策".into(),
            summary: "集中度复盘".into(),
            content: serde_json::json!({ "lesson": "集中度复盘" }),
            created_at: "2026-01-01".into(),
            occurred_at: "2026-01-01".into(),
            status: "已复盘".into(),
            reviewed: true,
            contradiction: false,
            tags: vec!["指数".into(), "集中度".into()],
            retrieval: None,
        }];
        let request = AnalysisRequest {
            question: "检查指数集中度".into(),
            workflow: "deep".into(),
            use_memory: true,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        };
        let snapshot = snapshot();
        let evidence = vec![ResearchEvidence {
            id: "e1".into(),
            asset_name: "指数".into(),
            title: "指数方法说明".into(),
            publisher: "指数公司".into(),
            source_url: "https://example.com/index".into(),
            source_tier: "一手来源".into(),
            evidence_type: "公司披露".into(),
            stance: "背景".into(),
            as_of_date: "2026-01-01".into(),
            claim: "指数采用公开方法编制".into(),
            notes: String::new(),
            active: true,
            captured_at: "2026-01-01".into(),
        }];
        let context = ContextBuilder::build(&request, &snapshot, &[], &[], &evidence, &memory);
        let output = InvestmentOrchestrator::new(&provider, &retriever)
            .run(&request, &context, &memory)
            .await
            .unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
        assert!(output
            .stages
            .iter()
            .any(|stage| stage.contains("多轮本地记忆检索")));
        assert!(output
            .stages
            .iter()
            .any(|stage| stage.contains("独立纠错反思")));
        assert!(output.answer.contains("最终"));
        assert_eq!(output.workflow_trace.alternatives.len(), 2);
        assert!(output.workflow_trace.research_plan.is_some());
        assert_eq!(output.workflow_trace.memory_items.len(), 1);
        assert_eq!(
            output.workflow_trace.memory_items[0]
                .retrieval
                .as_ref()
                .unwrap()
                .passes,
            vec!["用户问题复核", "研究计划扩展"]
        );
        assert!(output.workflow_trace.critique.is_some());
        assert_eq!(output.workflow_trace.calls.len(), 5);
        assert_eq!(output.transparency.model_calls, 5);
        assert_eq!(output.transparency.input_tokens, Some(500));
        assert_eq!(output.transparency.output_tokens, Some(100));
        assert_eq!(output.transparency.memory_items_used, 1);
        assert_eq!(output.transparency.reviewed_memory_items_used, 1);
        assert_eq!(output.transparency.conflicting_memory_items_used, 0);
        assert_eq!(output.transparency.evidence_items_used, 1);
        assert!(output.transparency.citations_required);
        assert!(output.transparency.external_data_used);
        assert!(!output.transparency.api_key_sent);
    }

    #[tokio::test]
    async fn quick_workflow_is_honest_about_using_a_single_model_call() {
        let provider = MockProvider {
            calls: AtomicUsize::new(0),
        };
        let retriever = HybridMemoryRetriever::default();
        let request = AnalysisRequest {
            question: "只做快速风险摘要".into(),
            workflow: "quick".into(),
            use_memory: true,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        };
        let snapshot = snapshot();
        let context = ContextBuilder::build(&request, &snapshot, &[], &[], &[], &[]);
        let output = InvestmentOrchestrator::new(&provider, &retriever)
            .run(&request, &context, &[])
            .await
            .unwrap();

        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(output.transparency.model_calls, 1);
        assert_eq!(output.workflow_trace.calls.len(), 1);
        assert!(output.workflow_trace.research_plan.is_none());
        assert!(output.workflow_trace.alternatives.is_empty());
        assert!(output.workflow_trace.critique.is_none());
        assert!(!output
            .stages
            .iter()
            .any(|stage| stage.contains("本地记忆检索")));
    }
}
