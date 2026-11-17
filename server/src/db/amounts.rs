//! Exact persisted accounting and server-owned portfolio totals; floats are projections only.
use crate::models::accounting::{
    has_public_dividends, AccountingHolding, AccountingMetric, AccountingSnapshot,
    AccountingTotals, DividendIncome, HoldingAmounts,
};
use crate::storage::params;
use crate::storage::OptionalRow;
use crate::{
    db::Database,
    models::portfolio::{Holding, HoldingInput, Snapshot},
    storage::Connection,
    AppError, AppResult,
};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::BTreeMap;

fn dec(value: &str) -> AppResult<Decimal> {
    Decimal::from_str_exact(value).map_err(|_| AppError::Validation("精确会计金额无效".into()))
}
fn text(value: Decimal) -> String {
    value.normalize().to_string()
}
fn product(a: Decimal, b: Decimal) -> AppResult<Decimal> {
    a.checked_mul(b)
        .ok_or_else(|| AppError::Validation("会计金额计算溢出".into()))
}
pub(super) fn read(conn: &Connection, id: &str) -> AppResult<Option<HoldingAmounts>> {
    conn.query_row(
        "SELECT payload FROM holding_amounts WHERE holding_id=$1",
        [id],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|s| Ok(serde_json::from_str(&s)?))
    .transpose()
}
pub(super) fn put(conn: &Connection, id: &str, amounts: &HoldingAmounts) -> AppResult<()> {
    let payload = serde_json::to_string(amounts)?;
    conn.execute("INSERT INTO holding_amounts(holding_id,payload) VALUES ($1,$2) ON CONFLICT(holding_id) DO UPDATE SET payload=excluded.payload",params![id,payload])?;
    Ok(())
}
fn from_holding(h: &Holding) -> HoldingAmounts {
    HoldingAmounts {
        price_status: "unconfirmed".into(),
        market_value: h.market_value.to_string(),
        cost_basis: h.cost_basis.to_string(),
        quantity: h.quantity.map(|v| v.to_string()),
        cost_price: h.cost_price.map(|v| v.to_string()),
        current_price: h.current_price.map(|v| v.to_string()),
    }
}
pub(super) fn normalize(input: &mut HoldingInput) -> AppResult<()> {
    if let Some(values) = &input.exact_values {
        let parse = |v: &str| -> AppResult<f64> {
            let n = dec(v)?;
            if n < Decimal::ZERO || n > Decimal::from(1_000_000_000_000_000_i64) {
                return Err(AppError::Validation("金额超出范围".into()));
            }
            n.to_f64()
                .ok_or_else(|| AppError::Validation("金额超出范围".into()))
        };
        input.market_value = parse(&values.market_value)?;
        // Average costs produced by executions can have more than eight decimals;
        // their authority is checked against the transaction replay in record_input.
        input.cost_basis = dec(&values.cost_basis)?
            .to_f64()
            .ok_or_else(|| AppError::Validation("成本无效".into()))?;
        input.quantity = values.quantity.as_deref().map(parse).transpose()?;
        input.current_price = values.current_price.as_deref().map(parse).transpose()?;
        input.cost_price = values
            .cost_price
            .as_deref()
            .map(|v| {
                dec(v)?
                    .to_f64()
                    .ok_or_else(|| AppError::Validation("成本价无效".into()))
            })
            .transpose()?;
    }
    Ok(())
}
pub(super) fn record_input(
    conn: &Connection,
    id: &str,
    input: &HoldingInput,
    trade_managed: bool,
) -> AppResult<()> {
    if input.asset_class == "现金" {
        return Ok(());
    }
    let mut values = input.exact_values.clone().unwrap_or(HoldingAmounts {
        price_status: "manual".into(),
        market_value: input.market_value.to_string(),
        cost_basis: input.cost_basis.to_string(),
        quantity: input.quantity.map(|v| v.to_string()),
        cost_price: input.cost_price.map(|v| v.to_string()),
        current_price: input.current_price.map(|v| v.to_string()),
    });
    if values.price_status != "provisional" {
        values.price_status = "manual".into();
    }
    if trade_managed {
        let trades = super::trades::load(conn)?
            .into_iter()
            .filter(|t| t.holding_id == id)
            .collect::<Vec<_>>();
        let summary = super::corporate::position(conn, id, &trades)?;
        values.quantity = Some(summary.quantity);
        values.cost_basis = summary.cost_basis;
        values.cost_price = Some(summary.average_cost);
    } else if let (Some(quantity), Some(price)) = (&values.quantity, &values.cost_price) {
        values.cost_basis = text(product(dec(quantity)?, dec(price)?)?);
    }
    if let (Some(quantity), Some(price)) = (&values.quantity, &values.current_price) {
        values.market_value = text(product(dec(quantity)?, dec(price)?)?);
    }
    put(conn, id, &values)
}

