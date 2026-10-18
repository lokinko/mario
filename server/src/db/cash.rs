//! One current cash balance per currency; executions settle only their change in cash flow.
use super::*;
use rust_decimal::{prelude::ToPrimitive, Decimal};

pub(super) const CURRENCIES: [&str; 3] = ["CNY", "HKD", "USD"];

#[cfg(test)]
pub(crate) fn fund(db: &Database, currency: &str) {
    let conn = db.conn().unwrap();
    if conn
        .query_row(
            "SELECT COUNT(*) FROM holdings WHERE asset_class='现金' AND currency=$1",
            [currency],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
        == 0
    {
        conn.execute("INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,valuation_date,created_at,updated_at) VALUES ($1,'',$2,'现金',10000,0,0,$3,'2026-01-01','9999-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            params![Uuid::new_v4().to_string(),format!("{currency} 存款"),currency]).unwrap();
        initialize_balances(&conn).unwrap();
        conn.execute("UPDATE cash_state SET baseline_id='00000000-0000-0000-0000-000000000001' WHERE currency=$1",[currency]).unwrap();
    }
}

fn decimal(value: f64) -> AppResult<Decimal> {
    Decimal::from_str_exact(&value.to_string())
        .map_err(|_| AppError::Validation("存款余额无效".into()))
}

pub(super) fn states(conn: &Connection) -> AppResult<Vec<crate::models::CashBalanceState>> {
    let mut statement=conn.prepare("SELECT currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation FROM cash_state ORDER BY currency")?;
    Ok(statement
        .query_map([], |r| {
            Ok(crate::models::CashBalanceState {
                currency: r.get(0)?,
                holding_id: r.get(1)?,
                amount: r.get(2)?,
                baseline_id: r.get(3)?,
                confirmed_on: r.get(4)?,
                confirmed_at: r.get(5)?,
                needs_reconciliation: r.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}

pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    use rust_decimal::prelude::ToPrimitive;
    for state in states(conn)? {
        let amount = crate::models::decimal::parse(&state.amount)?;
        let holding = conn
            .query_row(
                "SELECT currency,asset_class,market_value FROM holdings WHERE id=$1",
                [&state.holding_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, f64>(2)?,
                    ))
                },
            )
            .optional()?;
        if !holding.is_some_and(|(currency, class, value)| {
            currency == state.currency && class == "现金" && amount.to_f64() == Some(value)
        }) {
            return Err(AppError::Validation("存款精确余额与资产投影不一致".into()));
        }
        Uuid::parse_str(&state.baseline_id)
            .map_err(|_| AppError::Validation("存款核对基准编号无效".into()))?;
        chrono::NaiveDate::parse_from_str(&state.confirmed_on, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("存款核对日期无效".into()))?;
        chrono::DateTime::parse_from_rfc3339(&state.confirmed_at)
            .map_err(|_| AppError::Validation("存款核对时间无效".into()))?;
    }
    let mut statement =
        conn.prepare("SELECT amount,baseline_id,included FROM trade_settlements")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (amount, baseline, included) in rows {
        let amount = Decimal::from_str_exact(&amount)
            .map_err(|_| AppError::Validation("交易现金归属金额无效".into()))?;
        if amount.scale() > 8
            || amount.abs() > Decimal::from(1_000_000_000_000_000_i64)
            || ![0, 1].contains(&included)
            || (!baseline.is_empty() && Uuid::parse_str(&baseline).is_err())
        {
            return Err(AppError::Validation("交易现金归属记录无效".into()));
        }
    }
    Ok(())
}

/// Establish current-balance cutovers without replaying any previously recorded executions.
pub(super) fn initialize_balances(conn: &Connection) -> AppResult<()> {
    let mut statement = conn.prepare(
        "SELECT id,currency,market_value,valuation_date FROM holdings WHERE asset_class='现金'",
    )?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, currency, amount, day) in rows {
        let invalid_date = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_err();
        let day = if invalid_date { "1970-01-01" } else { &day };
        conn.execute("INSERT INTO cash_state(currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(currency) DO NOTHING",params![currency,id,decimal(amount)?.normalize().to_string(),Uuid::new_v4().to_string(),day,Utc::now().to_rfc3339(),i64::from(invalid_date)])?;
    }
    Ok(())
}

pub(super) fn confirm(
    conn: &Connection,
    id: &str,
    currency: &str,
    amount: Decimal,
    day: &str,
) -> AppResult<()> {
    conn.execute("INSERT INTO cash_state(currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation) VALUES ($1,$2,$3,$4,$5,$6,0) ON CONFLICT(currency) DO UPDATE SET holding_id=excluded.holding_id,amount=excluded.amount,baseline_id=excluded.baseline_id,confirmed_on=excluded.confirmed_on,confirmed_at=excluded.confirmed_at,needs_reconciliation=0",params![currency,id,amount.normalize().to_string(),Uuid::new_v4().to_string(),day,Utc::now().to_rfc3339()])?;
    Ok(())
}

/// Apply only a trade's contribution after the latest verified cash cutover.
pub(super) fn settle_trade(
    conn: &Connection,
    currency: &str,
    id: &str,
    flow: Option<Decimal>,
    mode: crate::models::SettlementMode,
    expected_baseline: &str,
) -> AppResult<Decimal> {
    initialize_balances(conn)?;
    let state = states(conn)?.into_iter().find(|s| s.currency == currency);
    let baseline = state.as_ref().map_or("", |s| s.baseline_id.as_str());
    let previous = conn
        .query_row(
            "SELECT amount,baseline_id,included FROM trade_settlements WHERE trade_id=$1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? != 0,
                ))
            },
        )
        .optional()?;
    // A balance confirmation includes every trade recorded before it. Corrections
    // to that history require reconciliation, rather than changing the verified cash.
    let covered = previous.as_ref().is_some_and(|(_, old, _)| old != baseline)
        || (previous.is_none()
            && conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM holding_trades WHERE id=$1)",
                [id],
                |r| r.get::<_, bool>(0),
            )?);
    let old = if covered {
        Decimal::ZERO
    } else {
        previous
            .as_ref()
            .map(|(amount, _, _)| Decimal::from_str_exact(amount))
            .transpose()
            .map_err(|_| AppError::Validation("结算记录金额无效".into()))?
            .unwrap_or(Decimal::ZERO)
    };
    let included = previous.as_ref().is_some_and(|(_, _, included)| *included);
    let cash_flow = flow.unwrap_or(Decimal::ZERO);
    let original_flow = super::trades::load(conn)?
        .into_iter()
        .find(|t| t.id == id)
        .map(|t| -> AppResult<Decimal> {
            let gross = Decimal::from_str_exact(&t.gross_amount)
                .map_err(|_| AppError::Validation("原成交金额无效".into()))?;
            let fees = Decimal::from_str_exact(&t.fees)
                .map_err(|_| AppError::Validation("原成交费用无效".into()))?;
            let taxes = Decimal::from_str_exact(&t.taxes)
                .map_err(|_| AppError::Validation("原成交税款无效".into()))?;
            Ok(if t.kind == crate::models::TradeKind::Buy {
                -(gross + fees + taxes)
            } else {
                gross - fees - taxes
            })
        })
        .transpose()?;
    let new = if covered || flow.is_none() {
        Decimal::ZERO
    } else if previous.is_some() {
        if !included {
            Decimal::ZERO
        } else {
            cash_flow
        }
    } else {
        match mode {
            crate::models::SettlementMode::History => Decimal::ZERO,
            crate::models::SettlementMode::Settle => cash_flow,
        }
    };
    if (covered || (previous.is_some() && !included)) && original_flow != flow {
        conn.execute(
            "UPDATE cash_state SET needs_reconciliation=1 WHERE currency=$1",
            [currency],
        )?;
    }
    let delta = new - old;
    if previous.is_none()
        && mode == crate::models::SettlementMode::Settle
        && (state.is_some() || new >= Decimal::ZERO)
        && state.as_ref().map_or("", |s| s.baseline_id.as_str()) != expected_baseline
    {
        return Err(AppError::Conflict(
            "存款核对基准已变化，请重新核对这笔成交是否已计入余额后再提交".into(),
        ));
    }
    if !delta.is_zero() && state.as_ref().is_some_and(|s| s.needs_reconciliation) {
        return Err(AppError::Validation(
            "存款存在历史更正差异，请先按实际余额重新核对后结算".into(),
        ));
    }
    settle(conn, currency, delta)?;
    // Persist the classification even for zero cash effect; later corrections
    // cannot accidentally start settling an already included historical trade.
    let baseline = states(conn)?
        .into_iter()
        .find(|s| s.currency == currency)
        .map(|s| s.baseline_id)
        .unwrap_or_default();
    let included = !covered
        && flow.is_some()
        && if previous.is_some() {
            included
        } else {
            mode == crate::models::SettlementMode::Settle
        };
    conn.execute("INSERT INTO trade_settlements(trade_id,currency,amount,baseline_id,included) VALUES ($1,$2,$3,$4,$5) ON CONFLICT(trade_id) DO UPDATE SET amount=excluded.amount,baseline_id=excluded.baseline_id,included=excluded.included",params![id,currency,new.normalize().to_string(),baseline,i64::from(included)])?;
    Ok(delta)
}

