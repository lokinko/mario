use super::{
    params, AppError, AppResult, Connection, Database, HoldingInput, Local, NaiveDate, Snapshot,
    Utc, Uuid,
};
use crate::models::{HoldingTrade, HoldingTradeInput};
use std::collections::HashSet;

pub(super) fn load(conn: &Connection) -> AppResult<Vec<HoldingTrade>> {
    let mut stmt = conn.prepare("SELECT payload FROM holding_trades ORDER BY created_at, id")?;
    let payloads = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    payloads
        .iter()
        .map(|p| Ok(serde_json::from_str(p)?))
        .collect()
}

fn validate(t: &HoldingTrade) -> AppResult<()> {
    Uuid::parse_str(&t.id).map_err(|_| AppError::Validation("交易请求编号无效".into()))?;
    let date = NaiveDate::parse_from_str(&t.occurred_on, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("交易日期无效".into()))?;
    if date > Local::now().date_naive() || date.format("%Y-%m-%d").to_string() != t.occurred_on {
        return Err(AppError::Validation("交易日期不能晚于今天".into()));
    }
    chrono::DateTime::parse_from_rfc3339(&t.created_at)
        .map_err(|_| AppError::Validation("交易记录时间无效".into()))?;
    super::validate_currency(&t.currency)?;
    if [t.quantity, t.price, t.fees]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1e15).contains(v))
        || !t.quantity.mul_add(t.price, t.fees).is_finite()
        || t.quantity * t.price + t.fees > 1e15
        || t.account.chars().count() > 120
        || t.external_id.chars().count() > 200
        || t.note.chars().count() > 2000
    {
        return Err(AppError::Validation(
            "请核对交易数量、价格、费用或说明长度".into(),
        ));
    }
    match t.kind.as_str() {
        "buy" | "sell"
            if t.quantity > 0.0
                && t.price > 0.0
                && !t.account.trim().is_empty()
                && t.void_of.is_none() =>
        {
            Ok(())
        }
        "opening" if t.fees == 0.0 && t.void_of.is_none() && t.external_id.is_empty() => Ok(()),
        "void"
            if t.quantity == 0.0
                && t.price == 0.0
                && t.fees == 0.0
                && t.void_of.is_some()
                && !t.note.trim().is_empty()
                && t.external_id.is_empty() =>
        {
            Ok(())
        }
        _ => Err(AppError::Validation("交易类型或字段不完整".into())),
    }
}

