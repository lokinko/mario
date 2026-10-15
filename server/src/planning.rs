use crate::models::{FinancialProfile, Holding, PortfolioPlan, RebalanceAction};

pub fn analyze(profile: &FinancialProfile, holdings: &[Holding]) -> PortfolioPlan {
    use rust_decimal::{prelude::ToPrimitive, Decimal};
    let income = profile
        .exact_amounts
        .as_ref()
        .map(|a| a.monthly_income.clone())
        .unwrap_or_else(|| profile.monthly_income.to_string());
    let expense = profile
        .exact_amounts
        .as_ref()
        .map(|a| a.monthly_expense.clone())
        .unwrap_or_else(|| profile.monthly_expense.to_string());
    let exact_monthly_surplus = Decimal::from_str_exact(&income)
        .ok()
        .zip(Decimal::from_str_exact(&expense).ok())
        .map(|(a, b)| (a - b).max(Decimal::ZERO).normalize().to_string());
    let monthly_surplus = exact_monthly_surplus
        .as_deref()
        .and_then(|v| Decimal::from_str_exact(v).ok())
        .and_then(|v| v.to_f64())
        .unwrap_or(0.0);
    let has_portfolio = holdings.iter().any(|holding| holding.market_value > 0.0);
    let (annual_return, annual_volatility) = portfolio_assumptions(profile, holdings);
    let emergency_months = if profile.monthly_expense > 0.0 {
        profile.emergency_fund / profile.monthly_expense
    } else {
        0.0
    };
    let mut risk_capacity = profile.max_drawdown_pct.clamp(0.0, 100.0);
    if profile.horizon_years <= 2 {
        risk_capacity = risk_capacity.min(8.0);
    }
    if profile.monthly_expense > 0.0 && emergency_months < 3.0 {
        risk_capacity *= 0.7;
    }
    if profile.monthly_income > 0.0 && profile.liabilities > profile.monthly_income * 12.0 {
        risk_capacity *= 0.85;
    }
    let stress_loss = if has_portfolio {
        (annual_volatility * 200.0).min(100.0)
    } else {
        0.0
    };
    let risk_status = if !has_portfolio {
        "insufficient"
    } else if stress_loss <= risk_capacity {
        "within"
    } else if stress_loss <= risk_capacity * 1.2 {
        "near"
    } else {
        "over"
    };

    PortfolioPlan {
        exact_monthly_surplus,
        monthly_surplus,
        modeled_annual_return_pct: annual_return * 100.0,
        modeled_annual_volatility_pct: annual_volatility * 100.0,
        stress_loss_pct: stress_loss,
        risk_capacity_pct: risk_capacity,
        risk_status: risk_status.into(),
        rebalancing: rebalance_actions(holdings),
        assumptions: "基于当前资产类别的长期示例假设估算风险与再平衡偏差；不含税费、通胀、资产相关性突变和未来收入变化，不是收益预测。".into(),
    }
}

fn portfolio_assumptions(profile: &FinancialProfile, holdings: &[Holding]) -> (f64, f64) {
    let total: f64 = holdings.iter().map(|holding| holding.market_value).sum();
    if total <= 0.0 {
        return match profile.risk_level.as_str() {
            "保守" => (0.03, 0.05),
            "均衡" => (0.055, 0.12),
            "进取" => (0.07, 0.17),
            _ => (0.042, 0.08),
        };
    }

    let mut expected_return = 0.0;
    let mut linear_volatility = 0.0;
    let mut independent_variance = 0.0;
    for holding in holdings {
        let weight = holding.market_value / total;
        let (asset_return, asset_volatility) = asset_assumptions(&holding.asset_class);
        expected_return += weight * asset_return;
        linear_volatility += weight * asset_volatility;
        independent_variance += (weight * asset_volatility).powi(2);
    }
    // Blend a diversified and a fully correlated estimate. This intentionally avoids
    // presenting category labels as a precise covariance model.
    let volatility = independent_variance.sqrt() * 0.4 + linear_volatility * 0.6;
    (expected_return, volatility)
}

fn asset_assumptions(asset_class: &str) -> (f64, f64) {
    match asset_class {
        "现金" => (0.02, 0.01),
        "债券" | "固收" => (0.035, 0.06),
        "股票" | "权益" => (0.07, 0.20),
        "基金" => (0.055, 0.14),
        "黄金" => (0.04, 0.18),
        "另类" => (0.045, 0.20),
        _ => (0.03, 0.20),
    }
}

fn rebalance_actions(holdings: &[Holding]) -> Vec<RebalanceAction> {
    let total: f64 = holdings.iter().map(|holding| holding.market_value).sum();
    let target_total: f64 = holdings.iter().map(|holding| holding.target_pct).sum();
    if total <= 0.0 || !(99.0..=101.0).contains(&target_total) {
        return Vec::new();
    }

    let mut actions: Vec<_> = holdings
        .iter()
        .filter_map(|holding| {
            let current_pct = holding.market_value / total * 100.0;
            let deviation = current_pct - holding.target_pct;
            if deviation.abs() < 3.0 {
                return None;
            }
            let amount = holding.target_pct / 100.0 * total - holding.market_value;
            Some(RebalanceAction {
                exact_amount: None,
                holding_id: holding.id.clone(),
                name: holding.name.clone(),
                current_pct,
                target_pct: holding.target_pct,
                deviation_pct: deviation,
                amount: amount.abs(),
                direction: if amount >= 0.0 { "增加" } else { "减少" }.into(),
            })
        })
        .collect();
    actions.sort_by(|a, b| b.deviation_pct.abs().total_cmp(&a.deviation_pct.abs()));
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebalancing_requires_complete_targets_and_ignores_small_drift() {
        let holdings = vec![
            Holding {
                id: "one".into(),
                symbol: "A".into(),
                name: "A".into(),
                asset_class: "股票".into(),
                market_value: 70.0,
                cost_basis: 60.0,
                cost_price: None,
                quantity: None,
                current_price: None,
                target_pct: 60.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
            Holding {
                id: "two".into(),
                symbol: "B".into(),
                name: "B".into(),
                asset_class: "债券".into(),
                market_value: 30.0,
                cost_basis: 30.0,
                cost_price: None,
                quantity: None,
                current_price: None,
                target_pct: 40.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        ];
        let actions = rebalance_actions(&holdings);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].direction, "减少");
        assert_eq!(actions[1].direction, "增加");
    }

    #[test]
    fn stress_loss_uses_percentage_units_and_respects_capacity() {
        let profile = FinancialProfile {
            max_drawdown_pct: 25.0,
            ..Default::default()
        };
        let holdings = vec![
            Holding {
                id: "one".into(),
                symbol: "A".into(),
                name: "权益".into(),
                asset_class: "股票".into(),
                market_value: 70.0,
                cost_basis: 60.0,
                cost_price: None,
                quantity: None,
                current_price: None,
                target_pct: 60.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
            Holding {
                id: "two".into(),
                symbol: "B".into(),
                name: "债券".into(),
                asset_class: "债券".into(),
                market_value: 30.0,
                cost_basis: 30.0,
                cost_price: None,
                quantity: None,
                current_price: None,
                target_pct: 40.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        ];
        let plan = analyze(&profile, &holdings);
        assert!(plan.stress_loss_pct > 30.0);
        assert_eq!(plan.risk_status, "over");
    }

    #[test]
    fn empty_portfolio_does_not_claim_risk_alignment() {
        let plan = analyze(&FinancialProfile::default(), &[]);
        assert_eq!(plan.stress_loss_pct, 0.0);
        assert_eq!(plan.risk_status, "insufficient");
    }
}
