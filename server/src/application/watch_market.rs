//! Public dividend lookup shared by all accounts; source I/O stays behind the market port.
use crate::{
    db::PublicDividends,
    models::watch::{DividendHistory, WatchMarketData},
    ports::market::WatchMarketProvider,
    AppError, AppResult,
};
use chrono::NaiveDate;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};

const FAILURE_RETRY: Duration = Duration::from_secs(60);

pub(crate) struct SharedWatchProvider {
    source: Arc<dyn WatchMarketProvider>,
    store: Arc<PublicDividends>,
    locks: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
    failures: Mutex<BTreeMap<(String, NaiveDate), Instant>>,
}

impl SharedWatchProvider {
    pub fn new(source: Arc<dyn WatchMarketProvider>, store: Arc<PublicDividends>) -> Self {
        Self {
            source,
            store,
            locks: Mutex::new(BTreeMap::new()),
            failures: Mutex::new(BTreeMap::new()),
        }
    }

    async fn symbol_lock(&self, symbol: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(symbol).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(symbol.into(), Arc::downgrade(&lock));
        lock
    }
}

#[async_trait::async_trait]
impl WatchMarketProvider for SharedWatchProvider {
    fn dividend_seed(&self, symbol: &str, day: NaiveDate) -> Option<DividendHistory> {
        match self.store.get(symbol) {
            Ok(history) => history.filter(|h| h.basis.as_of <= day),
            Err(error) => {
                tracing::warn!(symbol, %error, "public dividend lookup failed");
                None
            }
        }
    }

    async fn dividends(&self, symbol: &str, day: NaiveDate) -> AppResult<DividendHistory> {
        let lock = self.symbol_lock(symbol).await;
        let _guard = lock.lock().await;
        if let Some(history) = self.store.get(symbol)?.filter(|h| h.basis.as_of == day) {
            return Ok(history);
        }
        let key = (symbol.to_owned(), day);
        {
            let mut failures = self.failures.lock().await;
            failures.retain(|_, time| time.elapsed() < FAILURE_RETRY);
            if failures.contains_key(&key) {
                return Err(AppError::MarketData(
                    "分红来源暂不可用，请稍后重新检查".into(),
                ));
            }
        }
        let result = self.source.dividends(symbol, day).await;
        match result {
            Ok(history) => {
                if history.basis.as_of != day {
                    return Err(AppError::MarketData("分红来源覆盖日期与请求不符".into()));
                }
                self.store.save(symbol, &history)?;
                Ok(history)
            }
            Err(error) => {
                self.failures.lock().await.insert(key, Instant::now());
                Err(error)
            }
        }
    }

    async fn observation(&self, symbol: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
        let lock = self.symbol_lock(&format!("prices:{symbol}")).await;
        let _guard = lock.lock().await;
        self.source.observation(symbol, day).await
    }