pub(super) fn rebuild(
    conn: &Connection,
    h: &Holding,
    summary: &crate::models::portfolio::HoldingTradeSummary,
) -> AppResult<HoldingAmounts> {
    let old = read(conn, &h.id)?;
    let price_status = old
        .as_ref()
        .map(|v| v.price_status.clone())
        .unwrap_or_else(|| "provisional".into());
    let price = old
        .and_then(|v| v.current_price)
        .filter(|value| {
            dec(value)
                .ok()
                .and_then(|v| v.to_f64())
                .zip(h.current_price)
                .is_some_and(|(a, b)| (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(1.0))
        })
        .or_else(|| h.current_price.map(|p| p.to_string()))
        .unwrap_or_else(|| "0".into());
    let values = HoldingAmounts {
        price_status,
        market_value: text(if let Some(c) = super::options::get(conn, &h.id)? {
            c.value(dec(&summary.quantity)?, dec(&price)?)?
        } else {
            product(dec(&summary.quantity)?, dec(&price)?)?
        }),
        cost_basis: summary.cost_basis.clone(),
        quantity: Some(summary.quantity.clone()),
        cost_price: Some(summary.average_cost.clone()),
        current_price: Some(price),
    };
    if dec(&values.market_value)? > Decimal::from(1_000_000_000_000_000_i64) {
        return Err(AppError::Validation("持仓市值超过允许范围".into()));
    }
    put(conn, &h.id, &values)?;
    Ok(values)
}

pub(super) fn apply_quote(
    conn: &Connection,
    id: &str,
    quantity: f64,
    quote: &crate::models::market::quotes::SecurityPriceQuote,
) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    let h = snapshot
        .holdings
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| AppError::Validation("持仓不存在".into()))?;
    let mut values = read(conn, id)?.unwrap_or_else(|| from_holding(h));
    if let Some(summary) = snapshot.holding_trade_summaries.get(id) {
        values.quantity = Some(summary.quantity.clone());
    } else if values
        .quantity
        .as_deref()
        .map(dec)
        .transpose()?
        .and_then(|value| value.to_f64())
        != Some(quantity)
    {
        // The caller already updated the holdings float projection. Compare the
        // persisted exact quantity so refreshing a quote cannot round it away.
        values.quantity = Some(quantity.to_string());
    }
    values.current_price = Some(
        quote
            .exact_close
            .clone()
            .unwrap_or_else(|| quote.close.to_string()),
    );
    values.price_status = "quoted".into();
    values.market_value = text(product(
        dec(values.quantity.as_deref().unwrap())?,
        dec(values.current_price.as_deref().unwrap())?,
    )?);
    if let Some(summary) = snapshot.holding_trade_summaries.get(id) {
        values.cost_basis = summary.cost_basis.clone();
        values.cost_price = Some(summary.average_cost.clone());
    } else if let Some(price) = &values.cost_price {
        values.cost_basis = text(product(
            dec(values.quantity.as_deref().unwrap())?,
            dec(price)?,
        )?);
    }
    put(conn, id, &values)
}

pub(super) fn set_market_value(conn: &Connection, h: &Holding, amount: Decimal) -> AppResult<()> {
    if h.asset_class == "现金" {
        return Ok(());
    }
    let mut values = read(conn, &h.id)?.unwrap_or_else(|| from_holding(h));
    values.market_value = text(amount);
    values.price_status = "manual".into();
    if let Some(quantity) = &values.quantity {
        let quantity = dec(quantity)?;
        if !quantity.is_zero() {
            values.current_price =
                Some(text(amount.checked_div(quantity).ok_or_else(|| {
                    AppError::Validation("单位价格计算失败".into())
                })?));
        }
    }
    put(conn, &h.id, &values)
}
pub(super) fn initialize(conn: &Connection) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    for h in &snapshot.holdings {
        if h.asset_class != "现金" && read(conn, &h.id)?.is_none() {
            put(conn, &h.id, &from_holding(h))?;
        }
    }
    Ok(())
}

