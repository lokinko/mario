//! Precise, deterministic AI facts. No repositories, providers or model computation.
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};

use crate::models::{ContextSelection, FinancialProfile, IssueCode, Snapshot};

fn decimal(value: &str) -> Option<Decimal> {
    Decimal::from_str_exact(value).ok()
}
fn text(value: Decimal) -> String {
    value.round_dp(4).normalize().to_string()
}
fn amount(value: Option<&str>) -> Value {
    value
        .filter(|v| decimal(v).is_some_and(|n| n > Decimal::ZERO))
        .map_or(json!("未填写"), |v| json!(v))
}
fn metric(value: &crate::models::accounting::AccountingMetric) -> Value {
    if value.missing == 0 {
        json!(value.value)
    } else {
        Value::Null
    }
}

pub fn fact_sheet(snapshot: &Snapshot, selection: &ContextSelection) -> Value {
    let mut summary = Map::new();
    if selection.include_profile || selection.include_holdings {
        summary.insert("baseCurrency".into(), json!(snapshot.profile.base_currency));
    }
    if selection.include_profile {
        let profile = &snapshot.profile;
        // Older saved profiles may predate exactAmounts. Preserve their stored
        // decimal projection as a string; never invent precision already lost.
        let saved = profile.exact_amounts.clone().unwrap_or_else(|| {
            crate::models::accounting::ProfileAmounts {
                monthly_income: profile.monthly_income.to_string(),
                monthly_expense: profile.monthly_expense.to_string(),
                emergency_fund: profile.emergency_fund.to_string(),
                liabilities: profile.liabilities.to_string(),
                investable_assets: profile.investable_assets.to_string(),
            }
        });
        let exact = Some(&saved);
        summary.insert(
            "monthlyIncome".into(),
            amount(exact.map(|v| v.monthly_income.as_str())),
        );
        summary.insert(
            "monthlyExpense".into(),
            amount(exact.map(|v| v.monthly_expense.as_str())),
        );
        summary.insert(
            "emergencyFund".into(),
            amount(exact.map(|v| v.emergency_fund.as_str())),
        );
        summary.insert(
            "liabilities".into(),
            amount(exact.map(|v| v.liabilities.as_str())),
        );
        let income = exact
            .and_then(|v| decimal(&v.monthly_income))
            .filter(|v| *v > Decimal::ZERO);
        let expense = exact
            .and_then(|v| decimal(&v.monthly_expense))
            .filter(|v| *v > Decimal::ZERO);
        let emergency = exact
            .and_then(|v| decimal(&v.emergency_fund))
            .filter(|v| *v > Decimal::ZERO);
        summary.insert(
            "monthlySurplus".into(),
            income
                .zip(expense)
                .and_then(|(i, e)| i.checked_sub(e))
                .map_or(json!("未填写"), |v| json!(v.normalize().to_string())),
        );
        summary.insert(
            "emergencyMonths".into(),
            emergency
                .zip(expense)
                .and_then(|(f, e)| f.checked_div(e))
                .map_or(json!("未填写"), |v| json!(text(v))),
        );
        let initial = FinancialProfile::default();
        summary.insert(
            "horizonYears".into(),
            if profile.horizon_years == initial.horizon_years || profile.horizon_years == 0 {
                json!("未填写")
            } else {
                json!(profile.horizon_years)
            },
        );
        summary.insert(
            "riskLevel".into(),
            if profile.risk_level == initial.risk_level || profile.risk_level.is_empty() {
                json!("未填写")
            } else {
                json!(profile.risk_level)
            },
        );
        summary.insert(
            "maxDrawdownPct".into(),
            if profile.max_drawdown_pct == initial.max_drawdown_pct
                || profile.max_drawdown_pct == 0.0
            {
                json!("未填写")
            } else {
                json!(profile.max_drawdown_pct.to_string())
            },
        );
    }
    let mut sheet = Map::new();
    if selection.include_holdings {
        summary.insert(
            "valuationDate".into(),
            json!(snapshot.valuation_status.aligned_valuation_date),
        );
        summary.insert(
            "portfolioValue".into(),
            json!(snapshot
                .accounting
                .as_ref()
                .and_then(|a| a.total_value.as_ref())),
        );
        summary.insert(
            "investmentValue".into(),
            snapshot
                .accounting
                .as_ref()
                .map_or(Value::Null, |a| metric(&a.investments.value)),
        );
        summary.insert(
            "cashValue".into(),
            snapshot
                .accounting
                .as_ref()
                .map_or(Value::Null, |a| metric(&a.cash.value)),
        );
        let total = snapshot
            .accounting
            .as_ref()
            .and_then(|a| a.total_value.as_deref())
            .and_then(decimal)
            .filter(|v| *v > Decimal::ZERO);
        let holdings = snapshot.holdings.iter().map(|holding| {
            let source = snapshot.accounting.as_ref().and_then(|a| a.holdings.get(&holding.id));
            let usable = source.is_some_and(|a| a.usable);
            let market = source.filter(|_| usable).map(|a| a.amounts.market_value.as_str());
            let cost = source.filter(|_| usable).map(|a| a.amounts.cost_basis.as_str());
            let weight = market.and_then(decimal)
                .zip(source.and_then(|a| a.fx_rate.as_deref()).and_then(decimal))
                .and_then(|(v, fx)| v.checked_mul(fx)).zip(total)
                .and_then(|(v, total)| v.checked_div(total))
                .and_then(|v| v.checked_mul(Decimal::ONE_HUNDRED));
            let target = decimal(&holding.target_pct.to_string());
            let profit = source.filter(|_| usable).and_then(|a| a.unrealized_profit.as_deref()).and_then(decimal)
                .zip(cost.and_then(decimal).filter(|c| !c.is_zero()))
                .and_then(|(profit, cost)| profit.checked_div(cost.abs()))
                .and_then(|v| v.checked_mul(Decimal::ONE_HUNDRED));
            let valuation = snapshot.holding_valuations.iter().find(|v| v.holding_id == holding.id);
            (holding.id.clone(), json!({
                "id":holding.id,"name":holding.name,"symbol":holding.symbol,"assetClass":holding.asset_class,"currency":holding.currency,
                "marketValue":market,"weightPct":weight.map(text),"targetPct":target.map(text),
                "deviationPct":weight.zip(target).and_then(|(w,t)| w.checked_sub(t)).map(text),
                "costBasis":cost,"profitPct":profit.map(text),
                "priceDate":valuation.map_or(holding.valuation_date.as_str(), |v| v.observed_on.as_str()),
                "priceSource":valuation.map_or("用户填写", |v| v.provider_name.as_str()),
                "priceBasis":valuation.map(|v| v.price_basis.as_str()),
                "issueCodes":snapshot.valuation_status.issues.iter().filter(|i| i.holding_ids.contains(&holding.id)).map(|i| i.code).collect::<Vec<_>>()
            }))
        }).collect::<Map<_,_>>();
        sheet.insert("holdings".into(), Value::Object(holdings));
        let scope = |code: IssueCode| match code {
            IssueCode::MissingFxRate => vec!["portfolioValue", "weightPct", "rebalancing"],
            IssueCode::PendingCorporateAction | IssueCode::QuarantinedTrades => vec![
                "holdingAmounts",
                "portfolioValue",
                "weightPct",
                "rebalancing",
            ],
            IssueCode::CashNeedsReconciliation => vec!["currencyCashReconciliation"],
            IssueCode::InvalidTargetWeights => vec!["rebalancing"],
        };
        let mut issues = snapshot.valuation_status.issues.iter().map(|issue| json!({"code":issue.code,"holdingIds":issue.holding_ids,"currency":issue.currency,"affects":scope(issue.code)})).collect::<Vec<_>>();
        for code in &snapshot.plan.rebalancing_blocked_by {
            if !snapshot
                .valuation_status
                .issues
                .iter()
                .any(|i| i.code == *code)
            {
                issues.push(json!({"code":code,"holdingIds":[],"affects":scope(*code)}));
            }
        }
        if !snapshot.plan.excluded_holding_ids.is_empty() {
            issues.push(json!({"code":"rebalancingExcluded","holdingIds":snapshot.plan.excluded_holding_ids,"affects":["rebalancing"]}));
        }
        sheet.insert("dataIssues".into(), json!(issues));
        if !snapshot.option_contracts.is_empty() {
            sheet.insert("options".into(), json!(snapshot.option_contracts.iter().map(|(id,c)| (id.clone(), json!({"underlyingId":c.underlying_id,"type":c.option_type,"side":c.side,"strike":c.strike,"expiresOn":c.expires_on,"multiplier":c.multiplier}))).collect::<Map<_,_>>()));
        }
    }
    sheet.insert("summary".into(), Value::Object(summary));
    Value::Object(sheet)
}