    async fn invalidate(&self) {
        self.failures.lock().await.clear();
        self.source.invalidate().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::Database,
        models::watch::{tests::market_fixture, DividendBasis, WatchInput},
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Source {
        calls: AtomicUsize,
        fail: AtomicBool,
        price_fail: AtomicBool,
    }
    fn source() -> Arc<Source> {
        Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
            price_fail: AtomicBool::new(false),
        })
    }
    fn history(day: NaiveDate) -> DividendHistory {
        let (mut data, end) = market_fixture();
        let mut events = data.dividends.take().unwrap();
        for event in &mut events {
            event.ex_date += day - end;
            event.report_date = Some(event.ex_date - chrono::Duration::days(180));
        }
        DividendHistory {
            basis: DividendBasis {
                as_of: day,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            },
            sources: vec!["https://example.test/dividends".into()],
            events,
        }
    }
    #[async_trait::async_trait]
    impl WatchMarketProvider for Source {
        async fn dividends(&self, _: &str, day: NaiveDate) -> AppResult<DividendHistory> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            if self.fail.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("测试来源失败".into()));
            }
            Ok(history(day))
        }
        async fn observation(&self, _: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
            if self.price_fail.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("测试行情失败".into()));
            }
            let (mut data, end) = market_fixture();
            for bar in &mut data.bars {
                bar.day += day - end;
            }
            for date in &mut data.calendar {
                *date += day - end;
            }
            data.dividends = None;
            Ok(data)
        }
    }
    fn watch(db: &Database, symbol: &str) {
        db.save_watch(WatchInput {
            symbol: symbol.into(),
            name: "公司".into(),
            thesis: String::new(),
        })
        .unwrap();
    }

    #[tokio::test]
    async fn concurrent_accounts_share_one_fetch_and_restarted_provider_reads_persisted_data() {
        let day = super::super::watch::cutoff();
        let source = source();
        let store = Arc::new(PublicDividends::test());
        let provider = Arc::new(SharedWatchProvider::new(source.clone(), store.clone()));
        let (a, b) = tokio::join!(
            provider.dividends("600036.SS", day),
            provider.dividends("600036.SS", day)
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        let first = Database::test();
        let second = Database::test();
        for db in [&first, &second] {
            watch(db, "600036.SS");
            let _lock = db.lock_account().unwrap();
            let board = super::super::watch::refresh(db, provider.clone(), false)
                .await
                .unwrap();
            assert_eq!(board.rows[0].latest.as_ref().unwrap().yield_pct, Some(5.0));
        }
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        first.delete_watch("600036.SS").unwrap();
        assert!(second.watch_dividends("600036.SS").unwrap().is_some());
        let restarted = SharedWatchProvider::new(source.clone(), store.clone());
        restarted.dividends("600036.SS", day).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert!(store.get("600036.SS").unwrap().is_some());
        assert!(provider
            .locks
            .lock()
            .await
            .values()
            .all(|lock| lock.strong_count() == 0));
    }

    #[tokio::test]
    async fn failures_preserve_old_facts_and_success_persists_even_without_prices() {
        let day = super::super::watch::cutoff();
        let source = source();
        source.fail.store(true, Ordering::SeqCst);
        let store = Arc::new(PublicDividends::test());
        let old = history(day - chrono::Duration::days(1));
        store.save("600036.SS", &old).unwrap();
        let provider = Arc::new(SharedWatchProvider::new(source.clone(), store.clone()));
        assert!(provider.dividends("600036.SS", day).await.is_err());
        assert!(provider.dividends("600036.SS", day).await.is_err());
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert_eq!(store.get("600036.SS").unwrap(), Some(old));
        provider.invalidate().await;
        source.fail.store(false, Ordering::SeqCst);
        provider.dividends("600036.SS", day).await.unwrap();
        source.price_fail.store(true, Ordering::SeqCst);
        let db = Database::test();
        watch(&db, "600000.SS");
        let _lock = db.lock_account().unwrap();
        let board = super::super::watch::refresh(&db, provider.clone(), false)
            .await
            .unwrap();
        assert!(board.rows[0].refresh_failed);
        assert!(board.rows[0].latest.is_none());
        assert!(store.get("600000.SS").unwrap().is_some());
        assert!(db.watch_dividends("600000.SS").unwrap().is_some());
        source.price_fail.store(false, Ordering::SeqCst);
        let interrupted = Database::test();
        watch(&interrupted, "600001.SS");
        crate::db::fail_inserts(
            &interrupted.conn().unwrap(),
            "watch_observations",
            "injected account failure",
        );
        let _interrupted_lock = interrupted.lock_account().unwrap();
        assert!(super::super::watch::refresh(&interrupted, provider, false)
            .await
            .is_err());
        assert!(store.get("600001.SS").unwrap().is_some());
        assert!(interrupted.watch_dividends("600001.SS").unwrap().is_none());
    }

    #[tokio::test]
    async fn newer_public_facts_refresh_an_account_already_checked_today() {
        let day = super::super::watch::cutoff();
        let source = source();
        let store = Arc::new(PublicDividends::test());
        let mut facts = history(day);
        store.save("600036.SS", &facts).unwrap();
        let provider = Arc::new(SharedWatchProvider::new(source.clone(), store.clone()));
        let db = Database::test();
        watch(&db, "600036.SS");
        let _lock = db.lock_account().unwrap();
        super::super::watch::refresh(&db, provider.clone(), false)
            .await
            .unwrap();
        facts.basis.fetched_at = (chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339();
        facts.events[0].cash_per_ten = "60".into();
        store.save("600036.SS", &facts).unwrap();
        let board = super::super::watch::refresh(&db, provider.clone(), false)
            .await
            .unwrap();
        assert_eq!(board.rows[0].latest.as_ref().unwrap().yield_pct, Some(6.0));
        assert_eq!(db.watch_dividends("600036.SS").unwrap(), Some(facts));
        provider.invalidate().await;
        super::super::watch::refresh(&db, provider, true)
            .await
            .unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    }
}
