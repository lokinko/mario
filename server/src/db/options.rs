//! Option associations and quotes share the existing financial transaction boundary.
use super::*;
use crate::models::options::{checked, text, OptionContract, OptionPriceInput, StockOptionTotals};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::BTreeMap;

pub(super) fn load(conn: &Connection) -> AppResult<BTreeMap<String, OptionContract>> {
    let mut stmt =
        conn.prepare("SELECT holding_id,payload FROM holding_options ORDER BY holding_id")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, raw)| Ok((id, serde_json::from_str(&raw)?)))
        .collect()
}
pub(super) fn get(conn: &Connection, id: &str) -> AppResult<Option<OptionContract>> {
    conn.query_row(
        "SELECT payload FROM holding_options WHERE holding_id=$1",
        [id],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|raw| Ok(serde_json::from_str(&raw)?))
    .transpose()
}
pub(super) fn guard_parent(
    snapshot: &Snapshot,
    contract: &OptionContract,
    currency: &str,
) -> AppResult<()> {
    contract.validate()?;
    let parent = snapshot
        .holdings
        .iter()
        .find(|h| h.id == contract.underlying_id)
        .ok_or_else(|| AppError::Validation("请选择当前账号的标的股票持仓".into()))?;
    if parent.asset_class != "股票"
        || parent.currency != currency
        || snapshot.option_contracts.contains_key(&parent.id)
        || snapshot.cleared_trade_holdings.contains(&parent.id)
        || snapshot
            .holding_trade_summaries
            .get(&parent.id)
            .is_some_and(|s| s.account != contract.account)
        || snapshot
            .option_contracts
            .values()
            .any(|c| c.underlying_id == parent.id && c.account != contract.account)
    {
        return Err(AppError::Validation(
            "期权必须关联同账户、同币种且交易链正常的股票".into(),
        ));
    }
    Ok(())
}
fn same_contract(a: &OptionContract, b: &OptionContract) -> bool {
    a.underlying_id == b.underlying_id
        && a.account == b.account
        && a.option_type == b.option_type
        && a.side == b.side
        && a.expires_on == b.expires_on
        && Decimal::from_str_exact(&a.strike).ok() == Decimal::from_str_exact(&b.strike).ok()
        && Decimal::from_str_exact(&a.multiplier).ok()
            == Decimal::from_str_exact(&b.multiplier).ok()
}
pub(super) fn create(
    conn: &Connection,
    snapshot: &Snapshot,
    id: &str,
    contract: &OptionContract,
    currency: &str,
    account: &str,
    operation_id: &str,
) -> AppResult<()> {
    guard_parent(snapshot, contract, currency)?;
    if account.trim() != contract.account {
        return Err(AppError::Validation("期权成交账户与合约账户不一致".into()));
    }
    if snapshot
        .option_contracts
        .values()
        .any(|c| same_contract(c, contract))
    {
        return Err(AppError::Conflict(
            "该期权合约已有记录，请在原合约追加成交".into(),
        ));
    }
    super::financial::journal(
        conn,
        id,
        "option.create",
        &Option::<OptionContract>::None,
        contract,
        "0",
        ("创建股票关联期权合约", Some(operation_id)),
    )?;
    conn.execute(
        "INSERT INTO holding_options(holding_id,underlying_id,payload) VALUES ($1,$2,$3)",
        params![id, contract.underlying_id, serde_json::to_string(contract)?],
    )?;
    Ok(())
}
pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    let mut stmt = conn.prepare("SELECT holding_id,underlying_id FROM holding_options")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, parent) in rows {
        let c = &snapshot.option_contracts[&id];
        guard_parent(
            &snapshot,
            c,
            &snapshot
                .holdings
                .iter()
                .find(|h| h.id == id)
                .ok_or_else(|| AppError::Validation("期权持仓不存在".into()))?
                .currency,
        )?;
        if snapshot
            .option_contracts
            .iter()
            .any(|(other, candidate)| other != &id && same_contract(c, candidate))
        {
            return Err(AppError::Validation("备份含重复期权合约".into()));
        }
        if c.underlying_id != parent
            || snapshot
                .corporate_actions
                .iter()
                .any(|a| a.holding_id == id)
            || snapshot
                .holding_valuations
                .iter()
                .any(|v| v.holding_id == id)
        {
            return Err(AppError::Validation("期权关联或估值来源无效".into()));
        }
    }
    for h in &snapshot.holdings {
        if (h.asset_class == "期权") != snapshot.option_contracts.contains_key(&h.id) {
            return Err(AppError::Validation(
                "期权持仓缺少有效的标的合约关联".into(),
            ));
        }
        if h.asset_class == "期权" {
            let values = super::amounts::read(conn, &h.id)?
                .ok_or_else(|| AppError::Validation("期权缺少精确估值".into()))?;
            let quantity = crate::models::decimal::parse(values.quantity.as_deref().unwrap_or(""))?;
            let price =
                crate::models::decimal::parse(values.current_price.as_deref().unwrap_or(""))?;
            if text(snapshot.option_contracts[&h.id].value(quantity, price)?)
                != text(
                    Decimal::from_str_exact(&values.market_value)
                        .map_err(|_| AppError::Validation("期权市值无效".into()))?,
                )
            {
                return Err(AppError::Validation(
                    "期权市值与张数、权利金、合约乘数不一致".into(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn totals(
    snapshot: &Snapshot,
    accounting: &crate::models::accounting::AccountingSnapshot,
) -> AppResult<BTreeMap<String, StockOptionTotals>> {
    let dec = |s: &str| {
        Decimal::from_str_exact(s).map_err(|_| AppError::Validation("期权汇总金额无效".into()))
    };
    let mut result = BTreeMap::new();
    for contract in snapshot.option_contracts.values() {
        let parent = &contract.underlying_id;
        if result.contains_key(parent) {
            continue;
        }
        let stock = accounting
            .holdings
            .get(parent)
            .ok_or_else(|| AppError::Validation("期权标的不存在".into()))?;
        let mut market = dec(&stock.amounts.market_value)?;
        let mut basis = dec(&stock.amounts.cost_basis)?;
        let mut settled = Decimal::ZERO;
        let mut unrealized = Decimal::ZERO;
        let mut usable = stock.usable;
        let mut open = 0;
        for (id, c) in &snapshot.option_contracts {
            if &c.underlying_id != parent {
                continue;
            }
            let holding = accounting
                .holdings
                .get(id)
                .ok_or_else(|| AppError::Validation("期权持仓不存在".into()))?;
            market = checked(market.checked_add(dec(&holding.amounts.market_value)?))?;
            basis = checked(basis.checked_add(dec(&holding.amounts.cost_basis)?))?;
            settled = checked(
                settled.checked_add(dec(holding.settled_profit.as_deref().unwrap_or("0"))?),
            )?;
            unrealized = checked(
                unrealized.checked_add(dec(holding.unrealized_profit.as_deref().unwrap_or("0"))?),
            )?;
            usable &= holding.usable;
            if holding
                .amounts
                .quantity
                .as_deref()
                .is_some_and(|q| q != "0")
            {
                open += 1;
            }
        }
        // A documented zero-size stock anchor has no missing cost or return.
        let zero_anchor = stock.amounts.quantity.as_deref() == Some("0")
            && dec(&stock.amounts.cost_basis)?.is_zero();
        result.insert(
            parent.clone(),
            StockOptionTotals {
                market_value: text(market),
                cost_basis: text(basis),
                unrealized_profit: stock
                    .unrealized_profit
                    .as_deref()
                    .or(if zero_anchor { Some("0") } else { None })
                    .map(|v| Ok::<_, AppError>(text(checked(dec(v)?.checked_add(unrealized))?)))
                    .transpose()?,
                settled_profit: Some(text(checked(
                    dec(stock.settled_profit.as_deref().unwrap_or("0"))?.checked_add(settled),
                )?)),
                option_settled_profit: text(settled),
                option_unrealized_profit: text(unrealized),
                usable,
                open_contracts: open,
            },
        );
    }
    Ok(result)
}

impl Database {
    pub fn update_option_price(&self, id: &str, input: &OptionPriceInput) -> AppResult<Snapshot> {
        Uuid::parse_str(&input.request_id)
            .map_err(|_| AppError::Validation("请求编号无效".into()))?;
        let price = crate::models::decimal::parse(&input.price)?;
        let day = chrono::NaiveDate::parse_from_str(&input.observed_on, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("估值日期无效".into()))?;
        if day.to_string() != input.observed_on
            || input.observed_on > crate::models::market::today()
        {
            return Err(AppError::Validation("期权估值日期无效或晚于今天".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let operation =
            super::financial::operation("option.price", &input.request_id, &(id, input))?;
        if let Some(s) = super::financial::replay(&tx, &operation)? {
            return Ok(s);
        }
        let before = super::daily::prepare(&tx)?;
        let contract = before
            .option_contracts
            .get(id)
            .ok_or_else(|| AppError::Validation("期权合约不存在".into()))?;
        if before.holding_revisions.get(id) != Some(&input.expected_revision) {
            return Err(AppError::Conflict("期权已变化，请刷新后重新核对".into()));
        }
        if before.cleared_trade_holdings.iter().any(|h| h == id) {
            return Err(AppError::Validation("期权交易链已隔离，请先修复".into()));
        }
        if input.observed_on > contract.expires_on {
            return Err(AppError::Validation(
                "到期后的合约请先核对交割或记录到期作废".into(),
            ));
        }
        let h = before.holdings.iter().find(|h| h.id == id).unwrap();
        let mut amounts = super::amounts::read(&tx, id)?
            .ok_or_else(|| AppError::Validation("期权缺少精确估值".into()))?;
        let original = amounts.clone();
        let q = crate::models::decimal::parse(amounts.quantity.as_deref().unwrap_or("0"))?;
        let market = contract.value(q, price)?;
        amounts.current_price = Some(text(price));
        amounts.market_value = text(market);
        amounts.price_status = "manual".into();
        super::amounts::put(&tx, id, &amounts)?;
        tx.execute("UPDATE holdings SET current_price=$2,market_value=$3,valuation_date=$4,updated_at=$5 WHERE id=$1",params![id,price.to_f64().unwrap(),market.to_f64().unwrap(),input.observed_on,Utc::now().to_rfc3339()])?;
        super::financial::journal(
            &tx,
            &h.id,
            "option.price",
            &original,
            &amounts,
            "0",
            ("按券商期权报价核对权利金", Some(&operation.0)),
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        HoldingDeleteInput, HoldingTradeInput, HoldingTradeUpdateInput, TradeKind,
    };
    fn setup(db: &Database) -> (Snapshot, String) {
        super::super::cash::fund(db, "CNY");
        let input: HoldingTradeInput=serde_json::from_value(serde_json::json!({"requestId":Uuid::new_v4().to_string(),"newHolding":{"name":"标的股票","symbol":"","assetClass":"股票","currency":"CNY"},"kind":"buy","occurredOn":"2026-01-01","sequence":1,"quantity":"10","price":"10","grossAmount":"100","fees":"0","taxes":"0","account":"券商A","settlementMode":"settle","expectedCashBaseline":"00000000-0000-0000-0000-000000000001"})).unwrap();
        let s = db.save_holding_trade(&input).unwrap();
        let id = s.holding_trades[0].holding_id.clone();
        (s, id)
    }
    fn opening(s: &Snapshot, parent: &str, side: &str, multiplier: &str) -> HoldingTradeInput {
        serde_json::from_value(serde_json::json!({"requestId":Uuid::new_v4().to_string(),"newHolding":{"name":"股票期权","symbol":"","assetClass":"期权","currency":"CNY","option":{"underlyingId":parent,"account":"券商A","optionType":"put","side":side,"strike":"10","expiresOn":"2026-02-01","multiplier":multiplier}},"kind":if side=="short" {"sell"} else {"buy"},"occurredOn":"2026-01-02","sequence":1,"quantity":"2","price":"3","grossAmount":text(Decimal::from(6)*Decimal::from_str_exact(multiplier).unwrap()),"fees":"2","taxes":"0","account":"券商A","settlementMode":"settle","expectedCashBaseline":s.cash_balances[0].baseline_id})).unwrap()
    }
    fn next(
        s: &Snapshot,
        id: &str,
        kind: TradeKind,
        q: &str,
        p: &str,
        day: &str,
    ) -> HoldingTradeInput {
        let c = &s.option_contracts[id];
        serde_json::from_value(serde_json::json!({"requestId":Uuid::new_v4().to_string(),"holdingId":id,"expectedRevision":s.holding_revisions[id],"kind":kind,"occurredOn":day,"sequence":1,"quantity":q,"price":p,"grossAmount":text(Decimal::from_str_exact(q).unwrap()*Decimal::from_str_exact(p).unwrap()*Decimal::from_str_exact(&c.multiplier).unwrap()),"fees":"1","taxes":"0","account":"券商A","settlementMode":"settle","expectedCashBaseline":s.cash_balances[0].baseline_id})).unwrap()
    }
    fn hash(db: &Database) -> String {
        db.export_sync_data().unwrap().content_hash().unwrap()
    }
    #[test]
    fn option_long_short_multiplier_cash_returns_grouping_and_roundtrip() {
        for (side, multiplier, kind, market, basis, realized, unrealized) in [
            ("long", "100", TradeKind::Sell, "500", "301", "198", "199"),
            (
                "short",
                "100",
                TradeKind::Buy,
                "-500",
                "-299",
                "-202",
                "-201",
            ),
            ("long", "10", TradeKind::Sell, "50", "31", "18", "19"),
        ] {
            let db = Database::test();
            let (s, parent) = setup(&db);
            let start = Decimal::from_str_exact(&s.cash_balances[0].amount).unwrap();
            let input = opening(&s, &parent, side, multiplier);
            let s = db.save_holding_trade(&input).unwrap();
            let id = s.option_contracts.keys().next().unwrap().clone();
            assert_eq!(
                s.holdings.iter().find(|h| h.id == parent).unwrap().quantity,
                Some(10.0)
            );
            let s = db
                .save_holding_trade(&next(&s, &id, kind, "1", "5", "2026-01-03"))
                .unwrap();
            let quote = OptionPriceInput {
                request_id: Uuid::new_v4().to_string(),
                expected_revision: s.holding_revisions[&id].clone(),
                price: "5".into(),
                observed_on: "2026-01-03".into(),
            };
            let s = db.update_option_price(&id, &quote).unwrap();
            let a = &s.accounting.as_ref().unwrap().holdings[&id];
            assert_eq!(a.amounts.market_value, market);
            assert_eq!(a.amounts.cost_basis, basis);
            assert_eq!(a.settled_profit.as_deref(), Some(realized));
            assert_eq!(a.unrealized_profit.as_deref(), Some(unrealized));
            assert_eq!(
                s.stock_option_totals[&parent].option_settled_profit,
                realized
            );
            assert_eq!(
                s.stock_option_totals[&parent].option_unrealized_profit,
                unrealized
            );
            assert_eq!(
                Decimal::from_str_exact(&s.stock_option_totals[&parent].market_value).unwrap(),
                Decimal::from(100) + Decimal::from_str_exact(market).unwrap()
            );
            let expected_flow = if side == "short" {
                Decimal::from(6) * Decimal::from_str_exact(multiplier).unwrap()
                    - Decimal::from(2)
                    - Decimal::from(5) * Decimal::from_str_exact(multiplier).unwrap()
                    - Decimal::ONE
            } else {
                -Decimal::from(6) * Decimal::from_str_exact(multiplier).unwrap() - Decimal::from(2)
                    + Decimal::from(5) * Decimal::from_str_exact(multiplier).unwrap()
                    - Decimal::ONE
            };
            assert_eq!(
                Decimal::from_str_exact(&s.cash_balances[0].amount).unwrap(),
                start + expected_flow
            );
            assert!(!s.valuation_status.decision_ready);
            assert!(s.plan.rebalancing.is_empty());
            let before = hash(&db);
            db.update_option_price(&id, &quote).unwrap();
            assert_eq!(before, hash(&db));
            assert!(db
                .replay_financial_request(
                    "PUT",
                    &format!("/api/holdings/{id}/option-price"),
                    &serde_json::to_value(&quote).unwrap()
                )
                .unwrap()
                .is_some());
            let copy = Database::test();
            copy.import_sync_data(&db.export_sync_data().unwrap())
                .unwrap();
            assert_eq!(
                serde_json::to_value(copy.snapshot().unwrap().option_contracts).unwrap(),
                serde_json::to_value(&s.option_contracts).unwrap()
            );
            assert_eq!(
                copy.snapshot().unwrap().stock_option_totals[&parent].option_settled_profit,
                realized
            );
            let export = copy.export_sync_data().unwrap();
            let mut bad = export.clone();
            let options = bad
                .tables
                .iter_mut()
                .find(|t| t.name == "holding_options")
                .unwrap();
            options.rows[0][1] = SyncValue::Text(Uuid::new_v4().to_string());
            let stable = hash(&copy);
            assert!(copy.import_sync_data(&bad).is_err());
            assert_eq!(stable, hash(&copy));
        }
    }
    #[test]
    fn option_expiry_is_explicit_and_releases_signed_basis_without_stock_or_cash_gross() {
        for (side, profit) in [("long", "-603"), ("short", "597")] {
            let db = Database::test();
            let (s, parent) = setup(&db);
            let opening = opening(&s, &parent, side, "100");
            let s = db.save_holding_trade(&opening).unwrap();
            let id = s.option_contracts.keys().next().unwrap().clone();
            let mut expiry = next(&s, &id, TradeKind::Expire, "2", "0", "2026-02-01");
            let s = db.save_holding_trade(&expiry).unwrap();
            assert_eq!(s.holding_trade_summaries[&id].settled_profit, profit);
            assert_eq!(s.holding_trade_summaries[&id].cost_basis, "0");
            assert_eq!(s.holding_trade_summaries[&id].quantity, "0");
            assert_eq!(
                s.accounting.as_ref().unwrap().holdings[&id]
                    .amounts
                    .market_value,
                "0"
            );
            assert_eq!(s.holding_trade_summaries[&parent].quantity, "10");
            let stable = hash(&db);
            expiry.request_id = Uuid::new_v4().to_string();
            expiry.expected_revision = Some(s.holding_revisions[&id].clone());
            expiry.sequence = 2;
            assert!(db.save_holding_trade(&expiry).is_err());
            assert_eq!(stable, hash(&db));
            let copy = Database::test();
            copy.import_sync_data(&db.export_sync_data().unwrap())
                .unwrap();
            assert_eq!(
                copy.snapshot().unwrap().stock_option_totals[&parent].option_settled_profit,
                profit
            );
        }
    }
    #[test]
    fn option_validation_rollback_edit_delete_migration_and_account_isolation() {
        let account = TestAccount::new();
        let db = account.open();
        let (s, parent) = setup(&db);
        let input = opening(&s, &parent, "long", "100");
        let baseline = hash(&db);
        for field in ["account", "multiplier", "underlyingId", "expiresOn"] {
            let mut bad = serde_json::to_value(&input).unwrap();
            bad["newHolding"]["option"][field] = serde_json::json!(match field {
                "account" => "其他账户",
                "multiplier" => "0.5",
                "expiresOn" => "2025-01-01",
                _ => "00000000-0000-0000-0000-000000000002",
            });
            assert!(db
                .save_holding_trade(&serde_json::from_value(bad).unwrap())
                .is_err());
            assert_eq!(baseline, hash(&db));
        }
        {
            let conn = db.conn().unwrap();
            super::super::fail_inserts(&conn, "financial_journal", "option rollback");
        }
        assert!(db.save_holding_trade(&input).is_err());
        db.conn()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_financial_journal ON financial_journal; DROP FUNCTION fail_financial_journal()")
            .unwrap();
        assert_eq!(baseline, hash(&db));
        let s = db.save_holding_trade(&input).unwrap();
        let id = s.option_contracts.keys().next().unwrap().clone();
        let stable = hash(&db);
        let mut dup = input.clone();
        dup.request_id = Uuid::new_v4().to_string();
        assert!(db.save_holding_trade(&dup).is_err());
        assert!(db
            .delete_holding(
                &parent,
                &HoldingDeleteInput {
                    expected_revision: s.holding_revisions[&parent].clone()
                }
            )
            .is_err());
        let mut fractional = next(&s, &id, TradeKind::Sell, "0.5", "3", "2026-01-03");
        assert!(db.save_holding_trade(&fractional).is_err());
        fractional.quantity = "3".into();
        fractional.gross_amount = "900".into();
        assert!(db.save_holding_trade(&fractional).is_err());
        assert_eq!(stable, hash(&db));
        let original = s
            .holding_trades
            .iter()
            .find(|t| t.id == input.request_id)
            .unwrap();
        let update:HoldingTradeUpdateInput=serde_json::from_value(serde_json::json!({"expectedRevision":s.holding_revisions[&id],"kind":"buy","occurredOn":original.occurred_on,"sequence":1,"quantity":"2","price":"4","grossAmount":"800","fees":"2","taxes":"0","account":"券商A","settlementMode":"history"})).unwrap();
        let s = db.update_holding_trade(&input.request_id, &update).unwrap();
        assert_eq!(s.holding_trade_summaries[&id].cost_basis, "802");
        let foreign = Database::test();
        assert!(foreign.save_holding_trade(&input).is_err());
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(db.snapshot().unwrap().option_contracts.len(), 1);
        let s = db.snapshot().unwrap();
        let s = db
            .delete_holding(
                &id,
                &HoldingDeleteInput {
                    expected_revision: s.holding_revisions[&id].clone(),
                },
            )
            .unwrap();
        assert!(s.option_contracts.is_empty());
        let journal = db.financial_history(&id).unwrap();
        assert!(journal["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "holding.delete"));
        let copy = Database::test();
        copy.import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
    }
    #[test]
    fn short_close_insufficient_cash_rolls_back_contract_execution_and_observation() {
        let db = Database::test();
        let (s, parent) = setup(&db);
        let input = opening(&s, &parent, "short", "100");
        let s = db.save_holding_trade(&input).unwrap();
        let id = s.option_contracts.keys().next().unwrap().clone();
        let cash = &s.cash_balances[0];
        let s = db
            .update_holding_amount(
                &cash.holding_id,
                &crate::models::daily::AmountInput {
                    request_id: Uuid::new_v4().to_string(),
                    expected_revision: s.holding_revisions[&cash.holding_id].clone(),
                    amount: "0".into(),
                },
            )
            .unwrap();
        let before = hash(&db);
        assert!(db
            .save_holding_trade(&next(&s, &id, TradeKind::Buy, "2", "4", "2026-01-03"))
            .is_err());
        assert_eq!(before, hash(&db));
    }
    #[test]
    fn backup_cannot_duplicate_a_contract_or_remove_its_underlying() {
        let db = Database::test();
        let (s, parent) = setup(&db);
        let s = db
            .save_holding_trade(&opening(&s, &parent, "long", "100"))
            .unwrap();
        let mut other = opening(&s, &parent, "long", "100");
        other
            .new_holding
            .as_mut()
            .unwrap()
            .option
            .as_mut()
            .unwrap()
            .strike = "11".into();
        db.save_holding_trade(&other).unwrap();
        let export = db.export_sync_data().unwrap();
        let mut duplicate = export.clone();
        let table = duplicate
            .tables
            .iter_mut()
            .find(|t| t.name == "holding_options")
            .unwrap();
        let source = table.rows[0][2].clone();
        table.rows[1][2] = source;
        let before = hash(&db);
        assert!(db.import_sync_data(&duplicate).is_err());
        assert_eq!(before, hash(&db));
        let mut dangling = export;
        dangling
            .tables
            .iter_mut()
            .find(|t| t.name == "holdings")
            .unwrap()
            .rows
            .retain(|r| r[0] != SyncValue::Text(parent.clone()));
        assert!(db.import_sync_data(&dangling).is_err());
        assert_eq!(before, hash(&db));
    }
    #[test]
    fn zero_share_manual_stock_anchor_does_not_create_missing_option_return_totals() {
        let db = Database::test();
        super::super::cash::fund(&db, "CNY");
        let input:HoldingInput=serde_json::from_value(serde_json::json!({"name":"未买入的标的","symbol":"","assetClass":"股票","marketValue":0,"costBasis":0,"quantity":0,"currentPrice":10,"costPrice":0,"targetPct":0,"currency":"CNY","valuationDate":crate::models::market::today()})).unwrap();
        let s = db.add_holding(&input).unwrap();
        let parent = s
            .holdings
            .iter()
            .find(|h| h.asset_class == "股票")
            .unwrap()
            .id
            .clone();
        let s = db
            .save_holding_trade(&opening(&s, &parent, "long", "100"))
            .unwrap();
        let a = &s.accounting.unwrap().investments;
        assert_eq!(a.cost.value.as_deref(), Some("602"));
        assert_eq!(a.cost.missing, 0);
        assert_eq!(a.unrealized.value.as_deref(), Some("-2"));
        assert_eq!(a.unrealized.missing, 0);
        assert_eq!(a.realized.value.as_deref(), Some("0"));
        assert_eq!(a.realized.missing, 0);
        assert_eq!(
            s.stock_option_totals[&parent].unrealized_profit.as_deref(),
            Some("-2")
        );
    }
}
