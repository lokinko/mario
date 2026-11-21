//! Per-account deposit balances tracked independently from securities executions.
use crate::storage::params;
use crate::storage::OptionalRow;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use {
    crate::{models::portfolio::HoldingInput, storage::Connection, AppError, AppResult},
    chrono::Utc,
    uuid::Uuid,
};

#[cfg(test)]
use crate::db::Database;
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
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,valuation_date,created_at,updated_at) VALUES ($1,'',$2,'现金',10000,0,0,$3,'2026-01-01','9999-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            params![id, format!("{currency} 存款"), currency],
        )
        .unwrap();
        initialize_balances(&conn).unwrap();
        conn.execute(
            "UPDATE cash_state SET baseline_id='00000000-0000-0000-0000-000000000001' WHERE holding_id=$1",
            [&id],
        )
        .unwrap();
    }
}

fn decimal(value: f64) -> AppResult<Decimal> {
    Decimal::from_str_exact(&value.to_string())
        .map_err(|_| AppError::Validation("存款余额无效".into()))
}

pub(super) fn states(
    conn: &Connection,
) -> AppResult<Vec<crate::models::portfolio::CashBalanceState>> {
    let mut statement = conn.prepare(
        "SELECT currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation FROM cash_state ORDER BY currency, holding_id",
    )?;
    Ok(statement
        .query_map([], |r| {
            Ok(crate::models::portfolio::CashBalanceState {
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

/// Ensure every deposit account has an exact balance state without merging accounts.
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
        conn.execute(
            "INSERT INTO cash_state(currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(holding_id) DO NOTHING",
            params![
                currency,
                id,
                decimal(amount)?.normalize().to_string(),
                Uuid::new_v4().to_string(),
                day,
                Utc::now().to_rfc3339(),
                i64::from(invalid_date)
            ],
        )?;
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
    conn.execute(
        "INSERT INTO cash_state(currency,holding_id,amount,baseline_id,confirmed_on,confirmed_at,needs_reconciliation) VALUES ($1,$2,$3,$4,$5,$6,0) ON CONFLICT(holding_id) DO UPDATE SET currency=excluded.currency,amount=excluded.amount,baseline_id=excluded.baseline_id,confirmed_on=excluded.confirmed_on,confirmed_at=excluded.confirmed_at,needs_reconciliation=0",
        params![
            currency,
            id,
            amount.normalize().to_string(),
            Uuid::new_v4().to_string(),
            day,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
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
    super::validation::validate_currency(&input.currency)?;
    input.name = input.name.trim().to_owned();
    if input.name.is_empty() || input.name.chars().count() > 200 {
        return Err(AppError::Validation(
            "请输入 200 字以内的存款账户名称".into(),
        ));
    }
    input.symbol.clear();
    input.cost_basis = 0.0;
    input.target_pct = 0.0;
    input.quantity = None;
    input.cost_price = None;
    input.current_price = None;
    Ok(())
}

pub(super) fn guard(conn: &Connection, id: Option<&str>, input: &HoldingInput) -> AppResult<()> {
    if let Some(id) = id {
        let old = conn
            .query_row("SELECT asset_class FROM holdings WHERE id=$1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        if let Some(old_class) = old {
            if (old_class == "现金") != (input.asset_class == "现金") {
                return Err(AppError::Validation("不能将存款与证券互相转换".into()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deposit(name: &str, currency: &str, amount: &str) -> HoldingInput {
        serde_json::from_value(serde_json::json!({
            "symbol": "ignored",
            "name": name,
            "assetClass": "现金",
            "marketValue": 0,
            "cashAmount": amount,
            "costBasis": 0,
            "targetPct": 0,
            "currency": currency,
            "valuationDate": "2026-01-01"
        }))
        .unwrap()
    }

    #[test]
    fn upgrade_and_legacy_import_preserve_multiple_deposit_accounts_without_merging() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        fund(&db, "CNY");
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO holdings(id,symbol,name,asset_class,market_value,cost_basis,target_pct,currency,valuation_date,created_at,updated_at) VALUES ('legacy-cash','','工资卡','现金',123.45,0,0,'CNY','2026-01-01','9999-01-02T00:00:00Z','2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        let mut legacy = db.export_sync_data().unwrap();
        crate::db::tests::set_legacy_version(&mut legacy, 19);
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
        assert_eq!(s.holdings.len(), 2);
        assert_eq!(s.cash_balances.len(), 2);
        assert_eq!(s.total_value, 10123.45);
        assert!(s.holdings.iter().any(|h| h.name == "CNY 存款"));
        assert!(s.holdings.iter().any(|h| h.name == "工资卡"));
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
        assert_eq!(copy.snapshot().unwrap().holdings.len(), 2);
        assert_eq!(copy.snapshot().unwrap().cash_balances.len(), 2);
        copy.import_sync_data(&legacy).unwrap();
        assert_eq!(copy.snapshot().unwrap().total_value, 10123.45);
    }

    #[test]
    fn cash_supports_multiple_named_accounts_currency_edits_and_deletion() {
        let db = Database::test();
        let s = db
            .add_holding(&deposit("微信零钱", "CNY", "1234.56"))
            .unwrap();
        let wechat_id = s.holdings[0].id.clone();
        assert_eq!(s.holdings[0].name, "微信零钱");
        assert!(s.holdings[0].symbol.is_empty());

        db.add_holding(&deposit("招商银行", "CNY", "50000"))
            .unwrap();
        db.add_holding(&deposit("公积金账户", "CNY", "80000.44"))
            .unwrap();
        let s = db.add_holding(&deposit("欧元储蓄", "EUR", "0")).unwrap();
        assert_eq!(s.holdings.len(), 4);
        assert_eq!(s.cash_balances.len(), 4);
        assert_eq!(
            s.accounting.as_ref().unwrap().cash.value.value.as_deref(),
            Some("131235")
        );

        // Rename and switch currency on an existing deposit account.
        let mut renamed = deposit("招商银行港元", "HKD", "200");
        renamed.fx_rate_to_base = Some(0.9);
        let cmb_id = s
            .holdings
            .iter()
            .find(|h| h.name == "招商银行")
            .unwrap()
            .id
            .clone();
        let s = db.update_holding(&cmb_id, &renamed).unwrap();
        let updated = s.holdings.iter().find(|h| h.id == cmb_id).unwrap();
        assert_eq!(updated.name, "招商银行港元");
        assert_eq!(updated.currency, "HKD");
        assert_eq!(
            s.cash_balances
                .iter()
                .find(|b| b.holding_id == cmb_id)
                .unwrap()
                .currency,
            "HKD"
        );

        // Reclassification between cash and securities remains forbidden.
        let mut reclassified = deposit("微信零钱", "CNY", "1234.56");
        reclassified.asset_class = "其他".into();
        assert!(db.update_holding(&wechat_id, &reclassified).is_err());

        // Blank names and invalid currency codes are rejected.
        assert!(db.add_holding(&deposit("   ", "CNY", "10")).is_err());
        assert!(db.add_holding(&deposit("非法币种", "RMB1", "10")).is_err());

        // Individual deposit accounts can be deleted cleanly.
        let revision = s.holding_revisions[&wechat_id].clone();
        let s = db
            .delete_holding(
                &wechat_id,
                &crate::models::portfolio::HoldingDeleteInput {
                    expected_revision: revision,
                },
            )
            .unwrap();
        assert_eq!(s.holdings.len(), 3);
        assert_eq!(s.cash_balances.len(), 3);
        assert!(!s.cash_balances.iter().any(|b| b.holding_id == wechat_id));

        // Backup and restore preserve every deposit account and its exact balance.
        let copy = Database::test();
        copy.import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(copy.snapshot().unwrap().holdings.len(), 3);
        assert_eq!(copy.snapshot().unwrap().cash_balances.len(), 3);
    }

    #[test]
    fn migrating_legacy_currency_primary_key_allows_multiple_accounts_per_currency() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        fund(&db, "CNY");
        {
            let conn = db.conn().unwrap();
            conn.execute_batch(
                "ALTER TABLE cash_state DROP CONSTRAINT cash_state_pkey;
                 ALTER TABLE cash_state ADD PRIMARY KEY (currency);",
            )
            .unwrap();
        }
        drop(db);
        let migrated = account.reopen_from_old_release();
        let s = migrated
            .add_holding(&deposit("微信零钱", "CNY", "500"))
            .unwrap();
        assert_eq!(s.holdings.len(), 2);
        assert_eq!(s.cash_balances.len(), 2);
        assert_eq!(s.total_value, 10500.0);
    }
}
