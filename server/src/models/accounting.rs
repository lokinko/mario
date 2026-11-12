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

// Profiles and backups saved before investable assets were retired still carry
// `investableAssets`, so unknown fields are ignored rather than rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAmounts {
    pub monthly_income: String,
    pub monthly_expense: String,
    pub emergency_fund: String,
    pub liabilities: String,
}

impl super::FinancialProfile {
    pub fn field_provided(&self, field: super::ProfileField) -> bool {
        use super::ProfileField;
        if let Some(fields) = &self.provided_fields {
            return fields.contains(&field);
        }
        // Historical profiles have no presence information. Keep their previous
        // conservative interpretation until the user explicitly edits a field.
        match field {
            ProfileField::MonthlyIncome => self.monthly_income > 0.0,
            ProfileField::MonthlyExpense => self.monthly_expense > 0.0,
            ProfileField::EmergencyFund => self.emergency_fund > 0.0,
            ProfileField::Liabilities => self.liabilities > 0.0,
            ProfileField::HorizonYears => self.horizon_years > 0 && self.horizon_years != 5,
            ProfileField::MaxDrawdownPct => {
                self.max_drawdown_pct > 0.0 && self.max_drawdown_pct != 15.0
            }
        }
    }
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
    }
    if [
        normalized.monthly_income,
        normalized.monthly_expense,
        normalized.emergency_fund,
        normalized.liabilities,
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

#[cfg(test)]
mod tests {
    #[test]
    fn profiles_saved_with_retired_fields_still_load_and_drop_them() {
        let profile: super::super::FinancialProfile = serde_json::from_value(serde_json::json!({
            "monthlyIncome": 1, "monthlyExpense": 0, "emergencyFund": 0, "liabilities": 0,
            "investableAssets": 5, "horizonYears": 5, "maxDrawdownPct": 15,
            "riskLevel": "进取", "baseCurrency": "CNY",
            "exactAmounts": {
                "monthlyIncome": "1", "monthlyExpense": "0", "emergencyFund": "0",
                "liabilities": "0", "investableAssets": "5"
            }
        }))
        .unwrap();
        assert_eq!(
            super::normalize_profile(&profile).unwrap().monthly_income,
            1.0
        );
        let saved = serde_json::to_value(&profile).unwrap();
        assert!(saved.get("riskLevel").is_none() && saved.get("investableAssets").is_none());
        assert!(saved["exactAmounts"].get("investableAssets").is_none());
    }
}
