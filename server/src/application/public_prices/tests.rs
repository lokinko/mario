use super::*;
use crate::models::market::history::tests::fixture;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Source {
    calls: AtomicUsize,
    fail: AtomicBool,
}

struct UnevenSource;
#[async_trait::async_trait]
impl SecurityHistoryProvider for UnevenSource {
    async fn history(&self, symbol: &str, through: NaiveDate) -> AppResult<PriceHistory> {
        if symbol == "000001.SZ" {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(fixture(symbol, through))
    }
}

#[tokio::test(start_paused = true)]
async fn slow_security_does_not_block_queue_and_round_timeout_persists_failure_without_a_dangling_lease(
) {
    let store = Arc::new(PublicPrices::test());
    let symbols = (1..=8).map(|i| format!("{i:06}.SZ")).collect::<Vec<_>>();
    for symbol in &symbols {
        store.register(symbol).unwrap();
    }
    let service = PublicPriceService::new(store.clone(), Arc::new(UnevenSource));
    service
        .refresh_symbols(&symbols, Duration::from_secs(5))
        .await;
    for symbol in &symbols {
        let state = store.state(symbol).unwrap().unwrap();
        assert_eq!(
            state.lease_until, 0,
            "{symbol} must be completed rather than cancelled"
        );
        if symbol == "000001.SZ" {
            assert!(state.last_error.contains("时限"));
        } else {
            assert!(
                state.history.is_some(),
                "{symbol} must not wait for the slow first security"
            );
        }
    }
    let restarted = PublicPriceService::new(store.clone(), Arc::new(UnevenSource));
    assert!(restarted.load("000001.SZ", false).await.is_err());
    assert!(store.state("000008.SZ").unwrap().unwrap().history.is_some());
}
#[async_trait::async_trait]
impl SecurityHistoryProvider for Source {
    async fn history(&self, symbol: &str, through: NaiveDate) -> AppResult<PriceHistory> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::MarketData("offline".into()));
        }
        Ok(fixture(symbol, through))
    }
}
#[tokio::test]
async fn accounts_aliases_and_restarts_reuse_one_persisted_revision() {
    let store = Arc::new(PublicPrices::test());
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let provider = Arc::new(PublicPriceService::new(store.clone(), source.clone()));
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..16 {
        let p = provider.clone();
        tasks.spawn(async move {
            p.quote(&SecurityPriceQuery {
                symbol: if i % 2 == 0 { "600941.SH" } else { "600941.SS" }.into(),
                on_date: crate::models::market::today(),
                action_from: None,
            })
            .await
            .unwrap()
        });
    }
    while let Some(q) = tasks.join_next().await {
        assert_eq!(q.unwrap().exact_close.as_deref(), Some("100"));
    }
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.symbols().unwrap(), vec!["600941.SS"]);
    source.fail.store(true, Ordering::SeqCst);
    let restarted = PublicPriceService::new(store, source.clone());
    let query = SecurityPriceQuery {
        symbol: "600941.SS".into(),
        on_date: crate::models::market::today(),
        action_from: None,
    };
    assert!(restarted.quote(&query).await.is_ok());
    assert!(restarted.cached_quote(&query).unwrap().is_some());
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn read_only_cached_lookup_never_calls_a_source_and_failures_back_off_in_the_database() {
    let store = Arc::new(PublicPrices::test());
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(true),
    });
    let provider = PublicPriceService::new(store.clone(), source.clone());
    let query = SecurityPriceQuery {
        symbol: "600941.SS".into(),
        on_date: crate::models::market::today(),
        action_from: None,
    };
    assert!(provider.cached_quote(&query).unwrap().is_none());
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    assert!(provider.quote(&query).await.is_err());
    assert!(provider.quote(&query).await.is_err());
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert!(!store
        .state("600941.SS")
        .unwrap()
        .unwrap()
        .last_error
        .is_empty());
}

