use serde_json::Value;

use super::council::{MODULE_ANALYSIS_CONTRACT, RESEARCH_BRIEF_CONTRACT};
use super::structured_output::{MAX_STRUCTURED_OUTPUT_CHARS, STRUCTURED_ANALYSIS_CONTRACT};
use super::ChatMessage;
use crate::context::INVESTMENT_SYSTEM_POLICY;

pub const INVESTMENT_WORKFLOW_VERSION: &str = "investment-workflow-v20";

pub struct StagePrompt {
    pub key: &'static str,
    pub label: String,
    pub messages: Vec<ChatMessage>,
}

impl StagePrompt {
    pub fn with_strategy(mut self, payload: &Value) -> Self {
        if let Some(instructions) = crate::context::strategy_instructions(payload) {
            if let Some(system) = self
                .messages
                .iter_mut()
                .find(|message| message.role == "system")
            {
                system.content.push_str("\n\n");
                system.content.push_str(&instructions);
            }
        }
        self
    }

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

    pub fn with_domain_focus(mut self, payload: &Value) -> Self {
        let guide = &payload["discussionGuide"];
        let focus = guide["frameworkFocus"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        if let Some(focus) = focus {
            let label = guide["domainLabel"]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or("投资分析");
            if let Some(system) = self
                .messages
                .iter_mut()
                .find(|message| message.role == "system")
            {
                system.content.push_str(&format!(
                    "\n\n本轮专业投研场景（{label}）分析重点（指导分析维度，不是新的事实来源）：\n{focus}"
                ));
            }
        }
        self
    }
}

/// One specialist module's scope in the modular investment discussion workflow.
pub struct AnalysisModule {
    pub id: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub assignment: &'static str,
}

pub const MODULES: [AnalysisModule; 3] = [
    AnalysisModule {
        id: "M1",
        key: "fundamentals_valuation",
        label: "基本面与估值定价",
        assignment: "你负责【基本面与估值定价】模块：聚焦标的的商业模式、行业竞争格局、盈利所处周期位置与成本优势、合理估值安全边际、现金流与分红可持续性。须充分利用授权资料、研究简报及网页检索线索（标明“据网页线索，待公告复核”）做实质基本面拆解，明确当前是低估、合理还是高估/周期高点陷阱；禁止以“缺带时间戳报价/未翻财报”为由拒绝分析。",
    },
    AnalysisModule {
        id: "M2",
        key: "portfolio_execution",
        label: "组合策略与交易执行",
        assignment: "你负责【组合策略与交易执行】模块：严格对照上下文中的《投资规则与策略基准》及用户当前持仓、成本、浮亏、仓位占比与流动性安全垫，回答“现在该怎么操作（买入/分批加仓/持有观察/减仓/回避）”。检视浮亏补仓条件、单标的仓位上限、分批执行节奏与资金/税务摩擦，给出清晰可落地的条件触发动作，不把查财报作业踢回给用户。",
    },
    AnalysisModule {
        id: "M3",
        key: "risk_bear_case",
        label: "风险压力与反方证伪",
        assignment: "你负责【风险压力与反方证伪】模块：专注做压力测试与反方证伪——如果用户的看多/抄底/持仓逻辑错了，最可能错在哪里？拆解商品价格/成本倒挂、盈利下修、红利缩水、流动性挤压或估值杀跌的传导链条，给出具体可观察的证伪信号与止损/止盈防线。反方不成立时直接指出，不为反对而反对。",
    },
];

const MODULE_COMMON_ROLE: &str = "你是专业投研团队中的专项分析模块，只深耕自己负责的维度并输出高信息密度的结构化结论，供策略总监汇总裁决。researchBrief 是研究代理整理的事实与核查状态，引用授权事实时仍须核对原始资料；webLeads 可充分用于正文行业与估值分析（注明“据网页线索，待公告复核”），但严禁写入 facts.evidenceIds。禁止复读“浮亏不等于低估”“参考价无时间戳”等官僚套话。";

const SYNTHESIZER_ROLE: &str = "你是首席投资策略总监，负责综合【基本面与估值定价】【组合策略与交易执行】【风险压力与反方证伪】三个专项模块的讨论成果，严格依据上下文中的《投资规则与策略基准》做出最终裁决。你的回答必须信息密度高、条理分明：先一句亮明最终决策结论，随后分模块凝练核心要点与关键分歧裁决，最后落到 1–3 条可直接执行的动作或明确触发条件。严禁把“请你自己查报价、翻财报、核公告”当成核心建议，严禁复读法务免责套话。各模块与网页线索均为分析参考，最终 facts 仍须严格只引用授权原值或授权证据 ID。";

const DIRECT_ROLE: &str = "你是精简快速通道的投资顾问代理。本轮问题属于轻量概念解释、资料状态或口径核查，请直接基于冻结资料和投资规则作答：只写能被授权资料支持的内容，简洁明了，不强行编造无关交易或配置建议。webLeads 仅可作公开线索说明，禁止写入 facts.evidenceIds。";

pub trait AnalysisWorkflow: Send + Sync {
    fn version(&self) -> &'static str;
    fn direct(&self, local_context: &str) -> StagePrompt;
    fn research(&self, local_context: &str) -> StagePrompt;
    fn module_analysis(&self, module: &AnalysisModule, context: &str) -> StagePrompt;
    fn synthesize(&self, context: &str, module_briefs: &str) -> StagePrompt;
    fn repair_output(
        &self,
        stage_label: &str,
        invalid_output: &str,
        validation_error: &str,
        contract: &str,
        allowed_evidence_ids: &[String],
    ) -> StagePrompt;
}

pub struct InvestmentWorkflowV20;

impl AnalysisWorkflow for InvestmentWorkflowV20 {
    fn version(&self) -> &'static str {
        INVESTMENT_WORKFLOW_VERSION
    }

