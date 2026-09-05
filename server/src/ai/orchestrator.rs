use super::{ChatMessage, ModelProvider};
use crate::{
    context::{BuiltContext, INVESTMENT_SYSTEM_POLICY},
    error::AppResult,
    memory::MemoryRetriever,
    models::{AnalysisRequest, AnalysisResult, AnalysisTransparency, MemoryItem},
};

pub struct InvestmentOrchestrator<'a> {
    provider: &'a dyn ModelProvider,
    retriever: &'a dyn MemoryRetriever,
}

impl<'a> InvestmentOrchestrator<'a> {
    pub fn new(provider: &'a dyn ModelProvider, retriever: &'a dyn MemoryRetriever) -> Self {
        Self {
            provider,
            retriever,
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
        if built_context.evidence_items > 0 {
            stages.push("冻结带来源研究证据".into());
        }

        if request.workflow == "quick" {
            let answer = self.provider.complete(vec![
                ChatMessage::system(INVESTMENT_SYSTEM_POLICY),
                ChatMessage::user(format!("用户问题：{}\n\n本地投资档案：{}\n\n请给出结构化分析，并说明仍需核实的信息。", request.question, context)),
            ]).await?;
            stages.push(format!("{} 快速分析", self.provider.model_name()));
            return Ok(result(self.provider, answer, stages, built_context, &[]));
        }

        let plan = self.provider.complete(vec![
            ChatMessage::system(format!("{}\n你现在是研究规划模块，只制定分析计划和检索线索，不给最终结论。", INVESTMENT_SYSTEM_POLICY)),
            ChatMessage::user(format!("问题：{}\n已知档案摘要：{}\n请列出需要核实的假设、反方问题，以及用于检索历史决策的关键词。", request.question, built_context.payload)),
        ]).await?;
        stages.push("生成研究计划".into());

        let memories = if request.use_memory {
            let first = self.retriever.search(&request.question, memory_pool, 4);
            let second = self.retriever.search(&plan, memory_pool, 6);
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

        let draft_instruction = if request.explore_alternatives {
            stages.push("探索多个可行方案".into());
            "提出至少两个可行方案（包括保持不动），比较适用条件、主要风险、机会成本和需要验证的证据；不要为了凑数制造方案。"
        } else {
            "提出一个最稳健的分析框架，说明适用条件、风险和需要验证的证据。"
        };

        let draft = self
            .provider
            .complete(vec![
                ChatMessage::system(INVESTMENT_SYSTEM_POLICY),
                ChatMessage::user(format!(
                "用户问题：{}\n\n本地投资档案：{}\n\n研究计划：{}\n\n相关历史记忆：{}\n\n任务：{}",
                request.question, context, plan, memory_context, draft_instruction
            )),
            ])
            .await?;

        let critique = if request.reflect {
            stages.push("独立纠错反思".into());
            self.provider.complete(vec![
                ChatMessage::system(format!("{}\n你现在是独立风险审查模块。只找问题，不迎合上一位分析者。", INVESTMENT_SYSTEM_POLICY)),
                ChatMessage::user(format!("原问题：{}\n候选分析：{}\n请检查：事实与推断是否混淆、是否忽略极端风险、是否过度自信、是否给了隐性买卖指令、是否缺少更简单的基准方案。", request.question, draft)),
            ]).await?
        } else {
            "未启用独立反思。".into()
        };

        let answer = self.provider.complete(vec![
            ChatMessage::system(format!("{}\n你是最终整合模块。吸收审查意见，但要自行判断，不机械拼接。", INVESTMENT_SYSTEM_POLICY)),
            ChatMessage::user(format!(
                "原问题：{}\n\n档案：{}\n\n研究计划：{}\n\n候选分析：{}\n\n独立审查：{}\n\n请输出：①当前最重要判断；②风险与未知；③方案比较；④下一步行动；⑤未来复盘/证伪条件。明确指出本次没有接入的外部实时数据。",
                request.question, context, plan, draft, critique
            )),
        ]).await?;
        stages.push("综合结论与行动清单".into());
        Ok(result(
            self.provider,
            answer,
            stages,
            built_context,
            &memories,
        ))
    }
}

fn merge_memories(
    first: Vec<MemoryItem>,
    second: Vec<MemoryItem>,
    limit: usize,
) -> Vec<MemoryItem> {
    let mut result = Vec::new();
    for item in first.into_iter().chain(second) {
        if !result
            .iter()
            .any(|existing: &MemoryItem| existing.id == item.id)
        {
            result.push(item);
        }
        if result.len() == limit {
            break;
        }
    }
    result
}

fn result(
    provider: &dyn ModelProvider,
    answer: String,
    stages: Vec<String>,
    built_context: &BuiltContext,
    memory_items: &[MemoryItem],
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
            evidence_items_used: built_context.evidence_items,
            citations_required: built_context.evidence_items > 0,
            external_data_used: built_context.evidence_items > 0,
            api_key_sent: false,
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        disclaimer: "本分析用于投资教育与决策支持，不构成收益保证或针对具体证券的买卖建议。".into(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        context::ContextBuilder,
        error::AppResult,
        memory::LexicalMemoryRetriever,
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
        async fn complete(&self, _messages: Vec<ChatMessage>) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(match call {
                0 => "研究计划：检索指数和集中度".into(),
                1 => "候选方案 A 与 B".into(),
                2 => "审查：需要说明未知信息".into(),
                _ => "最终：先处理集中风险并设置复盘条件".into(),
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
        let retriever = LexicalMemoryRetriever;
        let memory = vec![MemoryItem {
            id: "m1".into(),
            kind: "decision".into(),
            title: "指数决策".into(),
            content: "集中度复盘".into(),
            created_at: "2026-01-01".into(),
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
        assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
        assert!(output
            .stages
            .iter()
            .any(|stage| stage.contains("多轮本地记忆检索")));
        assert!(output
            .stages
            .iter()
            .any(|stage| stage.contains("独立纠错反思")));
        assert!(output.answer.contains("最终"));
        assert_eq!(output.transparency.memory_items_used, 1);
        assert_eq!(output.transparency.evidence_items_used, 1);
        assert!(output.transparency.citations_required);
        assert!(output.transparency.external_data_used);
        assert!(!output.transparency.api_key_sent);
    }
}
