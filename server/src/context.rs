use serde_json::{json, Map, Value};

use crate::models::{
    AnalysisPreview, AnalysisRequest, ContextGroup, ContextSelection, InvestmentRule, MemoryItem,
    Snapshot, SystemReviewRecord,
};

pub const INVESTMENT_SYSTEM_POLICY: &str = r#"
你是“知衡”的投资决策教练。你的任务不是荐股或承诺收益，而是提升用户的决策质量。
必须遵守：
1. 先检查财务安全垫、期限、流动性、负债、集中度和永久损失风险，再讨论潜在收益。
2. 明确区分：用户提供的事实、合理推断、未知信息。缺少实时市场数据时严禁编造。
3. 使用概率和情景，不用“一定”“稳赚”等确定性语言。
4. 区分资产价格与内在价值，区分好结果与好过程。
5. 给出可执行的检查项、证伪条件与复盘节点；避免直接下达买卖指令。
6. 输出中文，清楚、克制，优先解释为什么。
7. 用户的个人投资规则与周期复盘是待检验的长期约束：检查是否被违反，但不得静默替用户改写规则。
"#;

#[derive(Debug, Clone)]
pub struct BuiltContext {
    pub payload: Value,
    pub groups: Vec<ContextGroup>,
    pub payload_bytes: usize,
    pub revision: String,
}

pub struct ContextBuilder;

impl ContextBuilder {
    pub fn build(
        request: &AnalysisRequest,
        snapshot: &Snapshot,
        rules: &[InvestmentRule],
        system_reviews: &[SystemReviewRecord],
        memory_candidates: &[MemoryItem],
    ) -> BuiltContext {
        let mut payload = Map::new();
        payload.insert("question".into(), json!(request.question));

        let selection = &request.context_selection;
        if selection.include_profile {
            payload.insert(
                "financialProfile".into(),
                json!({
                    "profile": snapshot.profile,
                    "emergencyMonths": snapshot.emergency_months,
                }),
            );
        }
        if selection.include_goals {
            payload.insert("goals".into(), json!(snapshot.goals));
        }
        if selection.include_holdings {
            payload.insert(
                "portfolio".into(),
                json!({
                    "holdings": snapshot.holdings,
                    "totalValue": snapshot.total_value,
                    "concentrationPct": snapshot.concentration_pct,
                }),
            );
        }
        if selection.include_planning {
            payload.insert("deterministicPlan".into(), json!(snapshot.plan));
        }
        if selection.include_risk_findings {
            payload.insert("riskFindings".into(), json!(snapshot.findings));
        }
        if selection.include_rules {
            payload.insert(
                "personalInvestmentRules".into(),
                json!(rules.iter().filter(|rule| rule.active).collect::<Vec<_>>()),
            );
        }
        if selection.include_system_reviews {
            payload.insert("periodicSystemReviews".into(), json!(system_reviews));
        }

        let groups = context_groups(selection, snapshot, rules, system_reviews, request);
        let payload = Value::Object(payload);
        let serialized = serde_json::to_vec(&payload).unwrap_or_default();
        let payload_bytes = serialized.len();
        let mut revision_input = serialized.clone();
        revision_input.extend(serde_json::to_vec(memory_candidates).unwrap_or_default());
        let revision = context_revision(&revision_input);
        BuiltContext {
            payload,
            groups,
            payload_bytes,
            revision,
        }
    }

