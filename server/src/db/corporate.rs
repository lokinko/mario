//! Company actions are reviewed facts, separate from immutable broker executions.
use crate::models::{
    market::quotes::SecurityPriceQuote,
    portfolio::{ConfirmCorporateActionInput, CorporateActionRecord},
};
use crate::storage::{params, OptionalRow};
use rust_decimal::prelude::ToPrimitive;
use sha2::{Digest, Sha256};
use {
    crate::{db::Database, models::portfolio::Snapshot, storage::Connection, AppError, AppResult},
    chrono::Utc,
    std::collections::HashSet,
    uuid::Uuid,
};

pub(super) fn load(conn: &Connection) -> AppResult<Vec<CorporateActionRecord>> {
    let mut statement = conn.prepare("SELECT payload FROM corporate_actions ORDER BY id")?;
    let rows = statement
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
}

fn load_for_holding(conn: &Connection, holding_id: &str) -> AppResult<Vec<CorporateActionRecord>> {
    let mut statement =
        conn.prepare("SELECT payload FROM corporate_actions WHERE holding_id=$1 ORDER BY id")?;
    let rows = statement
        .query_map([holding_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
}

fn check_dismissed_and_pending(
    actions: &[CorporateActionRecord],
    holding_id: &str,
    quote: &SecurityPriceQuote,
) -> AppResult<bool> {
    if actions.iter().any(|a| {
        a.holding_id == holding_id
            && a.dismissed
            && !a.user_declared
            && (a.symbol.is_empty() || a.symbol == quote.symbol)
            && quote.splits.contains(&a.split)
    }) {
        return Err(AppError::Validation(
            "当前行情来源包含已否决的公司行动，请更正证券代码或按实际价格人工核对".into(),
        ));
    }
    Ok(actions
        .iter()
        .any(|a| a.holding_id == holding_id && !a.applied && !a.dismissed))
}

pub(super) fn position_with_loaded(
    option_contract: Option<&crate::models::options::OptionContract>,
    corporate_actions: &[CorporateActionRecord],
    id: &str,
    trades: &[crate::models::portfolio::HoldingTrade],
) -> AppResult<crate::models::portfolio::HoldingTradeSummary> {
    if let Some(contract) = option_contract {
        for trade in trades {
            super::trades::validate(trade, Some(contract))?;
        }
        return crate::models::options::position(contract, trades);
    }
    let actions = corporate_actions
        .iter()
        .filter(|a| a.holding_id == id && a.applied)
        .map(|a| a.split.clone())
        .collect::<Vec<_>>();
    super::trades::position_with_splits(trades, &actions)
}

pub(super) fn position(
    conn: &Connection,
    id: &str,
    trades: &[crate::models::portfolio::HoldingTrade],
) -> AppResult<crate::models::portfolio::HoldingTradeSummary> {
    let contract = super::options::get(conn, id)?;
    if contract.is_some() {
        return position_with_loaded(contract.as_ref(), &[], id, trades);
    }
    let actions = load(conn)?;
    position_with_loaded(None, &actions, id, trades)
}

pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    let mut statement = conn.prepare("SELECT id,holding_id,payload FROM corporate_actions")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut applied = HashSet::new();
    for (id, holding_id, payload) in rows {
        let action: CorporateActionRecord = serde_json::from_str(&payload)?;
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM holdings WHERE id=$1)",
            [&holding_id],
            |r| r.get::<_, bool>(0),
        )?;
        if id != action.id
            || holding_id != action.holding_id
            || !exists
            || (action.applied && !applied.insert((holding_id, action.split.effective_on.clone())))
        {
            return Err(AppError::Validation("公司行动关联或同日版本无效".into()));
        }
        super::trades::split_ratio(&action.split)?;
        if action.applied && action.dismissed {
            return Err(AppError::Validation("公司行动状态无效".into()));
        }
        if action.user_declared {
            if !action.source_url.is_empty() {
                return Err(AppError::Validation("人工公司行动不得伪造来源".into()));
            }
            continue;
        }
        let url = reqwest::Url::parse(&action.source_url)
            .map_err(|_| AppError::Validation("公司行动来源无效".into()))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(
                url.host_str(),
                Some("query1.finance.yahoo.com" | "query2.finance.yahoo.com")
            )
        {
            return Err(AppError::Validation(
                "公司行动来源不是受支持的行情来源".into(),
            ));
        }
    }
    Ok(())
}

