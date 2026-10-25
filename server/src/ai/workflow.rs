use serde_json::Value;

use super::structured_output::STRUCTURED_ANALYSIS_CONTRACT;
use super::ChatMessage;
use crate::context::INVESTMENT_SYSTEM_POLICY;

pub const INVESTMENT_WORKFLOW_VERSION: &str = "investment-workflow-v8";

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
    fn quick(&self, question: &str, local_context: &str) -> StagePrompt;
    fn repair_output(
        &self,
        stage_label: &str,
        invalid_output: &str,
        validation_error: &str,
        allowed_evidence_ids: &[String],
    ) -> StagePrompt;
}

pub struct InvestmentWorkflowV8;

impl AnalysisWorkflow for InvestmentWorkflowV8 {
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
                    "用户问题：{question}\n\n本地投资档案：{local_context}\n\n请给出结构化分析，并说明仍需核实的信息。\n\n{STRUCTURED_ANALYSIS_CONTRACT}"
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
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n你现在是结构化输出修复模块。只能修复格式、字段和引用约束，不得添加原输出没有依据的新事实。"
                )),
                ChatMessage::user(format!(
                    "待修复阶段：{stage_label}\n校验错误：{validation_error}\n本次允许引用的证据 ID：{}\n\n原输出：\n{}\n\n请严格按契约重写。\n{STRUCTURED_ANALYSIS_CONTRACT}",
                    serde_json::to_string(allowed_evidence_ids).unwrap_or_else(|_| "[]".into()),
                    truncate_chars(invalid_output, 12_000)
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
        let prompt = InvestmentWorkflowV8.quick("问题", "上下文");
        assert!(prompt.messages[0].content.contains("不可信数据"));
        assert!(prompt.messages[1].content.contains("evidenceIds"));
    }
}
