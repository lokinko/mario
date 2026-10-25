use super::*;
use crate::models::{
    ConfirmCorporateActionInput, HoldingTradeDeleteInput, HoldingTradeInput,
    HoldingTradeUpdateInput, SecurityPriceQuote,
};

fn buy(symbol: &str, mode: &str, date: &str) -> HoldingTradeInput {
    serde_json::from_value(serde_json::json!({"expectedCashBaseline":"00000000-0000-0000-0000-000000000001","requestId":Uuid::new_v4().to_string(),"newHolding":{"name":symbol,"symbol":symbol,"assetClass":"股票","currency":"CNY"},"kind":"buy","occurredOn":date,"sequence":1,"quantity":"10","price":"10","grossAmount":"100","fees":"0","taxes":"0","account":"测试券商","settlementMode":mode})).unwrap()
}
fn funded() -> Database {
    let db = Database::test();
    cash::fund(&db, "CNY");
    db
}
fn confirm(db: &Database, amount: &str) -> Snapshot {
    let s = db.snapshot().unwrap();
    let cash = s.holdings.iter().find(|h| h.asset_class == "现金").unwrap();
    db.update_holding_amount(
        &cash.id,
        &crate::models::daily::AmountInput {
            amount: amount.into(),
            request_id: Uuid::new_v4().to_string(),
            expected_revision: s.holding_revisions[&cash.id].clone(),
        },
    )
    .unwrap()
}

fn sell_all(db: &Database, snapshot: &Snapshot, id: &str, price: &str) -> Snapshot {
    let mut input = buy("", "settle", "2026-01-02");
    input.new_holding = None;
    input.holding_id = id.into();
    input.expected_revision = Some(snapshot.holding_revisions[id].clone());
    input.expected_cash_baseline = snapshot.cash_balances[0].baseline_id.clone();
    input.kind = crate::models::TradeKind::Sell;
    input.price = price.into();
    input.gross_amount = (crate::models::decimal::parse(price).unwrap()
        * rust_decimal::Decimal::from(10))
    .normalize()
    .to_string();
    db.save_holding_trade(&input).unwrap()
}