/// Reverse the whole position at once: intermediate refunds or debits must not
/// reject a valid net change, or let a history flag block the remaining reversals.
pub(super) fn remove_holding_trades(
    conn: &Connection,
    currency: &str,
    trades: &[&crate::models::HoldingTrade],
) -> AppResult<Decimal> {
    initialize_balances(conn)?;
    let state = states(conn)?.into_iter().find(|s| s.currency == currency);
    let baseline = state.as_ref().map_or("", |s| s.baseline_id.as_str());
    let mut delta = Decimal::ZERO;
    let mut reconcile = false;
    for trade in trades {
        let previous = conn
            .query_row(
                "SELECT amount,baseline_id,included FROM trade_settlements WHERE trade_id=$1",
                [&trade.id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                    ))
                },
            )
            .optional()?;
        match previous {
            Some((amount, old_baseline, included)) if old_baseline == baseline => {
                let amount = Decimal::from_str_exact(&amount)
                    .map_err(|_| AppError::Validation("结算记录金额无效".into()))?;
                delta = delta
                    .checked_sub(amount)
                    .ok_or_else(|| AppError::Validation("累计结算金额超过允许范围".into()))?;
                reconcile |= !included;
            }
            _ => reconcile = true,
        }
    }
    if !delta.is_zero() && state.as_ref().is_some_and(|s| s.needs_reconciliation) {
        return Err(AppError::Validation(
            "存款存在历史更正差异，请先按实际余额重新核对后删除持仓".into(),
        ));
    }
    if !delta.is_zero() {
        settle(conn, currency, delta)?;
    }
    if reconcile {
        conn.execute(
            "UPDATE cash_state SET needs_reconciliation=1 WHERE currency=$1",
            [currency],
        )?;
    }
    for trade in trades {
        conn.execute(
            "DELETE FROM trade_settlements WHERE trade_id=$1",
            [&trade.id],
        )?;
    }
    Ok(delta)
}

