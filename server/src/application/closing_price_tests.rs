use super::refresh_after_close;
use crate::{
    db::{Database, TestAccount},
    models::{
        market::{canonical_symbol, close, METHODOLOGY_URL},
        HoldingInput, SecurityPriceQuery, SecurityPriceQuote,
    },
    ports::market::SecurityPriceProvider,
    AppError, AppResult,
};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};

fn now() -> DateTime<Utc> {
    "2026-09-08T07:05:00Z".parse().unwrap()
}
fn holding(symbol: &str, quantity: Option<f64>) -> HoldingInput {
    serde_json::from_value(serde_json::json!({
        "symbol":symbol,"name":format!("测试资产 {symbol}"),"assetClass":"股票","currency":"CNY",
        "quantity":quantity,"currentPrice":quantity.map(|_|10.0),
        "costBasis":50,"marketValue":quantity.map_or(100.0, |q|q*10.0),
        "targetPct":0,"valuationDate":"2026-09-07"
    }))
    .unwrap()
}
fn quote(query: &SecurityPriceQuery, observed: &str, price: f64) -> SecurityPriceQuote {
    SecurityPriceQuote {
        exact_close: Some(price.to_string()),
        splits: vec![],
        symbol: query.symbol.clone(),
        currency: "CNY".into(),
        close: price,
        requested_on: query.on_date.clone(),
        observed_on: observed.into(),
        staleness_days: (query.on_date.parse::<NaiveDate>().unwrap()
            - observed.parse::<NaiveDate>().unwrap())
        .num_days(),
        provider_code: "yahoo_daily_close".into(),
        provider_name: "Yahoo Finance".into(),
        exchange: "SHH".into(),
        mic_code: String::new(),
        instrument_type: "EQUITY".into(),
        price_basis: "split_adjusted_daily_close".into(),
        source_url: format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d",
            canonical_symbol(&query.symbol).unwrap()
        ),
        methodology_url: METHODOLOGY_URL.into(),
        disclaimer: "测试日收盘价".into(),
    }
}
enum Mutation {
    Edit,
    Delete,
    NewerQuote,
}
struct Provider {
    calls: AtomicUsize,
    failed_symbol: Option<&'static str>,
    observed: Option<&'static str>,
    peer: Option<(Database, Mutation)>,
}
impl Provider {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            failed_symbol: None,
            observed: None,
            peer: None,
        }
    }
}
#[async_trait]
impl SecurityPriceProvider for Provider {
    async fn quote(&self, query: &SecurityPriceQuery, _: &str) -> AppResult<SecurityPriceQuote> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.failed_symbol == Some(query.symbol.as_str()) {
            return Err(AppError::MarketData("来源延迟".into()));
        }
        let result = quote(query, self.observed.unwrap_or("2026-09-08"), 12.0);
        if let Some((peer, mutation)) = &self.peer {
            let acquired: bool = peer
                .conn()
                .unwrap()
                .query_row(
                    "SELECT pg_try_advisory_lock(hashtextextended(current_schema(),0))",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                acquired,
                "the quote must not hold the account lock during network I/O"
            );
            let snapshot = peer.snapshot().unwrap();
            let h = snapshot
                .holdings
                .iter()
                .find(|h| h.symbol == query.symbol)
                .unwrap();
            match mutation {
                Mutation::Edit => {
                    peer.update_holding(&h.id, &holding("601988.SS", Some(20.0)))
                        .unwrap();
                }
                Mutation::Delete => {
                    peer.delete_holding(
                        &h.id,
                        &crate::models::HoldingDeleteInput {
                            expected_revision: snapshot.holding_revisions[&h.id].clone(),
                        },
                    )
                    .unwrap();
                }
                Mutation::NewerQuote => {
                    peer.apply_verified_holding_valuation(
                        &h.id,
                        10.0,
                        &quote(query, "2026-09-08", 30.0),
                    )
                    .unwrap();
                }
            }
            peer.conn()
                .unwrap()
                .execute_batch("SELECT pg_advisory_unlock(hashtextextended(current_schema(),0))")
                .unwrap();
        }
        Ok(result)
    }
}

#[tokio::test]
async fn same_day_close_updates_value_once_and_skips_cash_closed_and_manual_assets() {
    let db = Database::test();
    let id = db
        .add_holding(&holding("600519.SS", Some(10.0)))
        .unwrap()
        .holdings[0]
        .id
        .clone();
    db.add_holding(&holding("", None)).unwrap();
    db.add_holding(&holding("000001.SZ", Some(0.0))).unwrap();
    let mut cash = holding("", None);
    cash.asset_class = "现金".into();
    db.add_holding(&cash).unwrap();
    let provider = Provider::new();
    let result = refresh_after_close(&db, &provider, now()).await.unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.updated_count, 1);
    let h = result
        .snapshot
        .holdings
        .iter()
        .find(|h| h.id == id)
        .unwrap();
    assert_eq!(
        (h.quantity, h.current_price, h.market_value, h.cost_basis),
        (Some(10.0), Some(12.0), 120.0, 50.0)
    );
    assert_eq!(
        result.snapshot.holding_valuations[0].observed_on,
        "2026-09-08"
    );
    let revision = db.revision().unwrap();
    assert_eq!(
        refresh_after_close(&db, &provider, now())
            .await
            .unwrap()
            .updated_count,
        0
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(db.revision().unwrap(), revision);
    let history = db.daily_history(&Default::default()).unwrap();
    assert_eq!(
        history.records[0]
            .assets
            .iter()
            .find(|a| a.holding.id == id)
            .unwrap()
            .holding
            .market_value,
        120.0
    );
    let other_account = Database::test();
    assert!(other_account.snapshot().unwrap().holdings.is_empty());
}

