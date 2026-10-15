//! Company actions are reviewed facts, separate from immutable broker executions.
use super::*;
use crate::models::{ConfirmCorporateActionInput, CorporateActionRecord, SecurityPriceQuote};
use rust_decimal::prelude::ToPrimitive;
use sha2::{Digest, Sha256};

pub(super) fn load(conn: &Connection) -> AppResult<Vec<CorporateActionRecord>> {
    let mut statement = conn.prepare("SELECT payload FROM corporate_actions ORDER BY id")?;
    let rows = statement
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
}

pub(super) fn position(
    conn: &Connection,
    id: &str,
    trades: &[crate::models::HoldingTrade],
) -> AppResult<crate::models::HoldingTradeSummary> {
    let actions = load(conn)?
        .into_iter()
        .filter(|a| a.holding_id == id && a.applied)
        .map(|a| a.split)
        .collect::<Vec<_>>();
    super::trades::position_with_splits(trades, &actions)
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
            "SELECT EXISTS(SELECT 1 FROM holdings WHERE id=?1)",
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
        input: &crate::models::ManualCorporateActionInput,
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
            "INSERT INTO corporate_actions(id,holding_id,payload) VALUES (?1,?2,?3)",
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
            "UPDATE holdings SET updated_at=?2 WHERE id=?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }

    pub fn register_corporate_actions(
        &self,
        id: &str,
        quote: &SecurityPriceQuote,
    ) -> AppResult<bool> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let snapshot = Self::snapshot_on(&tx)?;
        let holding = snapshot
            .holdings
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| AppError::Validation("持仓不存在".into()))?;
        super::validate_holding_valuation_evidence(
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
        for split in quote.splits.iter().filter(|s| {
            s.effective_on.as_str() > from && s.effective_on <= crate::models::market::today()
        }) {
            let _ = super::trades::split_ratio(split)?;
            if load(&tx)?.iter().any(|a| {
                a.holding_id == holding.id
                    && !a.dismissed
                    && a.split.effective_on == split.effective_on
                    && super::trades::split_ratio(&a.split).ok()
                        == super::trades::split_ratio(split).ok()
            }) {
                continue;
            }
            let id = format!(
                "{:x}",
                Sha256::digest(format!(
                    "{id}:{}:{}:{}:{}",
                    quote.symbol, split.effective_on, split.numerator, split.denominator
                ))
            );
            let action = CorporateActionRecord {
                id: id.clone(),
                holding_id: holding.id.clone(),
                split: split.clone(),
                applied: false,
                dismissed: false,
                user_declared: false,
                symbol: quote.symbol.clone(),
                source_url: quote.source_url.clone(),
            };
            if tx.execute("INSERT INTO corporate_actions(id,holding_id,payload) VALUES (?1,?2,?3) ON CONFLICT(id) DO NOTHING",params![id,holding.id,serde_json::to_string(&action)?])? > 0 {
                super::financial::journal(&tx,&holding.id,"corporate.detected",&Option::<CorporateActionRecord>::None,&action,"0",("行情发现公司行动，等待核对",None))?;
                tx.execute("UPDATE holdings SET updated_at=?2 WHERE id=?1",params![holding.id,Utc::now().to_rfc3339()])?;
            }
        }
        if load(&tx)?.iter().any(|a| {
            a.holding_id == holding.id
                && a.dismissed
                && !a.user_declared
                && (a.symbol.is_empty() || a.symbol == quote.symbol)
                && quote.splits.contains(&a.split)
        }) {
            return Err(AppError::Validation(
                "当前行情来源包含已否决的公司行动，请更正证券代码或按实际价格人工核对".into(),
            ));
        }
        let pending = load(&tx)?
            .iter()
            .any(|a| a.holding_id == holding.id && !a.applied && !a.dismissed);
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
            "UPDATE corporate_actions SET payload=?2 WHERE id=?1",
            params![id, serde_json::to_string(&action)?],
        )?;
        let mut holding = before
            .holdings
            .iter()
            .find(|h| h.id == action.holding_id)
            .unwrap()
            .clone();
        let adjusted_split = if mode == "revert" {
            crate::models::SplitEvent {
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
                "UPDATE holdings SET valuation_date='' WHERE id=?1",
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
            "UPDATE holdings SET updated_at=?2 WHERE id=?1",
            params![holding.id, Utc::now().to_rfc3339()],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = super::financial::complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }
}
