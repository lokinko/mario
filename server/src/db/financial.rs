//! Durable financial operation receipts and append-only evidence.
use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(super) fn initialize(conn: &Connection) -> AppResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS financial_operations (
        id TEXT PRIMARY KEY, request_hash TEXT NOT NULL, payload TEXT NOT NULL, created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS financial_journal (
        id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, kind TEXT NOT NULL,
        before_payload TEXT NOT NULL, after_payload TEXT NOT NULL,
        cash_delta TEXT NOT NULL, reason TEXT NOT NULL, created_at TEXT NOT NULL, operation_id TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS financial_journal_holding ON financial_journal(holding_id,created_at);
    CREATE TABLE IF NOT EXISTS trading_quarantine (
        holding_id TEXT PRIMARY KEY, payload TEXT NOT NULL, reason TEXT NOT NULL, created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS cash_state (
        currency TEXT PRIMARY KEY, holding_id TEXT NOT NULL, amount TEXT NOT NULL,
        baseline_id TEXT NOT NULL, confirmed_on TEXT NOT NULL, confirmed_at TEXT NOT NULL,
        needs_reconciliation INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS trade_settlements (
        trade_id TEXT PRIMARY KEY, currency TEXT NOT NULL, amount TEXT NOT NULL, baseline_id TEXT NOT NULL,
        included INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS corporate_actions (
        id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, payload TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS holding_amounts (
        holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE, payload TEXT NOT NULL
    );")?;
    Ok(())
}

pub(super) fn operation<T: Serialize>(
    kind: &str,
    id: &str,
    input: &T,
) -> AppResult<(String, String)> {
    let bytes = serde_json::to_vec(input)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let key = if kind == "create" || kind == "amount" || kind == "corporate.declare" {
        format!("{kind}:{id}")
    } else {
        format!("{kind}:{id}:{hash}")
    };
    Ok((key, hash))
}

pub(super) fn replay(
    conn: &Connection,
    operation: &(String, String),
) -> AppResult<Option<Snapshot>> {
    let stored = conn
        .query_row(
            "SELECT request_hash,payload FROM financial_operations WHERE id=?1",
            [&operation.0],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    stored
        .map(|(hash, _payload)| {
            if hash != operation.1 {
                return Err(AppError::Conflict(
                    "请求编号已用于不同内容，请核对原操作".into(),
                ));
            }
            // Confirm the receipt but return current state: later edits must never be
            // replaced in the client by the snapshot captured by an earlier operation.
            Database::snapshot_on(conn)
        })
        .transpose()
}

pub(super) fn complete(conn: &Connection, operation: &(String, String)) -> AppResult<Snapshot> {
    let snapshot = Database::snapshot_on(conn)?;
    conn.execute(
        "INSERT INTO financial_operations(id,request_hash,payload,created_at) VALUES (?1,?2,?3,?4)",
        params![
            operation.0,
            operation.1,
            serde_json::to_string(
                &serde_json::json!({"committed":true,"operationId":operation.0})
            )?,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(snapshot)
}

pub(super) fn journal<T: Serialize, U: Serialize>(
    conn: &Connection,
    holding_id: &str,
    kind: &str,
    before: &T,
    after: &U,
    cash_delta: &str,
    metadata: (&str, Option<&str>),
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO financial_journal(id,holding_id,kind,before_payload,after_payload,cash_delta,reason,created_at,operation_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id,holding_id,kind,serde_json::to_string(before)?,serde_json::to_string(after)?,cash_delta,metadata.0,Utc::now().to_rfc3339(),metadata.1.unwrap_or(&id)])?;
    Ok(())
}

impl Database {
    pub fn preview_trade_repair(
        &self,
        id: &str,
        input: &crate::models::TradeRepairInput,
    ) -> AppResult<crate::models::HoldingTradeSummary> {
        let conn = self.conn()?;
        validate_repair(&conn, id, input)
    }

    pub fn repair_trades(
        &self,
        id: &str,
        input: &crate::models::TradeRepairInput,
    ) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let operation = operation("repair", id, input)?;
        if let Some(snapshot) = replay(&tx, &operation)? {
            return Ok(snapshot);
        }
        let summary = validate_repair(&tx, id, input)?;
        let before = super::daily::prepare(&tx)?;
        let raw: String = tx.query_row(
            "SELECT payload FROM trading_quarantine WHERE holding_id=?1",
            [id],
            |r| r.get(0),
        )?;
        // This is an explicitly reviewed replacement; the original rows remain permanently in the journal.
        journal(
            &tx,
            id,
            "trade.repair",
            &raw,
            &input.trades,
            "0",
            (&input.reason, Some(&operation.0)),
        )?;
        tx.execute("DELETE FROM holding_trades WHERE holding_id=?1", [id])?;
        for trade in &input.trades {
            tx.execute(
                "INSERT INTO holding_trades(id,holding_id,payload,created_at) VALUES (?1,?2,?3,?4)",
                params![
                    trade.id,
                    id,
                    serde_json::to_string(trade)?,
                    trade.created_at
                ],
            )?;
        }
        tx.execute("DELETE FROM trading_quarantine WHERE holding_id=?1", [id])?;
        let holding = before.holdings.iter().find(|h| h.id == id).unwrap();
        super::trades::rebuild(&tx, holding, &summary)?;
        // Repair does not replay already reflected cash flows; reconciliation is required separately.
        super::cash::initialize_balances(&tx)?;
        tx.execute(
            "UPDATE cash_state SET needs_reconciliation=1 WHERE currency=?1",
            [&holding.currency],
        )?;
        super::daily::finish(&tx, &before, None)?;
        let snapshot = complete(&tx, &operation)?;
        tx.commit()?;
        Ok(snapshot)
    }

    /// Called under the authenticated account lock, before rejecting a stale global revision.
    pub fn replay_financial_request(
        &self,
        method: &str,
        path: &str,
        input: &serde_json::Value,
    ) -> AppResult<Option<Snapshot>> {
        let operation = match (method, path.strip_prefix("/api/")) {
            ("POST", Some("holding-trades")) => {
                let input: crate::models::HoldingTradeInput =
                    serde_json::from_value(input.clone())?;
                operation("create", &input.request_id, &input)?
            }
            (method, Some(path)) if path.starts_with("holding-trades/") => {
                let id = &path[15..];
                match method {
                    "PUT" => operation(
                        "edit",
                        id,
                        &serde_json::from_value::<crate::models::HoldingTradeUpdateInput>(
                            input.clone(),
                        )?,
                    )?,
                    "DELETE" => operation(
                        "delete",
                        id,
                        &serde_json::from_value::<crate::models::HoldingTradeDeleteInput>(
                            input.clone(),
                        )?,
                    )?,
                    _ => return Ok(None),
                }
            }
            ("PUT", Some(path)) if path.starts_with("holdings/") && path.ends_with("/amount") => {
                let input: crate::models::daily::AmountInput =
                    serde_json::from_value(input.clone())?;
                operation("amount", &input.request_id, &(path, &input))?
            }
            ("POST", Some(path))
                if path.starts_with("holdings/") && path.ends_with("/trade-repair") =>
            {
                let id = path
                    .trim_start_matches("holdings/")
                    .trim_end_matches("/trade-repair");
                operation(
                    "repair",
                    id,
                    &serde_json::from_value::<crate::models::TradeRepairInput>(input.clone())?,
                )?
            }
            ("POST", Some(path))
                if path.starts_with("holdings/") && path.ends_with("/corporate-actions") =>
            {
                let id = path
                    .trim_start_matches("holdings/")
                    .trim_end_matches("/corporate-actions");
                let input: crate::models::ManualCorporateActionInput =
                    serde_json::from_value(input.clone())?;
                operation("corporate.declare", &input.request_id, &(id, &input))?
            }
            ("POST", Some(path))
                if path.starts_with("corporate-actions/")
                    && ["/confirm", "/reject", "/revert"]
                        .iter()
                        .any(|s| path.ends_with(s)) =>
            {
                let (id, mode) = path
                    .trim_start_matches("corporate-actions/")
                    .rsplit_once('/')
                    .unwrap();
                let kind = if mode == "confirm" {
                    "corporate".to_owned()
                } else {
                    format!("corporate.{mode}")
                };
                operation(
                    &kind,
                    id,
                    &serde_json::from_value::<crate::models::ConfirmCorporateActionInput>(
                        input.clone(),
                    )?,
                )?
            }
            _ => return Ok(None),
        };
        replay(&*self.conn()?, &operation)
    }

    pub fn financial_history(&self, holding_id: &str) -> AppResult<serde_json::Value> {
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT id,kind,before_payload,after_payload,cash_delta,reason,created_at,operation_id FROM financial_journal WHERE holding_id=?1 ORDER BY created_at,id")?;
        let records = statement.query_map([holding_id], |r| Ok(serde_json::json!({
            "id":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,
            "before":r.get::<_,String>(2)?,"after":r.get::<_,String>(3)?,
            "cashDelta":r.get::<_,String>(4)?,"reason":r.get::<_,String>(5)?,"createdAt":r.get::<_,String>(6)?,"operationId":r.get::<_,String>(7)?
        })))?.collect::<Result<Vec<_>,_>>()?;
        let quarantine = conn.query_row("SELECT payload,reason,created_at FROM trading_quarantine WHERE holding_id=?1", [holding_id],
            |r| Ok(serde_json::json!({"payload":r.get::<_,String>(0)?,"reason":r.get::<_,String>(1)?,"createdAt":r.get::<_,String>(2)?}))).optional()?;
        Ok(serde_json::json!({"records":records,"quarantine":quarantine}))
    }
}

fn validate_repair(
    conn: &Connection,
    id: &str,
    input: &crate::models::TradeRepairInput,
) -> AppResult<crate::models::HoldingTradeSummary> {
    let snapshot = Database::snapshot_on(conn)?;
    if !snapshot.cleared_trade_holdings.iter().any(|h| h == id) {
        return Err(AppError::Validation("该持仓没有隔离记录".into()));
    }
    if snapshot.holding_revisions.get(id) != Some(&input.expected_revision) {
        return Err(AppError::Conflict("持仓已更新，请重新预览修复".into()));
    }
    if input.reason.trim().is_empty()
        || input.reason.chars().count() > 2000
        || input.trades.is_empty()
    {
        return Err(AppError::Validation(
            "修复需保留至少一笔有效交易并填写原因".into(),
        ));
    }
    let holding = snapshot
        .holdings
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| AppError::Validation("找不到对应持仓".into()))?;
    if input
        .trades
        .iter()
        .any(|t| t.holding_id != id || t.currency != holding.currency)
    {
        return Err(AppError::Validation(
            "修复交易必须属于当前持仓与币种".into(),
        ));
    }
    for t in &input.trades {
        if snapshot.holding_trades.iter().any(|old| {
            old.id == t.id
                || (!t.external_id.is_empty()
                    && old.account == t.account
                    && old.external_id == t.external_id)
        }) {
            return Err(AppError::Conflict("修复记录与其他持仓交易编号冲突".into()));
        }
    }
    super::corporate::position(conn, id, &input.trades)
}