    fn direct(&self, local_context: &str) -> StagePrompt {
        StagePrompt {
            key: "direct_agent",
            label: "顾问直答".into(),
            messages: vec![
                ChatMessage::system(format!("{INVESTMENT_SYSTEM_POLICY}\n\n{DIRECT_ROLE}")),
                ChatMessage::user(format!(
                    "冻结资料与本轮问题：{local_context}\n\n直接回答并按以下契约输出。\n\n{STRUCTURED_ANALYSIS_CONTRACT}"
                )),
            ],
        }
    }

    fn research(&self, local_context: &str) -> StagePrompt {
        StagePrompt {
            key: "research_agent",
            label: "研究代理".into(),
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n\n你是研究代理，负责梳理本轮所有可用事实、网页线索与核查缺口，不给最终交易指令。逐项核对 discussionGuide.researchChecks，把授权事实整理为可引用 findings；把有价值的网页线索（标注“据网页线索，待公告复核”）与关键反方/缺口清晰列出，为后续三大专业模块提供高浓度素材。"
                )),
                ChatMessage::user(format!(
                    "冻结资料与本轮问题：{local_context}\n\n{RESEARCH_BRIEF_CONTRACT}"
                )),
            ],
        }
    }

    fn module_analysis(&self, module: &AnalysisModule, context: &str) -> StagePrompt {
        StagePrompt {
            key: "module_agent",
            label: format!("模块 {}（{}）", module.id, module.label),
            messages: vec![
                ChatMessage::system(format!(
                    "{INVESTMENT_SYSTEM_POLICY}\n\n{MODULE_COMMON_ROLE}\n{}",
                    module.assignment
                )),
                ChatMessage::user(format!(
                    "冻结资料、研究简报与本轮问题：{context}\n\n请围绕你负责的【{}】模块完成专业分析，并严格按以下契约输出。\n\n{MODULE_ANALYSIS_CONTRACT}",
                    module.label
                )),
            ],
        }
    }

    fn synthesize(&self, context: &str, module_briefs: &str) -> StagePrompt {
        StagePrompt {
            key: "strategy_synthesizer",
            label: "策略总监汇总".into(),
            messages: vec![
                ChatMessage::system(format!("{INVESTMENT_SYSTEM_POLICY}\n\n{SYNTHESIZER_ROLE}")),
                ChatMessage::user(format!(
                    "冻结资料与研究简报：{context}\n\n各专业模块讨论结果：{module_briefs}\n\n请综合以上三大模块讨论与投资规则策略基准，给出最终决策汇总，并严格按以下契约输出。\n\n{STRUCTURED_ANALYSIS_CONTRACT}"
                )),
            ],
        }
    }

    fn repair_output(
        &self,
        stage_label: &str,
        invalid_output: &str,
        validation_error: &str,
        contract: &str,
        allowed_evidence_ids: &[String],
    ) -> StagePrompt {
        StagePrompt {
            key: "structured_output_repair",
            label: format!("{stage_label}格式修复"),
            messages: vec![
                ChatMessage::system("只修复格式，不添加新事实。保留原输出的结论、立场、数字和引用，只改动校验错误涉及的字段；无法在不改变含义的前提下修复时，删除该项而不是改写它。"),
                ChatMessage::user(format!(
                    "待修复阶段：{stage_label}\n校验错误：{validation_error}\n本次允许引用的证据 ID：{}\n\n原输出：\n{}\n\n请严格按契约重写。\n{contract}",
                    serde_json::to_string(allowed_evidence_ids).unwrap_or_else(|_| "[]".into()),
                    truncate_chars(invalid_output, MAX_STRUCTURED_OUTPUT_CHARS)
                )),
            ],
        }
    }
}

