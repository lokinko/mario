use super::{params, AppError, AppResult, Connection, Database, HoldingInput, Snapshot, Utc, Uuid};
use crate::models::{
    Holding, HoldingTrade, HoldingTradeDeleteInput, HoldingTradeInput, HoldingTradeSummary,
    HoldingTradeUpdateInput,
};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::{BTreeMap, HashSet};

fn invalid(s: &str) -> AppError {
    AppError::Validation(s.into())
}
fn number(s: &str) -> AppResult<Decimal> {
    if s.is_empty() || s.len() > 32 || !s.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return Err(invalid("数量和金额须为普通十进制数，最多 8 位小数"));
    }
    let n = Decimal::from_str_exact(s).map_err(|_| invalid("数量或金额格式无效"))?;
    if n.scale() > 8 || n < Decimal::ZERO || n > Decimal::from(1_000_000_000_000_000_i64) {
        return Err(invalid("数量或金额超出范围，最多 8 位小数"));
    }
    Ok(n)
}
fn text(n: Decimal) -> String {
    n.normalize().to_string()
}
fn checked(n: Option<Decimal>) -> AppResult<Decimal> {
    n.filter(|n| n.abs() <= Decimal::from(1_000_000_000_000_000_i64))
        .ok_or_else(|| invalid("累计数量或金额超过允许范围"))
}
fn values(t: &HoldingTrade) -> AppResult<[Decimal; 5]> {
    Ok([
        number(&t.quantity)?,
        number(&t.price)?,
        number(&t.gross_amount)?,
        number(&t.fees)?,
        number(&t.taxes)?,
    ])
}
fn validate(t: &HoldingTrade) -> AppResult<()> {
    Uuid::parse_str(&t.id).map_err(|_| invalid("交易请求编号无效"))?;
    let date = chrono::NaiveDate::parse_from_str(&t.occurred_on, "%Y-%m-%d")
        .map_err(|_| invalid("交易日期无效"))?;
    if date.to_string() != t.occurred_on || t.occurred_on > crate::models::market::today() {
        return Err(invalid("交易日期不能晚于今天"));
    }
    chrono::DateTime::parse_from_rfc3339(&t.created_at).map_err(|_| invalid("交易创建时间无效"))?;
    chrono::DateTime::parse_from_rfc3339(&t.updated_at).map_err(|_| invalid("交易更新时间无效"))?;
    super::validate_currency(&t.currency)?;
    if t.sequence == 0
        || t.sequence > 1_000_000
        || t.account.trim().is_empty()
        || t.account != t.account.trim()
        || t.account.chars().count() > 120
        || t.external_id.chars().count() > 200
        || t.note.chars().count() > 2000
    {
        return Err(invalid("请核对账户、同日成交顺序或说明长度"));
    }
    let [q, p, gross, fees, taxes] = values(t)?;
    if gross <= Decimal::ZERO {
        return Err(invalid("成交金额或税前分红须大于零"));
    }
    match t.kind.as_str() {
        "buy" | "sell" if q > Decimal::ZERO && p > Decimal::ZERO => {
            let computed = checked(q.checked_mul(p))?;
            let tolerance = if t.currency == "JPY" {
                Decimal::ONE
            } else {
                Decimal::new(1, 2)
            };
            if (computed - gross).abs() > tolerance {
                return Err(invalid("成交金额与数量 × 单价不一致，请按券商成交单核对"));
            }
        }
        "dividend" if q.is_zero() && p.is_zero() => {}
        _ => {
            return Err(invalid(
                "仅支持买入、卖出和现金分红；分红不填写成交数量和单价",
            ))
        }
    }
    checked(gross.checked_add(fees).and_then(|n| n.checked_add(taxes)))?;
    Ok(())
}
pub(super) fn load(conn: &Connection) -> AppResult<Vec<HoldingTrade>> {
    let mut stmt = conn.prepare("SELECT payload FROM holding_trades WHERE holding_id NOT IN (SELECT holding_id FROM trading_quarantine) ORDER BY created_at, id")?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
}

// The server alone replays dated executions; decimal quantities never depend on float tolerances.
pub(super) fn split_ratio(split: &crate::models::SplitEvent) -> AppResult<Decimal> {
    split_value(Decimal::ONE, split)
}

pub(super) fn split_value(
    quantity: Decimal,
    split: &crate::models::SplitEvent,
) -> AppResult<Decimal> {
    let numerator = number(&split.numerator)?;
    let denominator = number(&split.denominator)?;
    if numerator.is_zero()
        || denominator.is_zero()
        || chrono::NaiveDate::parse_from_str(&split.effective_on, "%Y-%m-%d").is_err()
        || split.effective_on > crate::models::market::today()
    {
        return Err(invalid("公司行动日期或比例无效"));
    }
    let result = checked(
        quantity
            .checked_mul(numerator)
            .and_then(|q| q.checked_div(denominator)),
    )?;
    Ok(result)
}

pub(super) fn split_quantity(
    quantity: Decimal,
    split: &crate::models::SplitEvent,
) -> AppResult<Decimal> {
    let result = split_value(quantity, split)?;
    if result.normalize().scale() > 8 {
        return Err(invalid(
            "公司行动产生零碎股或超过 8 位小数，请核对零碎股现金结算，不能自动取整",
        ));
    }
    Ok(result)
}