impl Database {
    pub fn declare_corporate_action(
        &self,
        id: &str,
        input: &crate::models::portfolio::ManualCorporateActionInput,
    ) -> AppResult<Snapshot> {
        Uuid::parse_str(&input.request_id)
            .map_err(|_| AppError::Validation("公司行动请求编号无效".into()))?;
        super::trades::split_ratio(&input.split)?;
        if input.reason.trim().is_empty() || input.reason.len() > 2000 {
            return Err(AppError::Validation("请填写券商通知或核对依据".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let operation =
            super::financial::operation("corporate.declare", &input.request_id, &(id, input))?;
        if let Some(snapshot) = super::financial::replay(&tx, &operation)? {
            return Ok(snapshot);
        }
        let before = super::daily::prepare(&tx)?;
        if before.option_contracts.contains_key(id) {
            return Err(AppError::Validation(
                "期权合约调整须按券商通知核对，不能套用股票拆股".into(),
            ));
        }
        let holding = before
            .holdings
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| AppError::NotFound("持仓不存在".into()))?;
        if before.holding_revisions.get(id) != Some(&input.expected_revision) {
            return Err(AppError::Conflict("持仓已变化，请重新核对".into()));
        }
        if !before
            .holding_trades
            .iter()
            .any(|t| t.holding_id == id && t.occurred_on < input.split.effective_on)
            || before.cleared_trade_holdings.contains(&id.to_string())
        {
            return Err(AppError::Validation(
                "公司行动需要生效日前的有效成交链".into(),
            ));
        }
        if load(&tx)?.iter().any(|a| {
            a.holding_id == id && !a.dismissed && a.split.effective_on == input.split.effective_on
        }) {
            return Err(AppError::Conflict(
                "同日已有公司行动，请核对已有记录".into(),
            ));
        }
        let action = CorporateActionRecord {
            id: input.request_id.clone(),
            holding_id: id.into(),
            split: input.split.clone(),
            applied: false,
            dismissed: false,
            user_declared: true,
            symbol: holding.symbol.clone(),
            source_url: String::new(),
        };
        tx.execute(
            "INSERT INTO corporate_actions(id,holding_id,payload) VALUES ($1,$2,$3)",
            params![action.id, id, serde_json::to_string(&action)?],
        )?;
        super::financial::journal(
            &tx,
            id,
            "corporate.declare",
            &Option::<CorporateActionRecord>::None,
            &action,
            "0",
            (&input.reason, Some(&operation.0)),
        )?;
        tx.execute(
            "UPDATE holdings SET updated_at=$2 WHERE id=$1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }

    /// Fast in-memory check using an already-loaded snapshot; falls back to
    /// `register_corporate_actions` only when a quote carries an unrecorded split.
    pub fn check_or_register_corporate_actions(
        &self,
        snapshot: &Snapshot,
        id: &str,
        quote: &SecurityPriceQuote,
    ) -> AppResult<bool> {
        if snapshot.option_contracts.contains_key(id) {
            return Err(AppError::Validation(
                "期权不能套用股票行情或拆股调整".into(),
            ));
        }
        let holding = snapshot
            .holdings
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| AppError::Validation("持仓不存在".into()))?;
        crate::db::validation::validate_holding_valuation_evidence(
            holding.quantity.filter(|q| *q > 0.0).unwrap_or(1.0),
            quote,
        )?;
        let first = snapshot
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == id)
            .map(|t| t.occurred_on.as_str())
            .min();
        let from = first.unwrap_or(&holding.valuation_date);
        let today = crate::models::market::today();
        for split in quote
            .splits
            .iter()
            .filter(|s| s.effective_on.as_str() > from && s.effective_on <= today)
        {
            let ratio = super::trades::split_ratio(split)?;
            let exists = snapshot.corporate_actions.iter().any(|a| {
                a.holding_id == holding.id
                    && !a.dismissed
                    && a.split.effective_on == split.effective_on
                    && super::trades::split_ratio(&a.split).ok() == Some(ratio)
            });
            if !exists {
                return self.register_corporate_actions(id, quote);
            }
        }
        check_dismissed_and_pending(&snapshot.corporate_actions, &holding.id, quote)
    }

    pub fn register_corporate_actions(
        &self,
        id: &str,
        quote: &SecurityPriceQuote,
    ) -> AppResult<bool> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        if super::options::get(&tx, id)?.is_some() {
            return Err(AppError::Validation(
                "期权不能套用股票行情或拆股调整".into(),
            ));
        }
        let (quantity, valuation_date): (Option<f64>, String) = tx
            .query_row(
                "SELECT quantity, valuation_date FROM holdings WHERE id=$1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("持仓不存在".into()))?;
        crate::db::validation::validate_holding_valuation_evidence(
            quantity.filter(|q| *q > 0.0).unwrap_or(1.0),
            quote,
        )?;
        let mut trade_stmt = tx.prepare(
            "SELECT payload FROM holding_trades
             WHERE holding_id=$1
               AND holding_id NOT IN (SELECT holding_id FROM trading_quarantine)",
        )?;
        let trade_rows = trade_stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(trade_stmt);
        let mut first: Option<String> = None;
        for raw in trade_rows {
            let trade: crate::models::portfolio::HoldingTrade = serde_json::from_str(&raw)?;
            if first
                .as_deref()
                .is_none_or(|current| trade.occurred_on.as_str() < current)
            {
                first = Some(trade.occurred_on);
            }
        }
        let from = first.as_deref().unwrap_or(valuation_date.as_str());
        let today = crate::models::market::today();
        let mut actions = load_for_holding(&tx, id)?;
        for split in quote
            .splits
            .iter()
            .filter(|s| s.effective_on.as_str() > from && s.effective_on <= today)
        {
            let ratio = super::trades::split_ratio(split)?;
            if actions.iter().any(|a| {
                a.holding_id == id
                    && !a.dismissed
                    && a.split.effective_on == split.effective_on
                    && super::trades::split_ratio(&a.split).ok() == Some(ratio)
            }) {
                continue;
            }
            let action_id = format!(
                "{:x}",
                Sha256::digest(format!(
                    "{id}:{}:{}:{}:{}",
                    quote.symbol, split.effective_on, split.numerator, split.denominator
                ))
            );
            let action = CorporateActionRecord {
                id: action_id.clone(),
                holding_id: id.to_owned(),
                split: split.clone(),
                applied: false,
                dismissed: false,
                user_declared: false,
                symbol: quote.symbol.clone(),
                source_url: quote.source_url.clone(),
            };
            if tx.execute("INSERT INTO corporate_actions(id,holding_id,payload) VALUES ($1,$2,$3) ON CONFLICT(id) DO NOTHING",params![action_id,id,serde_json::to_string(&action)?])? > 0 {
                super::financial::journal(&tx,id,"corporate.detected",&Option::<CorporateActionRecord>::None,&action,"0",("行情发现公司行动，等待核对",None))?;
                tx.execute("UPDATE holdings SET updated_at=$2 WHERE id=$1",params![id,Utc::now().to_rfc3339()])?;
                actions.push(action);
            }
        }
        let pending = check_dismissed_and_pending(&actions, id, quote)?;
        tx.commit()?;
        Ok(pending)
    }

    #[cfg(test)]
    pub fn confirm_corporate_action(
        &self,
        id: &str,
        input: &ConfirmCorporateActionInput,
    ) -> AppResult<Snapshot> {
        self.review_corporate_action(id, input, "confirm")
    }

    pub fn review_corporate_action(
        &self,
        id: &str,
        input: &ConfirmCorporateActionInput,
        mode: &str,
    ) -> AppResult<Snapshot> {
        if !["confirm", "reject", "revert"].contains(&mode) {
            return Err(AppError::Validation("公司行动操作无效".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let kind = if mode == "confirm" {
            "corporate".to_owned()
        } else {
            format!("corporate.{mode}")
        };
        let operation = super::financial::operation(&kind, id, input)?;
        if let Some(snapshot) = super::financial::replay(&tx, &operation)? {
            return Ok(snapshot);
        }
        let before = super::daily::prepare(&tx)?;
        let mut action = load(&tx)?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| AppError::NotFound("公司行动不存在".into()))?;
        if (mode == "revert") != action.applied || action.dismissed {
            return Err(AppError::Conflict("公司行动状态已变化，请重新核对".into()));
        }
        let original_action = action.clone();
        if input.reason.trim().is_empty() || input.reason.len() > 2000 {
            return Err(AppError::Validation("请填写核对依据".into()));
        }
        if before.holding_revisions.get(&action.holding_id) != Some(&input.expected_revision) {
            return Err(AppError::Conflict("持仓已更新，请重新核对".into()));
        }
        if before.cleared_trade_holdings.contains(&action.holding_id) {
            return Err(AppError::Validation("请先修复隔离交易链".into()));
        }
        if mode == "confirm"
            && load(&tx)?.iter().any(|a| {
                a.holding_id == action.holding_id
                    && a.applied
                    && a.split.effective_on == action.split.effective_on
            })
        {
            return Err(AppError::Conflict(
                "同日公司行动存在不同版本，请核对来源后修复".into(),
            ));
        }
        action.applied = mode == "confirm";
        action.dismissed = mode != "confirm";
        tx.execute(
            "UPDATE corporate_actions SET payload=$2 WHERE id=$1",
            params![id, serde_json::to_string(&action)?],
        )?;
        let mut holding = before
            .holdings
            .iter()
            .find(|h| h.id == action.holding_id)
            .unwrap()
            .clone();
        let adjusted_split = if mode == "revert" {
            crate::models::market::quotes::SplitEvent {
                numerator: action.split.denominator.clone(),
                denominator: action.split.numerator.clone(),
                effective_on: action.split.effective_on.clone(),
            }
        } else {
            action.split.clone()
        };
        if mode != "reject" {
            let factor = super::trades::split_ratio(&adjusted_split)?;
            holding.current_price = holding.current_price.map(|p| p / factor.to_f64().unwrap());
            super::amounts::split_reference(&tx, &holding.id, &adjusted_split)?;
            let trades = before
                .holding_trades
                .iter()
                .filter(|t| t.holding_id == holding.id)
                .cloned()
                .collect::<Vec<_>>();
            if trades.is_empty() {
                return Err(AppError::Validation(
                    "手工持仓请直接按实际数量核对；公司行动确认需要真实成交链".into(),
                ));
            }
            let summary = position(&tx, &holding.id, &trades)?;
            super::trades::rebuild(&tx, &holding, &summary)?;
            tx.execute(
                "UPDATE holdings SET valuation_date='' WHERE id=$1",
                [&holding.id],
            )?;
        }
        super::financial::journal(
            &tx,
            &holding.id,
            &format!("corporate.{mode}"),
            &original_action,
            &action,
            "0",
            (&input.reason, Some(&operation.0)),
        )?;
        tx.execute(
            "UPDATE holdings SET updated_at=$2 WHERE id=$1",
            params![holding.id, Utc::now().to_rfc3339()],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }
}