pub(super) fn split_reference(
    conn: &Connection,
    id: &str,
    split: &crate::models::market::quotes::SplitEvent,
) -> AppResult<()> {
    if let Some(mut values) = read(conn, id)? {
        values.price_status = "corporate_pending_price".into();
        if let Some(price) = &values.current_price {
            values.current_price = Some(text(
                dec(price)?
                    .checked_div(super::trades::split_ratio(split)?)
                    .ok_or_else(|| AppError::Validation("公司行动参考价格溢出".into()))?,
            ));
            put(conn, id, &values)?;
        }
    }
    Ok(())
}
pub(super) fn build(
    conn: &Connection,
    snapshot: &super::snapshot::SnapshotData,
) -> AppResult<AccountingSnapshot> {
    let mut holdings = std::collections::BTreeMap::new();
    for holding in &snapshot.holdings {
        let stored = read(conn, &holding.id)?;
        let authoritative = stored.is_some();
        let mut values = stored.unwrap_or_else(|| from_holding(holding));
        let summary = snapshot.holding_trade_summaries.get(&holding.id);
        if let Some(summary) = summary {
            values.quantity = Some(summary.quantity.clone());
            values.cost_basis = summary.cost_basis.clone();
            values.cost_price = Some(summary.average_cost.clone());
        }
        if holding.asset_class == "现金" {
            if let Some(cash) = snapshot
                .cash_balances
                .iter()
                .find(|b| b.holding_id == holding.id)
            {
                values.market_value = cash.amount.clone();
            }
        } else if !authoritative {
            if let (Some(quantity), Some(price)) = (&values.quantity, &values.current_price) {
                values.market_value = text(product(dec(quantity)?, dec(price)?)?);
            }
        }
        let empty_option_anchor = snapshot
            .option_contracts
            .values()
            .any(|c| c.underlying_id == holding.id)
            && values
                .quantity
                .as_deref()
                .map(dec)
                .transpose()?
                .is_some_and(|q| q.is_zero())
            && dec(&values.market_value)?.is_zero()
            && dec(&values.cost_basis)?.is_zero();
        let unrealized = if dec(&values.cost_basis)? != Decimal::ZERO
            || snapshot.option_contracts.contains_key(&holding.id)
        {
            Some(text(dec(&values.market_value)? - dec(&values.cost_basis)?))
        } else if empty_option_anchor || summary.is_some_and(|s| s.quantity == "0") {
            Some("0".into())
        } else {
            None
        };
        let fx_rate = super::fx::exact_rate(conn, holding, &snapshot.profile.base_currency)?;
        holdings.insert(
            holding.id.clone(),
            AccountingHolding {
                base_currency: snapshot.profile.base_currency.clone(),
                amounts: values,
                fx_rate,
                unrealized_profit: unrealized,
                settled_profit: summary
                    .map(|s| s.settled_profit.clone())
                    .or_else(|| empty_option_anchor.then(|| "0".into())),
                usable: !snapshot.cleared_trade_holdings.contains(&holding.id)
                    && !snapshot
                        .corporate_actions
                        .iter()
                        .any(|a| a.holding_id == holding.id && !a.applied && !a.dismissed),
            },
        );
    }
    let totals = |cash: bool| -> AppResult<AccountingTotals> {
        let selected = snapshot
            .holdings
            .iter()
            .filter(|h| (h.asset_class == "现金") == cash)
            .map(|h| &holdings[&h.id])
            .collect::<Vec<_>>();
        let metric = |name: &str| -> AppResult<AccountingMetric> {
            let mut total = Decimal::ZERO;
            let mut known = 0;
            let mut missing = 0;
            for holding in &selected {
                let amount = match name {
                    "value" => Some(&holding.amounts.market_value),
                    "cost" => holding
                        .unrealized_profit
                        .as_ref()
                        .map(|_| &holding.amounts.cost_basis),
                    "unrealized" => holding.unrealized_profit.as_ref(),
                    _ => holding.settled_profit.as_ref(),
                };
                if !holding.usable {
                    missing += 1;
                    continue;
                }
                if let Some(amount) = amount {
                    let amount = dec(amount)?;
                    if let Some(rate) = &holding.fx_rate {
                        total = total
                            .checked_add(product(amount, dec(rate)?)?)
                            .ok_or_else(|| AppError::Validation("组合合计超出范围".into()))?;
                        known += 1;
                    } else if amount.is_zero() {
                        known += 1;
                    } else {
                        missing += 1;
                    }
                } else {
                    missing += 1;
                }
            }
            Ok(AccountingMetric {
                value: if !selected.is_empty() && known == 0 {
                    None
                } else {
                    Some(text(total))
                },
                missing,
            })
        };
        Ok(AccountingTotals {
            value: metric("value")?,
            cost: metric("cost")?,
            unrealized: metric("unrealized")?,
            realized: metric("realized")?,
        })
    };
    let investments = totals(false)?;
    let cash = totals(true)?;
    let total_value = if investments.value.missing == 0 && cash.value.missing == 0 {
        investments
            .value
            .value
            .as_deref()
            .zip(cash.value.value.as_deref())
            .map(|(a, b)| {
                Ok::<_, AppError>(text(
                    dec(a)?
                        .checked_add(dec(b)?)
                        .ok_or_else(|| AppError::Validation("总资产超出范围".into()))?,
                ))
            })
            .transpose()?
    } else {
        None
    };
    Ok(AccountingSnapshot {
        total_value,
        holdings,
        investments,
        cash,
    })
}