pub(super) fn position_with_splits(
    trades: &[HoldingTrade],
    splits: &[crate::models::SplitEvent],
) -> AppResult<HoldingTradeSummary> {
    let mut ordered: Vec<_> = trades.iter().collect();
    ordered.sort_by(|a, b| (&a.occurred_on, a.sequence).cmp(&(&b.occurred_on, b.sequence)));
    let (mut ids, mut sequences, mut identities) = (HashSet::new(), HashSet::new(), HashSet::new());
    let (
        mut quantity,
        mut basis,
        mut realized,
        mut dividends,
        mut fees_total,
        mut taxes_total,
        mut cash,
    ) = (
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
    );
    let mut result = HoldingTradeSummary::default();
    let mut bought = false;
    let mut splits = splits.iter().collect::<Vec<_>>();
    splits.sort_by_key(|s| &s.effective_on);
    let mut splits = splits.into_iter().peekable();
    for t in ordered {
        while splits
            .peek()
            .is_some_and(|s| s.effective_on <= t.occurred_on)
        {
            quantity = split_quantity(quantity, splits.next().unwrap())?;
        }
        validate(t)?;
        let first = trades.first().unwrap();
        if t.holding_id != first.holding_id
            || t.currency != first.currency
            || t.account != first.account
        {
            return Err(invalid("每个持仓只对应同一账户、同一证券和同一币种"));
        }
        if !ids.insert(&t.id) || !sequences.insert((&t.occurred_on, t.sequence)) {
            return Err(invalid("交易编号或同日成交顺序重复"));
        }
        if !t.external_id.is_empty() && !identities.insert((&t.account, &t.external_id)) {
            return Err(AppError::Conflict("同一账户的成交编号已存在".into()));
        }
        let [q, _, gross, fees, taxes] = values(t)?;
        let expenses = fees + taxes;
        let flow;
        match t.kind.as_str() {
            "buy" => {
                bought = true;
                quantity = checked(quantity.checked_add(q))?;
                basis = checked(
                    basis
                        .checked_add(gross)
                        .and_then(|n| n.checked_add(expenses)),
                )?;
                flow = -(gross + expenses);
            }
            "sell" => {
                if q > quantity {
                    return Err(invalid(&format!(
                        "{} 第 {} 笔卖出超过当时持仓 {}，请先补全此前买入",
                        t.occurred_on,
                        t.sequence,
                        text(quantity)
                    )));
                }
                let sold = if q == quantity {
                    basis
                } else {
                    checked(
                        q.checked_div(quantity)
                            .and_then(|fraction| basis.checked_mul(fraction)),
                    )?
                };
                let profit = gross - expenses - sold;
                realized = checked(realized.checked_add(profit))?;
                result.profits.insert(t.id.clone(), text(profit));
                quantity -= q;
                basis = if quantity.is_zero() {
                    Decimal::ZERO
                } else {
                    basis - sold
                };
                flow = gross - expenses;
            }
            "dividend" => {
                // A payment can arrive after shares were sold following the ex-dividend date.
                if !bought {
                    return Err(invalid("现金分红前须有该账户证券的买入记录"));
                }
                flow = gross - expenses;
                dividends = checked(dividends.checked_add(flow))?;
            }
            _ => unreachable!(),
        }
        cash = checked(cash.checked_add(flow))?;
        fees_total = checked(fees_total.checked_add(fees))?;
        taxes_total = checked(taxes_total.checked_add(taxes))?;
        result.cash_flows.insert(t.id.clone(), text(flow));
        result.account = t.account.clone();
    }
    for split in splits {
        quantity = split_quantity(quantity, split)?;
    }
    result.quantity = text(quantity);
    result.cost_basis = text(basis);
    result.average_cost = text(if quantity.is_zero() {
        Decimal::ZERO
    } else {
        checked(basis.checked_div(quantity))?
    });
    result.realized_profit = text(realized);
    result.dividend_income = text(dividends);
    result.settled_profit = text(checked(realized.checked_add(dividends))?);
    result.total_fees = text(fees_total);
    result.total_taxes = text(taxes_total);
    result.net_cash_flow = text(cash);
    Ok(result)
}
pub(super) fn summaries(
    conn: &Connection,
    trades: &[HoldingTrade],
) -> AppResult<BTreeMap<String, HoldingTradeSummary>> {
    let mut groups: BTreeMap<String, Vec<HoldingTrade>> = BTreeMap::new();
    let mut identities = HashSet::new();
    for t in trades {
        if !t.external_id.is_empty() && !identities.insert((&t.account, &t.external_id)) {
            return Err(AppError::Conflict("同一账户的成交编号已存在".into()));
        }
        groups
            .entry(t.holding_id.clone())
            .or_default()
            .push(t.clone());
    }
    groups
        .into_iter()
        .map(|(id, rows)| {
            let summary = super::corporate::position(conn, &id, &rows)?;
            Ok((id, summary))
        })
        .collect()
}
pub(super) fn guard_edit(conn: &Connection, id: &str, input: &mut HoldingInput) -> AppResult<bool> {
    let s = Database::snapshot_on(conn)?;
    if s.cleared_trade_holdings.iter().any(|item| item == id) {
        return Err(invalid("交易链已隔离，请先修复原始记录"));
    }
    if s.holding_trade_summaries.contains_key(id) {
        let h = s.holdings.iter().find(|h| h.id == id).unwrap();
        // JSON float parsing can move an unchanged decimal-derived average by a few ULPs.
        let same_number = |a: f64, b: f64| {
            a.is_finite() && b.is_finite() && (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(1.0)
        };
        let same_optional = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => same_number(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same_optional(h.quantity, input.quantity)
            || !same_optional(h.cost_price, input.cost_price)
            || !same_number(h.cost_basis, input.cost_basis)
            || h.currency != input.currency.trim().to_ascii_uppercase()
            || h.asset_class != input.asset_class
        {
            return Err(invalid(
                "交易持仓的数量、成本、币种和资产类别由真实成交管理，请编辑交易",
            ));
        }
        // Always persist the server's execution-derived values, never their float round trip.
        input.quantity = h.quantity;
        input.cost_price = h.cost_price;
        input.cost_basis = h.cost_basis;
        return Ok(true);
    }
    Ok(false)
}
pub(super) fn rebuild(conn: &Connection, h: &Holding, s: &HoldingTradeSummary) -> AppResult<()> {
    let quantity = number(&s.quantity)?.to_f64().unwrap();
    let basis = Decimal::from_str_exact(&s.cost_basis)
        .unwrap()
        .to_f64()
        .unwrap();
    let cost_price = Decimal::from_str_exact(&s.average_cost)
        .unwrap()
        .to_f64()
        .unwrap();
    let exact = super::amounts::rebuild(conn, h, s)?;
    let price = Decimal::from_str_exact(exact.current_price.as_deref().unwrap())
        .map_err(|_| invalid("价格无效"))?
        .to_f64()
        .unwrap();
    let market_value = Decimal::from_str_exact(&exact.market_value)
        .map_err(|_| invalid("市值无效"))?
        .to_f64()
        .unwrap();
    if !market_value.is_finite() || market_value > 1e15 {
        return Err(invalid("持仓市值超过允许范围"));
    }
    // Never overwrite a market quote with an execution price or date.
    conn.execute("UPDATE holdings SET quantity=?2,cost_basis=?3,cost_price=?4,market_value=?5,current_price=?6,updated_at=?7 WHERE id=?1",
        params![h.id,quantity,basis,cost_price,market_value,price,Utc::now().to_rfc3339()])?;
    conn.execute(
        "DELETE FROM holding_valuations WHERE holding_id=?1",
        [&h.id],
    )?;
    super::fx::apply_holdings(conn)?;
    Ok(())
}
impl Database {
    pub fn delete_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeDeleteInput,
    ) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let operation = super::financial::operation("delete", id, input)?;
        if let Some(snapshot) = super::financial::replay(&tx, &operation)? {
            return Ok(snapshot);
        }
        let before = super::daily::prepare(&tx)?;
        // A retry after a successful deletion must not alter any other record.
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM holding_trades t JOIN trading_quarantine q ON q.holding_id=t.holding_id WHERE t.id=?1)",[id],|r|r.get::<_,bool>(0))? {
            return Err(invalid("交易链已隔离，请先预览修复，原始记录不能直接删除"));
        }
        let Some(trade) = before.holding_trades.iter().find(|t| t.id == id) else {
            let snapshot = super::financial::complete(&tx, &operation)?;
            tx.commit()?;
            return Ok(snapshot);
        };
        let h = before
            .holdings
            .iter()
            .find(|h| h.id == trade.holding_id)
            .ok_or_else(|| invalid("交易对应持仓不存在"))?;
        if before.holding_revisions.get(&h.id) != Some(&input.expected_revision) {
            return Err(AppError::Conflict(
                "持仓已更新，请刷新并核对后重新删除".into(),
            ));
        }
        let chain: Vec<_> = before
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == h.id && t.id != id)
            .cloned()
            .collect();
        let summary = super::corporate::position(&tx, &h.id, &chain).map_err(|e| match e {
            AppError::Validation(message) => invalid(&format!(
                "无法删除这笔交易：{message}；请先编辑或删除依赖它的后续交易"
            )),
            other => other,
        })?;
        let cash_delta = super::cash::settle_trade(
            &tx,
            &h.currency,
            id,
            &trade.occurred_on,
            None,
            crate::models::SettlementMode::Auto,
            "",
        )?;
        super::financial::journal(
            &tx,
            &h.id,
            "trade.delete",
            trade,
            &Option::<HoldingTrade>::None,
            &text(cash_delta),
            ("用户删除交易", Some(&operation.0)),
        )?;
        tx.execute("DELETE FROM holding_trades WHERE id=?1", [id])?;
        rebuild(&tx, h, &summary)?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }
    pub fn save_holding_trade(&self, input: &HoldingTradeInput) -> AppResult<Snapshot> {
        self.write_holding_trade(None, input)
    }
    pub fn update_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeUpdateInput,
    ) -> AppResult<Snapshot> {
        self.write_holding_trade(
            Some(id),
            &HoldingTradeInput {
                expected_cash_baseline: String::new(),
                request_id: id.into(),
                holding_id: String::new(),
                expected_revision: Some(input.expected_revision.clone()),
                new_holding: None,
                kind: input.kind.clone(),
                occurred_on: input.occurred_on.clone(),
                sequence: input.sequence,
                quantity: input.quantity.clone(),
                price: input.price.clone(),
                gross_amount: input.gross_amount.clone(),
                fees: input.fees.clone(),
                taxes: input.taxes.clone(),
                account: input.account.clone(),
                external_id: input.external_id.clone(),
                note: input.note.clone(),
                settlement_mode: input.settlement_mode,
            },
        )
    }
    fn write_holding_trade(
        &self,
        edit_id: Option<&str>,
        input: &HoldingTradeInput,
    ) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let operation = if let Some(id) = edit_id {
            let update = HoldingTradeUpdateInput {
                expected_revision: input.expected_revision.clone().unwrap_or_default(),
                kind: input.kind.clone(),
                occurred_on: input.occurred_on.clone(),
                sequence: input.sequence,
                quantity: input.quantity.clone(),
                price: input.price.clone(),
                gross_amount: input.gross_amount.clone(),
                fees: input.fees.clone(),
                taxes: input.taxes.clone(),
                account: input.account.clone(),
                external_id: input.external_id.clone(),
                note: input.note.clone(),
                settlement_mode: input.settlement_mode,
            };
            super::financial::operation("edit", id, &update)?
        } else {
            super::financial::operation("create", &input.request_id, input)?
        };
        if let Some(snapshot) = super::financial::replay(&tx, &operation)? {
            return Ok(snapshot);
        }
        let before = super::daily::prepare(&tx)?;
        let original = before
            .holding_trades
            .iter()
            .find(|t| t.id == input.request_id);
        if edit_id.is_some() && original.is_none() {
            return Err(AppError::NotFound("找不到交易记录".into()));
        }
        let now = Utc::now().to_rfc3339();
        let h = if let Some(t) = original {
            before
                .holdings
                .iter()
                .find(|h| h.id == t.holding_id)
                .cloned()
                .ok_or_else(|| invalid("交易对应持仓不存在"))?
        } else if let Some(h) = &input.new_holding {
            if !input.holding_id.is_empty() || input.kind != "buy" {
                return Err(invalid("新证券须从第一笔真实买入开始"));
            }
            super::validate_currency(&h.currency)?;
            if h.name.trim().is_empty()
                || h.name.chars().count() > 200
                || h.symbol.chars().count() > 80
                || !["股票", "基金", "债券", "黄金", "其他"].contains(&h.asset_class.as_str())
            {
                return Err(invalid("请核对证券名称、代码和资产类别"));
            }
            if before.holdings.iter().any(|existing| {
                existing.currency == h.currency.trim().to_ascii_uppercase()
                    && existing.asset_class == h.asset_class
                    && (before.cleared_trade_holdings.contains(&existing.id)
                        || before
                            .holding_trade_summaries
                            .get(&existing.id)
                            .is_some_and(|s| s.account == input.account.trim()))
                    && if h.symbol.trim().is_empty() {
                        existing.symbol.is_empty() && existing.name == h.name.trim()
                    } else {
                        existing.symbol.eq_ignore_ascii_case(h.symbol.trim())
                    }
            }) {
                return Err(AppError::Conflict(
                    "该账户证券已有交易持仓，请在原持仓的交易明细里追加成交".into(),
                ));
            }
            let id = Uuid::new_v4().to_string();
            // Use the first execution as a provisional reference until a current quote is available.
            tx.execute("INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,fx_rate_to_base,valuation_date,fx_rate_source,fx_rate_observed_on,created_at,updated_at,quantity,current_price) VALUES (?1,?2,?3,?4,0,0,0,?5,NULL,?6,'','',?7,?7,0,?8)",
                params![id,h.symbol.trim(),h.name.trim(),h.asset_class,h.currency.trim().to_ascii_uppercase(),crate::models::market::today(),now,number(input.price.trim())?.to_f64().unwrap()])?;
            Database::snapshot_on(&tx)?
                .holdings
                .into_iter()
                .find(|h| h.id == id)
                .unwrap()
        } else {
            before
                .holdings
                .iter()
                .find(|h| h.id == input.holding_id)
                .cloned()
                .ok_or_else(|| invalid("请选择交易对应的证券持仓"))?
        };
        if h.asset_class == "现金" {
            return Err(invalid("现金账户不能录入证券交易"));
        }
        if before.cleared_trade_holdings.contains(&h.id) {
            return Err(invalid(
                "交易链已隔离，请先预览并修复原始记录；不能重新录入导致重复结算",
            ));
        }
        if before
            .corporate_actions
            .iter()
            .any(|a| a.holding_id == h.id && !a.applied && !a.dismissed)
        {
            return Err(invalid("存在待确认公司行动，请先核对数量与成本调整"));
        }
        let mut trade = HoldingTrade {
            id: input.request_id.clone(),
            holding_id: h.id.clone(),
            kind: input.kind.clone(),
            occurred_on: input.occurred_on.trim().into(),
            sequence: input.sequence,
            quantity: text(number(input.quantity.trim())?),
            price: text(number(input.price.trim())?),
            gross_amount: text(number(input.gross_amount.trim())?),
            fees: text(number(input.fees.trim())?),
            taxes: text(number(input.taxes.trim())?),
            currency: h.currency.clone(),
            account: input.account.trim().into(),
            external_id: input.external_id.trim().into(),
            note: input.note.trim().into(),
            created_at: original.map_or_else(|| now.clone(), |t| t.created_at.clone()),
            updated_at: original.map_or_else(|| now.clone(), |t| t.updated_at.clone()),
        };
        validate(&trade)?;
        if let Some(old) = original {
            if edit_id.is_none()
                && ((!input.holding_id.is_empty() && input.holding_id != h.id)
                    || input.new_holding.as_ref().is_some_and(|n| {
                        n.name.trim() != h.name
                            || n.symbol.trim() != h.symbol
                            || n.asset_class != h.asset_class
                            || n.currency.trim().to_ascii_uppercase() != h.currency
                    }))
            {
                return Err(AppError::Conflict("请求编号已用于另一笔交易".into()));
            }
            if &trade == old {
                let snapshot = super::financial::complete(&tx, &operation)?;
                tx.commit()?;
                return Ok(snapshot);
            }
            if edit_id.is_none() {
                return Err(AppError::Conflict("请求编号已用于另一笔交易".into()));
            }
        }
        if (edit_id.is_some() || input.new_holding.is_none())
            && before.holding_revisions.get(&h.id) != input.expected_revision.as_ref()
        {
            return Err(AppError::Conflict(
                "持仓已更新，请刷新并核对后重新填写".into(),
            ));
        }
        if !trade.external_id.is_empty()
            && before.holding_trades.iter().any(|t| {
                t.id != trade.id && t.account == trade.account && t.external_id == trade.external_id
            })
        {
            return Err(AppError::Conflict("同一账户的成交编号已存在".into()));
        }
        trade.updated_at = now;
        let mut chain: Vec<_> = before
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == h.id && t.id != trade.id)
            .cloned()
            .collect();
        chain.push(trade.clone());
        let summary = super::corporate::position(&tx, &h.id, &chain)?;
        let cash_delta = super::cash::settle_trade(
            &tx,
            &h.currency,
            &trade.id,
            &trade.occurred_on,
            Some(Decimal::from_str_exact(&summary.cash_flows[&trade.id]).unwrap()),
            input.settlement_mode,
            &input.expected_cash_baseline,
        )?;
        super::financial::journal(
            &tx,
            &h.id,
            if edit_id.is_some() {
                "trade.edit"
            } else {
                "trade.create"
            },
            &original,
            &trade,
            &text(cash_delta),
            (
                if trade.note.is_empty() {
                    if edit_id.is_some() {
                        "用户更正原记录"
                    } else {
                        "用户新增成交"
                    }
                } else {
                    &trade.note
                },
                Some(&operation.0),
            ),
        )?;
        tx.execute("INSERT INTO holding_trades(id,holding_id,payload,created_at) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
            params![trade.id,h.id,serde_json::to_string(&trade)?,trade.created_at])?;
        rebuild(&tx, &h, &summary)?;
        tx.execute(
            "DELETE FROM settings WHERE key=?1",
            [format!("trading.reset.{}", h.id)],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }
}
pub(super) fn validate_stored(conn: &Connection) -> AppResult<()> {
    let s = Database::snapshot_on(conn)?;
    for t in &s.holding_trades {
        if !s
            .holdings
            .iter()
            .any(|h| h.id == t.holding_id && h.currency == t.currency && h.asset_class != "现金")
        {
            return Err(invalid("交易的证券关联或币种无效"));
        }
    }
    for h in &s.holdings {
        let Some(summary) = s.holding_trade_summaries.get(&h.id) else {
            continue;
        };
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-8_f64.max(b.abs() * 1e-10);
        if !h
            .quantity
            .is_some_and(|n| near(n, number(&summary.quantity).unwrap().to_f64().unwrap()))
            || !near(
                h.cost_basis,
                Decimal::from_str_exact(&summary.cost_basis)
                    .unwrap()
                    .to_f64()
                    .unwrap(),
            )
            || !h.cost_price.is_some_and(|n| {
                near(
                    n,
                    Decimal::from_str_exact(&summary.average_cost)
                        .unwrap()
                        .to_f64()
                        .unwrap(),
                )
            })
        {
            return Err(invalid("持仓数量和成本与成交明细不一致"));
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
        if s.cleared_trade_holdings.contains(&holding_id) {
            continue;
        }
        let t: HoldingTrade = serde_json::from_str(&payload)?;
        if t.id != id || t.holding_id != holding_id || t.created_at != created_at {
            return Err(invalid("交易字段与数据库记录不一致"));
        }
    }
    for holding_id in &s.cleared_trade_holdings {
        let raw: String = conn.query_row(
            "SELECT payload FROM trading_quarantine WHERE holding_id=?1",
            [holding_id],
            |r| r.get(0),
        )?;
        let original: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
        let mut statement = conn.prepare(
            "SELECT id,payload,created_at FROM holding_trades WHERE holding_id=?1 ORDER BY id",
        )?;
        let actual = statement
            .query_map([holding_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut original = original
            .into_iter()
            .map(|r| {
                if r["holdingId"].as_str() != Some(holding_id.as_str()) {
                    return Err(invalid("隔离原文关联不一致"));
                }
                Ok((
                    r["id"]
                        .as_str()
                        .ok_or_else(|| invalid("隔离编号缺失"))?
                        .to_owned(),
                    r["payload"]
                        .as_str()
                        .ok_or_else(|| invalid("隔离原文缺失"))?
                        .to_owned(),
                    r["createdAt"]
                        .as_str()
                        .ok_or_else(|| invalid("隔离时间缺失"))?
                        .to_owned(),
                ))
            })
            .collect::<AppResult<Vec<_>>>()?;
        original.sort();
        if original.is_empty() || original != actual {
            return Err(invalid("隔离证据与原始行不一致，不能恢复"));
        }
    }
    Ok(())
}
// Preserve every source row. Invalid chains are excluded from derived calculations until repaired.
pub(super) fn quarantine_invalid(conn: &Connection) -> AppResult<()> {
    let mut statement =
        conn.prepare("SELECT id,holding_id,payload,created_at FROM holding_trades")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut groups: BTreeMap<String, Vec<HoldingTrade>> = BTreeMap::new();
    let mut raw: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    let mut invalid_ids = HashSet::new();
    let mut identities: BTreeMap<(String, String), HashSet<String>> = BTreeMap::new();
    for (id, holding_id, payload, created_at) in rows {
        raw.entry(holding_id.clone()).or_default().push(serde_json::json!({"id":id,"holdingId":holding_id,"payload":payload,"createdAt":created_at}));
        match serde_json::from_str::<HoldingTrade>(&payload) {
            Ok(t) if t.id == id && t.holding_id == holding_id && t.created_at == created_at => {
                if !t.external_id.is_empty() {
                    identities
                        .entry((t.account.clone(), t.external_id.clone()))
                        .or_default()
                        .insert(holding_id.clone());
                }
                groups.entry(holding_id).or_default().push(t);
            }
            _ => {
                invalid_ids.insert(holding_id);
            }
        }
    }
    for ids in identities.values().filter(|ids| ids.len() > 1) {
        invalid_ids.extend(ids.iter().cloned());
    }
    for (id, trades) in &groups {
        let matching = conn.query_row(
            "SELECT currency,asset_class FROM holdings WHERE id=?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        if super::corporate::position(conn, id, trades).is_err()
            || !matching.is_ok_and(|(currency, class)| {
                class != "现金" && trades.iter().all(|t| t.currency == currency)
            })
        {
            invalid_ids.insert(id.clone());
        }
    }
    for id in invalid_ids {
        let payload = serde_json::to_string(&raw[&id])?;
        let reason = "原始交易格式、关联或交易链校验失败；数据已保留，请预览修复后恢复计算";
        conn.execute("INSERT INTO trading_quarantine(holding_id,payload,reason,created_at) VALUES (?1,?2,?3,?4) ON CONFLICT(holding_id) DO NOTHING",params![id,payload,reason,Utc::now().to_rfc3339()])?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn balance(s: &Snapshot, currency: &str) -> f64 {
        s.holdings
            .iter()
            .find(|h| h.asset_class == "现金" && h.currency == currency)
            .map_or(0.0, |h| h.market_value)
    }

    #[test]
    fn trades_settle_each_currency_and_keep_total_assets_correct() {
        for currency in ["CNY", "HKD", "USD"] {
            let db = Database::open(std::path::Path::new(":memory:")).unwrap();
            db.save_profile(&crate::models::FinancialProfile {
                base_currency: currency.into(),
                ..Default::default()
            })
            .unwrap();
            super::super::cash::fund(&db, currency);
            let mut buy = first();
            buy.new_holding.as_mut().unwrap().currency = currency.into();
            buy.taxes = "1".into();
            let s = db.save_holding_trade(&buy).unwrap();
            assert_eq!(balance(&s, currency), 9897.0);
            assert_eq!(s.total_value, 9997.0); // Only fees/taxes leave the portfolio.
            let s = db.save_holding_trade(&buy).unwrap();
            assert_eq!(balance(&s, currency), 9897.0);
            let mut sale = next(&s, "sell", "5", "12", "2026-01-02");
            sale.fees = "1".into();
            sale.taxes = "2".into();
            let s = db.save_holding_trade(&sale).unwrap();
            assert_eq!(balance(&s, currency), 9954.0);
            let mut dividend = next(&s, "dividend", "0", "0", "2026-01-03");
            dividend.gross_amount = "10".into();
            dividend.taxes = "2".into();
            let s = db.save_holding_trade(&dividend).unwrap();
            assert_eq!(balance(&s, currency), 9962.0);
            let mut correction = edit(&s, &buy.request_id);
            correction.fees = "5".into();
            let s = db
                .update_holding_trade(&buy.request_id, &correction)
                .unwrap();
            assert_eq!(balance(&s, currency), 9959.0);
            assert_eq!(
                balance(
                    &db.update_holding_trade(&buy.request_id, &correction)
                        .unwrap(),
                    currency
                ),
                9959.0
            );
            let mut s = s;
            for id in [&dividend.request_id, &sale.request_id, &buy.request_id] {
                let revision = s.holding_revisions[&s
                    .holding_trades
                    .iter()
                    .find(|t| &t.id == id)
                    .unwrap()
                    .holding_id]
                    .clone();
                s = db
                    .delete_holding_trade(
                        id,
                        &HoldingTradeDeleteInput {
                            expected_revision: revision,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(balance(&s, currency), 10000.0);
            assert_eq!(s.total_value, 10000.0);
        }
    }

    #[test]
    fn insufficient_cash_and_failed_daily_writes_roll_back_the_entire_trade() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let buy = first();
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        assert!(db
            .save_holding_trade(&buy)
            .unwrap_err()
            .to_string()
            .contains("存款余额不足"));
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
        assert!(db.snapshot().unwrap().holdings.is_empty());
        super::super::cash::fund(&db, "CNY");
        let s = db.save_holding_trade(&buy).unwrap();
        let cash = s.holdings.iter().find(|h| h.asset_class == "现金").unwrap();
        db.update_holding_amount(
            &cash.id,
            &crate::models::daily::AmountInput {
                amount: 0.0.to_string(),
                expected_revision: s.holding_revisions[&cash.id].clone(),
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .unwrap();
        let s = db.snapshot().unwrap();
        let mut edited = edit(&s, &buy.request_id);
        edited.fees = "3".into();
        let changed = db.update_holding_trade(&buy.request_id, &edited).unwrap();
        assert_eq!(balance(&changed, "CNY"), 0.0);
        assert!(changed.cash_balances[0].needs_reconciliation);
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
        db.conn().unwrap().execute_batch("CREATE TRIGGER fail_cash_daily BEFORE INSERT ON daily_entries BEGIN SELECT RAISE(ABORT,'daily write failed'); END;").unwrap();
        edited.fees = "1".into(); // Would refund one unit before the daily write fails.
        assert!(db.update_holding_trade(&buy.request_id, &edited).is_err());
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
    }

    #[test]
    fn deleting_income_covered_by_a_new_balance_confirmation_requires_reconciliation() {
        let db = funded_open(std::path::Path::new(":memory:")).unwrap();
        let buy = first();
        let s = db.save_holding_trade(&buy).unwrap();
        let sale = next(&s, "sell", "10", "20", "2026-01-02");
        let s = db.save_holding_trade(&sale).unwrap();
        let cash = s.holdings.iter().find(|h| h.asset_class == "现金").unwrap();
        db.update_holding_amount(
            &cash.id,
            &crate::models::daily::AmountInput {
                amount: 0.0.to_string(),
                expected_revision: s.holding_revisions[&cash.id].clone(),
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .unwrap();
        let s = db.snapshot().unwrap();
        let changed = db
            .delete_holding_trade(
                &sale.request_id,
                &HoldingTradeDeleteInput {
                    expected_revision: s.holding_revisions[&s.holdings[0].id].clone(),
                },
            )
            .unwrap();
        assert_eq!(balance(&changed, "CNY"), 0.0);
        assert!(changed.cash_balances[0].needs_reconciliation);
    }

    #[test]
    fn cash_is_shared_between_positions_but_never_between_currencies() {
        let db = funded_open(std::path::Path::new(":memory:")).unwrap();
        db.save_holding_trade(&first()).unwrap();
        let mut other = first();
        other.new_holding.as_mut().unwrap().name = "另一证券".into();
        let s = db.save_holding_trade(&other).unwrap();
        assert_eq!(balance(&s, "CNY"), 9796.0);
        let mut usd = first();
        usd.new_holding.as_mut().unwrap().currency = "USD".into();
        assert!(db
            .save_holding_trade(&usd)
            .unwrap_err()
            .to_string()
            .contains("USD 存款余额不足"));
        assert_eq!(balance(&db.snapshot().unwrap(), "CNY"), 9796.0);
        let cash = s.holdings.iter().find(|h| h.asset_class == "现金").unwrap();
        db.update_holding_amount(
            &cash.id,
            &crate::models::daily::AmountInput {
                amount: 0.30.to_string(),
                expected_revision: s.holding_revisions[&cash.id].clone(),
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .unwrap();
        let mut tiny = next(
            &db.snapshot().unwrap(),
            "buy",
            "0.1",
            "1",
            &crate::models::market::today(),
        );
        for expected in [0.2, 0.1, 0.0] {
            tiny.request_id = Uuid::new_v4().to_string();
            tiny.expected_revision = db
                .snapshot()
                .unwrap()
                .holding_revisions
                .get(&tiny.holding_id)
                .cloned();
            let s = db.save_holding_trade(&tiny).unwrap();
            assert_eq!(balance(&s, "CNY"), expected);
            tiny.sequence += 1;
        }
        tiny.request_id = Uuid::new_v4().to_string();
        tiny.expected_revision = db
            .snapshot()
            .unwrap()
            .holding_revisions
            .get(&tiny.holding_id)
            .cloned();
        assert!(db.save_holding_trade(&tiny).is_err());
    }

    #[test]
    fn legacy_backup_keeps_current_cash_and_later_corrections_only_settle_the_difference() {
        let db = funded_open(std::path::Path::new(":memory:")).unwrap();
        let buy = first();
        db.save_holding_trade(&buy).unwrap();
        let mut backup = db.export_sync_data().unwrap();
        backup.schema_version = 19;
        backup.tables.retain(|t| {
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
        let copy = Database::open(std::path::Path::new(":memory:")).unwrap();
        copy.import_sync_data(&backup).unwrap();
        let s = copy.snapshot().unwrap();
        assert_eq!(balance(&s, "CNY"), 9898.0);
        let mut correction = edit(&s, &buy.request_id);
        correction.fees = "3".into();
        let s = copy
            .update_holding_trade(&buy.request_id, &correction)
            .unwrap();
        assert_eq!(balance(&s, "CNY"), 9898.0);
        assert!(s.cash_balances[0].needs_reconciliation);
        // A fresh backup also carries the already settled current balance.
        db.import_sync_data(&copy.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(balance(&db.snapshot().unwrap(), "CNY"), 9898.0);
    }

    #[test]
    fn sub_float_unit_cash_debits_are_preserved_in_the_exact_balance() {
        let db = funded_open(std::path::Path::new(":memory:")).unwrap();
        let s = db.snapshot().unwrap();
        let cash = &s.holdings[0];
        db.update_holding_amount(
            &cash.id,
            &crate::models::daily::AmountInput {
                amount: 1e15.to_string(),
                expected_revision: s.holding_revisions[&cash.id].clone(),
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .unwrap();
        let mut buy = first();
        buy.quantity = "0.01".into();
        buy.price = "1".into();
        buy.gross_amount = "0.01".into();
        buy.fees = "0".into();
        buy.occurred_on = crate::models::market::today();
        buy.expected_cash_baseline = db.snapshot().unwrap().cash_balances[0].baseline_id.clone();
        let snapshot = db.save_holding_trade(&buy).unwrap();
        assert_eq!(snapshot.cash_balances[0].amount, "999999999999999.99");
        let restored = Database::open(std::path::Path::new(":memory:")).unwrap();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(
            restored.snapshot().unwrap().cash_balances[0].amount,
            "999999999999999.99"
        );
    }
    fn funded_open(path: &std::path::Path) -> AppResult<Database> {
        let db = Database::open(path)?;
        super::super::cash::fund(&db, "CNY");
        Ok(db)
    }
    #[test]
    fn internal_stock_price_edits_preserve_execution_totals_after_json_round_trip() {
        for (quantity, second_quantity, gross, second_gross) in
            [("10", "7", "100", "119.91"), ("0.1", "0.07", "1", "1.1991")]
        {
            let db = funded_open(std::path::Path::new(":memory:")).unwrap();
            let mut buy = first();
            buy.new_holding.as_mut().unwrap().name = "OPPO 股票".into();
            buy.quantity = quantity.into();
            buy.gross_amount = gross.into();
            let s = db.save_holding_trade(&buy).unwrap();
            let mut buy = next(&s, "buy", second_quantity, "17.13", "2026-01-02");
            buy.gross_amount = second_gross.into();
            let original = db.save_holding_trade(&buy).unwrap();
            let id = &original.holdings[0].id;
            let mut s = original.clone();
            for price in [25.0, 30.12345678, 0.0] {
                // from_value bypasses HTTP's JSON float parser and misses this regression.
                let mut input: HoldingInput =
                    serde_json::from_str(&serde_json::to_string(&s.holdings[0]).unwrap()).unwrap();
                input.current_price = Some(price);
                s = db.update_holding(id, &input).unwrap();
                let h = &s.holdings[0];
                assert_eq!(h.current_price, Some(price));
                assert_eq!(h.market_value, h.quantity.unwrap() * price);
                assert_eq!(h.quantity, original.holdings[0].quantity);
                assert_eq!(h.cost_price, original.holdings[0].cost_price);
                assert_eq!(h.cost_basis, original.holdings[0].cost_basis);
                assert_eq!(s.holding_trades, original.holding_trades);
                assert_eq!(s.holding_trade_summaries, original.holding_trade_summaries);
            }
            let input: HoldingInput =
                serde_json::from_str(&serde_json::to_string(&s.holdings[0]).unwrap()).unwrap();
            for field in [
                "quantity",
                "costPrice",
                "costBasis",
                "currency",
                "assetClass",
            ] {
                let mut changed = serde_json::to_value(&input).unwrap();
                changed[field] = match field {
                    "currency" => serde_json::json!("USD"),
                    "assetClass" => serde_json::json!("基金"),
                    _ => serde_json::json!(999.0),
                };
                let changed = serde_json::from_value(changed).unwrap();
                assert!(db.update_holding(id, &changed).is_err(), "{field}");
                let unchanged = db.snapshot().unwrap();
                assert_eq!(
                    serde_json::to_value(&unchanged.holdings).unwrap(),
                    serde_json::to_value(&s.holdings).unwrap()
                );
                assert_eq!(unchanged.holding_trades, s.holding_trades);
            }
        }
    }
    fn first() -> HoldingTradeInput {
        serde_json::from_value(serde_json::json!({
            "expectedCashBaseline":"00000000-0000-0000-0000-000000000001","requestId":Uuid::new_v4().to_string(),"newHolding":{"name":"交易测试","symbol":"","assetClass":"股票","currency":"CNY"},
            "kind":"buy","occurredOn":"2026-01-01","sequence":1,"quantity":"10","price":"10","grossAmount":"100","fees":"2","taxes":"0","account":"券商A","settlementMode":"settle"
        })).unwrap()
    }
    fn next(s: &Snapshot, kind: &str, q: &str, p: &str, date: &str) -> HoldingTradeInput {
        let h = &s.holdings[0];
        let gross = text(number(q).unwrap() * number(p).unwrap());
        HoldingTradeInput {
            expected_cash_baseline: s
                .cash_balances
                .iter()
                .find(|c| c.currency == h.currency)
                .map(|c| c.baseline_id.clone())
                .unwrap_or_default(),
            request_id: Uuid::new_v4().to_string(),
            new_holding: None,
            holding_id: h.id.clone(),
            expected_revision: s.holding_revisions.get(&h.id).cloned(),
            kind: kind.into(),
            occurred_on: date.into(),
            sequence: s
                .holding_trades
                .iter()
                .filter(|t| t.occurred_on == date)
                .map(|t| t.sequence)
                .max()
                .unwrap_or(0)
                + 1,
            quantity: q.into(),
            price: p.into(),
            gross_amount: gross,
            fees: "0".into(),
            taxes: "0".into(),
            account: "券商A".into(),
            external_id: String::new(),
            note: String::new(),
            settlement_mode: crate::models::SettlementMode::Settle,
        }
    }
    fn edit(s: &Snapshot, id: &str) -> HoldingTradeUpdateInput {
        let t = s.holding_trades.iter().find(|t| t.id == id).unwrap();
        serde_json::from_value(serde_json::json!({
            "expectedRevision":s.holding_revisions[&t.holding_id],"kind":t.kind,"occurredOn":t.occurred_on,"sequence":t.sequence,
            "quantity":t.quantity,"price":t.price,"grossAmount":t.gross_amount,"fees":t.fees,"taxes":t.taxes,
            "account":t.account,"externalId":t.external_id,"note":t.note
        })).unwrap()
    }
    #[test]
    fn real_executions_dividends_fees_and_taxes_drive_one_server_summary() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("real.db")).unwrap();
        let mut buy = first();
        buy.taxes = "1".into();
        let s = db.save_holding_trade(&buy).unwrap();
        assert_eq!(s.holding_trades.len(), 1);
        assert_eq!(s.holdings[0].current_price, Some(10.0));
        let mut b = next(&s, "buy", "10", "20", "2026-01-02");
        b.fees = "3".into();
        b.taxes = "2".into();
        let s = db.save_holding_trade(&b).unwrap();
        let mut sale = next(&s, "sell", "5", "30", "2026-01-03");
        sale.fees = "1".into();
        sale.taxes = "2".into();
        let s = db.save_holding_trade(&sale).unwrap();
        let mut d = next(&s, "dividend", "0", "0", "2026-01-04");
        d.gross_amount = "20".into();
        d.fees = "1".into();
        d.taxes = "3".into();
        let s = db.save_holding_trade(&d).unwrap();
        let summary = &s.holding_trade_summaries[&s.holdings[0].id];
        assert_eq!(summary.quantity, "15");
        assert_eq!(summary.cost_basis, "231");
        assert_eq!(summary.realized_profit, "70");
        assert_eq!(summary.dividend_income, "16");
        assert_eq!(summary.settled_profit, "86");
        assert_eq!(summary.net_cash_flow, "-145");
        assert_eq!(summary.total_fees, "7");
        assert_eq!(summary.total_taxes, "8");
        assert_eq!(summary.profits[&sale.request_id], "70");
        assert_eq!(summary.cash_flows[&d.request_id], "16");
        assert_eq!(s.holdings[0].valuation_date, crate::models::market::today());
        assert_eq!(s.holdings[0].market_value, 150.0);
        assert_eq!(db.save_holding_trade(&d).unwrap().holding_trades.len(), 4);
        let copy = Database::open(&dir.path().join("copy.db")).unwrap();
        let export = db.export_sync_data().unwrap();
        assert_eq!(
            export.schema_version,
            super::super::SYNC_DATASET_SCHEMA_VERSION
        );
        copy.import_sync_data(&export).unwrap();
        assert_eq!(
            copy.snapshot().unwrap().holding_trade_summaries,
            s.holding_trade_summaries
        );
        let mut old = export;
        old.schema_version = 17;
        super::super::tests::add_retired_tables(&mut old);
        old.tables.retain(|table| {
            !matches!(
                table.name.as_str(),
                "decisions" | "decision_reviews" | "portfolio_checkins"
            )
        });
        assert!(copy.import_sync_data(&old).is_err());
        assert_eq!(copy.snapshot().unwrap().holding_trades, s.holding_trades);
    }
    #[test]
    fn fractional_close_is_exact_and_dividend_can_arrive_after_close() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("fraction.db")).unwrap();
        let mut b = first();
        b.quantity = "0.1".into();
        b.gross_amount = "1".into();
        b.fees = "0".into();
        let s = db.save_holding_trade(&b).unwrap();
        let s = db
            .save_holding_trade(&next(&s, "buy", "0.2", "20", "2026-01-02"))
            .unwrap();
        let s = db
            .save_holding_trade(&next(&s, "sell", "0.3", "30", "2026-01-03"))
            .unwrap();
        assert_eq!(s.holdings[0].quantity, Some(0.0));
        assert_eq!(s.holdings[0].cost_basis, 0.0);
        let mut d = next(&s, "dividend", "0", "0", "2026-01-04");
        d.gross_amount = "1.25".into();
        d.taxes = "0.25".into();
        let s = db.save_holding_trade(&d).unwrap();
        let sum = &s.holding_trade_summaries[&s.holdings[0].id];
        assert_eq!(sum.realized_profit, "4");
        assert_eq!(sum.settled_profit, "5");
        assert_eq!(sum.quantity, "0");
    }
    #[test]
    fn deletions_replay_costs_profits_charges_and_zero_the_last_position() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("delete.db");
        let db = funded_open(&file).unwrap();
        let b = first();
        let s = db.save_holding_trade(&b).unwrap();
        let second = next(&s, "buy", "10", "20", "2026-01-02");
        let s = db.save_holding_trade(&second).unwrap();
        let mut sale = next(&s, "sell", "5", "30", "2026-01-03");
        sale.fees = "1".into();
        sale.taxes = "2".into();
        let s = db.save_holding_trade(&sale).unwrap();
        let mut d = next(&s, "dividend", "0", "0", "2026-01-04");
        d.gross_amount = "20".into();
        d.fees = "1".into();
        d.taxes = "3".into();
        let s = db.save_holding_trade(&d).unwrap();
        let id = s.holdings[0].id.clone();
        let mut h: HoldingInput =
            serde_json::from_value(serde_json::to_value(&s.holdings[0]).unwrap()).unwrap();
        h.current_price = Some(23.0);
        let mut s = db.update_holding(&id, &h).unwrap();
        let delete = |s: &Snapshot, trade_id: &str| {
            db.delete_holding_trade(
                trade_id,
                &HoldingTradeDeleteInput {
                    expected_revision: s.holding_revisions[&id].clone(),
                },
            )
            .unwrap()
        };
        let before_revision = s.holding_revisions[&id].clone();
        s = delete(&s, &second.request_id);
        let sum = &s.holding_trade_summaries[&id];
        assert_eq!(sum.quantity, "5");
        assert_eq!(sum.cost_basis, "51");
        assert_eq!(sum.realized_profit, "96");
        assert_eq!(sum.settled_profit, "112");
        assert_eq!(sum.total_fees, "4");
        assert_eq!(sum.total_taxes, "5");
        assert_eq!(sum.net_cash_flow, "61");
        assert_eq!(s.holdings[0].market_value, 115.0);
        assert_eq!(s.holdings[0].current_price, Some(23.0));
        assert_eq!(s.holdings[0].valuation_date, h.valuation_date);
        assert_ne!(s.holding_revisions[&id], before_revision);
        assert!(!sum.cash_flows.contains_key(&second.request_id));
        let daily = db.daily_history(&Default::default()).unwrap();
        assert_eq!(
            daily.records[0]
                .assets
                .iter()
                .find(|a| a.holding.id == id)
                .unwrap()
                .holding
                .market_value,
            115.0
        );
        // An unknown / already deleted ID is a no-op even with the previous revision.
        let baseline = db.export_sync_data().unwrap().content_hash().unwrap();
        db.delete_holding_trade(
            &second.request_id,
            &HoldingTradeDeleteInput {
                expected_revision: before_revision,
            },
        )
        .unwrap();
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            baseline
        );
        s = delete(&s, &sale.request_id);
        assert_eq!(s.holdings[0].quantity, Some(10.0));
        assert_eq!(s.holding_trade_summaries[&id].settled_profit, "16");
        assert!(s.holding_trade_summaries[&id].profits.is_empty());
        s = delete(&s, &d.request_id);
        assert_eq!(s.holding_trade_summaries[&id].dividend_income, "0");
        assert_eq!(s.holding_trade_summaries[&id].net_cash_flow, "-102");
        s = delete(&s, &b.request_id);
        assert!(s.holding_trades.is_empty());
        assert!(s.holding_trade_summaries.is_empty());
        assert_eq!(s.holdings.len(), 2);
        assert_eq!(s.holdings[0].quantity, Some(0.0));
        assert_eq!(s.holdings[0].cost_price, Some(0.0));
        assert_eq!(s.holdings[0].cost_basis, 0.0);
        assert_eq!(s.holdings[0].market_value, 0.0);
        assert_eq!(s.holdings[0].current_price, Some(23.0));
        let copy = Database::open(&dir.path().join("copy.db")).unwrap();
        copy.import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert!(copy.snapshot().unwrap().holding_trades.is_empty());
        assert_eq!(
            serde_json::to_value(copy.snapshot().unwrap().holdings).unwrap(),
            serde_json::to_value(&s.holdings).unwrap()
        );
        drop(db);
        let reopened = Database::open(&file).unwrap();
        assert!(reopened.snapshot().unwrap().holding_trades.is_empty());
        assert_eq!(
            serde_json::to_value(reopened.snapshot().unwrap().holdings).unwrap(),
            serde_json::to_value(&s.holdings).unwrap()
        );
        // The retained empty security can receive a corrected first execution.
        let s = reopened
            .save_holding_trade(&next(&s, "buy", "2", "10", "2026-01-01"))
            .unwrap();
        assert_eq!(s.holdings[0].quantity, Some(2.0));
        assert_eq!(s.holdings[0].market_value, 46.0);
    }
    #[test]
    fn deletions_reject_stale_revisions_oversells_and_orphan_dividends_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("delete-reject.db")).unwrap();
        let b = first();
        let initial = db.save_holding_trade(&b).unwrap();
        let sale = next(&initial, "sell", "10", "20", "2026-01-02");
        let s = db.save_holding_trade(&sale).unwrap();
        let id = &s.holdings[0].id;
        let baseline = db.export_sync_data().unwrap().content_hash().unwrap();
        let stale = HoldingTradeDeleteInput {
            expected_revision: initial.holding_revisions[id].clone(),
        };
        assert!(matches!(
            db.delete_holding_trade(&sale.request_id, &stale),
            Err(AppError::Conflict(_))
        ));
        let current = HoldingTradeDeleteInput {
            expected_revision: s.holding_revisions[id].clone(),
        };
        let error = db
            .delete_holding_trade(&b.request_id, &current)
            .unwrap_err();
        assert!(
            matches!(error, AppError::Validation(ref message) if message.contains("卖出超过当时持仓"))
        );
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            baseline
        );
        let s = db.delete_holding_trade(&sale.request_id, &current).unwrap();
        let mut d = next(&s, "dividend", "0", "0", "2026-01-03");
        d.gross_amount = "10".into();
        let s = db.save_holding_trade(&d).unwrap();
        let baseline = db.export_sync_data().unwrap().content_hash().unwrap();
        let error = db
            .delete_holding_trade(
                &b.request_id,
                &HoldingTradeDeleteInput {
                    expected_revision: s.holding_revisions[id].clone(),
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, AppError::Validation(ref message) if message.contains("现金分红前须有"))
        );
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            baseline
        );
    }
    #[test]
    fn corrections_preserve_identity_quote_and_retry_without_extra_records() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("edit.db");
        let db = funded_open(&file).unwrap();
        let b = first();
        let s = db.save_holding_trade(&b).unwrap();
        let sale = next(&s, "sell", "10", "20", "2026-01-02");
        let s = db.save_holding_trade(&sale).unwrap();
        let mut h: HoldingInput =
            serde_json::from_value(serde_json::to_value(&s.holdings[0]).unwrap()).unwrap();
        h.current_price = Some(23.0);
        let s = db.update_holding(&s.holdings[0].id, &h).unwrap();
        let mut correction = edit(&s, &sale.request_id);
        correction.quantity = "5".into();
        correction.price = "30".into();
        correction.gross_amount = "150".into();
        correction.fees = "2".into();
        correction.occurred_on = "2026-01-03".into();
        let corrected = db
            .update_holding_trade(&sale.request_id, &correction)
            .unwrap();
        let sum = &corrected.holding_trade_summaries[&s.holdings[0].id];
        assert_eq!(sum.realized_profit, "97");
        assert_eq!(corrected.holdings[0].market_value, 115.0);
        assert_eq!(corrected.holdings[0].current_price, Some(23.0));
        assert_eq!(
            corrected.holdings[0].valuation_date,
            s.holdings[0].valuation_date
        );
        assert_eq!(corrected.holding_trades.len(), 2);
        assert_eq!(
            corrected.holding_trades[1].created_at,
            s.holding_trades[1].created_at
        );
        assert_eq!(
            db.update_holding_trade(&sale.request_id, &correction)
                .unwrap()
                .holding_revisions,
            corrected.holding_revisions
        );
        correction.note = "过期".into();
        assert!(db
            .update_holding_trade(&sale.request_id, &correction)
            .is_err());
        drop(db);
        assert_eq!(
            Database::open(&file)
                .unwrap()
                .snapshot()
                .unwrap()
                .holding_trades,
            corrected.holding_trades
        );
    }
    #[test]
    fn chronological_validation_rejects_oversells_duplicates_and_invalid_fields_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("reject.db")).unwrap();
        let b = first();
        let s = db.save_holding_trade(&b).unwrap();
        let s = db
            .save_holding_trade(&next(&s, "sell", "5", "20", "2026-01-01"))
            .unwrap();
        let baseline = db.export_sync_data().unwrap().content_hash().unwrap();
        let mut bad = edit(&s, &b.request_id);
        bad.sequence = 3;
        assert!(db.update_holding_trade(&b.request_id, &bad).is_err());
        for field in [
            "quantity",
            "price",
            "grossAmount",
            "fees",
            "taxes",
            "account",
            "kind",
            "sequence",
        ] {
            let mut value = serde_json::to_value(edit(&s, &b.request_id)).unwrap();
            value[field] = match field {
                "quantity" => serde_json::json!("1"),
                "price" => serde_json::json!("0"),
                "grossAmount" => serde_json::json!("200"),
                "fees" => serde_json::json!("1e3"),
                "taxes" => serde_json::json!("0.000000001"),
                "account" => serde_json::json!("B"),
                "kind" => serde_json::json!("void"),
                _ => serde_json::json!(2),
            };
            let bad: HoldingTradeUpdateInput = serde_json::from_value(value).unwrap();
            assert!(
                db.update_holding_trade(&b.request_id, &bad).is_err(),
                "{field}"
            );
            assert_eq!(
                db.export_sync_data().unwrap().content_hash().unwrap(),
                baseline
            );
        }
        let mut sale = next(&s, "sell", "6", "20", "2026-01-02");
        assert!(db.save_holding_trade(&sale).is_err());
        sale.quantity = "1".into();
        sale.gross_amount = "20".into();
        sale.external_id = "receipt".into();
        let s = db.save_holding_trade(&sale).unwrap();
        let mut duplicate = next(&s, "buy", "1", "20", "2026-01-03");
        duplicate.external_id = "receipt".into();
        assert!(db.save_holding_trade(&duplicate).is_err());
        let mut invalid = serde_json::to_value(first()).unwrap();
        invalid["voidOf"] = serde_json::json!("old");
        assert!(serde_json::from_value::<HoldingTradeInput>(invalid).is_err());
    }
    #[test]
    fn manual_positions_are_rebuilt_from_history_without_artificial_opening_or_date_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("manual.db")).unwrap();
        let h:HoldingInput=serde_json::from_value(serde_json::json!({"name":"旧快照","symbol":"","assetClass":"股票","currency":"CNY",
            "valuationDate":crate::models::market::today(),"quantity":100,"currentPrice":23,"costPrice":10,"costBasis":1000,"marketValue":2300,"targetPct":0})).unwrap();
        let s = db.add_holding(&h).unwrap();
        let s = db
            .save_holding_trade(&next(&s, "buy", "2", "10", "2026-01-01"))
            .unwrap();
        assert_eq!(s.holding_trades.len(), 1);
        assert_eq!(s.holdings[0].quantity, Some(2.0));
        assert_eq!(s.holdings[0].cost_basis, 20.0);
        assert_eq!(s.holdings[0].market_value, 46.0);
        assert_eq!(s.holdings[0].valuation_date, crate::models::market::today());
        let mut forbidden = h;
        forbidden.quantity = Some(100.0);
        assert!(db.update_holding(&s.holdings[0].id, &forbidden).is_err());
    }
    #[test]
    fn restart_quarantines_invalid_chains_preserves_balances_and_requires_reviewed_repair() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clear.db");
        let db = funded_open(&file).unwrap();
        let first = first();
        let s = db.save_holding_trade(&first).unwrap();
        let bad_id = s.holdings[0].id.clone();
        let mut other = first.clone();
        other.request_id = Uuid::new_v4().to_string();
        other.account = "券商B".into();
        other.new_holding.as_mut().unwrap().name = "合法持仓".into();
        let s = db.save_holding_trade(&other).unwrap();
        let valid_id = s
            .holding_trades
            .iter()
            .find(|t| t.id == other.request_id)
            .unwrap()
            .holding_id
            .clone();
        let valid = s
            .holding_trades
            .iter()
            .find(|t| t.id == other.request_id)
            .unwrap()
            .clone();
        db.conn()
            .unwrap()
            .execute(
                "UPDATE holding_trades SET payload=?2 WHERE id=?1",
                params![first.request_id, "{\"kind\":\"opening\"}"],
            )
            .unwrap();
        drop(db);
        let db = funded_open(&file).unwrap();
        let s = db.snapshot().unwrap();
        assert_eq!(s.holding_trades, vec![valid]);
        assert!(s.cleared_trade_holdings.contains(&bad_id));
        assert_eq!(
            s.holdings.iter().find(|h| h.id == bad_id).unwrap().quantity,
            Some(10.0)
        );
        assert_eq!(
            s.holdings
                .iter()
                .find(|h| h.id == valid_id)
                .unwrap()
                .quantity,
            Some(10.0)
        );
        let mut reenter = first.clone();
        reenter.request_id = Uuid::new_v4().to_string();
        reenter.new_holding = None;
        reenter.holding_id = bad_id.clone();
        reenter.expected_revision = s.holding_revisions.get(&bad_id).cloned();
        assert!(db.save_holding_trade(&reenter).is_err());
        assert!(!s.valuation_status.comparable);
        let raw = db.financial_history(&bad_id).unwrap();
        assert!(raw["quarantine"]["payload"]
            .as_str()
            .unwrap()
            .contains("opening"));
        let copy = Database::open(std::path::Path::new(":memory:")).unwrap();
        copy.import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(copy.financial_history(&bad_id).unwrap(), raw);
        assert!(copy
            .snapshot()
            .unwrap()
            .cleared_trade_holdings
            .contains(&bad_id));
        assert!(!copy.snapshot().unwrap().valuation_status.decision_ready);
        let mut repaired: HoldingTrade=serde_json::from_str(&serde_json::to_string(&serde_json::json!({
            "id":first.request_id,"holdingId":bad_id,"kind":"buy","occurredOn":first.occurred_on,"sequence":1,
            "quantity":"10","price":"10","grossAmount":"100","fees":"2","taxes":"0","currency":"CNY","account":"券商A","externalId":"","note":"","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"
        })).unwrap()).unwrap();
        repaired.id = reenter.request_id.clone();
        let repair = crate::models::TradeRepairInput {
            expected_revision: s.holding_revisions[&bad_id].clone(),
            trades: vec![repaired],
            reason: "按券商成交单核对恢复".into(),
        };
        let cash_before = balance(&s, "CNY");
        db.preview_trade_repair(&bad_id, &repair).unwrap();
        let s = db.repair_trades(&bad_id, &repair).unwrap();
        assert_eq!(balance(&s, "CNY"), cash_before);
        assert_eq!(
            db.financial_history(&bad_id).unwrap()["records"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(!s.cleared_trade_holdings.contains(&bad_id));
        assert_eq!(s.holding_trades.len(), 2);
    }

    #[test]
    fn fees_can_exceed_small_sale_proceeds_and_existing_account_security_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let db = funded_open(&dir.path().join("fees.db")).unwrap();
        let mut b = first();
        b.quantity = "1".into();
        b.price = "0.01".into();
        b.gross_amount = "0.01".into();
        b.fees = "0".into();
        let s = db.save_holding_trade(&b).unwrap();
        let mut duplicate = b.clone();
        duplicate.request_id = Uuid::new_v4().to_string();
        assert!(db.save_holding_trade(&duplicate).is_err());
        let mut sale = next(&s, "sell", "1", "0.01", "2026-01-02");
        sale.fees = "1".into();
        let s = db.save_holding_trade(&sale).unwrap();
        let sum = &s.holding_trade_summaries[&s.holdings[0].id];
        assert_eq!(sum.cash_flows[&sale.request_id], "-0.99");
        assert_eq!(sum.realized_profit, "-1");
    }
    #[test]
    fn quarantine_does_not_rewrite_daily_observations_or_source_records() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rollback.db");
        let db = funded_open(&file).unwrap();
        let b = first();
        let s = db.save_holding_trade(&b).unwrap();
        let id = s.holdings[0].id.clone();
        let mut h: HoldingInput =
            serde_json::from_value(serde_json::to_value(&s.holdings[0]).unwrap()).unwrap();
        h.current_price = Some(23.0);
        h.market_value = 230.0;
        db.update_holding(&id, &h).unwrap();
        {
            let conn = db.conn().unwrap();
            conn.execute(
                "UPDATE holding_trades SET payload=?2 WHERE id=?1",
                params![b.request_id, "{\"kind\":\"opening\"}"],
            )
            .unwrap();
            conn.execute_batch("CREATE TRIGGER reject_cleanup BEFORE INSERT ON daily_entries BEGIN SELECT RAISE(ABORT,'test cleanup failure'); END;").unwrap();
        }
        drop(db);
        let reopened = Database::open(&file).unwrap();
        assert!(!reopened.snapshot().unwrap().valuation_status.comparable);
        let conn = Connection::open(&file).unwrap();
        assert_eq!(
            conn.query_row("SELECT quantity FROM holdings WHERE id=?1", [&id], |r| r
                .get::<_, f64>(0))
                .unwrap(),
            10.0
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM holding_trades", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