// Rebuild from dated executions, using weighted average cost. Voids retain the audit trail.
pub(super) fn position(trades: &[HoldingTrade]) -> AppResult<(f64, f64, f64)> {
    let mut ids = HashSet::new();
    let mut identities = HashSet::new();
    let mut voided = HashSet::new();
    let mut account = None;
    let opening = trades
        .iter()
        .find(|t| t.kind == "opening")
        .ok_or_else(|| AppError::Validation("交易明细缺少期初余额".into()))?;
    if trades.iter().filter(|t| t.kind == "opening").count() != 1 {
        return Err(AppError::Validation("交易明细存在重复期初余额".into()));
    }
    for t in trades {
        validate(t)?;
        if !ids.insert(&t.id)
            || t.holding_id != opening.holding_id
            || t.currency != opening.currency
            || t.occurred_on < opening.occurred_on
        {
            return Err(AppError::Validation(
                "交易明细的关联、币种或期初日期不一致".into(),
            ));
        }
        if !t.external_id.is_empty() && !identities.insert((&t.account, &t.external_id)) {
            return Err(AppError::Conflict("相同账户的成交编号已存在".into()));
        }
        if let Some(id) = &t.void_of {
            let original = trades
                .iter()
                .find(|r| &r.id == id)
                .ok_or_else(|| AppError::Validation("撤销关联的交易不存在".into()))?;
            if !["buy", "sell"].contains(&original.kind.as_str())
                || t.kind != "void"
                || t.occurred_on < original.occurred_on
                || !voided.insert(id)
            {
                return Err(AppError::Validation("该交易不能重复撤销".into()));
            }
        }
    }
    let mut sorted: Vec<_> = trades
        .iter()
        .filter(|t| t.kind != "void" && !voided.contains(&t.id))
        .collect();
    sorted.sort_by(|a, b| {
        (
            a.occurred_on.as_str(),
            a.kind != "opening",
            a.created_at.as_str(),
            a.id.as_str(),
        )
            .cmp(&(
                b.occurred_on.as_str(),
                b.kind != "opening",
                b.created_at.as_str(),
                b.id.as_str(),
            ))
    });
    let (mut quantity, mut basis, mut realized) = (0.0, 0.0, 0.0);
    for t in sorted {
        if ["buy", "sell"].contains(&t.kind.as_str()) {
            if account.is_some_and(|a| a != t.account.as_str()) {
                return Err(AppError::Validation(
                    "不同账户请分别建立持仓，避免混合成本".into(),
                ));
            }
            account = Some(t.account.as_str());
        }
        match t.kind.as_str() {
            "opening" | "buy" => {
                quantity += t.quantity;
                basis += t.quantity * t.price + t.fees;
            }
            "sell" => {
                if t.quantity - quantity > quantity.abs() * 1e-12 || quantity <= 0.0 {
                    return Err(AppError::Validation(format!(
                        "{} 的卖出数量超过当时可用持仓 {}",
                        t.occurred_on, quantity
                    )));
                }
                let sold_basis = basis * (t.quantity.min(quantity) / quantity);
                realized += t.quantity * t.price - t.fees - sold_basis;
                let previous_quantity = quantity;
                quantity = (quantity - t.quantity).max(0.0);
                if quantity < previous_quantity * 1e-12 {
                    quantity = 0.0;
                }
                basis = if quantity == 0.0 {
                    0.0
                } else {
                    basis - sold_basis
                };
            }
            _ => unreachable!(),
        }
        if !quantity.is_finite()
            || quantity > 1e15
            || !basis.is_finite()
            || basis > 1e15
            || !realized.is_finite()
        {
            return Err(AppError::Validation(
                "累计持仓数量或成本超过允许范围".into(),
            ));
        }
    }
    Ok((quantity, basis, realized))
}

fn insert(conn: &Connection, trade: &HoldingTrade) -> AppResult<()> {
    conn.execute(
        "INSERT INTO holding_trades(id,holding_id,payload,created_at) VALUES (?1,?2,?3,?4)",
        params![
            trade.id,
            trade.holding_id,
            serde_json::to_string(trade)?,
            trade.created_at
        ],
    )?;
    Ok(())
}

pub(super) fn guard_edit(conn: &Connection, id: &str, input: &HoldingInput) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    if snapshot.holding_trades.iter().any(|t| t.holding_id == id) {
        let h = snapshot.holdings.iter().find(|h| h.id == id).unwrap();
        if h.quantity != input.quantity
            || h.cost_price != input.cost_price
            || (h.cost_basis - input.cost_basis).abs() > 1e-8_f64.max(h.cost_basis.abs() * 1e-10)
            || h.currency != input.currency.trim().to_ascii_uppercase()
            || h.symbol != input.symbol.trim()
            || h.asset_class != input.asset_class
        {
            return Err(AppError::Validation(
                "交易持仓的数量、成本、币种和证券信息由明细管理；请录入买卖或撤销误录".into(),
            ));
        }
        if snapshot
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == id)
            .any(|t| t.occurred_on > input.valuation_date)
        {
            return Err(AppError::Validation("估值日期不能早于交易明细日期".into()));
        }
    }
    Ok(())
}

