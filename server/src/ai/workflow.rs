use serde_json::Value;

use super::ChatMessage;
use crate::context::INVESTMENT_SYSTEM_POLICY;

pub const INVESTMENT_WORKFLOW_VERSION: &str = "investment-workflow-v2";

pub struct StagePrompt {
    pub key: &'static str,
    pub label: &'static str,
    pub messages: Vec<ChatMessage>,
}

pub struct AlternativeSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub lens: &'static str,
    pub instruction: &'static str,
}

pub struct CandidateContext<'a> {
    pub question: &'a str,
    pub local_context: &'a str,
    pub research_plan: &'a str,
    pub memory_context: &'a str,
}

pub trait AnalysisWorkflow: Send + Sync {
    fn version(&self) -> &'static str;
    fn quick(&self, question: &str, local_context: &str) -> StagePrompt;
    fn research_plan(&self, question: &str, payload: &Value) -> StagePrompt;
    fn alternative_specs(&self, explore: bool) -> Vec<AlternativeSpec>;
    fn alternative(&self, context: &CandidateContext<'_>, spec: &AlternativeSpec) -> StagePrompt;
    fn critique(&self, question: &str, alternatives: &str) -> StagePrompt;
    fn synthesis(
        &self,
        question: &str,
        local_context: &str,
        research_plan: &str,
        alternatives: &str,
        critique: &str,
    ) -> StagePrompt;
}

pub struct InvestmentWorkflowV2;

impl AnalysisWorkflow for InvestmentWorkflowV2 {
    fn version(&self) -> &'static str {
        INVESTMENT_WORKFLOW_VERSION
    }

    fn quick(&self, question: &str, local_context: &str) -> StagePrompt {
        StagePrompt {
            key: "quick_analysis",
            label: "快速分析",
            messages: vec![
                ChatMessage::system(INVESTMENT_SYSTEM_POLICY),
                ChatMessage::user(format!(
                    "用户问题：{question}\n\n本地投资档案：{local_context}\n\n请给出结构化分析，并说明仍需核实的信息。"
                )),
            ],
        }
    }

    fn research_plan(&self, question: &str, payload: &Value) -> StagePrompt {
        StagePrompt {
            key: "research_plan",
            label: "研究计划",
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n你现在是研究规划模块，只制定分析计划和检索线索，不给最终结论。"
                )),
                ChatMessage::user(format!(
                    "问题：{question}\n已知档案摘要：{payload}\n请列出需要核实的假设、反方问题、缺失信息，以及用于检索历史决策的关键词。"
                )),
            ],
        }
    }

    fn alternative_specs(&self, explore: bool) -> Vec<AlternativeSpec> {
        if explore {
            vec![
                AlternativeSpec {
                    id: "baseline",
                    label: "稳健基准方案",
                    lens: "资本保护与最小行动",
                    instruction: "优先降低不可逆损失。必须认真评估保持不动、减少复杂度、改善现金流或降低集中度等基准路径。",
                },
                AlternativeSpec {
                    id: "goal_progress",
                    label: "目标推进方案",
                    lens: "目标达成与机会成本",
                    instruction: "在风险预算内寻找改善目标达成概率的路径，并与保持不动比较机会成本；不得依赖短期行情预测。",
                },
            ]
        } else {
            vec![AlternativeSpec {
                id: "baseline",
                label: "稳健基准方案",
                lens: "资本保护与最小行动",
                instruction: "提出一个最稳健的基准路径，说明保持不动是否更优，以及适用条件、风险和待验证证据。",
            }]
        }
    }

    fn alternative(&self, context: &CandidateContext<'_>, spec: &AlternativeSpec) -> StagePrompt {
        StagePrompt {
            key: spec.id,
            label: spec.label,
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n你是独立方案研究模块。只从指定视角形成候选方案，不读取其他候选方案，也不直接作最终裁决。"
                )),
                ChatMessage::user(format!(
                    "用户问题：{}\n\n本地投资档案：{}\n\n研究计划：{}\n\n相关历史记忆：{}\n\n指定视角：{}\n任务：{}\n输出适用条件、建议动作、主要风险、机会成本、证据依据与证伪条件。",
                    context.question,
                    context.local_context,
                    context.research_plan,
                    context.memory_context,
                    spec.lens,
                    spec.instruction
                )),
            ],
        }
    }

    fn critique(&self, question: &str, alternatives: &str) -> StagePrompt {
        StagePrompt {
            key: "risk_critique",
            label: "独立风险审查",
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n你现在是独立风险审查模块。只找问题，不迎合候选分析者，也不直接给最终结论。"
                )),
                ChatMessage::user(format!(
                    "原问题：{question}\n候选方案：{alternatives}\n请逐项检查：事实与推断是否混淆、证据是否真的支持主张、是否忽略极端风险、是否过度自信、是否给了隐性买卖指令、是否违反用户风险预算、是否缺少更简单的基准方案。最后列出最终裁决必须修正的事项。"
                )),
            ],
        }
    }

    fn synthesis(
        &self,
        question: &str,
        local_context: &str,
        research_plan: &str,
        alternatives: &str,
        critique: &str,
    ) -> StagePrompt {
        StagePrompt {
            key: "synthesis",
            label: "最终综合裁决",
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n你是最终整合模块。比较候选方案并吸收审查意见，但要自行判断，不机械拼接。"
                )),
                ChatMessage::user(format!(
                    "原问题：{question}\n\n档案：{local_context}\n\n研究计划：{research_plan}\n\n独立候选方案：{alternatives}\n\n独立审查：{critique}\n\n请输出：①当前最重要判断；②风险与未知；③候选方案比较与取舍；④可逆的下一步行动；⑤未来复盘/证伪条件。区分用户保存的带来源证据与尚未接入的实时外部数据，不得把未知信息写成事实。"
                )),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exploration_uses_independent_and_distinct_lenses() {
        let workflow = InvestmentWorkflowV2;
        let alternatives = workflow.alternative_specs(true);
        assert_eq!(alternatives.len(), 2);
        assert_ne!(alternatives[0].id, alternatives[1].id);
        assert!(alternatives.iter().any(|item| item.id == "baseline"));
        assert!(alternatives.iter().any(|item| item.id == "goal_progress"));
    }

    #[test]
    fn disabling_exploration_preserves_a_no_action_baseline() {
        let workflow = InvestmentWorkflowV2;
        let alternatives = workflow.alternative_specs(false);
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0].id, "baseline");
    }

    #[test]
    fn every_model_stage_inherits_the_untrusted_data_boundary() {
        let workflow = InvestmentWorkflowV2;
        let prompts = [
            workflow.quick("问题", "上下文"),
            workflow.research_plan("问题", &serde_json::json!({})),
            workflow.critique("问题", "候选"),
            workflow.synthesis("问题", "上下文", "计划", "候选", "审查"),
        ];
        for prompt in prompts {
            assert!(prompt.messages[0].content.contains("不可信数据"));
        }

        let spec = workflow.alternative_specs(true).remove(0);
        let context = CandidateContext {
            question: "问题",
            local_context: "上下文",
            research_plan: "计划",
            memory_context: "记忆",
        };
        assert!(workflow.alternative(&context, &spec).messages[0]
            .content
            .contains("不可信数据"));
    }
}
