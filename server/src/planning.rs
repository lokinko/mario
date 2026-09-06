use chrono::{Datelike, NaiveDate, Utc};

use crate::models::{
    FinancialProfile, Goal, GoalProjection, Holding, PortfolioPlan, RebalanceAction,
};

const SIMULATIONS: usize = 2_000;

pub fn analyze(profile: &FinancialProfile, goals: &[Goal], holdings: &[Holding]) -> PortfolioPlan {
    analyze_at(profile, goals, holdings, Utc::now().date_naive())
}

fn analyze_at(
    profile: &FinancialProfile,
    goals: &[Goal],
    holdings: &[Holding],
    today: NaiveDate,
) -> PortfolioPlan {
    let monthly_surplus = (profile.monthly_income - profile.monthly_expense).max(0.0);
    let committed_monthly = goals.iter().map(|goal| goal.monthly_contribution).sum();
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

    let goal_projections = goals
        .iter()
        .map(|goal| project_goal(goal, today, annual_return, annual_volatility))
        .collect();

    PortfolioPlan {
        monthly_surplus,
        committed_monthly,
        modeled_annual_return_pct: annual_return * 100.0,
        modeled_annual_volatility_pct: annual_volatility * 100.0,
        stress_loss_pct: stress_loss,
        risk_capacity_pct: risk_capacity,
        risk_status: risk_status.into(),
        goal_projections,
        rebalancing: rebalance_actions(holdings),
        assumptions: format!(
            "基于当前资产类别的长期示例假设，执行 {SIMULATIONS} 次可重复模拟；不含税费、通胀、资产相关性突变和未来收入变化，不是收益预测。"
        ),
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

fn project_goal(
    goal: &Goal,
    today: NaiveDate,
    annual_return: f64,
    annual_volatility: f64,
) -> GoalProjection {
    let target_date = NaiveDate::parse_from_str(&goal.target_date, "%Y-%m-%d").ok();
    let months = target_date
        .map(|date| months_between(today, date))
        .unwrap_or(0);
    let funded_pct = if goal.target_amount > 0.0 {
        goal.current_amount / goal.target_amount * 100.0
    } else {
        0.0
    };

    if months <= 0 || goal.target_amount <= 0.0 || goal.current_amount >= goal.target_amount {
        let reached = goal.current_amount >= goal.target_amount && goal.target_amount > 0.0;
        return GoalProjection {
            goal_id: goal.id.clone(),
            name: goal.name.clone(),
            months_remaining: months.max(0),
            funded_pct,
            estimated_success_pct: if reached { 100.0 } else { 0.0 },
            conservative_amount: goal.current_amount,
            median_amount: goal.current_amount,
            required_monthly_contribution: if reached { 0.0 } else { goal.target_amount },
            monthly_gap: if reached { 0.0 } else { goal.target_amount },
            status: if reached { "reached" } else { "expired" }.into(),
        };
    }

    let conservative_planning_return = (annual_return - annual_volatility * 0.75).max(-0.02);
    let required = required_monthly_contribution(
        goal.current_amount,
        goal.target_amount,
        months,
        conservative_planning_return,
    );
    let monthly_gap = (required - goal.monthly_contribution).max(0.0);
    let mut terminal_values = Vec::with_capacity(SIMULATIONS);
    let mut successes = 0_usize;
    let mut random = DeterministicRng::new(stable_seed(&goal.id));
    let monthly_drift = (annual_return - 0.5 * annual_volatility.powi(2)) / 12.0;
    let monthly_volatility = annual_volatility / 12.0_f64.sqrt();

    for _ in 0..SIMULATIONS {
        let mut balance = goal.current_amount;
        for _ in 0..months {
            let growth = (monthly_drift + monthly_volatility * random.normal()).exp();
            balance = balance * growth + goal.monthly_contribution;
        }
        if balance >= goal.target_amount {
            successes += 1;
        }
        terminal_values.push(balance);
    }
    terminal_values.sort_by(f64::total_cmp);
    let success_pct = successes as f64 / SIMULATIONS as f64 * 100.0;
    let status = if success_pct >= 75.0 {
        "on-track"
    } else if success_pct >= 50.0 {
        "watch"
    } else {
        "off-track"
    };

    GoalProjection {
        goal_id: goal.id.clone(),
        name: goal.name.clone(),
        months_remaining: months,
        funded_pct,
        estimated_success_pct: success_pct,
        conservative_amount: percentile(&terminal_values, 0.1),
        median_amount: percentile(&terminal_values, 0.5),
        required_monthly_contribution: required,
        monthly_gap,
        status: status.into(),
    }
}

fn months_between(start: NaiveDate, end: NaiveDate) -> i64 {
    if end <= start {
        return 0;
    }
    let calendar_months =
        (end.year() - start.year()) as i64 * 12 + end.month() as i64 - start.month() as i64;
    if end.day() > start.day() {
        calendar_months + 1
    } else {
        calendar_months.max(1)
    }
}

fn required_monthly_contribution(
    current: f64,
    target: f64,
    months: i64,
    annual_return: f64,
) -> f64 {
    if months <= 0 {
        return (target - current).max(0.0);
    }
    let monthly_rate = annual_return / 12.0;
    let growth = (1.0 + monthly_rate).powi(months as i32);
    let remaining = (target - current * growth).max(0.0);
    if monthly_rate.abs() < 1e-9 {
        remaining / months as f64
    } else {
        remaining * monthly_rate / (growth - 1.0)
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

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index]
}

fn stable_seed(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn uniform(&mut self) -> f64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        ((value >> 11) as f64 + 0.5) / ((1_u64 << 53) as f64)
    }

    fn normal(&mut self) -> f64 {
        let first = self.uniform().max(f64::MIN_POSITIVE);
        let second = self.uniform();
        (-2.0 * first.ln()).sqrt() * (std::f64::consts::TAU * second).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(monthly_contribution: f64) -> Goal {
        Goal {
            id: "goal-1".into(),
            name: "养老".into(),
            target_amount: 500_000.0,
            current_amount: 100_000.0,
            monthly_contribution,
            target_date: "2036-01-01".into(),
            priority: "重要".into(),
        }
    }

    #[test]
    fn higher_contributions_improve_goal_success() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let profile = FinancialProfile::default();
        let low = analyze_at(&profile, &[goal(500.0)], &[], today);
        let high = analyze_at(&profile, &[goal(3_000.0)], &[], today);
        assert!(
            high.goal_projections[0].estimated_success_pct
                > low.goal_projections[0].estimated_success_pct
        );
        assert!(high.goal_projections[0].monthly_gap < low.goal_projections[0].monthly_gap);
    }

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
                target_pct: 60.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
            },
            Holding {
                id: "two".into(),
                symbol: "B".into(),
                name: "B".into(),
                asset_class: "债券".into(),
                market_value: 30.0,
                cost_basis: 30.0,
                target_pct: 40.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
            },
        ];
        let actions = rebalance_actions(&holdings);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].direction, "减少");
        assert_eq!(actions[1].direction, "增加");
    }

    #[test]
    fn projection_is_repeatable() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let profile = FinancialProfile::default();
        let first = analyze_at(&profile, &[goal(2_000.0)], &[], today);
        let second = analyze_at(&profile, &[goal(2_000.0)], &[], today);
        assert_eq!(
            first.goal_projections[0].estimated_success_pct,
            second.goal_projections[0].estimated_success_pct
        );
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
                target_pct: 60.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
            },
            Holding {
                id: "two".into(),
                symbol: "B".into(),
                name: "债券".into(),
                asset_class: "债券".into(),
                market_value: 30.0,
                cost_basis: 30.0,
                target_pct: 40.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
            },
        ];
        let plan = analyze_at(
            &profile,
            &[],
            &holdings,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        assert!(plan.stress_loss_pct > 30.0);
        assert_eq!(plan.risk_status, "over");
    }

    #[test]
    fn empty_portfolio_does_not_claim_risk_alignment() {
        let plan = analyze_at(
            &FinancialProfile::default(),
            &[],
            &[],
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        assert_eq!(plan.stress_loss_pct, 0.0);
        assert_eq!(plan.risk_status, "insufficient");
    }

    #[test]
    fn already_funded_goal_is_marked_reached_before_due_date() {
        let mut funded = goal(0.0);
        funded.current_amount = funded.target_amount;
        let plan = analyze_at(
            &FinancialProfile::default(),
            &[funded],
            &[],
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        assert_eq!(plan.goal_projections[0].status, "reached");
        assert_eq!(plan.goal_projections[0].required_monthly_contribution, 0.0);
    }
}
