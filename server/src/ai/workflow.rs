use serde_json::Value;

use super::structured_output::{MAX_STRUCTURED_OUTPUT_CHARS, STRUCTURED_ANALYSIS_CONTRACT};
use super::ChatMessage;
use crate::context::INVESTMENT_SYSTEM_POLICY;

pub const INVESTMENT_WORKFLOW_VERSION: &str = "investment-workflow-v9";

pub struct StagePrompt {
    pub key: &'static str,
    pub label: &'static str,
    pub messages: Vec<ChatMessage>,
}

impl StagePrompt {
    pub fn with_methodology(mut self, payload: &Value) -> Self {
        if let Some(prompt) = payload["investmentMethodology"].as_str() {
            if let Some(system) = self
                .messages
                .iter_mut()
                .find(|message| message.role == "system")
            {
                system.content.push_str("\n\n");
                system
                    .content
                    .push_str(&crate::context::methodology_instructions(prompt));
            }
        }
        self
    }
}

pub trait AnalysisWorkflow: Send + Sync {
    fn version(&self) -> &'static str;
    fn quick(&self, local_context: &str) -> StagePrompt;
    fn repair_output(
        &self,
        stage_label: &str,
        invalid_output: &str,
        validation_error: &str,
        allowed_evidence_ids: &[String],
    ) -> StagePrompt;
}

pub struct InvestmentWorkflowV9;

impl AnalysisWorkflow for InvestmentWorkflowV9 {
    fn version(&self) -> &'static str {
        INVESTMENT_WORKFLOW_VERSION
    }

    fn quick(&self, local_context: &str) -> StagePrompt {
        StagePrompt {
            key: "quick_analysis",
            label: "快速分析",
            messages: vec![
                ChatMessage::system(INVESTMENT_SYSTEM_POLICY),
                ChatMessage::user(format!(
                    "我的资料与本轮问题：{local_context}\n\n直接回答并按以下契约输出。\n\n{STRUCTURED_ANALYSIS_CONTRACT}"
                )),
            ],
        }
    }

    fn repair_output(
        &self,
        stage_label: &str,
        invalid_output: &str,
        validation_error: &str,
        allowed_evidence_ids: &[String],
    ) -> StagePrompt {
        StagePrompt {
            key: "structured_output_repair",
            label: "结构化输出修复",
            messages: vec![
                ChatMessage::system("只修复格式，不添加新事实。"),
                ChatMessage::user(format!(
                    "待修复阶段：{stage_label}\n校验错误：{validation_error}\n本次允许引用的证据 ID：{}\n\n原输出：\n{}\n\n请严格按契约重写。\n{STRUCTURED_ANALYSIS_CONTRACT}",
                    serde_json::to_string(allowed_evidence_ids).unwrap_or_else(|_| "[]".into()),
                    truncate_chars(invalid_output, MAX_STRUCTURED_OUTPUT_CHARS)
                )),
            ],
        }
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_keeps_untrusted_data_boundary_and_machine_contract() {
        let prompt = InvestmentWorkflowV9.quick("上下文");
        assert!(prompt.messages[0].content.contains("不可信数据"));
        assert!(prompt.messages[1].content.contains("evidenceIds"));
        let repair = InvestmentWorkflowV9.repair_output("生成", "无效输出", "字段缺失", &[]);
        assert_eq!(repair.messages[0].content, "只修复格式，不添加新事实。");
    }

    #[test]
    fn repair_preserves_the_tail_of_detailed_output_within_the_parser_budget() {
        let invalid = serde_json::json!({
            "answer": "待修复的理由".repeat(2_500),
            "tail": "最后的条件与引用不能被旧截断丢弃"
        })
        .to_string();
        assert!(invalid.chars().count() > 12_000);
        assert!(invalid.chars().count() < MAX_STRUCTURED_OUTPUT_CHARS);
        let prompt = InvestmentWorkflowV9.repair_output("生成", &invalid, "字段无效", &[]);
        assert!(prompt.messages[1].content.contains(&invalid));
    }

    #[test]
    fn repair_bounds_oversized_originals_by_unicode_characters() {
        let retained = "🙂".repeat(MAX_STRUCTURED_OUTPUT_CHARS);
        let oversized = format!("{retained}BEYOND_ORIGINAL_BUDGET");
        let prompt = InvestmentWorkflowV9.repair_output("生成", &oversized, "输出过大", &[]);
        assert!(prompt.messages[1].content.contains(&retained));
        assert!(!prompt.messages[1]
            .content
            .contains("BEYOND_ORIGINAL_BUDGET"));
    }
}