pub(super) fn normalize_input(input: &mut HoldingInput) -> AppResult<()> {
    if input.asset_class != "现金" {
        return Ok(());
    }
    if let Some(amount) = &input.cash_amount {
        input.market_value = crate::models::decimal::parse(amount)?
            .to_f64()
            .ok_or_else(|| AppError::Validation("金额超出范围".into()))?;
    }
    input.currency = input.currency.trim().to_ascii_uppercase();
    if !CURRENCIES.contains(&input.currency.as_str()) {
        return Err(AppError::Validation("存款仅支持 CNY、HKD 和 USD".into()));
    }
    input.name = format!("{} 存款", input.currency);
    input.symbol.clear();
    input.cost_basis = 0.0;
    input.target_pct = 0.0;
    input.quantity = None;
    input.cost_price = None;
    input.current_price = None;
    Ok(())
}

pub(super) fn guard(conn: &Connection, id: Option<&str>, input: &HoldingInput) -> AppResult<()> {
    let s = Database::snapshot_on(conn)?;
    if let Some(old) = s.holdings.iter().find(|h| Some(h.id.as_str()) == id) {
        if (old.asset_class == "现金") != (input.asset_class == "现金")
            || (old.asset_class == "现金" && old.currency != input.currency)
        {
            return Err(AppError::Validation(
                "存款币种固定，不能将存款与证券互相转换".into(),
            ));
        }
    }
    if input.asset_class == "现金"
        && s.holdings.iter().any(|h| {
            h.asset_class == "现金" && h.currency == input.currency && Some(h.id.as_str()) != id
        })
    {
        return Err(AppError::Conflict("该币种已有存款，请更新现有余额".into()));
    }
    Ok(())
}

