//! Public plan fetching and account-scoped calendar projection; no model execution or ledger writes.
use crate::{
    db::PublicWatch,
    models::watch::{
        calendar::{companies, CalendarBoard, CalendarCompany, CalendarScope, PlanSnapshot},
        WatchInput,
    },
    ports::market::WatchMarketProvider,
    AppError, AppResult,
};
use chrono::{NaiveDate, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::Mutex;

pub(crate) struct DividendCalendarService {
    store: Arc<PublicWatch>,
    provider: Arc<dyn WatchMarketProvider>,
    locks: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
}
pub(crate) fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive()
}
impl DividendCalendarService {
    pub fn new(store: Arc<PublicWatch>, provider: Arc<dyn WatchMarketProvider>) -> Self {
        Self {
            store,
            provider,
            locks: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn companies(
        &self,
        personal: &[WatchInput],
        scope: CalendarScope,
    ) -> AppResult<Vec<CalendarCompany>> {
        Ok(companies(&self.store.catalog()?.0, personal, scope))
    }
    pub fn board(
        &self,
        personal: &[WatchInput],
        scope: CalendarScope,
        start: NaiveDate,
    ) -> AppResult<CalendarBoard> {
        let mut snapshots = Vec::new();
        for company in self.companies(personal, scope)? {
            let snapshot = self.store.calendar_snapshot(&company.symbol)?;
            snapshots.push((company, snapshot));
        }
        CalendarBoard::build(start, today(), scope, snapshots)
    }
    pub async fn refresh_public(self: &Arc<Self>) -> AppResult<()> {
        let symbols = self
            .companies(&[], CalendarScope::Public)?
            .into_iter()
            .map(|c| c.symbol)
            .collect::<Vec<_>>();
        self.refresh(&symbols, false).await
    }
    pub async fn refresh(self: &Arc<Self>, symbols: &[String], force: bool) -> AppResult<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let day = today();
        let mut tasks = tokio::task::JoinSet::new();
        for symbol in symbols {
            let service = self.clone();
            let symbol = symbol.clone();
            tasks.spawn(async move { service.check(&symbol, day, force, deadline).await });
        }
        while let Some(result) = tasks.join_next().await {
            result.map_err(|error| AppError::MarketData(error.to_string()))??;
        }
        Ok(())
    }
    async fn check(
        &self,
        symbol: &str,
        day: NaiveDate,
        force: bool,
        deadline: tokio::time::Instant,
    ) -> AppResult<()> {
        let before = self.store.calendar_snapshot(symbol)?;
        let lock = {
            let mut locks = self.locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(symbol).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(symbol.into(), Arc::downgrade(&lock));
                lock
            }
        };
        let Ok(_guard) = tokio::time::timeout_at(deadline, lock.lock()).await else {
            return Ok(());
        };
        let previous = self.store.calendar_snapshot(symbol)?;
        if previous.attempted_at != before.attempted_at || (!force && fresh(&previous)) {
            return Ok(());
        }
        let attempted_at = Some(Utc::now().to_rfc3339());
        let result =
            tokio::time::timeout_at(deadline, self.provider.dividend_plans(symbol, day)).await;
        let snapshot = match result {
            Ok(Ok(plans)) if plans.iter().all(|plan| plan.symbol == symbol) => PlanSnapshot {
                plans,
                attempted_at,
                fetched_at: Some(Utc::now().to_rfc3339()),
                refresh_failed: false,
            },
            other => {
                tracing::warn!(symbol, result=?other, "dividend calendar source failed; retaining last successful plans");
                PlanSnapshot {
                    attempted_at,
                    refresh_failed: true,
                    ..previous
                }
            }
        };
        self.store.save_calendar_snapshot(symbol, &snapshot)
    }
}
fn fresh(snapshot: &PlanSnapshot) -> bool {
    let ttl = if snapshot.refresh_failed { 60 } else { 3600 };
    snapshot
        .attempted_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|date| (Utc::now() - date.with_timezone(&Utc)).num_seconds() < ttl)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::watch::calendar::{tests::plan, DividendPlan, PlanStage};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    struct Source {
        calls: AtomicUsize,
        fail: AtomicBool,
    }
    #[async_trait::async_trait]
    impl WatchMarketProvider for Source {
        async fn dividend_plans(&self, symbol: &str, _: NaiveDate) -> AppResult<Vec<DividendPlan>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            if self.fail.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("测试来源失败".into()));
            }
            let mut p = plan(symbol, PlanStage::Scheduled);
            p.announced_on = today();
            p.record_date = Some(today() + chrono::Duration::days(2));
            p.ex_date = Some(today() + chrono::Duration::days(3));
            Ok(vec![p])
        }
        async fn dividends(
            &self,
            _: &str,
            _: NaiveDate,
        ) -> AppResult<crate::models::watch::DividendHistory> {
            unreachable!()
        }
        async fn observation(
            &self,
            _: &str,
            _: NaiveDate,
        ) -> AppResult<crate::models::watch::WatchMarketData> {
            unreachable!()
        }
    }
    pub(crate) fn fixture_service() -> Arc<DividendCalendarService> {
        Arc::new(DividendCalendarService::new(
            Arc::new(PublicWatch::test()),
            Arc::new(Source {
                calls: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
            }),
        ))
    }
    #[tokio::test]
    async fn scoped_reads_do_no_network_and_failures_keep_prior_plans_without_advancing_accounts() {
        let store = Arc::new(PublicWatch::test());
        let source = Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
        });
        let service = Arc::new(DividendCalendarService::new(store.clone(), source.clone()));
        let a = crate::db::Database::test();
        let b = crate::db::Database::test();
        let input = WatchInput {
            symbol: "600000.SS".into(),
            name: "私有自选名称".into(),
            thesis: "私有理由".into(),
        };
        a.save_watch(input.clone()).unwrap();
        let revision = a.revision().unwrap();
        assert!(service
            .board(
                std::slice::from_ref(&input),
                CalendarScope::Favorites,
                today()
            )
            .unwrap()
            .entries
            .is_empty());
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
        let symbols = vec![input.symbol.clone()];
        let (x, y) = tokio::join!(
            service.refresh(&symbols, true),
            service.refresh(&symbols, true)
        );
        x.unwrap();
        y.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            service
                .board(
                    std::slice::from_ref(&input),
                    CalendarScope::Favorites,
                    today()
                )
                .unwrap()
                .entries
                .len(),
            1
        );
        assert!(service
            .board(&[], CalendarScope::Favorites, today())
            .unwrap()
            .entries
            .is_empty());
        source.fail.store(true, Ordering::SeqCst);
        service.refresh(&symbols, true).await.unwrap();
        let board = service
            .board(&[input], CalendarScope::Favorites, today())
            .unwrap();
        assert_eq!(board.entries.len(), 1);
        assert!(board.coverage[0].refresh_failed);
        assert_eq!(a.revision().unwrap(), revision);
        assert!(b.watch_entries().unwrap().is_empty());
    }
}
