//! Shared public catalogue checks. Personal watch history and alerts stay account-owned.
use crate::{
    db::PublicWatch,
    models::watch::{
        catalog::{Catalog, CatalogBoard, CatalogRow, CatalogSnapshot},
        evaluate, WatchStatus,
    },
    ports::market::WatchMarketProvider,
    AppResult,
};
use chrono::{Datelike, NaiveDate, Utc};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub(crate) struct WatchCatalogService {
    store: Arc<PublicWatch>,
    provider: Arc<dyn WatchMarketProvider>,
    locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl WatchCatalogService {
    pub fn new(store: Arc<PublicWatch>, provider: Arc<dyn WatchMarketProvider>) -> Self {
        Self {
            store,
            provider,
            locks: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn save(&self, catalog: &Catalog, revision: i64, user_id: &str) -> AppResult<CatalogBoard> {
        self.store.save_catalog(catalog, revision, user_id)?;
        self.board()
    }

    pub fn board(&self) -> AppResult<CatalogBoard> {
        let (catalog, revision) = self.store.catalog()?;
        let mut rows = Vec::new();
        for industry in &catalog.industries {
            for company in &industry.companies {
                rows.push(CatalogRow::new(
                    company.clone(),
                    industry.id.clone(),
                    self.store.get(&company.symbol)?,
                ));
            }
        }
        Ok(CatalogBoard::new(catalog, rows, revision))
    }

    pub async fn refresh(
        self: &Arc<Self>,
        industry: Option<&str>,
        force: bool,
    ) -> AppResult<CatalogBoard> {
        self.refresh_on(industry, force, super::watch::cutoff())
            .await
    }

    async fn refresh_on(
        self: &Arc<Self>,
        industry: Option<&str>,
        force: bool,
        day: NaiveDate,
    ) -> AppResult<CatalogBoard> {
        let (catalog, _) = self.store.catalog()?;
        let companies: Vec<_> = match industry {
            Some(id) => catalog.industry(id)?.companies.iter().collect(),
            None => catalog
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .collect(),
        };
        if force {
            self.provider.invalidate().await;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let mut work = tokio::task::JoinSet::new();
        for company in companies {
            let service = self.clone();
            let symbol = company.symbol.clone();
            work.spawn(async move {
                match tokio::time::timeout_at(
                    deadline,
                    service.check(&symbol, day, force, deadline),
                )
                .await
                {
                    Ok(result) => result,
                    Err(error) => {
                        tracing::warn!(symbol, %error, "public catalogue refresh deadline reached");
                        Ok(())
                    }
                }
            });
        }
        while let Some(result) = work.join_next().await {
            result.map_err(|error| crate::AppError::MarketData(error.to_string()))??;
        }
        self.board()
    }

    async fn check(
        &self,
        symbol: &str,
        day: NaiveDate,
        force: bool,
        deadline: tokio::time::Instant,
    ) -> AppResult<()> {
        let before = self.store.get(symbol)?;
        let lock = self
            .locks
            .lock()
            .await
            .entry(symbol.into())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let previous = self.store.get(symbol)?;
        // A concurrent caller already refreshed this company while we were waiting.
        if previous.checked_at != before.checked_at {
            return Ok(());
        }
        if !force && !needs_check(&previous, day) {
            return Ok(());
        }
        let checked_at = Utc::now().to_rfc3339();
        let cached = self.provider.dividend_seed(symbol, day);
        let network = async {
            tokio::join!(self.provider.observation(symbol, day), async {
                if let Some(history) = cached.as_ref().filter(|h| h.basis.as_of >= day) {
                    return Ok(history.clone());
                }
                self.provider.dividends(symbol, day).await
            })
        };
        // Finish before the round's deadline so timeout state can be persisted.
        let (market, dividends) =
            tokio::time::timeout_at(deadline - Duration::from_millis(50), network)
                .await
                .unwrap_or_else(|_| {
                    (
                        Err(crate::AppError::MarketData("本次收盘检查超时".into())),
                        Err(crate::AppError::MarketData("分红检查超时".into())),
                    )
                });
        let dividend_failed = dividends.is_err();
        if let Err(error) = &dividends {
            tracing::warn!(symbol, %error, "public catalogue dividend check failed; retaining dated basis");
        }
        let latest = match market {
            Ok(mut data) => {
                if let Some(history) = dividends.ok().or(cached) {
                    data.dividends = Some(history.events);
                    data.dividend_basis = Some(history.basis);
                    data.sources.extend(history.sources);
                }
                let observation = evaluate(symbol, &data, day, checked_at.clone());
                if previous
                    .latest
                    .as_ref()
                    .is_some_and(|old| old.observed_on > observation.observed_on)
                {
                    previous.latest
                } else {
                    Some(observation)
                }
            }
            Err(error) => {
                tracing::warn!(symbol, %error, "public catalogue price check failed; retaining dated result");
                self.store.save(
                    symbol,
                    &CatalogSnapshot {
                        latest: previous.latest,
                        checked_at: Some(checked_at),
                        checked_for: Some(day.to_string()),
                        refresh_failed: true,
                    },
                )?;
                return Ok(());
            }
        };
        self.store.save(
            symbol,
            &CatalogSnapshot {
                latest,
                checked_at: Some(checked_at),
                checked_for: Some(day.to_string()),
                refresh_failed: dividend_failed,
            },
        )
    }
}

fn needs_check(snapshot: &CatalogSnapshot, day: NaiveDate) -> bool {
    snapshot.refresh_failed
        || snapshot.checked_for.as_deref() != Some(day.to_string().as_str())
        || snapshot.latest.as_ref().is_none_or(|o| {
            o.status == WatchStatus::Incomplete
                || (day.weekday().num_days_from_monday() < 5 && o.observed_on < day.to_string())
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        db::Database,
        models::watch::{
            tests::market_fixture, DividendBasis, DividendHistory, WatchInput, WatchMarketData,
        },
        AppError,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Source {
        prices: AtomicUsize,
        dividends: AtomicUsize,
        fail_price: AtomicBool,
        fail_dividend: AtomicBool,
        lagging: AtomicBool,
        delay: Duration,
    }

    impl Source {
        fn new() -> Self {
            Self {
                prices: AtomicUsize::new(0),
                dividends: AtomicUsize::new(0),
                fail_price: AtomicBool::new(false),
                fail_dividend: AtomicBool::new(false),
                lagging: AtomicBool::new(false),
                delay: Duration::from_millis(10),
            }
        }
    }

    #[async_trait::async_trait]
    impl WatchMarketProvider for Source {
        async fn observation(&self, symbol: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
            self.prices.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            if symbol == "600036.SS" && self.fail_price.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("测试价格失败".into()));
            }
            let (mut data, end) = market_fixture();
            let shift =
                day - end - chrono::Duration::days(i64::from(self.lagging.load(Ordering::SeqCst)));
            for date in &mut data.calendar {
                *date += shift;
            }
            for bar in &mut data.bars {
                bar.day += shift;
            }
            data.dividends = None;
            data.dividend_basis = None;
            Ok(data)
        }
        async fn dividends(&self, _: &str, day: NaiveDate) -> AppResult<DividendHistory> {
            self.dividends.fetch_add(1, Ordering::SeqCst);
            if self.fail_dividend.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("测试分红失败".into()));
            }
            let (mut data, end) = market_fixture();
            let mut events = data.dividends.take().unwrap();
            for event in &mut events {
                event.ex_date += day - end;
                event.report_date = Some(event.ex_date - chrono::Duration::days(100));
            }
            Ok(DividendHistory {
                basis: DividendBasis {
                    as_of: day,
                    fetched_at: Utc::now().to_rfc3339(),
                },
                sources: vec!["https://example.test/dividends".into()],
                events,
            })
        }
    }

    fn service(source: Arc<Source>) -> Arc<WatchCatalogService> {
        Arc::new(WatchCatalogService::new(
            Arc::new(PublicWatch::test()),
            source,
        ))
    }

    pub(crate) fn fixture_service() -> Arc<WatchCatalogService> {
        service(Arc::new(Source::new()))
    }

    #[tokio::test]
    async fn maintenance_changes_the_live_pool_and_inflight_removal_does_not_restore_membership() {
        let source = Arc::new(Source::new());
        let service = service(source.clone());
        let board = service.board().unwrap();
        let mut catalog = board.catalog;
        let mut company = catalog.industries[0].companies[0].clone();
        company.symbol = "600000.SS".into();
        company.name = "浦发银行".into();
        catalog.industries[0].companies = vec![company];
        let added = service.save(&catalog, board.revision, "owner").unwrap();
        let checking = service.clone();
        let day = market_fixture().1;
        let task = tokio::spawn(async move { checking.refresh_on(Some("banks"), true, day).await });
        while source.prices.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        catalog.industries[0].companies.clear();
        service.save(&catalog, added.revision, "owner").unwrap();
        let finished = task.await.unwrap().unwrap();
        assert!(finished.catalog.industries[0].companies.is_empty());
        assert!(!finished
            .rows
            .iter()
            .any(|row| row.company.symbol == "600000.SS"));
        assert!(service.store.get("600000.SS").unwrap().latest.is_some());
        service.refresh_on(Some("banks"), true, day).await.unwrap();
        assert_eq!(source.prices.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn public_reads_and_refreshes_preserve_private_accounts_and_use_the_same_rules() {
        let first = Database::test();
        let second = Database::test();
        first
            .save_watch(WatchInput {
                symbol: "600036.SS".into(),
                name: "个人公司名".into(),
                thesis: "私有理由".into(),
            })
            .unwrap();
        first.set_watch_pinned("600036.SS", true).unwrap();
        let version = first.revision().unwrap();
        let source = Arc::new(Source::new());
        let service = service(source.clone());
        assert!(service
            .board()
            .unwrap()
            .rows
            .iter()
            .all(|r| r.snapshot.latest.is_none()));
        assert_eq!(source.prices.load(Ordering::SeqCst), 0);
        let day = market_fixture().1;
        let board = service.refresh_on(Some("banks"), false, day).await.unwrap();
        assert_eq!(source.prices.load(Ordering::SeqCst), 3);
        for row in &board.rows[..3] {
            let observation = row.snapshot.latest.as_ref().unwrap();
            assert_eq!(observation.status, WatchStatus::Attention);
            assert_eq!(observation.yield_pct, Some(5.0));
            assert_eq!(observation.observed_on, day.to_string());
            assert_eq!(
                row.dividend_prices
                    .iter()
                    .find(|p| p.yield_pct == 5)
                    .unwrap()
                    .price,
                "100.00"
            );
            assert_eq!(observation.sources.len(), 2);
        }
        assert!(board.rows[3..].iter().all(|r| r.snapshot.latest.is_none()));
        let serialized = serde_json::to_value(board).unwrap();
        for row in serialized["rows"].as_array().unwrap() {
            for key in ["entry", "thesis", "alert", "pinned", "gridPlan", "trades"] {
                assert!(row.get(key).is_none());
            }
        }
        assert_eq!(first.revision().unwrap(), version);
        let personal = first.watch_board(day).unwrap();
        assert!(personal.rows[0].latest.is_none());
        assert!(personal.weekly.is_empty());
        assert_eq!(personal.rows[0].entry.input.thesis, "私有理由");
        assert!(personal.rows[0].entry.pinned);
        assert!(second.watch_entries().unwrap().is_empty());
        service.refresh_on(Some("banks"), false, day).await.unwrap();
        assert_eq!(source.prices.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn concurrent_manual_rounds_coalesce_and_one_failure_retains_its_previous_date() {
        let source = Arc::new(Source::new());
        let service = service(source.clone());
        let day = market_fixture().1;
        let (a, b) = tokio::join!(
            service.refresh_on(Some("banks"), true, day),
            service.refresh_on(Some("banks"), true, day)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(source.prices.load(Ordering::SeqCst), 3);
        source.fail_price.store(true, Ordering::SeqCst);
        let next = day + chrono::Duration::days(3);
        let failed = service.refresh_on(Some("banks"), true, next).await.unwrap();
        assert!(failed.rows[0].snapshot.refresh_failed);
        assert_eq!(
            failed.rows[0].snapshot.latest.as_ref().unwrap().observed_on,
            day.to_string()
        );
        assert_eq!(
            failed.rows[1].snapshot.latest.as_ref().unwrap().observed_on,
            next.to_string()
        );
        source.fail_price.store(false, Ordering::SeqCst);
        let retried = service
            .refresh_on(Some("banks"), false, next)
            .await
            .unwrap();
        assert!(!retried.rows[0].snapshot.refresh_failed);
        assert_eq!(
            retried.rows[0]
                .snapshot
                .latest
                .as_ref()
                .unwrap()
                .observed_on,
            next.to_string()
        );
        assert_eq!(source.prices.load(Ordering::SeqCst), 7);
    }

    #[tokio::test]
    async fn unknown_dividends_and_lagging_trading_dates_are_not_upgraded_to_complete() {
        let source = Arc::new(Source::new());
        source.fail_dividend.store(true, Ordering::SeqCst);
        let service = service(source.clone());
        let day = market_fixture().1;
        let board = service.refresh_on(Some("banks"), false, day).await.unwrap();
        let row = &board.rows[0];
        assert_eq!(row.snapshot.latest.as_ref().unwrap().price, Some(100.0));
        assert!(row.snapshot.latest.as_ref().unwrap().daily.is_some());
        assert!(row.snapshot.latest.as_ref().unwrap().yield_pct.is_none());
        assert!(row.dividend_prices.is_empty());
        source.fail_dividend.store(false, Ordering::SeqCst);
        source.lagging.store(true, Ordering::SeqCst);
        let lagging = self::service(source.clone());
        lagging.refresh_on(Some("banks"), false, day).await.unwrap();
        let board = lagging.refresh_on(Some("banks"), false, day).await.unwrap();
        assert_eq!(source.prices.load(Ordering::SeqCst), 9);
        assert!(board.rows[0].snapshot.latest.as_ref().unwrap().observed_on < day.to_string());
        assert!(needs_check(&board.rows[0].snapshot, day));
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_keeps_unknown_results_retryable_and_unknown_industries_do_no_work() {
        let mut source = Source::new();
        source.delay = Duration::from_secs(300);
        let source = Arc::new(source);
        let service = service(source.clone());
        let day = market_fixture().1;
        assert!(service
            .refresh_on(Some("unknown"), true, day)
            .await
            .is_err());
        assert_eq!(source.prices.load(Ordering::SeqCst), 0);
        let start = tokio::time::Instant::now();
        let board = service.refresh_on(Some("banks"), false, day).await.unwrap();
        assert!(start.elapsed() <= Duration::from_secs(120));
        assert!(board.rows[..3]
            .iter()
            .all(|r| r.snapshot.latest.is_none() && r.snapshot.refresh_failed));
    }
}