pub(super) fn settle(conn: &Connection, currency: &str, delta: Decimal) -> AppResult<()> {
    if !CURRENCIES.contains(&currency) {
        return Err(AppError::Validation(
            "交易结算仅支持 CNY、HKD 和 USD 存款".into(),
        ));
    }
    if delta.is_zero() {
        return Ok(());
    }
    let s = Database::snapshot_on(conn)?;
    let cash = s
        .holdings
        .iter()
        .find(|h| h.asset_class == "现金" && h.currency == currency);
    let state = states(conn)?.into_iter().find(|s| s.currency == currency);
    let balance = if let Some(state) = state {
        Decimal::from_str_exact(&state.amount)
            .map_err(|_| AppError::Validation("存款精确余额无效".into()))?
    } else {
        cash.map_or(Ok(Decimal::ZERO), |h| decimal(h.market_value))?
    };
    let next = balance
        .checked_add(delta)
        .filter(|n| *n <= Decimal::from(1_000_000_000_000_000_i64))
        .ok_or_else(|| AppError::Validation("存款余额超过允许范围".into()))?;
    if next < Decimal::ZERO {
        return Err(AppError::Validation(format!(
            "{currency} 存款余额不足：当前 {}，本次需扣除 {}；请先核对存款余额",
            balance.normalize(),
            (-delta).normalize()
        )));
    }
    let amount = next
        .to_f64()
        .ok_or_else(|| AppError::Validation("金额超出范围".into()))?;
    let now = Utc::now().to_rfc3339();
    let day = crate::models::market::today();
    if let Some(h) = cash {
        conn.execute(
            "UPDATE holdings SET market_value=$2,valuation_date=$3,updated_at=$4 WHERE id=$1",
            params![h.id, amount, day, now],
        )?;
    } else {
        conn.execute("INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,valuation_date,created_at,updated_at) VALUES ($1,'',$2,'现金',$3,0,0,$4,$5,$6,$6)",
            params![Uuid::new_v4().to_string(),format!("{currency} 存款"),amount,currency,day,now])?;
    }
    super::fx::apply_holdings(conn)?;
    initialize_balances(conn)?;
    conn.execute(
        "UPDATE cash_state SET amount=$2 WHERE currency=$1",
        params![currency, next.normalize().to_string()],
    )?;
    Ok(())
}