#[tokio::test]
async fn pre_close_and_weekends_skip_completed_dates_and_restart_uses_saved_evidence() {
    let db = Database::test();
    let id = db
        .add_holding(&holding("600519.SS", Some(10.0)))
        .unwrap()
        .holdings[0]
        .id
        .clone();
    let query = SecurityPriceQuery {
        symbol: "600519.SS".into(),
        on_date: "2026-09-08".into(),
        action_from: None,
    };
    db.apply_verified_holding_valuation(&id, 10.0, &quote(&query, "2026-09-07", 11.0))
        .unwrap();
    let provider = Provider::new();
    let before: DateTime<Utc> = "2026-09-08T07:04:59Z".parse().unwrap();
    assert_eq!(
        close::completed_day("600519.SS", "2026-09-08".parse().unwrap(), before).to_string(),
        "2026-09-07"
    );
    assert_eq!(
        refresh_after_close(&db, &provider, before)
            .await
            .unwrap()
            .updated_count,
        0
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        refresh_after_close(&db, &provider, now())
            .await
            .unwrap()
            .updated_count,
        1
    );
    let reopened = TestAccount::new();
    let a = reopened.open();
    a.import_sync_data(&db.export_sync_data().unwrap()).unwrap();
    drop(a);
    let db = reopened.open();
    assert_eq!(
        refresh_after_close(&db, &provider, now())
            .await
            .unwrap()
            .updated_count,
        0
    );
}

#[tokio::test]
async fn failures_and_stale_sources_preserve_values_while_other_holdings_continue_and_recover() {
    let db = Database::test();
    db.add_holding(&holding("600519.SS", Some(10.0))).unwrap();
    db.add_holding(&holding("000001.SZ", Some(10.0))).unwrap();
    let mut provider = Provider::new();
    provider.failed_symbol = Some("600519.SS");
    let result = refresh_after_close(&db, &provider, now()).await.unwrap();
    assert_eq!((result.updated_count, result.errors.len()), (1, 1));
    provider.failed_symbol = None;
    let result = refresh_after_close(&db, &provider, now()).await.unwrap();
    assert_eq!(result.updated_count, 1);
    let revision = db.revision().unwrap();
    provider.observed = Some("2026-09-07");
    let tomorrow = "2026-09-09T07:05:00Z".parse().unwrap();
    assert_eq!(
        refresh_after_close(&db, &provider, tomorrow)
            .await
            .unwrap()
            .updated_count,
        0
    );
    assert_eq!(db.revision().unwrap(), revision);
}

#[tokio::test]
async fn in_progress_prices_and_failed_daily_writes_cannot_change_the_portfolio() {
    for write_failure in [false, true] {
        let db = Database::test();
        db.add_holding(&holding("600519.SS", Some(10.0))).unwrap();
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        if write_failure {
            crate::db::fail_inserts(
                &db.conn().unwrap(),
                "daily_entries",
                "injected daily write failure",
            );
        }
        let at = if write_failure {
            now()
        } else {
            "2026-09-08T07:04:59Z".parse().unwrap()
        };
        let result = refresh_after_close(&db, &Provider::new(), at)
            .await
            .unwrap();
        assert_eq!((result.updated_count, result.errors.len()), (0, 1));
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn concurrent_edits_deletions_and_newer_prices_win_over_in_flight_quotes() {
    for mutation in [Mutation::Edit, Mutation::Delete, Mutation::NewerQuote] {
        let account = TestAccount::new();
        let db = account.open();
        let peer = account.open();
        db.add_holding(&holding("600519.SS", Some(10.0))).unwrap();
        let provider = Provider {
            peer: Some((peer, mutation)),
            ..Provider::new()
        };
        let result = refresh_after_close(&db, &provider, now()).await.unwrap();
        assert_eq!(result.updated_count, 0);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let saved = &result.snapshot.holdings;
        match &provider.peer.as_ref().unwrap().1 {
            Mutation::Edit => assert_eq!(
                (saved[0].symbol.as_str(), saved[0].quantity),
                ("601988.SS", Some(20.0))
            ),
            Mutation::Delete => assert!(saved.is_empty()),
            Mutation::NewerQuote => assert_eq!(saved[0].market_value, 300.0),
        }
    }
}