impl Database {
    pub fn save_holding_trade(&self, input: &HoldingTradeInput) -> AppResult<Snapshot> {
        if !["buy", "sell", "void"].contains(&input.kind.as_str()) {
            return Err(AppError::Validation("请选择买入、卖出或撤销".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let before = super::daily::prepare(&tx)?;
        // A stable request ID survives retries, even if the response was lost.
        if let Some(t) = before
            .holding_trades
            .iter()
            .find(|t| t.id == input.request_id)
        {
            if t.kind != input.kind
                || t.occurred_on != input.occurred_on.trim()
                || t.quantity != input.quantity
                || t.price != input.price
                || t.fees != input.fees
                || t.account != input.account.trim()
                || t.external_id != input.external_id.trim()
                || t.note != input.note.trim()
                || t.void_of != input.void_of
                || (!input.holding_id.is_empty() && input.holding_id != t.holding_id)
                || input.new_holding.as_ref().is_some_and(|h| {
                    !before.holdings.iter().any(|v| {
                        v.id == t.holding_id
                            && v.name == h.name.trim()
                            && v.currency == h.currency.trim().to_ascii_uppercase()
                            && v.symbol == h.symbol.trim()
                    })
                })
            {
                return Err(AppError::Conflict("请求编号已用于另一笔交易".into()));
            }
            return Ok(before);
        }
        let now = Utc::now().to_rfc3339();
        let holding = if let Some(h) = &input.new_holding {
            if !input.holding_id.is_empty()
                || input.kind != "buy"
                || h.asset_class == "现金"
                || h.quantity != Some(0.0)
                || h.cost_basis != 0.0
                || h.market_value != 0.0
                || h.cost_price.is_some()
                || h.current_price != Some(input.price)
                || h.valuation_date != input.occurred_on.trim()
            {
                return Err(AppError::Validation("新持仓应从第一笔买入开始".into()));
            }
            super::validate_holding(h, &before.profile.base_currency)?;
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,fx_rate_to_base,valuation_date,fx_rate_source,fx_rate_observed_on,created_at,updated_at,quantity,current_price) VALUES (?1,?2,?3,?4,0,0,?5,?6,NULL,?7,'','',?8,?8,0,?9)",
                params![id,h.symbol.trim(),h.name.trim(),h.asset_class,h.target_pct,h.currency.trim().to_ascii_uppercase(),h.valuation_date,now,input.price])?;
            Database::snapshot_on(&tx)?
                .holdings
                .into_iter()
                .find(|h| h.id == id)
                .unwrap()
        } else {
            let h = before
                .holdings
                .iter()
                .find(|h| h.id == input.holding_id)
                .ok_or_else(|| AppError::Validation("找不到交易对应的持仓".into()))?;
            if before.holding_revisions.get(&h.id) != input.expected_revision.as_ref() {
                return Err(AppError::Conflict(
                    "持仓已更新，请刷新并核对后重新录入".into(),
                ));
            }
            if h.asset_class == "现金" {
                return Err(AppError::Validation("现金余额不能录入证券买卖".into()));
            }
            h.clone()
        };
        let mut trades: Vec<_> = before
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == holding.id)
            .cloned()
            .collect();
        if trades.is_empty() {
            let quantity = holding.quantity.ok_or_else(|| {
                AppError::Validation("请先补充已有持仓的期初数量、总成本和当前价格".into())
            })?;
            if quantity > 0.0 && holding.cost_basis <= 0.0 {
                return Err(AppError::Validation("请先核对期初持仓的总成本".into()));
            }
            let opening = HoldingTrade {
                id: Uuid::new_v4().to_string(),
                holding_id: holding.id.clone(),
                kind: "opening".into(),
                occurred_on: holding.valuation_date.clone(),
                quantity,
                price: if quantity > 0.0 {
                    holding.cost_basis / quantity
                } else {
                    0.0
                },
                fees: 0.0,
                currency: holding.currency.clone(),
                account: String::new(),
                external_id: String::new(),
                note: if quantity > 0.0 {
                    "用户确认的期初持仓；此前成交明细未录入"
                } else {
                    "从零持仓开始"
                }
                .into(),
                void_of: None,
                created_at: now.clone(),
            };
            insert(&tx, &opening)?;
            trades.push(opening);
        }
        let trade = HoldingTrade {
            id: input.request_id.clone(),
            holding_id: holding.id.clone(),
            kind: input.kind.clone(),
            occurred_on: input.occurred_on.trim().into(),
            quantity: input.quantity,
            price: input.price,
            fees: input.fees,
            currency: holding.currency.clone(),
            account: input.account.trim().into(),
            external_id: input.external_id.trim().into(),
            note: input.note.trim().into(),
            void_of: input.void_of.clone(),
            created_at: now.clone(),
        };
        if !trade.external_id.is_empty()
            && before
                .holding_trades
                .iter()
                .any(|t| t.account == trade.account && t.external_id == trade.external_id)
        {
            return Err(AppError::Conflict(
                "相同账户的成交编号已存在，请核对明细".into(),
            ));
        }
        trades.push(trade.clone());
        let (quantity, basis, _) = position(&trades)?;
        let current_price = if trade.kind != "void" && trade.occurred_on > holding.valuation_date {
            trade.price
        } else {
            holding.current_price.unwrap_or(input.price)
        };
        let market_value = quantity * current_price;
        let cost_price = if quantity > 0.0 {
            basis / quantity
        } else {
            0.0
        };
        if !market_value.is_finite() || market_value > 1e15 {
            return Err(AppError::Validation("持仓市值超过允许范围".into()));
        }
        let date = holding
            .valuation_date
            .as_str()
            .max(trade.occurred_on.as_str());
        insert(&tx, &trade)?;
        tx.execute("UPDATE holdings SET quantity=?2,cost_basis=?3,cost_price=?4,market_value=?5,valuation_date=?6,fx_rate_to_base=CASE WHEN valuation_date=?6 THEN fx_rate_to_base ELSE NULL END,fx_rate_source=CASE WHEN valuation_date=?6 THEN fx_rate_source ELSE '' END,fx_rate_observed_on=CASE WHEN valuation_date=?6 THEN fx_rate_observed_on ELSE '' END,updated_at=?7,current_price=?8 WHERE id=?1",
            params![holding.id,quantity,basis,cost_price,market_value,date,now,current_price])?;
        tx.execute(
            "DELETE FROM holding_valuations WHERE holding_id=?1",
            [&holding.id],
        )?;
        super::fx::apply_holdings(&tx)?;
        super::daily::finish(&tx, &before, None)?;
        tx.commit()?;
        drop(conn);
        self.snapshot()
    }
}