/// A-share entitlement is the position held before each public ex-date; other
/// holdings fall back to their recorded gross dividends.
pub(super) fn trailing_dividends(
    snapshot: &Snapshot,
    histories: &BTreeMap<String, crate::models::market::dividends::DividendHistory>,
    today: chrono::NaiveDate,
) -> AppResult<DividendIncome> {
    let accounting = snapshot
        .accounting
        .as_ref()
        .ok_or_else(|| AppError::Validation("缺少精确会计数据".into()))?;
    let since = today - chrono::Duration::days(365);
    let overflow = || AppError::Validation("分红合计超出范围".into());
    let mut total = Decimal::ZERO;
    let mut missing = Vec::new();
    for holding in &snapshot.holdings {
        let Some(exact) = accounting.holdings.get(&holding.id) else {
            continue;
        };
        let trades = snapshot
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == holding.id)
            .cloned()
            .collect::<Vec<_>>();
        let held = exact
            .amounts
            .quantity
            .as_deref()
            .map(dec)
            .transpose()?
            .is_some_and(|q| q > Decimal::ZERO);
        let mut amount = Decimal::ZERO;
        if has_public_dividends(holding) {
            let Some(history) = histories.get(&holding.symbol) else {
                if held || !trades.is_empty() {
                    missing.push(holding.name.clone());
                }
                continue;
            };
            if trades.is_empty() {
                if held {
                    missing.push(holding.name.clone());
                }
                continue;
            }
            let splits = snapshot
                .corporate_actions
                .iter()
                .filter(|a| a.holding_id == holding.id && a.applied)
                .map(|a| a.split.clone())
                .collect::<Vec<_>>();
            for event in history
                .events
                .iter()
                .filter(|e| e.ex_date > since && e.ex_date <= today)
            {
                let ex = event.ex_date.to_string();
                let before = trades
                    .iter()
                    .filter(|t| t.occurred_on < ex)
                    .cloned()
                    .collect::<Vec<_>>();
                let splits = splits
                    .iter()
                    .filter(|s| s.effective_on < ex)
                    .cloned()
                    .collect::<Vec<_>>();
                let shares = dec(&super::trades::position_with_splits(&before, &splits)?.quantity)?;
                let cash = product(shares, dec(&event.cash_per_ten)?)? / Decimal::TEN;
                amount = amount.checked_add(cash).ok_or_else(overflow)?;
            }
        } else {
            let (start, end) = (since.to_string(), today.to_string());
            for trade in trades.iter().filter(|t| {
                t.kind == crate::models::portfolio::TradeKind::Dividend
                    && t.occurred_on > start
                    && t.occurred_on <= end
            }) {
                amount = amount
                    .checked_add(dec(&trade.gross_amount)?)
                    .ok_or_else(overflow)?;
            }
        }
        if amount.is_zero() {
            continue;
        }
        let Some(rate) = exact.fx_rate.as_deref().filter(|_| exact.usable) else {
            missing.push(holding.name.clone());
            continue;
        };
        total = total
            .checked_add(product(amount, dec(rate)?)?)
            .ok_or_else(overflow)?;
    }
    let value = total.round_dp(2);
    let assets_pct = accounting
        .total_value
        .as_deref()
        .map(dec)
        .transpose()?
        .filter(|assets| *assets > Decimal::ZERO)
        .map(|assets| text((value * Decimal::ONE_HUNDRED / assets).round_dp(2)));
    Ok(DividendIncome {
        base_currency: snapshot.profile.base_currency.clone(),
        since: since.to_string(),
        value: text(value),
        assets_pct,
        missing,
    })
}

pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    let profile = Database::snapshot_on(conn)?.profile;
    let normalized = crate::models::accounting::normalize_profile(&profile)?;
    if profile.monthly_income != normalized.monthly_income
        || profile.monthly_expense != normalized.monthly_expense
        || profile.emergency_fund != normalized.emergency_fund
        || profile.liabilities != normalized.liabilities
    {
        return Err(AppError::Validation("精确财务资料与数值投影不一致".into()));
    }
    let mut statement=conn.prepare("SELECT a.holding_id,a.payload,h.asset_class,h.market_value,h.cost_basis,h.quantity,h.cost_price,h.current_price FROM holding_amounts a JOIN holdings h ON h.id=a.holding_id")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, Option<f64>>(6)?,
                r.get::<_, Option<f64>>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let limit = Decimal::from(1_000_000_000_000_000_i64);
    let same = |source: &str, view: f64, signed: bool| -> AppResult<bool> {
        let source = dec(source)?;
        if (!signed && source < Decimal::ZERO) || source.abs() > limit {
            return Ok(false);
        }
        let value = source
            .to_f64()
            .ok_or_else(|| AppError::Validation("金额超出范围".into()))?;
        Ok((value - view).abs() <= 4.0 * f64::EPSILON * value.abs().max(1.0))
    };
    for (id, payload, class, market, cost, quantity, cost_price, current_price) in rows {
        let signed = super::options::get(conn, &id)?
            .is_some_and(|c| c.side == crate::models::options::OptionSide::Short);
        let values: HoldingAmounts = serde_json::from_str(&payload)?;
        if !matches!(
            values.price_status.as_str(),
            "manual" | "quoted" | "unconfirmed" | "provisional" | "corporate_pending_price"
        ) || class == "现金"
            || !same(&values.market_value, market, signed)?
            || !same(&values.cost_basis, cost, class == "期权")?
        {
            return Err(AppError::Validation("精确会计记录与资产投影不一致".into()));
        }
        for (source, view) in [
            (values.quantity, quantity),
            (values.cost_price, cost_price),
            (values.current_price, current_price),
        ] {
            match (source, view) {
                (Some(source), Some(view)) if same(&source, view, false)? => {}
                (None, None) => {}
                _ => {
                    return Err(AppError::Validation(
                        "精确数量或价格与资产投影不一致".into(),
                    ))
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(events: &[(&str, &str)]) -> crate::models::market::dividends::DividendHistory {
        crate::models::market::dividends::DividendHistory {
            basis: crate::models::market::dividends::DividendBasis {
                as_of: day("2026-06-30"),
                fetched_at: "2026-06-30T08:00:00Z".into(),
            },
            sources: vec![],
            events: events
                .iter()
                .map(
                    |(ex, cash)| crate::models::market::dividends::DividendEvent {
                        report_date: None,
                        ex_date: day(ex),
                        cash_per_ten: (*cash).into(),
                        bonus_per_ten: "0".into(),
                        transfer_per_ten: "0".into(),
                    },
                )
                .collect(),
        }
    }
    fn day(value: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn trailing_dividends_follow_positions_held_before_each_ex_date() {
        let db = Database::test();
        // 600900: +100 on 01-01, +200 on 01-03, -100 on 01-05; 000333: +100 on 01-01, sold 01-02.
        db.import_account_data(&super::super::watchlist_import::tests::fixture())
            .unwrap();
        let s = db.snapshot().unwrap();
        let mut histories = BTreeMap::from([
            (
                "600900.SS".to_string(),
                history(&[
                    ("2025-01-02", "99"),
                    ("2026-01-03", "10"),
                    ("2026-01-06", "5"),
                ]),
            ),
            ("000333.SZ".to_string(), history(&[("2026-01-02", "20")])),
        ]);
        let income = trailing_dividends(&s, &histories, day("2026-06-30")).unwrap();
        assert_eq!(income.since, "2025-06-30");
        // 100 shares × 1.0 + 200 × 0.5 + 100 sold on the ex-date × 2.0; the recorded 300 is replaced.
        assert_eq!(income.value, "400");
        assert!(income.missing.is_empty());
        let assets = dec(s
            .accounting
            .as_ref()
            .unwrap()
            .total_value
            .as_deref()
            .unwrap())
        .unwrap();
        assert_eq!(
            income.assets_pct,
            Some(text((Decimal::from(40000) / assets).round_dp(2)))
        );
        let later = trailing_dividends(&s, &histories, day("2027-01-04")).unwrap();
        assert_eq!(later.value, "100");
        histories.remove("000333.SZ");
        let partial = trailing_dividends(&s, &histories, day("2026-06-30")).unwrap();
        assert_eq!(partial.value, "200");
        assert_eq!(partial.missing, vec!["测试家电".to_string()]);
    }
}