    pub fn preview(
        request: &AnalysisRequest,
        context: BuiltContext,
        provider: String,
        model: String,
        memory_candidates: Vec<MemoryItem>,
    ) -> AnalysisPreview {
        let memory_enabled = request.workflow == "deep" && request.use_memory;
        let mut groups = context.groups.clone();
        groups.push(ContextGroup {
            key: "memory".into(),
            label: "相关历史记忆".into(),
            included: memory_enabled,
            record_count: if memory_enabled {
                memory_candidates.len()
            } else {
                0
            },
            sensitivity: "中".into(),
            description: if memory_enabled {
                "先按问题检索，再按模型生成的研究计划二次检索；去重后最多发送 8 条。".into()
            } else if request.workflow == "quick" && request.use_memory {
                "快速工作流不会读取历史记忆。".into()
            } else {
                "本次已关闭历史记忆。".into()
            },
        });
        let omitted: Vec<_> = groups
            .iter()
            .filter(|group| !group.included)
            .map(|group| group.label.clone())
            .collect();
        let mut local_only = vec![
            "模型 API Key（只用于 HTTP Authorization，不进入提示词）".into(),
            "SQLite 文件路径与内部数据库标识".into(),
        ];
        if !omitted.is_empty() {
            local_only.push(format!("本次未选择的数据组：{}", omitted.join("、")));
        }
        AnalysisPreview {
            provider,
            model,
            workflow: request.workflow.clone(),
            groups,
            memory_candidates: memory_candidates.clone(),
            payload: context.payload,
            payload_bytes: context.payload_bytes
                + serde_json::to_vec(&memory_candidates).map_or(0, |bytes| bytes.len()),
            context_revision: context.revision,
            memory_policy: if memory_enabled {
                "深度工作流最多使用 8 条相关本地记忆；实际条数会在结果审计中显示。".into()
            } else {
                "本次不会向模型发送历史记忆。".into()
            },
            system_policy: INVESTMENT_SYSTEM_POLICY.trim().into(),
            local_only,
            generated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn context_groups(
    selection: &ContextSelection,
    snapshot: &Snapshot,
    rules: &[InvestmentRule],
    system_reviews: &[SystemReviewRecord],
    request: &AnalysisRequest,
) -> Vec<ContextGroup> {
    vec![
        group(
            "question",
            "本次问题",
            true,
            1,
            "中",
            "用户本次输入的问题。",
        ),
        group(
            "profile",
            "财务档案",
            selection.include_profile,
            1,
            "高",
            "收入、支出、负债、应急资金、期限和风险边界。",
        ),
        group(
            "goals",
            "投资目标",
            selection.include_goals,
            snapshot.goals.len(),
            "高",
            "目标金额、已投入、月度投入、日期与优先级。",
        ),
        group(
            "holdings",
            "资产持仓",
            selection.include_holdings,
            snapshot.holdings.len(),
            "高",
            "资产名称、代码、类别、市值、成本与目标权重。",
        ),
        group(
            "planning",
            "确定性规划结果",
            selection.include_planning,
            snapshot.plan.goal_projections.len() + snapshot.plan.rebalancing.len(),
            "派生",
            "目标情景、月度缺口、压力损失、风险容量与再平衡偏差。",
        ),
        group(
            "riskFindings",
            "规则型风险检查",
            selection.include_risk_findings,
            snapshot.findings.len(),
            "派生",
            "本地规则引擎产生的风险事实与行动建议。",
        ),
        group(
            "rules",
            "个人投资规则",
            selection.include_rules,
            rules.iter().filter(|rule| rule.active).count(),
            "高",
            "用户明确建立且当前启用的决策、仓位、风险与行为规则。",
        ),
        group(
            "systemReviews",
            "周期系统复盘",
            selection.include_system_reviews,
            system_reviews.len(),
            "高",
            "最近的纪律执行、违规、经验修正、行动与当时组合快照。",
        ),
        group(
            "workflow",
            "工作流选择",
            true,
            1,
            "低",
            &format!(
                "{}；反思={}；多方案={}。",
                if request.workflow == "deep" {
                    "深度编排"
                } else {
                    "快速分析"
                },
                request.reflect,
                request.explore_alternatives
            ),
        ),
    ]
}

fn group(
    key: &str,
    label: &str,
    included: bool,
    record_count: usize,
    sensitivity: &str,
    description: &str,
) -> ContextGroup {
    ContextGroup {
        key: key.into(),
        label: label.into(),
        included,
        record_count,
        sensitivity: sensitivity.into(),
        description: description.into(),
    }
}

fn context_revision(serialized: &[u8]) -> String {
    let hash = serialized
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("ctx-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        FinancialProfile, InvestmentRule, PortfolioPlan, SystemReviewRecord, SystemReviewSnapshot,
    };

    fn snapshot() -> Snapshot {
        Snapshot {
            profile: FinancialProfile {
                monthly_income: 30_000.0,
                monthly_expense: 10_000.0,
                ..Default::default()
            },
            goals: Vec::new(),
            holdings: Vec::new(),
            findings: Vec::new(),
            total_value: 0.0,
            emergency_months: 0.0,
            concentration_pct: 0.0,
            plan: PortfolioPlan {
                monthly_surplus: 20_000.0,
                committed_monthly: 0.0,
                modeled_annual_return_pct: 4.2,
                modeled_annual_volatility_pct: 8.0,
                stress_loss_pct: 0.0,
                risk_capacity_pct: 15.0,
                risk_status: "insufficient".into(),
                goal_projections: Vec::new(),
                rebalancing: Vec::new(),
                assumptions: "测试".into(),
            },
            updated_at: "2026-01-01".into(),
        }
    }

    #[test]
    fn omits_unselected_sensitive_groups() {
        let request = AnalysisRequest {
            question: "如何控制风险？".into(),
            workflow: "deep".into(),
            use_memory: false,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection {
                include_profile: false,
                include_goals: false,
                include_holdings: false,
                include_planning: true,
                include_risk_findings: true,
                ..Default::default()
            },
            preview_revision: None,
        };
        let built = ContextBuilder::build(&request, &snapshot(), &[], &[], &[]);
        assert!(built.payload.get("financialProfile").is_none());
        assert!(built.payload.get("goals").is_none());
        assert!(built.payload.get("portfolio").is_none());
        assert!(built.payload.get("deterministicPlan").is_some());
        assert!(built.payload.to_string().contains("如何控制风险"));
    }

    #[test]
    fn preview_never_contains_api_key() {
        let request = AnalysisRequest {
            question: "测试".into(),
            workflow: "deep".into(),
            use_memory: true,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        };
        let memories = (0..8)
            .map(|index| MemoryItem {
                id: format!("memory-{index}"),
                kind: "decision".into(),
                title: format!("记录 {index}"),
                content: "测试".into(),
                created_at: "2026-01-01".into(),
            })
            .collect::<Vec<_>>();
        let built = ContextBuilder::build(&request, &snapshot(), &[], &[], &memories);
        let preview = ContextBuilder::preview(
            &request,
            built,
            "openai-compatible".into(),
            "test-model".into(),
            memories,
        );
        assert!(!preview.payload.to_string().contains("API Key"));
        assert_eq!(
            preview
                .groups
                .iter()
                .find(|group| group.key == "memory")
                .unwrap()
                .record_count,
            8
        );
    }

    #[test]
    fn memory_changes_invalidate_context_revision() {
        let request = AnalysisRequest {
            question: "测试".into(),
            workflow: "deep".into(),
            use_memory: true,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        };
        let snapshot = snapshot();
        let first = vec![MemoryItem {
            id: "one".into(),
            kind: "decision".into(),
            title: "记录".into(),
            content: "原始内容".into(),
            created_at: "2026-01-01".into(),
        }];
        let mut changed = first.clone();
        changed[0].content = "已经变化".into();
        assert_ne!(
            ContextBuilder::build(&request, &snapshot, &[], &[], &first).revision,
            ContextBuilder::build(&request, &snapshot, &[], &[], &changed).revision
        );
    }

    #[test]
    fn rules_and_reviews_follow_selection_and_revision_contract() {
        let request = AnalysisRequest {
            question: "复盘我的纪律".into(),
            workflow: "deep".into(),
            use_memory: false,
            reflect: true,
            explore_alternatives: true,
            context_selection: ContextSelection::default(),
            preview_revision: None,
        };
        let rule = InvestmentRule {
            id: "rule-1".into(),
            category: "风险".into(),
            statement: "单一仓位不超过 10%".into(),
            trigger: "加仓前".into(),
            rationale: "限制永久损失".into(),
            active: true,
            source_review_id: None,
            revision: 1,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
        };
        let review = SystemReviewRecord {
            id: "review-1".into(),
            period_label: "2026 Q3".into(),
            adherence_score: 4,
            process_summary: "按计划执行".into(),
            rule_violations: "无".into(),
            lessons: "减少计划外交易".into(),
            next_actions: "继续记录".into(),
            next_review_date: "2026-12-31".into(),
            snapshot: SystemReviewSnapshot {
                portfolio_value: 0.0,
                emergency_months: 0.0,
                concentration_pct: 0.0,
                risk_status: "insufficient".into(),
                high_risk_findings: 0,
                goal_total: 0,
                goals_on_track: 0,
                decision_total: 0,
                reviewed_decisions: 0,
                active_rules: 1,
            },
            created_at: "2026-09-30".into(),
        };
        let included = ContextBuilder::build(
            &request,
            &snapshot(),
            std::slice::from_ref(&rule),
            std::slice::from_ref(&review),
            &[],
        );
        assert!(included.payload.get("personalInvestmentRules").is_some());
        assert!(included.payload.get("periodicSystemReviews").is_some());

        let mut excluded_request = request.clone();
        excluded_request.context_selection.include_rules = false;
        excluded_request.context_selection.include_system_reviews = false;
        let excluded = ContextBuilder::build(
            &excluded_request,
            &snapshot(),
            std::slice::from_ref(&rule),
            std::slice::from_ref(&review),
            &[],
        );
        assert!(excluded.payload.get("personalInvestmentRules").is_none());
        assert!(excluded.payload.get("periodicSystemReviews").is_none());
        assert_ne!(included.revision, excluded.revision);
    }
}
