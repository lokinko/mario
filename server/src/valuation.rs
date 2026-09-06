use std::collections::BTreeMap;

use crate::models::{Holding, PortfolioAllocationChange, PortfolioValuationStatus};

pub fn normalize_holding(holding: &Holding, base_currency: &str) -> Option<Holding> {
    let rate = if holding.currency.eq_ignore_ascii_case(base_currency) {
        1.0
    } else {
        holding
            .fx_rate_to_base
            .filter(|rate| rate.is_finite() && *rate > 0.0)?
    };
    let mut normalized = holding.clone();
    normalized.market_value *= rate;
    normalized.cost_basis *= rate;
    normalized.currency = base_currency.to_ascii_uppercase();
    normalized.fx_rate_to_base = Some(1.0);
    Some(normalized)
}

pub fn status(base_currency: &str, holdings: &[Holding]) -> PortfolioValuationStatus {
    let base_currency = base_currency.trim().to_ascii_uppercase();
    let mut missing_fx_holdings = holdings
        .iter()
        .filter(|holding| {
            !holding.currency.eq_ignore_ascii_case(&base_currency)
                && holding
                    .fx_rate_to_base
                    .is_none_or(|rate| !rate.is_finite() || rate <= 0.0)
        })
        .map(|holding| holding.name.clone())
        .collect::<Vec<_>>();
    missing_fx_holdings.sort();
    let undated_holding_count = holdings
        .iter()
        .filter(|holding| holding.valuation_date.trim().is_empty())
        .count();
    let mut valuation_dates = holdings
        .iter()
        .map(|holding| holding.valuation_date.trim())
        .filter(|date| !date.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    valuation_dates.sort();
    valuation_dates.dedup();
    let aligned_valuation_date = (undated_holding_count == 0 && valuation_dates.len() == 1)
        .then(|| valuation_dates[0].clone());
    let mut warnings = Vec::new();
    if !missing_fx_holdings.is_empty() {
        warnings.push("存在未折算到基准币种的外币持仓".into());
    }
    if undated_holding_count > 0 {
        warnings.push("存在缺少估值日期的持仓".into());
    }
    if valuation_dates.len() > 1 {
        warnings.push("持仓估值日期不一致".into());
    }
    PortfolioValuationStatus {
        base_currency,
        comparable: missing_fx_holdings.is_empty(),
        missing_fx_holdings,
        undated_holding_count,
        valuation_dates,
        aligned_valuation_date,
        warnings,
    }
}

pub fn allocation_changes(
    previous_holdings: &[Holding],
    current_holdings: &[Holding],
    base_currency: &str,
) -> Vec<PortfolioAllocationChange> {
    let aggregate = |holdings: &[Holding]| {
        let mut values = BTreeMap::<String, f64>::new();
        for holding in holdings {
            if let Some(normalized) = normalize_holding(holding, base_currency) {
                *values.entry(holding.asset_class.clone()).or_default() += normalized.market_value;
            }
        }
        values
    };
    let previous = aggregate(previous_holdings);
    let current = aggregate(current_holdings);
    let previous_total = previous.values().sum::<f64>();
    let current_total = current.values().sum::<f64>();
    let mut categories = BTreeMap::<String, ()>::new();
    for category in previous.keys().chain(current.keys()) {
        categories.insert(category.clone(), ());
    }
    let mut changes = categories
        .into_keys()
        .map(|asset_class| {
            let previous_value = previous.get(&asset_class).copied().unwrap_or_default();
            let current_value = current.get(&asset_class).copied().unwrap_or_default();
            let previous_pct = if previous_total > 0.0 {
                previous_value / previous_total * 100.0
            } else {
                0.0
            };
            let current_pct = if current_total > 0.0 {
                current_value / current_total * 100.0
            } else {
                0.0
            };
            PortfolioAllocationChange {
                asset_class,
                previous_value,
                current_value,
                value_change: current_value - previous_value,
                previous_pct,
                current_pct,
                pct_point_change: current_pct - previous_pct,
            }
        })
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| {
        right
            .value_change
            .abs()
            .total_cmp(&left.value_change.abs())
            .then_with(|| left.asset_class.cmp(&right.asset_class))
    });
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(name: &str, currency: &str, value: f64, rate: Option<f64>, date: &str) -> Holding {
        Holding {
            id: name.into(),
            symbol: String::new(),
            name: name.into(),
            asset_class: "基金".into(),
            market_value: value,
            cost_basis: value,
            target_pct: 0.0,
            currency: currency.into(),
            fx_rate_to_base: rate,
            valuation_date: date.into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        }
    }

    #[test]
    fn reports_missing_fx_and_date_alignment_without_summing_raw_currencies() {
        let holdings = vec![
            holding("人民币", "CNY", 700.0, None, "2026-09-01"),
            holding("美元", "USD", 100.0, None, "2026-09-02"),
        ];
        let incomplete = status("CNY", &holdings);
        assert!(!incomplete.comparable);
        assert_eq!(incomplete.missing_fx_holdings, vec!["美元"]);
        assert_eq!(incomplete.valuation_dates.len(), 2);
        assert!(normalize_holding(&holdings[1], "CNY").is_none());

        let mut completed = holdings[1].clone();
        completed.fx_rate_to_base = Some(7.0);
        completed.valuation_date = "2026-09-01".into();
        assert_eq!(
            normalize_holding(&completed, "CNY").unwrap().market_value,
            700.0
        );
        let complete_status = status("CNY", &[holdings[0].clone(), completed]);
        assert!(complete_status.comparable);
        assert_eq!(
            complete_status.aligned_valuation_date.as_deref(),
            Some("2026-09-01")
        );
    }
}
