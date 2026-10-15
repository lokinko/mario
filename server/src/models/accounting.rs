use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact decimal accounting values. Numeric Holding fields are view projections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingAmounts {
    #[serde(default = "unconfirmed")]
    pub price_status: String,
    pub market_value: String,
    pub cost_basis: String,
    pub quantity: Option<String>,
    pub cost_price: Option<String>,
    pub current_price: Option<String>,
}

fn unconfirmed() -> String {
    "unconfirmed".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountingHolding {
    #[serde(default)]
    pub base_currency: String,
    #[serde(flatten)]
    pub amounts: HoldingAmounts,
    pub fx_rate: Option<String>,
    pub unrealized_profit: Option<String>,
    pub settled_profit: Option<String>,
    pub usable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountingMetric {
    pub value: Option<String>,
    pub missing: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountingTotals {
    pub value: AccountingMetric,
    pub cost: AccountingMetric,
    pub unrealized: AccountingMetric,
    pub realized: AccountingMetric,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountingSnapshot {
    pub total_value: Option<String>,
    pub holdings: BTreeMap<String, AccountingHolding>,
    pub investments: AccountingTotals,
    pub cash: AccountingTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileAmounts {
    pub monthly_income: String,
    pub monthly_expense: String,
    pub emergency_fund: String,
    pub liabilities: String,
    pub investable_assets: String,
}

pub(crate) fn normalize_profile(
    profile: &super::FinancialProfile,
) -> crate::AppResult<super::FinancialProfile> {
    use rust_decimal::prelude::ToPrimitive;
    let mut normalized = profile.clone();
    if let Some(exact) = &profile.exact_amounts {
        let value = |s: &str| -> crate::AppResult<f64> {
            super::decimal::parse(s)?
                .to_f64()
                .ok_or_else(|| crate::AppError::Validation("收支金额超出范围".into()))
        };
        normalized.monthly_income = value(&exact.monthly_income)?;
        normalized.monthly_expense = value(&exact.monthly_expense)?;
        normalized.emergency_fund = value(&exact.emergency_fund)?;
        normalized.liabilities = value(&exact.liabilities)?;
        normalized.investable_assets = value(&exact.investable_assets)?;
    }
    if [
        normalized.monthly_income,
        normalized.monthly_expense,
        normalized.emergency_fund,
        normalized.liabilities,
        normalized.investable_assets,
    ]
    .iter()
    .any(|v| !v.is_finite() || *v < 0.0 || *v > 1e15)
        || !(0..=80).contains(&normalized.horizon_years)
        || !normalized.max_drawdown_pct.is_finite()
        || !(0.0..=100.0).contains(&normalized.max_drawdown_pct)
    {
        return Err(crate::AppError::Validation(
            "财务金额、投资期限或回撤限制无效".into(),
        ));
    }
    Ok(normalized)
}