#[tokio::test]
async fn two_accounts_read_the_same_forward_price_without_vendor_io_and_keep_local_quantities_and_costs(
) {
    let store = Arc::new(PublicPrices::test());
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let provider = PublicPriceService::new(store, source.clone());
    let query = SecurityPriceQuery {
        symbol: "600941.SS".into(),
        on_date: crate::models::market::today(),
        action_from: None,
    };
    provider.quote(&query).await.unwrap();
    let accounts = [crate::db::Database::test(), crate::db::Database::test()];
    for (i, db) in accounts.iter().enumerate() {
        let input=serde_json::from_value(serde_json::json!({"symbol":if i==0{"600941.SH"}else{"600941.SS"},"name":"测试公司","assetClass":"股票","currency":"CNY","quantity":if i==0{10}else{20},"currentPrice":80,"marketValue":if i==0{800}else{1600},"costBasis":500,"targetPct":0,"valuationDate":crate::models::market::today()})).unwrap();
        db.add_holding(&input).unwrap();
        let _lock = db.lock_account().unwrap();
        let snapshot = crate::application::prices::apply_cached(db, &provider).unwrap();
        assert_eq!(snapshot.holdings[0].current_price, Some(100.0));
        assert_eq!(snapshot.holdings[0].cost_basis, 500.0);
        assert_eq!(
            snapshot.holding_valuations[0].price_basis,
            "forward_adjusted_daily_close"
        );
        let revision = db.revision().unwrap();
        crate::application::prices::apply_cached(db, &provider).unwrap();
        assert_eq!(db.revision().unwrap(), revision);
    }
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_corrected_shared_revision_is_adopted_on_reads_even_for_the_same_close_date() {
    let store = Arc::new(PublicPrices::test());
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let provider = PublicPriceService::new(store.clone(), source.clone());
    let query = SecurityPriceQuery {
        symbol: "600941.SS".into(),
        on_date: crate::models::market::today(),
        action_from: None,
    };
    provider.quote(&query).await.unwrap();
    let account = crate::db::Database::test();
    let input=serde_json::from_value(serde_json::json!({"symbol":"600941.SS","name":"公司","assetClass":"股票","currency":"CNY","quantity":10,"currentPrice":50,"marketValue":500,"costBasis":400,"targetPct":0,"valuationDate":crate::models::market::today()})).unwrap();
    account.add_holding(&input).unwrap();
    let _lock = account.lock_account().unwrap();
    crate::application::prices::apply_cached(&account, &provider).unwrap();
    let version = account.revision().unwrap();
    let mut h = store.state("600941.SS").unwrap().unwrap().history.unwrap();
    h.bars.last_mut().unwrap().raw_close = Some("104.23".into());
    h.bars.last_mut().unwrap().adjusted_close = Some("104.23".into());
    h = h.normalize().unwrap();
    assert!(store
        .claim(
            "600941.SS",
            chrono::Utc::now().timestamp_millis() + 121000,
            "correction"
        )
        .unwrap());
    store.finish("600941.SS", "correction", &Ok(h)).unwrap();
    let current = crate::application::prices::apply_cached(&account, &provider).unwrap();
    assert_eq!(current.holdings[0].current_price, Some(104.23));
    assert_eq!(current.holdings[0].cost_basis, 400.0);
    assert_ne!(account.revision().unwrap(), version);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fresh_five_year_cache_is_backfilled_and_failed_backfills_keep_the_original_revision() {
    for fail in [false, true] {
        let store = Arc::new(PublicPrices::test());
        let source = Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(fail),
        });
        let now = Utc::now();
        let today = now.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
        let end = close::completed_day("600941.SS", today, now);
        let mut old = fixture("600941.SS", end);
        old.window_start = end.checked_sub_months(chrono::Months::new(60)).unwrap();
        old.bars.retain(|b| b.day >= old.window_start);
        old = old.normalize().unwrap();
        assert!(store
            .claim("600941.SS", now.timestamp_millis() - 121000, "old")
            .unwrap());
        store.finish("600941.SS", "old", &Ok(old.clone())).unwrap();
        let service = PublicPriceService::new(store.clone(), source.clone());
        let result = service.history("600941.SS", end).await;
        assert_eq!(
            source.calls.load(Ordering::SeqCst),
            1,
            "fresh five-year timestamps cannot bypass backfill"
        );
        let state = store.state("600941.SS").unwrap().unwrap();
        if fail {
            assert!(result.is_err());
            assert_eq!(state.history, Some(old));
            assert!(state.last_error.contains("offline"));
        } else {
            assert_eq!(
                result.unwrap().window_start,
                crate::models::market::history::history_start(end)
            );
            assert!(state.last_error.is_empty());
        }
    }
}

#[tokio::test]
async fn completed_close_is_reused_for_twelve_hours_and_lagging_close_retries_after_thirty_minutes()
{
    let store = Arc::new(PublicPrices::test());
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let now = Utc::now();
    let today = now.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
    let end = close::completed_day("600941.SS", today, now);
    let service = PublicPriceService::new(store.clone(), source.clone());

    let seed = |symbol: &str, mut history: PriceHistory, age: chrono::Duration, token: &str| {
        let fetched = now - age;
        history.fetched_at = fetched.to_rfc3339();
        let ts = fetched.timestamp_millis();
        assert!(store.claim(symbol, ts, token).unwrap());
        store.finish_at(symbol, token, &Ok(history), ts).unwrap();
    };

    // 1. Current close fetched 11 hours ago stays cached within the 12-hour window.
    seed(
        "600941.SS",
        fixture("600941.SS", end),
        chrono::Duration::hours(11),
        "cur-fresh",
    );
    service.history("600941.SS", end).await.unwrap();
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        0,
        "completed close must reuse cache inside 12-hour TTL"
    );

    // 2. Current close fetched 13 hours ago refreshes once.
    seed(
        "600036.SS",
        fixture("600036.SS", end),
        chrono::Duration::hours(13),
        "cur-stale",
    );
    service.history("600036.SS", end).await.unwrap();
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        1,
        "completed close must refresh once 12-hour TTL elapses"
    );

    // 3. Lagging close (adjustment_anchor < end) stays cached inside 30 minutes.
    let mut lagging_fresh = fixture("600519.SS", end);
    lagging_fresh.bars.pop();
    lagging_fresh = lagging_fresh.normalize().unwrap();
    assert!(lagging_fresh.adjustment_anchor < end);
    seed(
        "600519.SS",
        lagging_fresh,
        chrono::Duration::minutes(20),
        "lag-fresh",
    );
    service.history("600519.SS", end).await.unwrap();
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        1,
        "lagging close must stay cached within 30-minute retry window"
    );

    // 4. Lagging close older than 30 minutes retries once.
    let mut lagging_stale = fixture("600900.SS", end);
    lagging_stale.bars.pop();
    lagging_stale = lagging_stale.normalize().unwrap();
    assert!(lagging_stale.adjustment_anchor < end);
    seed(
        "600900.SS",
        lagging_stale,
        chrono::Duration::minutes(35),
        "lag-stale",
    );
    service.history("600900.SS", end).await.unwrap();
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        2,
        "lagging close must retry after 30-minute window"
    );
}
