use super::{
    planning, risk, valuation, FinancialProfile, HashSet, Holding, HoldingValuationEvidence,
    Snapshot,
};

pub(super) fn build_snapshot(
    profile: FinancialProfile,
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
            action: "在“我的情况 → 汇率设置”重试自动更新或统一修改汇率。".into(),
        });
    }
    if valuation_status.undated_holding_count > 0 {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "持仓缺少估值日期".into(),
            detail: format!(
                "有 {} 项持仓缺少估值日期，不能建立可靠的周期比较基线。",
                valuation_status.undated_holding_count
            ),
            action: "补充缺少的持仓估值日期，便于核对组合价值。".into(),
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
    let mut plan = planning::analyze(&profile, &normalized_holdings);
    if !valuation_status.comparable {
        plan.rebalancing.clear();
        plan.assumptions = format!(
            "存在未折算到 {} 的外币持仓，组合风险和再平衡计算已暂停。",
            valuation_status.base_currency
        );
    } else if !holdings.is_empty() {
        plan.assumptions = format!(
            "组合数值已按 {} 折算；{}/{} 项持仓冻结了带来源证券价格，汇率来自统一设置，其余市值与日期仍依赖用户确认。{}",
            valuation_status.base_currency,
            holding_valuations.len(),
            holdings.len(),
            plan.assumptions
        );
    }
    if !valuation_status.decision_ready {
        plan.rebalancing.clear();
        plan.assumptions = format!(
            "估值质量未满足决策要求，再平衡行动暂停。{}；{}",
            valuation_status.warnings.join("；"),
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
    Snapshot {
        option_contracts: Default::default(),
        stock_option_totals: Default::default(),
        accounting: None,
        corporate_actions: Vec::new(),
        cash_balances: Vec::new(),
        price_refresh_errors: Vec::new(),
        holding_trades: Vec::new(),
        holding_trade_summaries: Default::default(),
        cleared_trade_holdings: Vec::new(),
        holding_revisions: Default::default(),
        profile,
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