#[test]
fn deleting_whole_position_nets_cash_preserves_evidence_and_replays_after_restore() {
    let db = funded();
    let s = confirm(&db, "999999999999999.99");
    let mut input = buy("REMOVE", "settle", "2026-01-01");
    input.expected_cash_baseline = s.cash_balances[0].baseline_id.clone();
    let s = db.save_holding_trade(&input).unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let s = sell_all(&db, &s, &id, "10");
    let declaration: crate::models::ManualCorporateActionInput = serde_json::from_value(
        serde_json::json!({"requestId":Uuid::new_v4().to_string(),"expectedRevision":s.holding_revisions[&id],"split":{"effectiveOn":"2026-02-01","numerator":"2","denominator":"1"},"reason":"券商通知"}),
    ).unwrap();
    let s = db.declare_corporate_action(&id, &declaration).unwrap();
    let original_actions = s.corporate_actions.clone();
    let original_trades = s.holding_trades.clone();
    let request = crate::models::HoldingDeleteInput {
        expected_revision: s.holding_revisions[&id].clone(),
    };
    let s = db.delete_holding(&id, &request).unwrap();
    assert!(s.holding_trades.is_empty());
    assert!(!s.holdings.iter().any(|h| h.id == id));
    assert!(!s.holding_revisions.contains_key(&id));
    assert!(!s.holding_trade_summaries.contains_key(&id));
    assert!(s.corporate_actions.is_empty());
    assert_eq!(s.cash_balances[0].amount, "999999999999999.99");
    assert!(!s.cash_balances[0].needs_reconciliation);
    let history = db.financial_history(&id).unwrap();
    let removal = history["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "holding.delete")
        .unwrap();
    let before: serde_json::Value =
        serde_json::from_str(removal["before"].as_str().unwrap()).unwrap();
    assert_eq!(
        before["trades"],
        serde_json::to_value(original_trades).unwrap()
    );
    assert_eq!(
        before["corporateActions"],
        serde_json::to_value(original_actions).unwrap()
    );
    assert_eq!(removal["cashDelta"], "0");
    let daily = db
        .daily_history(&crate::models::daily::HistoryQuery::default())
        .unwrap();
    assert!(daily.records[0]
        .assets
        .iter()
        .any(|a| a.holding.id == id && a.removed));
    let stable = db.export_sync_data().unwrap().content_hash().unwrap();
    db.delete_holding(&id, &request).unwrap();
    assert_eq!(
        stable,
        db.export_sync_data().unwrap().content_hash().unwrap()
    );
    let copy = Database::test();
    copy.import_sync_data(&db.export_sync_data().unwrap())
        .unwrap();
    let replay = copy
        .replay_financial_request(
            "DELETE",
            &format!("/api/holdings/{id}"),
            &serde_json::to_value(&request).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(replay.holding_trades.is_empty());
    assert_eq!(replay.cash_balances[0].amount, "999999999999999.99");
    assert_eq!(copy.financial_history(&id).unwrap(), history);
    // A late retry of the original create must not recreate a deleted position.
    assert!(copy
        .save_holding_trade(&input)
        .unwrap()
        .holding_trades
        .is_empty());
}

#[test]
fn deleting_mixed_history_and_live_position_reverses_only_current_cash() {
    let db = funded();
    let s = db
        .save_holding_trade(&buy("HISTORY-REMOVE", "history", "2026-01-01"))
        .unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let s = sell_all(&db, &s, &id, "11");
    assert_eq!(s.cash_balances[0].amount, "10110");
    let s = db
        .delete_holding(
            &id,
            &crate::models::HoldingDeleteInput {
                expected_revision: s.holding_revisions[&id].clone(),
            },
        )
        .unwrap();
    assert_eq!(s.cash_balances[0].amount, "10000");
    assert!(s.cash_balances[0].needs_reconciliation);
    assert_eq!(
        s.valuation_status.issues[0].code,
        crate::models::IssueCode::CashNeedsReconciliation
    );
    assert!(s.valuation_status.comparable);
    assert!(!s
        .plan
        .rebalancing_blocked_by
        .contains(&crate::models::IssueCode::CashNeedsReconciliation));
    assert!(s.holding_trades.is_empty());

    // A later cash confirmation covers all earlier executions.
    let s = confirm(&db, "500");
    let mut input = buy("COVERED-REMOVE", "settle", "2026-01-01");
    input.expected_cash_baseline = s.cash_balances[0].baseline_id.clone();
    let s = db.save_holding_trade(&input).unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let s = confirm(&db, "400");
    let s = db
        .delete_holding(
            &id,
            &crate::models::HoldingDeleteInput {
                expected_revision: s.holding_revisions[&id].clone(),
            },
        )
        .unwrap();
    assert_eq!(s.cash_balances[0].amount, "400");
    assert!(s.cash_balances[0].needs_reconciliation);
}

#[test]
fn deleting_position_rejects_stale_revision_and_insufficient_net_cash_atomically() {
    let db = funded();
    let s = confirm(&db, "100");
    let mut input = buy("SPENT", "settle", "2026-01-01");
    input.expected_cash_baseline = s.cash_balances[0].baseline_id.clone();
    let s = db.save_holding_trade(&input).unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let stale = crate::models::HoldingDeleteInput {
        expected_revision: s.holding_revisions[&id].clone(),
    };
    let s = sell_all(&db, &s, &id, "20");
    let current = crate::models::HoldingDeleteInput {
        expected_revision: s.holding_revisions[&id].clone(),
    };
    let mut spend = buy("OTHER", "settle", "2026-01-03");
    spend.expected_cash_baseline = s.cash_balances[0].baseline_id.clone();
    spend.price = "20".into();
    spend.gross_amount = "200".into();
    db.save_holding_trade(&spend).unwrap();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    assert!(matches!(
        db.delete_holding(&id, &stale),
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        db.delete_holding(&id, &Default::default()),
        Err(AppError::Conflict(_))
    ));
    assert!(db
        .delete_holding(&id, &current)
        .unwrap_err()
        .to_string()
        .contains("余额不足"));
    assert_eq!(
        before,
        db.export_sync_data().unwrap().content_hash().unwrap()
    );
    assert!(db
        .replay_financial_request(
            "DELETE",
            &format!("/api/holdings/{id}"),
            &serde_json::to_value(&current).unwrap()
        )
        .unwrap()
        .is_none());
}

#[test]
fn deleting_position_rolls_back_cash_and_trades_when_journal_fails() {
    let db = funded();
    let s = db
        .save_holding_trade(&buy("ATOMIC-REMOVE", "settle", "2026-01-01"))
        .unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    crate::db::fail_inserts(&db.conn().unwrap(), "financial_journal", "audit failure");
    let request = crate::models::HoldingDeleteInput {
        expected_revision: s.holding_revisions[&id].clone(),
    };
    assert!(db.delete_holding(&id, &request).is_err());
    assert_eq!(
        before,
        db.export_sync_data().unwrap().content_hash().unwrap()
    );
}

#[test]
fn cash_cutover_prevents_double_settlement_and_corrections_require_reconciliation() {
    let db = funded();
    confirm(&db, "1000.01");
    let history = buy("HISTORY", "history", "2026-01-01");
    let s = db.save_holding_trade(&history).unwrap();
    assert_eq!(s.cash_balances[0].amount, "1000.01");
    let t = s.holding_trades[0].clone();
    let edit:HoldingTradeUpdateInput=serde_json::from_value(serde_json::json!({"settlementMode":"history","expectedRevision":s.holding_revisions[&t.holding_id],"kind":t.kind,"occurredOn":t.occurred_on,"sequence":t.sequence,"quantity":t.quantity,"price":t.price,"grossAmount":t.gross_amount,"fees":"1","taxes":"0","account":t.account,"note":"按成交单更正"})).unwrap();
    let s = db.update_holding_trade(&t.id, &edit).unwrap();
    assert_eq!(s.cash_balances[0].amount, "1000.01");
    assert!(s.cash_balances[0].needs_reconciliation);
    let s = confirm(&db, "1000.01");
    assert!(!s.cash_balances[0].needs_reconciliation);
    let mut live = buy("LIVE", "settle", &crate::models::market::today());
    live.expected_cash_baseline = db.snapshot().unwrap().cash_balances[0].baseline_id.clone();
    let s = db.save_holding_trade(&live).unwrap();
    assert_eq!(s.cash_balances[0].amount, "900.01");
    let id = s
        .holding_trades
        .iter()
        .find(|t| t.id == live.request_id)
        .unwrap()
        .holding_id
        .clone();
    let s = db
        .delete_holding_trade(
            &live.request_id,
            &HoldingTradeDeleteInput {
                expected_revision: s.holding_revisions[&id].clone(),
            },
        )
        .unwrap();
    assert_eq!(s.cash_balances[0].amount, "1000.01");
    assert_eq!(
        db.financial_history(&id).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn durable_receipts_recover_lost_responses_without_recreating_deleted_trades() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    cash::fund(&db, "CNY");
    let buy = buy("RECEIPT", "settle", "2026-01-01");
    let s = db.save_holding_trade(&buy).unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let request = serde_json::to_value(&buy).unwrap();
    db.delete_holding_trade(
        &buy.request_id,
        &HoldingTradeDeleteInput {
            expected_revision: s.holding_revisions[&id].clone(),
        },
    )
    .unwrap();
    drop(db);
    let db = account.open();
    let s = db
        .replay_financial_request("POST", "/api/holding-trades", &request)
        .unwrap()
        .unwrap();
    assert!(s.holding_trades.is_empty());
    assert_eq!(s.cash_balances[0].amount, "10000");
    assert!(db
        .save_holding_trade(&buy)
        .unwrap()
        .holding_trades
        .is_empty());
    let mut changed = buy;
    changed.fees = "1".into();
    assert!(matches!(
        db.save_holding_trade(&changed),
        Err(AppError::Conflict(_))
    ));
    let copy = Database::test();
    copy.import_sync_data(&db.export_sync_data().unwrap())
        .unwrap();
    assert!(copy
        .replay_financial_request("POST", "/api/holding-trades", &request)
        .unwrap()
        .unwrap()
        .holding_trades
        .is_empty());
    assert_eq!(
        copy.financial_history(&id).unwrap(),
        db.financial_history(&id).unwrap()
    );
}

#[test]
fn journal_failure_rolls_back_cash_position_and_receipt_together() {
    let db = funded();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    crate::db::fail_inserts(&db.conn().unwrap(), "financial_journal", "audit failure");
    let input = buy("ATOMIC", "settle", "2026-01-01");
    assert!(db.save_holding_trade(&input).is_err());
    assert_eq!(
        db.export_sync_data().unwrap().content_hash().unwrap(),
        before
    );
    assert!(db
        .replay_financial_request(
            "POST",
            "/api/holding-trades",
            &serde_json::to_value(input).unwrap()
        )
        .unwrap()
        .is_none());
}

fn split_quote() -> SecurityPriceQuote {
    let today = crate::models::market::today();
    serde_json::from_value(serde_json::json!({"symbol":"SPLIT","currency":"CNY","close":5,"requestedOn":today,"observedOn":today,"stalenessDays":0,"providerCode":"yahoo_daily_close","providerName":"Yahoo Finance","exchange":"SHH","micCode":"","instrumentType":"EQUITY","priceBasis":"split_adjusted_daily_close","sourceUrl":"https://query1.finance.yahoo.com/v8/finance/chart/SPLIT?interval=1d","methodologyUrl":crate::models::market::METHODOLOGY_URL,"disclaimer":"按来源核对","splits":[{"effectiveOn":"2026-02-01","numerator":"2","denominator":"1"}]})).unwrap()
}

#[test]
fn split_review_rebuilds_quantity_and_cost_without_changing_original_executions_or_cash() {
    let db = funded();
    let input = buy("SPLIT", "settle", "2026-01-01");
    let s = db.save_holding_trade(&input).unwrap();
    let original = s.holding_trades.clone();
    let id = original[0].holding_id.clone();
    let cash = s.cash_balances[0].amount.clone();
    let quote = split_quote();
    let mut historical_quote = quote.clone();
    historical_quote.requested_on = "2026-01-31".into();
    historical_quote.observed_on = "2026-01-30".into();
    historical_quote.staleness_days = 1;
    assert!(db
        .register_corporate_actions(&id, &historical_quote)
        .unwrap());
    assert!(db.register_corporate_actions(&id, &quote).unwrap());
    let pending = db.snapshot().unwrap();
    assert_eq!(
        pending.valuation_status.issues[0].code,
        crate::models::IssueCode::PendingCorporateAction
    );
    assert_eq!(
        pending.valuation_status.issues[0].holding_ids,
        vec![id.clone()]
    );
    assert!(db
        .apply_verified_holding_valuation(&id, 10.0, &quote)
        .is_err());
    let action = &pending.corporate_actions[0];
    let confirm = ConfirmCorporateActionInput {
        expected_revision: pending.holding_revisions[&id].clone(),
        reason: "按券商公司行动通知核对".into(),
    };
    let s = db.confirm_corporate_action(&action.id, &confirm).unwrap();
    assert_eq!(s.holding_trade_summaries[&id].quantity, "20");
    assert_eq!(s.holding_trade_summaries[&id].cost_basis, "100");
    assert_eq!(s.holding_trades, original);
    assert_eq!(s.cash_balances[0].amount, cash);
    assert_eq!(
        db.confirm_corporate_action(&action.id, &confirm)
            .unwrap()
            .holding_trade_summaries[&id]
            .quantity,
        "20"
    );
    let s = db
        .apply_verified_holding_valuation(&id, 20.0, &quote)
        .unwrap();
    assert_eq!(
        s.holdings.iter().find(|h| h.id == id).unwrap().market_value,
        100.0
    );
    let restored = Database::test();
    restored
        .import_sync_data(&db.export_sync_data().unwrap())
        .unwrap();
    assert_eq!(
        restored.snapshot().unwrap().holding_trade_summaries[&id].quantity,
        "20"
    );
}

#[test]
fn cash_confirmation_fences_old_forms_but_note_only_history_edits_preserve_readiness() {
    let db = funded();
    let mut stale = buy("STALE", "settle", "2026-01-01");
    let s = confirm(&db, "1000.01");
    assert!(matches!(
        db.save_holding_trade(&stale),
        Err(AppError::Conflict(_))
    ));
    assert!(db.snapshot().unwrap().holding_trades.is_empty());
    stale.expected_cash_baseline = s.cash_balances[0].baseline_id.clone();
    let s = db.save_holding_trade(&stale).unwrap();
    assert_eq!(s.cash_balances[0].amount, "900.01");
    let s = confirm(&db, "900.01");
    let t = &s.holding_trades[0];
    let edit:HoldingTradeUpdateInput=serde_json::from_value(serde_json::json!({"settlementMode":"history","expectedRevision":s.holding_revisions[&t.holding_id],"kind":t.kind,"occurredOn":t.occurred_on,"sequence":t.sequence,"quantity":t.quantity,"price":t.price,"grossAmount":t.gross_amount,"fees":t.fees,"taxes":t.taxes,"account":t.account,"note":"仅补充成交说明"})).unwrap();
    let s = db.update_holding_trade(&t.id, &edit).unwrap();
    assert_eq!(s.cash_balances[0].amount, "900.01");
    assert!(!s.cash_balances[0].needs_reconciliation);
}

#[test]
fn manual_reverse_split_and_retraction_preserve_cash_and_reject_fractional_rounding() {
    let db = funded();
    let s = db
        .save_holding_trade(&buy("PRIVATE", "history", "2026-01-01"))
        .unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    let manual:crate::models::ManualCorporateActionInput=serde_json::from_value(serde_json::json!({"requestId":Uuid::new_v4().to_string(),"expectedRevision":s.holding_revisions[&id],"split":{"effectiveOn":"2026-02-01","numerator":"1","denominator":"2"},"reason":"按未上市证券通知核对"})).unwrap();
    let s = db.declare_corporate_action(&id, &manual).unwrap();
    assert!(s
        .valuation_status
        .issues
        .iter()
        .any(|i| i.code == crate::models::IssueCode::PendingCorporateAction));
    let confirm = ConfirmCorporateActionInput {
        expected_revision: s.holding_revisions[&id].clone(),
        reason: "核对券商通知".into(),
    };
    let s = db
        .confirm_corporate_action(&manual.request_id, &confirm)
        .unwrap();
    assert_eq!(s.holding_trade_summaries[&id].quantity, "5");
    assert_eq!(s.holding_trade_summaries[&id].cost_basis, "100");
    assert!(s.accounting.as_ref().unwrap().holdings[&id].usable);
    assert!(s.accounting.as_ref().unwrap().total_value.is_some());
    let undo = ConfirmCorporateActionInput {
        expected_revision: s.holding_revisions[&id].clone(),
        reason: "通知版本有误，撤回核对".into(),
    };
    let s = db
        .review_corporate_action(&manual.request_id, &undo, "revert")
        .unwrap();
    assert_eq!(s.holding_trade_summaries[&id].quantity, "10");
    assert_eq!(s.cash_balances[0].amount, "10000");
    assert!(s.corporate_actions[0].dismissed);
    let mut fractional = manual.clone();
    fractional.request_id = Uuid::new_v4().to_string();
    fractional.expected_revision = s.holding_revisions[&id].clone();
    fractional.split.denominator = "3".into();
    let s = db.declare_corporate_action(&id, &fractional).unwrap();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    let attempt = ConfirmCorporateActionInput {
        expected_revision: s.holding_revisions[&id].clone(),
        reason: "不得静默舍入零碎股".into(),
    };
    assert!(db
        .confirm_corporate_action(&fractional.request_id, &attempt)
        .is_err());
    assert_eq!(
        before,
        db.export_sync_data().unwrap().content_hash().unwrap()
    );
    let s = db
        .review_corporate_action(&fractional.request_id, &attempt, "reject")
        .unwrap();
    assert!(s.corporate_actions.iter().all(|a| a.dismissed));
    let copy = Database::test();
    copy.import_sync_data(&db.export_sync_data().unwrap())
        .unwrap();
    assert_eq!(
        copy.snapshot().unwrap().holding_trade_summaries[&id].quantity,
        "10"
    );
}

#[test]
fn decimal_source_and_daily_history_keep_cents_below_float_resolution_and_restore_them() {
    let db = funded();
    let s = confirm(&db, "999999999999999.01");
    let id = s.cash_balances[0].holding_id.clone();
    let s = confirm(&db, "999999999999999.02");
    assert_eq!(s.cash_balances[0].amount, "999999999999999.02");
    assert_eq!(
        s.accounting.as_ref().unwrap().total_value.as_deref(),
        Some("999999999999999.02")
    );
    let history = db
        .daily_history(&crate::models::daily::HistoryQuery::default())
        .unwrap();
    assert_eq!(
        history.records[0].exact_total_assets.as_deref(),
        Some("999999999999999.02")
    );
    assert_eq!(
        history.records[0]
            .assets
            .iter()
            .find(|a| a.holding.id == id)
            .unwrap()
            .exact_base_value
            .as_deref(),
        Some("999999999999999.02")
    );
    let copy = Database::test();
    let data = db.export_sync_data().unwrap();
    copy.import_sync_data(&data).unwrap();
    assert_eq!(
        copy.snapshot().unwrap().cash_balances[0].amount,
        "999999999999999.02"
    );
    assert_eq!(
        copy.daily_history(&crate::models::daily::HistoryQuery::default())
            .unwrap()
            .records[0]
            .exact_total_assets,
        history.records[0].exact_total_assets
    );
    let mut tampered = data;
    let table = tampered
        .tables
        .iter_mut()
        .find(|t| t.name == "cash_state")
        .unwrap();
    table.rows[0][2] = SyncValue::Text("7.00".into());
    let stable = copy.export_sync_data().unwrap().content_hash().unwrap();
    assert!(copy.import_sync_data(&tampered).is_err());
    assert_eq!(
        copy.export_sync_data().unwrap().content_hash().unwrap(),
        stable
    );
}

#[test]
fn old_price_observation_is_metadata_not_a_rebalancing_gate() {
    let db = funded();
    let s = db
        .save_holding_trade(&buy("SPLIT", "history", "2026-01-01"))
        .unwrap();
    let id = s.holding_trades[0].holding_id.clone();
    confirm(&db, "1000");
    let mut quote = split_quote();
    quote.splits.clear();
    quote.observed_on = (chrono::NaiveDate::parse_from_str(&quote.requested_on, "%Y-%m-%d")
        .unwrap()
        - chrono::Duration::days(8))
    .to_string();
    quote.staleness_days = 8;
    let s = db
        .apply_verified_holding_valuation(&id, 10.0, &quote)
        .unwrap();
    assert!(s.valuation_status.issues.is_empty());
    assert_eq!(s.holding_valuations[0].observed_on, quote.observed_on);
    assert!(!s
        .plan
        .rebalancing_blocked_by
        .contains(&crate::models::IssueCode::PendingCorporateAction));
}
