//! User-owned, authorized current-state projection. Ledger records never cross this boundary.
use crate::models::portfolio::{Holding, ProfileField, Snapshot};
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) struct AuthorizedFinancialContext {
    pub facts: Value,
    pub companies: Vec<(String, String)>,
    pub holding_count: usize,
    pub valuation_evidence: Vec<crate::models::portfolio::HoldingValuationEvidence>,
}
fn decimal(value: &str) -> Option<Decimal> {
    Decimal::from_str_exact(value).ok()
}
fn text(value: Decimal) -> String {
    value.normalize().to_string()
}
fn insert(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        map.insert(key.into(), json!(value));
    }
}

impl AuthorizedFinancialContext {
    pub fn from_snapshot(
        snapshot: &Snapshot,
        include_profile: bool,
        include_holdings: bool,
    ) -> Self {
        let mut summary = Map::new();
        if include_profile || include_holdings {
            summary.insert("baseCurrency".into(), json!(snapshot.profile.base_currency));
        }
        if include_profile {
            let p = &snapshot.profile;
            let exact = p
                .exact_amounts
                .clone()
                .unwrap_or_else(|| super::ProfileAmounts {
                    monthly_income: p.monthly_income.to_string(),
                    monthly_expense: p.monthly_expense.to_string(),
                    emergency_fund: p.emergency_fund.to_string(),
                    liabilities: p.liabilities.to_string(),
                });
            let amount = |field, value: &str| {
                p.field_provided(field)
                    .then(|| decimal(value))
                    .flatten()
                    .filter(|v| *v >= Decimal::ZERO)
            };
            let income = amount(ProfileField::MonthlyIncome, &exact.monthly_income);
            let expense = amount(ProfileField::MonthlyExpense, &exact.monthly_expense);
            let emergency = amount(ProfileField::EmergencyFund, &exact.emergency_fund);
            for (key, value) in [
                ("monthlyIncome", income),
                ("monthlyExpense", expense),
                ("emergencyFund", emergency),
                (
                    "liabilities",
                    amount(ProfileField::Liabilities, &exact.liabilities),
                ),
                (
                    "monthlySurplus",
                    income.zip(expense).and_then(|(a, b)| a.checked_sub(b)),
                ),
                (
                    "emergencyMonths",
                    emergency
                        .zip(expense)
                        .and_then(|(a, b)| a.checked_div(b))
                        .map(|v| v.round_dp(2)),
                ),
            ] {
                insert(&mut summary, key, value.map(text));
            }
            if p.field_provided(ProfileField::HorizonYears) {
                summary.insert("horizonYears".into(), json!(p.horizon_years));
            }
            if p.field_provided(ProfileField::MaxDrawdownPct) {
                insert(
                    &mut summary,
                    "maxDrawdownPct",
                    Some(p.max_drawdown_pct.to_string()),
                );
            }
        }
        let mut facts = Map::new();
        let mut companies = Vec::new();
        if include_holdings {
            if let Some(a) = &snapshot.accounting {
                insert(&mut summary, "portfolioValue", a.total_value.clone());
                for (key, metric) in [
                    ("investmentValue", &a.investments.value),
                    ("cashValue", &a.cash.value),
                ] {
                    if metric.missing == 0 {
                        insert(&mut summary, key, metric.value.clone());
                    }
                }
            }
            // Group the same instrument across accounts; option contracts retain their own identity.
            let mut groups = BTreeMap::<String, Vec<&Holding>>::new();
            for h in &snapshot.holdings {
                let identity = if let Some(c) = snapshot.option_contracts.get(&h.id) {
                    let underlying = snapshot
                        .holdings
                        .iter()
                        .find(|h| h.id == c.underlying_id)
                        .map(|h| h.symbol.as_str())
                        .unwrap_or(&c.underlying_id);
                    format!(
                        "{underlying}|{:?}|{:?}|{}|{}|{}",
                        c.option_type, c.side, c.strike, c.expires_on, c.multiplier
                    )
                } else if h.symbol.trim().is_empty() {
                    h.id.clone()
                } else {
                    crate::models::market::canonical_symbol(&h.symbol)
                        .unwrap_or_else(|_| h.symbol.clone())
                };
                groups
                    .entry(format!("{identity}|{}|{}", h.currency, h.asset_class))
                    .or_default()
                    .push(h);
            }
            let total = snapshot
                .accounting
                .as_ref()
                .and_then(|a| a.total_value.as_deref())
                .and_then(decimal)
                .filter(|v| *v > Decimal::ZERO);
            let mut holdings = Map::new();
            for mut group in groups.into_values() {
                group.sort_by(|a, b| a.id.cmp(&b.id));
                let h = group[0];
                let sources = group
                    .iter()
                    .map(|h| {
                        snapshot
                            .accounting
                            .as_ref()
                            .and_then(|a| a.holdings.get(&h.id))
                            .filter(|a| a.usable)
                    })
                    .collect::<Vec<_>>();
                let all_usable = sources.iter().all(Option::is_some);
                let sum = |field: fn(&super::AccountingHolding) -> Option<&str>| {
                    sources.iter().try_fold(Decimal::ZERO, |sum, source| {
                        sum.checked_add(decimal(field(source.as_ref().copied()?)?)?)
                    })
                };
                let market_value = sum(|a| Some(&a.amounts.market_value));
                let cost_basis = sum(|a| Some(&a.amounts.cost_basis));
                let current_quantity = sum(|a| a.amounts.quantity.as_deref());
                let unrealized_profit = sum(|a| a.unrealized_profit.as_deref());
                let is_cash = h.asset_class == "现金";
                // Omit closed positions and zero-balance accounts from current holdings.
                let is_closed_or_empty = all_usable
                    && market_value.is_some_and(|v| v.is_zero())
                    && (is_cash
                        || current_quantity.is_some_and(|q| q.is_zero())
                        || (current_quantity.is_none() && cost_basis.is_some_and(|c| c.is_zero())));
                if is_closed_or_empty {
                    continue;
                }
                companies.push((h.name.clone(), h.symbol.clone()));
                let mut row = Map::new();
                for (key, value) in [
                    ("name", &h.name),
                    ("symbol", &h.symbol),
                    ("currency", &h.currency),
                ] {
                    insert(&mut row, key, Some(value.clone()));
                }
                for (key, value) in [
                    ("marketValue", market_value),
                    (
                        "costBasis",
                        cost_basis.filter(|c| !(is_cash && c.is_zero())),
                    ),
                    ("currentQuantity", current_quantity),
                    ("unrealizedProfit", unrealized_profit),
                ] {
                    insert(&mut row, key, value.map(text));
                }
                let base = sources.iter().try_fold(Decimal::ZERO, |sum, source| {
                    let a = source.as_ref().copied()?;
                    sum.checked_add(
                        decimal(&a.amounts.market_value)?
                            .checked_mul(decimal(a.fx_rate.as_deref()?)?)?,
                    )
                });
                let weight = base
                    .zip(total)
                    .and_then(|(base, total)| base.checked_div(total))
                    .and_then(|w| w.checked_mul(Decimal::ONE_HUNDRED));
                insert(&mut row, "weightPct", weight.map(|v| text(v.round_dp(2))));
                if row.contains_key("currentQuantity") {
                    insert(
                        &mut row,
                        "quantityUnit",
                        Some(
                            match h.asset_class.as_str() {
                                "股票" => "股",
                                "期权" => "张",
                                _ => "份",
                            }
                            .into(),
                        ),
                    );
                }
                if let Some(c) = snapshot.option_contracts.get(&h.id) {
                    insert(
                        &mut row,
                        "contract",
                        Some(format!(
                            "{:?} {:?}; strike={} {}; expires={}; multiplier={}",
                            c.side, c.option_type, c.strike, h.currency, c.expires_on, c.multiplier
                        )),
                    );
                }
                holdings.insert(h.id.clone(), Value::Object(row));
            }
            facts.insert("holdings".into(), Value::Object(holdings));
        }
        facts.insert("summary".into(), Value::Object(summary));
        Self {
            facts: Value::Object(facts),
            holding_count: companies.len(),
            companies,
            valuation_evidence: if include_holdings {
                snapshot.holding_valuations.clone()
            } else {
                Vec::new()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/qa_eval/snapshot.json"
        ))
        .unwrap()
    }
    #[test]
    fn instruments_aggregate_across_accounts_but_not_currencies_and_keep_exact_values() {
        let mut s = snapshot();
        let mut h = s
            .holdings
            .iter()
            .find(|h| h.id == "fund-a")
            .unwrap()
            .clone();
        let mut source = s.accounting.as_ref().unwrap().holdings["fund-a"].clone();
        s.accounting
            .as_mut()
            .unwrap()
            .holdings
            .get_mut("fund-a")
            .unwrap()
            .amounts
            .quantity = Some("100.00000001".into());
        h.id = "fund-a-2".into();
        source.amounts.quantity = Some("200".into());
        source.amounts.market_value = "200000".into();
        source.amounts.cost_basis = "300000".into();
        source.unrealized_profit = Some("-100000".into());
        s.holdings.push(h.clone());
        s.accounting
            .as_mut()
            .unwrap()
            .holdings
            .insert(h.id.clone(), source.clone());
        h.id = "fund-a-usd".into();
        h.currency = "USD".into();
        s.holdings.push(h.clone());
        s.accounting
            .as_mut()
            .unwrap()
            .holdings
            .insert(h.id.clone(), source);
        s.price_refresh_errors.push("PRIVATE_DIAGNOSTIC".into());
        let f = AuthorizedFinancialContext::from_snapshot(&s, true, true);
        assert_eq!(
            f.facts["holdings"]["fund-a"]["currentQuantity"],
            "300.00000001"
        );
        assert_eq!(f.facts["holdings"]["fund-a"]["marketValue"], "1000000");
        assert_eq!(f.facts["holdings"]["fund-a"]["costBasis"], "1300000");
        assert_eq!(f.facts["holdings"]["fund-a"]["unrealizedProfit"], "-300000");
        assert!(f.facts["holdings"].get("fund-a-2").is_none());
        assert_eq!(f.facts["holdings"]["fund-a-usd"]["currency"], "USD");
        for forbidden in [
            "PRIVATE_DIAGNOSTIC",
            "dataIssues",
            "null",
            "recentTrades",
            "tradeBasis",
            "needsReconciliation",
        ] {
            assert!(!f.facts.to_string().contains(forbidden));
        }
        let hidden = AuthorizedFinancialContext::from_snapshot(&s, false, false);
        assert_eq!(hidden.facts, serde_json::json!({"summary":{}}));
        assert!(hidden.companies.is_empty());
        assert!(hidden.valuation_evidence.is_empty());
    }
    #[test]
    fn incomplete_groups_omit_unknown_values_and_preserve_other_available_states() {
        let mut s = snapshot();
        let mut h = s
            .holdings
            .iter()
            .find(|h| h.id == "fund-a")
            .unwrap()
            .clone();
        h.id = "fund-a-2".into();
        let mut source = s.accounting.as_ref().unwrap().holdings["fund-a"].clone();
        source.usable = false;
        s.holdings.push(h.clone());
        s.accounting.as_mut().unwrap().holdings.insert(h.id, source);
        let f = AuthorizedFinancialContext::from_snapshot(&s, true, true);
        assert!(f.facts["holdings"]["fund-a"].get("marketValue").is_none());
        assert!(f.facts["holdings"]["fund-a"].get("costBasis").is_none());
        assert_eq!(f.facts["summary"]["cashValue"], "10090");
        assert!(f.facts["summary"].get("portfolioValue").is_none());
    }
    #[test]
    fn omits_closed_positions_zero_cash_and_zero_cash_cost_basis() {
        let mut s = snapshot();
        let mut closed = s
            .holdings
            .iter()
            .find(|h| h.id == "fund-a")
            .unwrap()
            .clone();
        closed.id = "sold-stock".into();
        closed.symbol = "600000.SS".into();
        closed.name = "已清仓公司".into();
        closed.asset_class = "股票".into();
        let mut closed_acct = s.accounting.as_ref().unwrap().holdings["fund-a"].clone();
        closed_acct.amounts.quantity = Some("0".into());
        closed_acct.amounts.market_value = "0".into();
        closed_acct.amounts.cost_basis = "0".into();
        closed_acct.unrealized_profit = Some("0".into());
        s.holdings.push(closed);
        s.accounting
            .as_mut()
            .unwrap()
            .holdings
            .insert("sold-stock".into(), closed_acct);

        let mut empty_cash = s
            .holdings
            .iter()
            .find(|h| h.id == "cash-cny")
            .unwrap()
            .clone();
        empty_cash.id = "cash-usd-empty".into();
        empty_cash.symbol = "CASH-USD".into();
        empty_cash.currency = "USD".into();
        empty_cash.name = "美元存款".into();
        let mut empty_cash_acct = s.accounting.as_ref().unwrap().holdings["cash-cny"].clone();
        empty_cash_acct.amounts.market_value = "0".into();
        empty_cash_acct.amounts.cost_basis = "0".into();
        s.holdings.push(empty_cash);
        s.accounting
            .as_mut()
            .unwrap()
            .holdings
            .insert("cash-usd-empty".into(), empty_cash_acct);

        let f = AuthorizedFinancialContext::from_snapshot(&s, true, true);
        let holdings = f.facts["holdings"].as_object().unwrap();
        assert!(!holdings.contains_key("sold-stock"));
        assert!(!holdings.contains_key("cash-usd-empty"));
        assert!(holdings.contains_key("cash-cny"));
        assert!(holdings["cash-cny"].get("costBasis").is_none());
        assert_eq!(f.holding_count, holdings.len());
        assert!(!f.companies.iter().any(|(_, sym)| sym == "600000.SS"));
    }
}
