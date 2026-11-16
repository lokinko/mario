use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default)]
    pub dividends: DividendTotals,
}
/// Net dividends received in the trailing 365 days, converted at current rates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividendTotals {
    pub since: String,
    pub received: AccountingMetric,
    /// Percent of held cost basis returned by dividends of positions still held.
    pub held_yield_pct: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlice {
    pub label: String,
    pub value: String,
    pub pct: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub base_currency: String,
    pub by_class: Vec<AllocationSlice>,
    pub by_industry: Vec<AllocationSlice>,
    /// Holdings left out because they lack a rate or are pending review.
    pub missing: usize,
}

const MAX_SLICES: usize = 7;
pub const OTHER_STOCKS: &str = "其他股票";

/// Current base-currency weights; stocks outside `industries` share one bucket.
pub fn allocation(
    holdings: &[super::Holding],
    accounting: &AccountingSnapshot,
    base_currency: &str,
    industries: &HashMap<String, String>,
) -> crate::AppResult<Allocation> {
    let parse = |s: &str| {
        Decimal::from_str_exact(s)
            .map_err(|_| crate::AppError::Validation("精确会计金额无效".into()))
    };
    let mut by_class = BTreeMap::<String, Decimal>::new();
    let mut by_industry = BTreeMap::<String, Decimal>::new();
    let mut missing = 0;
    for holding in holdings {
        let Some(exact) = accounting.holdings.get(&holding.id) else {
            continue;
        };
        let market = parse(&exact.amounts.market_value)?;
        let rate = match (&exact.fx_rate, exact.usable) {
            (Some(rate), true) => parse(rate)?,
            _ => {
                if !market.is_zero() || !exact.usable {
                    missing += 1;
                }
                continue;
            }
        };
        let value = market
            .checked_mul(rate)
            .ok_or_else(|| crate::AppError::Validation("资产配置金额溢出".into()))?;
        if value <= Decimal::ZERO {
            continue;
        }
        let class = holding.asset_class.clone();
        let industry = if class == "股票" {
            industries
                .get(&holding.symbol)
                .cloned()
                .unwrap_or_else(|| OTHER_STOCKS.into())
        } else {
            class.clone()
        };
        *by_class.entry(class).or_default() += value;
        *by_industry.entry(industry).or_default() += value;
    }
    Ok(Allocation {
        base_currency: base_currency.into(),
        by_class: slices(by_class),
        by_industry: slices(by_industry),
        missing,
    })
}

fn slices(groups: BTreeMap<String, Decimal>) -> Vec<AllocationSlice> {
    let total: Decimal = groups.values().sum();
    let mut sorted: Vec<_> = groups.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if sorted.len() > MAX_SLICES {
        let rest = sorted.split_off(MAX_SLICES - 1);
        sorted.push(("其余".into(), rest.iter().map(|(_, v)| v).sum()));
    }
    sorted
        .into_iter()
        .map(|(label, value)| AllocationSlice {
            label,
            value: value.round_dp(2).normalize().to_string(),
            pct: (value * Decimal::ONE_HUNDRED / total)
                .round_dp(1)
                .normalize()
                .to_string(),
        })
        .collect()
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
    fn allocation_groups_by_class_and_industry_and_skips_unpriced_holdings() {
        let holding = |id: &str, symbol: &str, class: &str| -> super::super::Holding {
            serde_json::from_value(serde_json::json!({
                "id": id, "symbol": symbol, "name": id, "assetClass": class,
                "marketValue": 0, "costBasis": 0, "targetPct": 0, "currency": "CNY",
                "valuationDate": "", "fxRateSource": "", "fxRateObservedOn": ""
            }))
            .unwrap()
        };
        let exact = |value: &str, rate: Option<&str>, usable: bool| {
            serde_json::json!({
                "marketValue": value, "costBasis": "0", "quantity": null, "costPrice": null,
                "currentPrice": null, "fxRate": rate, "unrealizedProfit": null,
                "settledProfit": null, "usable": usable
            })
        };
        let metric = serde_json::json!({"value": null, "missing": 0});
        let totals = serde_json::json!({"value": metric, "cost": metric, "unrealized": metric, "realized": metric});
        let holdings = vec![
            holding("bank", "600036.SS", "股票"),
            holding("hk", "0700.HK", "股票"),
            holding("etf", "588000.SS", "基金"),
            holding("cash", "", "现金"),
            holding("nofx", "AAPL", "股票"),
            holding("quarantined", "600900.SS", "股票"),
        ];
        let accounting: super::AccountingSnapshot = serde_json::from_value(serde_json::json!({
            "totalValue": null, "investments": totals, "cash": totals,
            "holdings": {
                "bank": exact("300", Some("1"), true),
                "hk": exact("100", Some("0.9"), true),
                "etf": exact("110", Some("1"), true),
                "cash": exact("390", Some("1"), true),
                "nofx": exact("5", None, true),
                "quarantined": exact("50", Some("1"), false),
            }
        }))
        .unwrap();
        let industries = [("600036.SS".to_string(), "银行".to_string())].into();
        let result = super::allocation(&holdings, &accounting, "CNY", &industries).unwrap();
        fn pairs(slices: &[super::AllocationSlice]) -> Vec<(&str, &str, &str)> {
            slices
                .iter()
                .map(|s| (s.label.as_str(), s.value.as_str(), s.pct.as_str()))
                .collect()
        }
        assert_eq!(
            pairs(&result.by_class),
            [
                ("现金", "390", "43.8"),
                ("股票", "390", "43.8"),
                ("基金", "110", "12.4")
            ]
        );
        assert_eq!(
            pairs(&result.by_industry),
            [
                ("现金", "390", "43.8"),
                ("银行", "300", "33.7"),
                ("基金", "110", "12.4"),
                (super::OTHER_STOCKS, "90", "10.1")
            ]
        );
        assert_eq!(result.missing, 2);
    }

    #[test]
    fn allocation_folds_small_groups_into_one_slice() {
        let groups = (1..=9)
            .map(|i| (format!("行业{i}"), rust_decimal::Decimal::from(i)))
            .collect();
        let slices = super::slices(groups);
        assert_eq!(slices.len(), 7);
        assert_eq!(slices[6].label, "其余");
        assert_eq!(slices[6].value, "6");
    }

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
