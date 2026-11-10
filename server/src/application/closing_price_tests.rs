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

#[test]
fn eastmoney_prices_update_exact_accounting_and_restore_without_changing_costs() {
    for (symbol, exchange, currency) in [
        ("518880.SS", "SHH", "CNY"),
        ("588000.SS", "SHH", "CNY"),
        ("508077.SS", "SHH", "CNY"),
        ("0700.HK", "HKG", "HKD"),
        ("QQQ", "NASDAQ", "USD"),
        ("PDD", "NASDAQ", "USD"),
        ("VOO", "AMEX", "USD"),
    ] {
        let db = Database::test();
        let mut input = holding(symbol, Some(10.0));
        input.currency = currency.into();
        let before = db.add_holding(&input).unwrap();
        let id = before.holdings[0].id.clone();
        let day = crate::models::market::today();
        let query = SecurityPriceQuery {
            symbol: symbol.into(),
            on_date: day.clone(),
            action_from: None,
        };
        let mut q = quote(&query, &day, 12.0);
        q.provider_code = "eastmoney_forward_close".into();
        q.provider_name = "东方财富".into();
        q.price_basis = "forward_adjusted_daily_close".into();
        q.instrument_type = if symbol.ends_with(".SS") {
            "FUND"
        } else if matches!(symbol, "QQQ" | "VOO") {
            "ETF"
        } else {
            "EQUITY"
        }
        .into();
        q.currency = currency.into();
        q.exchange = exchange.into();
        let identity = crate::models::market::eastmoney::security(symbol, exchange).unwrap();
        q.source_url = format!(
            "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid={}&klt=101&fqt=1",
            identity.secid()
        );
        q.methodology_url = crate::models::market::eastmoney_methodology_url(symbol);
        let after = db.apply_verified_holding_valuation(&id, 10.0, &q).unwrap();
        let a = &after.accounting.as_ref().unwrap().holdings[&id];
        assert_eq!(a.amounts.market_value, "120");
        assert_eq!(a.amounts.cost_basis, "50");
        assert_eq!(a.amounts.quantity.as_deref(), Some("10"));
        assert!(after.holding_trades.is_empty());
        let copy = Database::test();
        copy.import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(copy.snapshot().unwrap().holdings[0].market_value, 120.0);
        let stable = db.export_sync_data().unwrap().content_hash().unwrap();
        for mutation in ["identity", "methodology", "adjustment"] {
            let mut wrong = q.clone();
            match mutation {
                "identity" => {
                    wrong.source_url = wrong.source_url.replace(&identity.secid(), "1.600941")
                }
                "methodology" => {
                    wrong.methodology_url = crate::models::market::ASHARE_METHODOLOGY_URL.into()
                }
                _ => wrong.source_url = wrong.source_url.replace("fqt=1", "fqt=0"),
            }
            assert!(db
                .apply_verified_holding_valuation(&id, 10.0, &wrong)
                .is_err());
            assert_eq!(
                db.export_sync_data().unwrap().content_hash().unwrap(),
                stable
            );
        }
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
    observed: Option<String>,
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
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.failed_symbol == Some(query.symbol.as_str()) {
            return Err(AppError::MarketData("来源延迟".into()));
        }
        let result = quote(
            query,
            self.observed.as_deref().unwrap_or("2026-09-08"),
            12.0,
        );
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
                        &quote(query, &query.on_date, 30.0),
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
async fn explicit_refresh_releases_the_request_lock_and_rechecks_each_holding_before_writing() {
    for mutation in [Mutation::Edit, Mutation::Delete, Mutation::NewerQuote] {
        let account = TestAccount::new();
        let db = account.open();
        let peer = account.open();
        db.add_holding(&holding("600941.SS", Some(10.0))).unwrap();
        let provider = Provider {
            peer: Some((peer, mutation)),
            ..Provider::new()
        };
        let _request_lock = db.lock_account().unwrap();
        let result = super::refresh(&db, &provider).await.unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.updated_count, 0);
        match &provider.peer.as_ref().unwrap().1 {
            Mutation::Edit => assert_eq!(
                (
                    result.snapshot.holdings[0].symbol.as_str(),
                    result.snapshot.holdings[0].quantity
                ),
                ("601988.SS", Some(20.0))
            ),
            Mutation::Delete => assert!(result.snapshot.holdings.is_empty()),
            Mutation::NewerQuote => {
                assert_eq!(result.snapshot.holdings[0].current_price, Some(30.0))
            }
        }
    }
}

struct SlowQuotes {
    calls: AtomicUsize,
}
#[async_trait]
impl SecurityPriceProvider for SlowQuotes {
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if query.symbol != "600941.SS" {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
        Ok(quote(query, &query.on_date, 12.0))
    }
}

#[tokio::test(start_paused = true)]
async fn a_refresh_deadline_retains_successes_reports_unfinished_holdings_and_stops_new_fetches() {
    let db = Database::test();
    for symbol in [
        "600941.SS",
        "600519.SS",
        "601988.SS",
        "600036.SS",
        "600900.SS",
        "601398.SS",
    ] {
        db.add_holding(&holding(symbol, Some(10.0))).unwrap();
    }
    let provider = SlowQuotes {
        calls: AtomicUsize::new(0),
    };
    let started = tokio::time::Instant::now();
    let result = refresh_after_close(&db, &provider, now()).await.unwrap();
    assert!(started.elapsed() <= std::time::Duration::from_secs(120));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    assert_eq!(result.updated_count, 1);
    assert_eq!(result.errors.len(), 5);
    assert_eq!(result.snapshot.price_refresh_errors, result.errors);
    for holding in result.snapshot.holdings {
        assert_eq!(
            holding.current_price,
            Some(if holding.symbol == "600941.SS" {
                12.0
            } else {
                10.0
            })
        );
    }
}

#[tokio::test]
async fn delayed_close_is_compared_to_observed_date_instead_of_request_date() {
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
    db.apply_verified_holding_valuation(&id, 10.0, &quote(&query, "2026-09-04", 11.0))
        .unwrap();
    let provider = Provider {
        observed: Some("2026-09-07".into()),
        ..Provider::new()
    };
    let before = "2026-09-08T07:04:59Z".parse().unwrap();
    let result = refresh_after_close(&db, &provider, before).await.unwrap();
    assert_eq!(result.updated_count, 1);
    assert_eq!(result.snapshot.holdings[0].current_price, Some(12.0));
    assert_eq!(
        result.snapshot.holding_valuations[0].observed_on,
        "2026-09-07"
    );
    assert_eq!(
        result.snapshot.holding_valuations[0].requested_on,
        "2026-09-08"
    );
    provider.calls.store(0, Ordering::SeqCst);
    assert_eq!(
        refresh_after_close(&db, &provider, before)
            .await
            .unwrap()
            .updated_count,
        0
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn ordinary_holdings_adopt_shared_prices_even_when_previously_manual() {
    for status in ["provisional", "manual"] {
        let db = Database::test();
        let mut input = holding("600519.SS", Some(10.0));
        input.valuation_date = "2026-09-08".into();
        input.exact_values = Some(
            serde_json::from_value(serde_json::json!({
                "priceStatus": status, "marketValue":"100", "costBasis":"50",
                "quantity":"10", "costPrice":null, "currentPrice":"10"
            }))
            .unwrap(),
        );
        db.add_holding(&input).unwrap();
        let provider = Provider {
            observed: Some("2026-09-07".into()),
            ..Provider::new()
        };
        let result = refresh_after_close(&db, &provider, "2026-09-08T07:04:59Z".parse().unwrap())
            .await
            .unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.updated_count, 1);
        assert_eq!(result.snapshot.holdings[0].current_price, Some(12.0));
    }
}

#[tokio::test]
async fn explicit_refresh_does_not_replace_a_newer_verified_close_with_a_delayed_source() {
    let db = Database::test();
    let _request_lock = db.lock_account().unwrap();
    let id = db
        .add_holding(&holding("600519.SS", Some(10.0)))
        .unwrap()
        .holdings[0]
        .id
        .clone();
    let day: NaiveDate = crate::models::market::today().parse().unwrap();
    let observed = (day - chrono::Duration::days(1)).to_string();
    let query = SecurityPriceQuery {
        symbol: "600519.SS".into(),
        on_date: day.to_string(),
        action_from: None,
    };
    db.apply_verified_holding_valuation(&id, 10.0, &quote(&query, &observed, 30.0))
        .unwrap();
    let hash = db.export_sync_data().unwrap().content_hash().unwrap();
    let mut provider = Provider {
        observed: Some((day - chrono::Duration::days(2)).to_string()),
        ..Provider::new()
    };
    let result = super::refresh(&db, &provider).await.unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.updated_count, 0);
    assert_eq!(result.snapshot.holdings[0].current_price, Some(30.0));
    assert_eq!(db.export_sync_data().unwrap().content_hash().unwrap(), hash);
    // An explicit refresh can still adopt a correction for the same trading date.
    provider.observed = Some(observed);
    let corrected = super::refresh(&db, &provider).await.unwrap();
    assert!(corrected.errors.is_empty(), "{:?}", corrected.errors);
    assert_eq!(corrected.updated_count, 1);
    assert_eq!(corrected.snapshot.holdings[0].current_price, Some(12.0));
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
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
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
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
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
    provider.observed = Some("2026-09-07".into());
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