// Existing balances already represent current cash. Never replay historical flows on upgrade/import.
// Preserve unsupported legacy currencies until the user has reconciled them, rather than dropping assets.
pub(super) fn consolidate(conn: &Connection) -> AppResult<()> {
    let before = Database::snapshot_on(conn)?;
    let mut changed = false;
    for currency in CURRENCIES {
        let rows: Vec<_> = before
            .holdings
            .iter()
            .filter(|h| h.asset_class == "现金" && h.currency == currency)
            .collect();
        let Some(first) = rows.first() else {
            continue;
        };
        let total = rows
            .iter()
            .try_fold(Decimal::ZERO, |sum, h| -> AppResult<_> {
                Ok(sum + decimal(h.market_value)?)
            })?;
        if total > Decimal::from(1_000_000_000_000_000_i64) {
            return Err(AppError::Validation(format!(
                "{currency} 合并存款超过允许范围"
            )));
        }
        let name = format!("{currency} 存款");
        if rows.len() == 1
            && first.name == name
            && first.symbol.is_empty()
            && first.cost_basis == 0.0
            && first.target_pct == 0.0
            && first.quantity.is_none()
            && first.cost_price.is_none()
            && first.current_price.is_none()
        {
            continue;
        }
        if !changed {
            super::daily::prepare(conn)?;
        }
        changed = true;
        for row in rows.iter().skip(1) {
            conn.execute("DELETE FROM holdings WHERE id=$1", [&row.id])?;
        }
        conn.execute(
            "DELETE FROM holding_valuations WHERE holding_id=$1",
            [&first.id],
        )?;
        conn.execute("UPDATE holdings SET name=$2,symbol='',market_value=$3,cost_basis=0,target_pct=0,quantity=NULL,cost_price=NULL,current_price=NULL,updated_at=$4 WHERE id=$1",
            params![first.id,name,total.to_f64().ok_or_else(||AppError::Validation("存款金额超出范围".into()))?,Utc::now().to_rfc3339()])?;
        confirm(
            conn,
            &first.id,
            currency,
            total,
            &crate::models::market::today(),
        )?;
        conn.execute(
            "UPDATE cash_state SET needs_reconciliation=1 WHERE currency=$1",
            [currency],
        )?;
    }
    if changed {
        super::fx::apply_holdings(conn)?;
        super::daily::finish(conn, &before, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_and_legacy_import_merge_currency_balances_without_replaying_trades() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        fund(&db, "CNY");
        db.conn().unwrap().execute("INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,valuation_date,created_at,updated_at) VALUES ('legacy-cash','','工资卡','现金',123.45,0,0,'CNY','2026-01-01','9999-01-02T00:00:00Z','2026-01-01T00:00:00Z')", []).unwrap();
        let mut legacy = db.export_sync_data().unwrap();
        legacy.schema_version = 19;
        super::super::tests::add_legacy_table_shapes(&mut legacy);
        legacy.tables.retain(|t| {
            ![
                "financial_operations",
                "financial_journal",
                "trading_quarantine",
                "cash_state",
                "trade_settlements",
                "corporate_actions",
                "holding_amounts",
            ]
            .contains(&t.name.as_str())
        });
        drop(db);
        let db = account.reopen_from_old_release();
        let s = db.snapshot().unwrap();
        assert_eq!(s.holdings.len(), 1);
        assert_eq!(s.holdings[0].market_value, 10123.45);
        assert_eq!(s.holdings[0].name, "CNY 存款");
        let stable = db.export_sync_data().unwrap().content_hash().unwrap();
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            stable
        );
        let copy = Database::test();
        copy.import_sync_data(&legacy).unwrap();
        assert_eq!(copy.snapshot().unwrap().total_value, 10123.45);
        assert_eq!(copy.snapshot().unwrap().holdings.len(), 1);
        copy.import_sync_data(&legacy).unwrap();
        assert_eq!(copy.snapshot().unwrap().total_value, 10123.45);
    }

    #[test]
    fn cash_has_one_balance_per_supported_currency_and_cannot_be_reclassified() {
        let db = Database::test();
        let input: HoldingInput = serde_json::from_value(serde_json::json!({"symbol":"bank","name":"工资卡","assetClass":"现金","marketValue":100,"costBasis":0,"targetPct":0,"currency":"CNY","valuationDate":"2026-01-01"})).unwrap();
        let s = db.add_holding(&input).unwrap();
        assert_eq!(s.holdings[0].name, "CNY 存款");
        assert!(s.holdings[0].symbol.is_empty());
        assert!(db.add_holding(&input).is_err());
        let mut changed = input.clone();
        changed.currency = "EUR".into();
        assert!(db.add_holding(&changed).is_err());
        changed.currency = "USD".into();
        assert!(db.update_holding(&s.holdings[0].id, &changed).is_err());
        changed.asset_class = "其他".into();
        assert!(db.update_holding(&s.holdings[0].id, &changed).is_err());
        assert_eq!(db.snapshot().unwrap().total_value, 100.0);
    }
}
