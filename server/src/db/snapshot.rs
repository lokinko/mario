use super::{
    planning, risk, valuation, FinancialProfile, Goal, HashSet, Holding, HoldingValuationEvidence,
    Snapshot,
};

pub(super) fn build_snapshot(
    profile: FinancialProfile,
    goals: Vec<Goal>,
    holdings: Vec<Holding>,
    holding_valuations: Vec<HoldingValuationEvidence>,
    updated_at: String,
) -> Snapshot {
    let valuation_status = valuation::status(&profile.base_currency, &holdings);
    let normalized_holdings = if valuation_status.comparable {
        holdings
            .iter()
            .filter_map(|holding| valuation::normalize_holding(holding, &profile.base_currency))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let raw_total = normalized_holdings
        .iter()
        .map(|holding| holding.market_value)
        .sum::<f64>();
    let total_value = if raw_total.abs() < f64::EPSILON {
        0.0
    } else {
        raw_total
    };
    let emergency_months = if profile.monthly_expense > 0.0 {
        profile.emergency_fund / profile.monthly_expense
    } else {
        0.0
    };
    let largest = normalized_holdings
        .iter()
        .map(|h| h.market_value)
        .fold(0.0_f64, f64::max);
    let concentration_pct = if total_value > 0.0 {
        largest / total_value * 100.0
    } else {
        0.0
    };
    let mut findings = risk::analyze(
        &profile,
        if valuation_status.comparable {
            &normalized_holdings
        } else {
            &holdings
        },
        valuation_status.comparable,
    );
    if !valuation_status.missing_fx_holdings.is_empty() {
        findings.push(crate::models::RiskFinding {
            level: "high".into(),
            title: "组合缺少外币折算汇率".into(),
            detail: format!(
                "{} 尚未折算为基准币种 {}，组合总值、集中度和规划已暂停。",
                valuation_status.missing_fx_holdings.join("、"),
                valuation_status.base_currency
            ),
            action: "补充估值日对应的汇率；不要把不同币种的原始金额直接相加。".into(),
        });
    }
    if valuation_status.undated_holding_count > 0 || valuation_status.valuation_dates.len() > 1 {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "持仓估值日期尚未对齐".into(),
            detail: if valuation_status.undated_holding_count > 0 {
                format!(
                    "有 {} 项持仓缺少估值日期，不能建立可靠的周期比较基线。",
                    valuation_status.undated_holding_count
                )
            } else {
                format!(
                    "当前持仓使用了 {} 个不同估值日期，组合变化可能混入时间错位。",
                    valuation_status.valuation_dates.len()
                )
            },
            action: "把全部持仓更新到同一估值日后，再冻结组合检查点。".into(),
        });
    }
    let verified_ids = holding_valuations
        .iter()
        .map(|valuation| valuation.holding_id.as_str())
        .collect::<HashSet<_>>();
    let unverified_holdings = holdings
        .iter()
        .filter(|holding| !verified_ids.contains(holding.id.as_str()))
        .map(|holding| holding.name.as_str())
        .collect::<Vec<_>>();
    if !unverified_holdings.is_empty() {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "部分持仓仍是用户声明估值".into(),
            detail: format!(
                "{} 没有冻结数量、单位价格与外部价格来源；组合计算可继续，但不能视为已核验业绩。",
                unverified_holdings.join("、")
            ),
            action: "对有公开代码的证券查询并采用指定估值日收盘价；现金和非上市资产继续保留人工口径说明。".into(),
        });
    }
    let mut plan = planning::analyze(&profile, &goals, &normalized_holdings);
    if !valuation_status.comparable {
        plan.goal_projections.clear();
        plan.rebalancing.clear();
        plan.assumptions = format!(
            "存在未折算到 {} 的外币持仓，组合风险、目标路径和再平衡计算已暂停。",
            valuation_status.base_currency
        );
    } else if !holdings.is_empty() {
        plan.assumptions = format!(
            "组合数值已按 {} 折算；{}/{} 项持仓冻结了带来源证券价格，其余市值、汇率与日期仍依赖用户确认。{}",
            valuation_status.base_currency,
            holding_valuations.len(),
            holdings.len(),
            plan.assumptions
        );
    }
    let target_total: f64 = holdings.iter().map(|holding| holding.target_pct).sum();
    if target_total > 0.0 && !(99.0..=101.0).contains(&target_total) {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "目标权重尚未闭合".into(),
            detail: format!(
                "当前持仓目标权重合计为 {target_total:.1}%，完成到 100% 后才能计算再平衡动作。"
            ),
            action: "检查每项持仓目标权重，避免无意中放大或遗漏风险预算。".into(),
        });
    }
    if plan.committed_monthly > plan.monthly_surplus
        && plan.committed_monthly > 0.0
        && (profile.monthly_income > 0.0 || profile.monthly_expense > 0.0)
    {
        findings.push(crate::models::RiskFinding {
            level: "high".into(),
            title: "目标投入超过月度结余".into(),
            detail: format!(
                "计划每月投入 {:.0} {}，但当前月度结余约 {:.0} {}。",
                plan.committed_monthly,
                profile.base_currency,
                plan.monthly_surplus,
                profile.base_currency
            ),
            action: "调整目标优先级、期限或月度投入，避免计划依赖新增负债。".into(),
        });
    }
    Snapshot {
        profile,
        goals,
        holdings,
        holding_valuations,
        findings,
        total_value,
        emergency_months,
        concentration_pct,
        valuation_status,
        plan,
        updated_at,
    }
}
