use super::{
    planning, risk, valuation, FinancialProfile, Holding, HoldingValuationEvidence, Snapshot,
};
use crate::models::{
    accounting::AccountingSnapshot, options::OptionContract, CashBalanceState,
    CorporateActionRecord, HoldingTrade, HoldingTradeSummary,
};
use std::collections::BTreeMap;

pub(super) struct SnapshotData {
    pub profile: FinancialProfile,
    pub holdings: Vec<Holding>,
    pub holding_valuations: Vec<HoldingValuationEvidence>,
    pub updated_at: String,
    pub holding_revisions: BTreeMap<String, String>,
    pub option_contracts: BTreeMap<String, OptionContract>,
    pub holding_trades: Vec<HoldingTrade>,
    pub cash_balances: Vec<CashBalanceState>,
    pub holding_trade_summaries: BTreeMap<String, HoldingTradeSummary>,
    pub corporate_actions: Vec<CorporateActionRecord>,
    pub cleared_trade_holdings: Vec<String>,
}

pub(super) fn build_snapshot(
    mut data: SnapshotData,
    accounting: AccountingSnapshot,
) -> super::AppResult<Snapshot> {
    use rust_decimal::{prelude::ToPrimitive, Decimal};
    let number = |value: &str| -> super::AppResult<f64> {
        Decimal::from_str_exact(value)
            .ok()
            .and_then(|v| v.to_f64())
            .ok_or_else(|| super::AppError::Validation("会计投影金额无效".into()))
    };
    for holding in &mut data.holdings {
        let exact = &accounting.holdings[&holding.id];
        if exact.usable {
            holding.market_value = number(&exact.amounts.market_value)?;
            holding.cost_basis = number(&exact.amounts.cost_basis)?;
            holding.quantity = exact.amounts.quantity.as_deref().map(number).transpose()?;
            holding.cost_price = exact
                .amounts
                .cost_price
                .as_deref()
                .map(number)
                .transpose()?;
            holding.current_price = exact
                .amounts
                .current_price
                .as_deref()
                .map(number)
                .transpose()?;
        }
    }
    let valuation_status = valuation::issues(
        &data.profile.base_currency,
        &data.holdings,
        &data.corporate_actions,
        &data.cleared_trade_holdings,
        &data.cash_balances,
    );
    let normalized = if valuation_status.comparable {
        data.holdings
            .iter()
            .filter_map(|h| valuation::normalize_holding(h, &data.profile.base_currency))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let total_value = accounting
        .total_value
        .as_deref()
        .map(number)
        .transpose()?
        .unwrap_or(0.0);
    let concentration_pct = if total_value > 0.0 {
        normalized
            .iter()
            .map(|h| h.market_value)
            .fold(0.0_f64, f64::max)
            / total_value
            * 100.0
    } else {
        0.0
    };
    let mut excluded = Vec::new();
    for (id, contract) in &data.option_contracts {
        if accounting.holdings[id]
            .amounts
            .quantity
            .as_deref()
            .is_some_and(|q| q != "0")
        {
            excluded.push(id.clone());
            excluded.push(contract.underlying_id.clone());
        }
    }
    excluded.sort();
    excluded.dedup();
    let findings = risk::analyze(
        &data.profile,
        if valuation_status.comparable {
            &normalized
        } else {
            &data.holdings
        },
        valuation_status.comparable,
    );
    let plan = planning::analyze(
        &data.profile,
        &normalized,
        &valuation_status.issues,
        &excluded,
        Some(&accounting),
    )?;
    Ok(Snapshot {
        emergency_months: if data.profile.monthly_expense > 0.0 {
            data.profile.emergency_fund / data.profile.monthly_expense
        } else {
            0.0
        },
        profile: data.profile,
        holdings: data.holdings,
        holding_valuations: data.holding_valuations,
        updated_at: data.updated_at,
        holding_revisions: data.holding_revisions,
        option_contracts: data.option_contracts,
        stock_option_totals: Default::default(),
        holding_trades: data.holding_trades,
        cash_balances: data.cash_balances,
        holding_trade_summaries: data.holding_trade_summaries,
        corporate_actions: data.corporate_actions,
        cleared_trade_holdings: data.cleared_trade_holdings,
        price_refresh_errors: Vec::new(),
        accounting: Some(accounting),
        findings,
        plan,
        valuation_status,
        total_value,
        concentration_pct,
    })
}