pub(super) fn validate_stored(conn: &Connection) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    let mut identities = HashSet::new();
    for t in &snapshot.holding_trades {
        if !snapshot
            .holdings
            .iter()
            .any(|h| h.id == t.holding_id && h.currency == t.currency)
            || (!t.external_id.is_empty() && !identities.insert((&t.account, &t.external_id)))
        {
            return Err(AppError::Validation(
                "同步交易关联无效或成交编号重复".into(),
            ));
        }
    }
    for h in &snapshot.holdings {
        let trades: Vec<_> = snapshot
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == h.id)
            .cloned()
            .collect();
        if trades.is_empty() {
            continue;
        }
        let (quantity, basis, _) = position(&trades)?;
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-8_f64.max(b.abs() * 1e-10);
        if h.asset_class == "现金"
            || !h.quantity.is_some_and(|q| near(q, quantity))
            || !near(h.cost_basis, basis)
            || !h.cost_price.is_some_and(|p| {
                near(
                    p,
                    if quantity > 0.0 {
                        basis / quantity
                    } else {
                        0.0
                    },
                )
            })
            || trades.iter().any(|t| t.occurred_on > h.valuation_date)
        {
            return Err(AppError::Validation(
                "同步持仓与交易计算的数量、成本或日期不一致".into(),
            ));
        }
    }
    let mut stmt = conn.prepare("SELECT id,holding_id,payload,created_at FROM holding_trades")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, holding_id, payload, created_at) in rows {
        let t: HoldingTrade = serde_json::from_str(&payload)?;
        if t.id != id || t.holding_id != holding_id || t.created_at != created_at {
            return Err(AppError::Validation("同步交易字段与明细不一致".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn new_trade() -> HoldingTradeInput {
        let h: HoldingInput = serde_json::from_value(serde_json::json!({
            "name":"测试股票", "symbol":"TEST", "assetClass":"股票", "currency":"CNY",
            "valuationDate":"2026-01-01", "quantity":0, "currentPrice":10,
            "costBasis":0, "marketValue":0, "targetPct":0
        }))
        .unwrap();
        HoldingTradeInput {
            request_id: Uuid::new_v4().to_string(),
            holding_id: String::new(),
            expected_revision: None,
            new_holding: Some(h),
            kind: "buy".into(),
            occurred_on: "2026-01-01".into(),
            quantity: 10.0,
            price: 10.0,
            fees: 2.0,
            account: "券商A".into(),
            external_id: "exec-1".into(),
            note: String::new(),
            void_of: None,
        }
    }
    fn next(
        snapshot: &Snapshot,
        kind: &str,
        quantity: f64,
        price: f64,
        fees: f64,
        date: &str,
    ) -> HoldingTradeInput {
        HoldingTradeInput {
            request_id: Uuid::new_v4().to_string(),
            holding_id: snapshot.holdings[0].id.clone(),
            expected_revision: snapshot
                .holding_revisions
                .get(&snapshot.holdings[0].id)
                .cloned(),
            new_holding: None,
            kind: kind.into(),
            occurred_on: date.into(),
            quantity,
            price,
            fees,
            account: "券商A".into(),
            external_id: String::new(),
            note: String::new(),
            void_of: None,
        }
    }
    #[test]
    fn executions_rebuild_cost_and_profit_and_survive_restart_sync_and_legacy_import() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("trades.db");
        let db = Database::open(&file).unwrap();
        let first = new_trade();
        let s = db.save_holding_trade(&first).unwrap();
        assert_eq!(s.holdings[0].quantity, Some(10.0));
        assert_eq!(s.holdings[0].cost_basis, 102.0);
        assert_eq!(
            db.save_holding_trade(&first).unwrap().holding_trades.len(),
            2
        );
        let s = db
            .save_holding_trade(&next(&s, "buy", 10.0, 20.0, 3.0, "2026-01-02"))
            .unwrap();
        let s = db
            .save_holding_trade(&next(&s, "sell", 5.0, 30.0, 1.0, "2026-01-03"))
            .unwrap();
        assert_eq!(s.holdings[0].quantity, Some(15.0));
        assert_eq!(s.holdings[0].cost_basis, 228.75);
        assert_eq!(s.holdings[0].cost_price, Some(15.25));
        assert_eq!(position(&s.holding_trades).unwrap().2, 72.75);
        assert!(db
            .daily_history(&super::super::daily::HistoryQuery::default())
            .unwrap()
            .records
            .iter()
            .any(|r| r.assets.iter().any(|a| a.holding.quantity == Some(15.0))));
        let export = db.export_sync_data().unwrap();
        let copy = Database::open(&dir.path().join("copy.db")).unwrap();
        copy.import_sync_data(&export).unwrap();
        assert_eq!(copy.snapshot().unwrap().holding_trades, s.holding_trades);
        let mut corrupt = export.clone();
        let table = corrupt
            .tables
            .iter_mut()
            .find(|t| t.name == "holding_trades")
            .unwrap();
        let super::super::SyncValue::Text(payload) = &mut table.rows[1][2] else {
            panic!()
        };
        let mut trade: HoldingTrade = serde_json::from_str(payload).unwrap();
        trade.quantity += 1.0;
        *payload = serde_json::to_string(&trade).unwrap();
        assert!(copy.import_sync_data(&corrupt).is_err());
        assert_eq!(copy.snapshot().unwrap().holding_trades, s.holding_trades);
        drop(db);
        assert_eq!(
            Database::open(&file)
                .unwrap()
                .snapshot()
                .unwrap()
                .holding_trades,
            s.holding_trades
        );
        let mut legacy = export;
        super::super::tests::add_retired_tables(&mut legacy);
        legacy.schema_version = 14;
        legacy.tables.retain(|t| t.name != "holding_trades");
        copy.import_sync_data(&legacy).unwrap();
        assert!(copy.snapshot().unwrap().holding_trades.is_empty());
    }
    #[test]
    fn rejects_duplicates_stale_edits_oversells_and_invalid_cancellations_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("reject.db")).unwrap();
        let s = db.save_holding_trade(&new_trade()).unwrap();
        let mut duplicate = next(&s, "buy", 1.0, 10.0, 0.0, "2026-01-02");
        duplicate.external_id = "exec-1".into();
        assert!(db.save_holding_trade(&duplicate).is_err());
        let mut other_account = next(&s, "buy", 1.0, 10.0, 0.0, "2026-01-02");
        other_account.account = "券商B".into();
        assert!(db.save_holding_trade(&other_account).is_err());
        assert!(db
            .save_holding_trade(&next(&s, "sell", 11.0, 10.0, 0.0, "2026-01-02"))
            .is_err());
        assert_eq!(db.snapshot().unwrap().holding_trades, s.holding_trades);
        let later = next(&s, "buy", 10.0, 20.0, 0.0, "2026-01-03");
        let s2 = db.save_holding_trade(&later).unwrap();
        assert!(db
            .save_holding_trade(&next(&s2, "sell", 15.0, 15.0, 0.0, "2026-01-02"))
            .is_err());
        assert!(db
            .save_holding_trade(&next(&s, "buy", 1.0, 10.0, 0.0, "2026-01-02"))
            .is_err());
        let s3 = db
            .save_holding_trade(&next(&s2, "sell", 15.0, 30.0, 0.0, "2026-01-04"))
            .unwrap();
        let mut void = next(&s3, "void", 0.0, 0.0, 0.0, "2026-01-05");
        void.void_of = Some(later.request_id);
        void.note = "买入数量误录".into();
        assert!(db.save_holding_trade(&void).is_err());
        assert_eq!(db.snapshot().unwrap().holding_trades, s3.holding_trades);
        let sell = s3.holding_trades.iter().find(|t| t.kind == "sell").unwrap();
        void.void_of = Some(sell.id.clone());
        let corrected = db.save_holding_trade(&void).unwrap();
        assert_eq!(corrected.holdings[0].quantity, Some(20.0));
        assert_eq!(corrected.holding_trades.len(), s3.holding_trades.len() + 1);
        assert_eq!(
            db.save_holding_trade(&void).unwrap().holding_trades.len(),
            corrected.holding_trades.len()
        );
        void.request_id = Uuid::new_v4().to_string();
        void.expected_revision = corrected.holding_revisions.get(&void.holding_id).cloned();
        assert!(db.save_holding_trade(&void).is_err());
        assert!(db.delete_holding(&corrected.holdings[0].id).is_err());
        let mut edit: HoldingInput =
            serde_json::from_value(serde_json::to_value(&corrected.holdings[0]).unwrap()).unwrap();
        edit.quantity = Some(99.0);
        assert!(db.update_holding(&corrected.holdings[0].id, &edit).is_err());
        let mut edit: HoldingInput =
            serde_json::from_value(serde_json::to_value(&corrected.holdings[0]).unwrap()).unwrap();
        edit.current_price = Some(40.0);
        assert_eq!(
            db.update_holding(&corrected.holdings[0].id, &edit)
                .unwrap()
                .holdings[0]
                .market_value,
            800.0
        );
    }
    #[test]
    fn opening_balances_and_fractional_close_out_have_explicit_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("opening.db")).unwrap();
        let mut first = new_trade();
        let mut h = first.new_holding.clone().unwrap();
        h.quantity = Some(0.3);
        h.cost_price = Some(5.0);
        h.cost_basis = 1.5;
        h.market_value = 3.0;
        let s = db.add_holding(&h).unwrap();
        let sell = next(&s, "sell", 0.3, 10.0, 0.0, "2026-01-01");
        let s = db.save_holding_trade(&sell).unwrap();
        assert_eq!(
            s.holding_trades
                .iter()
                .find(|t| t.kind == "opening")
                .unwrap()
                .quantity,
            0.3
        );
        assert_eq!(s.holdings[0].quantity, Some(0.0));
        assert_eq!(s.holdings[0].cost_basis, 0.0);
        assert!(db
            .save_holding_trade(&next(&s, "buy", 1.0, 1.0, 0.0, "2025-12-31"))
            .is_err());
        let other = Database::open(&dir.path().join("atomic.db")).unwrap();
        first.price = 0.0;
        first.new_holding.as_mut().unwrap().current_price = Some(0.0);
        assert!(other.save_holding_trade(&first).is_err());
        assert!(other.snapshot().unwrap().holdings.is_empty());
        assert!(other.snapshot().unwrap().holding_trades.is_empty());
        first.price = 10.0;
        first.quantity = 0.1;
        first.new_holding.as_mut().unwrap().current_price = Some(10.0);
        let s = other.save_holding_trade(&first).unwrap();
        let s = other
            .save_holding_trade(&next(&s, "buy", 0.2, 20.0, 0.0, "2026-01-02"))
            .unwrap();
        let s = other
            .save_holding_trade(&next(&s, "sell", 0.3, 30.0, 0.0, "2026-01-03"))
            .unwrap();
        assert_eq!(s.holdings[0].quantity, Some(0.0));
        assert_eq!(s.holdings[0].cost_basis, 0.0);
    }
}
