use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

pub const METHODOLOGY_LIMIT: usize = 4_000;
pub const DEFAULT_INVESTMENT_METHODOLOGY: &str = "先考虑财务安全垫、期限、流动性、负债和集中度，再讨论收益。区分资产价格与内在价值、好结果与好过程。可以给出明确的方向和比例，同时说明前提和主要风险。使用中文和日常语言，优先回答眼前的问题，解释为什么。不要要求我维护研究库或填写收益预测。";

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
