use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

pub const METHODOLOGY_LIMIT: usize = 4_000;
pub const DEFAULT_INVESTMENT_METHODOLOGY: &str = "你是我的投资决策教练，帮助我提升决策质量，而不是承诺收益。\n\n1. 先检查财务安全垫、期限、流动性、负债、集中度和永久损失风险，再讨论潜在收益。\n2. 使用概率和情景，不用“一定”“稳赚”等确定性语言。\n3. 区分资产价格与内在价值，区分好结果与好过程。\n4. 给出可执行的检查项、证伪条件与复盘节点，避免直接下达买卖指令。\n5. 输出中文，清楚、克制，优先解释为什么。\n6. 主动整理研究、比较方案和列出待核实信息。不要要求我维护研究库、填写收益预测或自行制定完整投资规则。每轮优先回答一个问题，最多追问一项关键缺失信息，使用日常语言。";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvestmentMethodologyInput {
    pub prompt: String,
}

impl InvestmentMethodologyInput {
    pub fn validate(&self) -> AppResult<()> {
        if self.prompt.trim().is_empty() || self.prompt.chars().count() > METHODOLOGY_LIMIT {
            return Err(AppError::Validation(format!(
                "投资方法论不能为空，且不能超过 {METHODOLOGY_LIMIT} 字符"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentMethodology {
    pub prompt: String,
    pub default_prompt: &'static str,
    pub max_length: usize,
}

impl InvestmentMethodology {
    pub fn new(prompt: String) -> Self {
        Self {
            prompt,
            default_prompt: DEFAULT_INVESTMENT_METHODOLOGY,
            max_length: METHODOLOGY_LIMIT,
        }
    }
}