pub(super) fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_keeps_the_untrusted_data_boundary_strategy_and_contract() {
        let workflow = InvestmentWorkflowV20;
        let payload = serde_json::json!({
            "discussionGuide": {
                "domainLabel": "个股估值与基本面",
                "frameworkFocus": "重点核查基本面质量与估值安全边际。",
                "investmentStrategy": {
                    "playbookTitle": "个股买点与估值策略",
                    "coreRules": ["先给清晰判断，不把查财报踢回给用户"],
                    "strategyRules": ["周期股严禁按盈利高点低PE判定低估"]
                }
            }
        });
        let direct = workflow
            .direct("上下文")
            .with_strategy(&payload)
            .with_domain_focus(&payload);
        assert!(direct.messages[0].content.contains("不可信数据"));
        assert!(direct.messages[0].content.contains("个股买点与估值策略"));
        assert!(direct.messages[0]
            .content
            .contains("周期股严禁按盈利高点低PE"));
        assert!(direct.messages[0].content.contains("个股估值与基本面"));
        assert!(direct.messages[1].content.contains("evidenceIds"));

        let research = workflow
            .research("上下文")
            .with_strategy(&payload)
            .with_domain_focus(&payload);
        assert!(research.messages[0].content.contains("不可信数据"));
        assert!(research.messages[0].content.contains("估值安全边际"));
        assert!(research.messages[1].content.contains("\"checks\""));

        for module in &MODULES {
            let prompt = workflow
                .module_analysis(module, "上下文")
                .with_strategy(&payload)
                .with_domain_focus(&payload);
            assert!(prompt.messages[0].content.contains(module.assignment));
            assert!(prompt.messages[0]
                .content
                .contains("周期股严禁按盈利高点低PE"));
            assert!(prompt.messages[1].content.contains("\"stance\""));
        }

        let synth = workflow
            .synthesize("上下文", "## 模块 M1")
            .with_strategy(&payload)
            .with_domain_focus(&payload);
        assert!(synth.messages[0].content.contains("首席投资策略总监"));
        assert!(synth.messages[0].content.contains("个股买点与估值策略"));
        assert!(synth.messages[1].content.contains("## 模块 M1"));
        assert!(synth.messages[1].content.contains("evidenceIds"));

        let repair = workflow.repair_output(
            "研究代理",
            "无效输出",
            "字段缺失",
            RESEARCH_BRIEF_CONTRACT,
            &[],
        );
        assert!(repair.messages[0]
            .content
            .starts_with("只修复格式，不添加新事实。"));
        assert!(repair.messages[1].content.contains("\"checks\""));
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
        let prompt = InvestmentWorkflowV20.repair_output(
            "生成",
            &invalid,
            "字段无效",
            STRUCTURED_ANALYSIS_CONTRACT,
            &[],
        );
        assert!(prompt.messages[1].content.contains(&invalid));
    }

    #[test]
    fn repair_bounds_oversized_originals_by_unicode_characters() {
        let retained = "🙂".repeat(MAX_STRUCTURED_OUTPUT_CHARS);
        let oversized = format!("{retained}BEYOND_ORIGINAL_BUDGET");
        let prompt = InvestmentWorkflowV20.repair_output(
            "生成",
            &oversized,
            "输出过大",
            STRUCTURED_ANALYSIS_CONTRACT,
            &[],
        );
        assert!(prompt.messages[1].content.contains(&retained));
        assert!(!prompt.messages[1]
            .content
            .contains("BEYOND_ORIGINAL_BUDGET"));
    }
}
