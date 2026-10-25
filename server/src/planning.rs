use crate::models::{
    accounting::AccountingSnapshot, DataIssue, FinancialProfile, Holding, IssueCode, PortfolioPlan,
    RebalanceAction,
};

pub fn analyze(
    profile: &FinancialProfile,
    holdings: &[Holding],
    issues: &[DataIssue],
    excluded: &[String],
    accounting: Option<&AccountingSnapshot>,
) -> crate::AppResult<PortfolioPlan> {
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

    let target_total_pct: f64 = holdings.iter().map(|h| h.target_pct).sum();
    let mut rebalancing_blocked_by = Vec::new();
    for issue in issues.iter().filter(|i| i.code.blocks_portfolio_value()) {
        if !rebalancing_blocked_by.contains(&issue.code) {
            rebalancing_blocked_by.push(issue.code);
        }
    }
    if (target_total_pct - 100.0).abs() > 1e-8 && !holdings.is_empty() {
        rebalancing_blocked_by.push(IssueCode::InvalidTargetWeights);
    }
    Ok(PortfolioPlan {
        exact_monthly_surplus,
        monthly_surplus,
        modeled_annual_return_pct: annual_return * 100.0,
        modeled_annual_volatility_pct: annual_volatility * 100.0,
        stress_loss_pct: stress_loss,
        risk_capacity_pct: risk_capacity,
        risk_status: risk_status.into(),
        rebalancing: if rebalancing_blocked_by.is_empty() { rebalance_actions(profile, holdings, excluded, accounting)? } else { Vec::new() },
        rebalancing_blocked_by,
        excluded_holding_ids: excluded.to_vec(),
        as_of: holdings.iter().map(|h| h.valuation_date.as_str()).filter(|d| !d.is_empty()).min().map(str::to_owned),
        target_total_pct,
        assumptions: "基于当前资产类别的长期示例假设估算风险与再平衡偏差；不含税费、通胀、资产相关性突变和未来收入变化，不是收益预测。".into(),
    })
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

fn rebalance_actions(
    profile: &FinancialProfile,
    holdings: &[Holding],
    excluded: &[String],
    accounting: Option<&AccountingSnapshot>,
) -> crate::AppResult<Vec<RebalanceAction>> {
    use rust_decimal::{prelude::ToPrimitive, Decimal, RoundingStrategy};
    let dec = |s: &str| {
        Decimal::from_str_exact(s).map_err(|_| crate::AppError::Validation("再平衡金额无效".into()))
    };
    let total = if let Some(accounting) = accounting {
        match accounting.total_value.as_deref() {
            Some(v) => dec(v)?,
            None => return Ok(Vec::new()),
        }
    } else {
        holdings.iter().try_fold(Decimal::ZERO, |sum, h| {
            dec(&h.market_value.to_string()).map(|v| sum + v)
        })?
    };
    if total <= Decimal::ZERO {
        return Ok(Vec::new());
    }
    let mut actions = Vec::new();
    for holding in holdings.iter().filter(|h| !excluded.contains(&h.id)) {
        let value = if let Some(accounting) = accounting {
            let source = &accounting.holdings[&holding.id];
            dec(&source.amounts.market_value)?
                .checked_mul(dec(source.fx_rate.as_deref().unwrap_or("0"))?)
        } else {
            Some(dec(&holding.market_value.to_string())?)
        }
        .ok_or_else(|| crate::AppError::Validation("再平衡金额超出范围".into()))?;
        let current_pct = (value / total * Decimal::from(100))
            .to_f64()
            .unwrap_or_default();
        let deviation_pct = current_pct - holding.target_pct;
        if deviation_pct.abs() < 3.0 {
            continue;
        }
        let target = total
            .checked_mul(dec(&holding.target_pct.to_string())? / Decimal::from(100))
            .ok_or_else(|| crate::AppError::Validation("再平衡金额超出范围".into()))?;
        let difference = target - value;
        let amount = difference.abs().round_dp_with_strategy(
            if profile.base_currency == "JPY" { 0 } else { 2 },
            RoundingStrategy::MidpointAwayFromZero,
        );
        actions.push(RebalanceAction {
            exact_amount: Some(amount.normalize().to_string()),
            holding_id: holding.id.clone(),
            name: holding.name.clone(),
            current_pct,
            target_pct: holding.target_pct,
            deviation_pct,
            amount: amount
                .to_f64()
                .ok_or_else(|| crate::AppError::Validation("再平衡金额超出范围".into()))?,
            direction: if difference >= Decimal::ZERO {
                "增加"
            } else {
                "减少"
            }
            .into(),
        });
    }
    actions.sort_by(|a, b| b.deviation_pct.abs().total_cmp(&a.deviation_pct.abs()));
    Ok(actions)
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
        let actions =
            rebalance_actions(&FinancialProfile::default(), &holdings, &[], None).unwrap();
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
        let plan = analyze(&profile, &holdings, &[], &[], None).unwrap();
        assert!(plan.stress_loss_pct > 30.0);
        assert_eq!(plan.risk_status, "over");
    }

    #[test]
    fn empty_portfolio_does_not_claim_risk_alignment() {
        let plan = analyze(&FinancialProfile::default(), &[], &[], &[], None).unwrap();
        assert_eq!(plan.stress_loss_pct, 0.0);
        assert_eq!(plan.risk_status, "insufficient");
    }
}
