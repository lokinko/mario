//! Market-owned public dividend-plan acquisition and cache reads.
use crate::{
    db::PublicDividendPlans,
    models::market::dividend_plans::PlanSnapshot,
    ports::market::{DividendPlanProvider, DividendPlanReader},
    AppError, AppResult,
};
use chrono::{NaiveDate, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::Mutex;
pub(crate) struct PublicDividendPlanService {
    store: Arc<PublicDividendPlans>,
    provider: Arc<dyn DividendPlanProvider>,
    locks: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
}
impl PublicDividendPlanService {
    pub fn new(store: Arc<PublicDividendPlans>, provider: Arc<dyn DividendPlanProvider>) -> Self {
        Self {
            store,
            provider,
            locks: Mutex::new(BTreeMap::new()),
        }
    }
    pub async fn refresh(self: &Arc<Self>, symbols: &[String]) -> AppResult<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let day = Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let mut tasks = tokio::task::JoinSet::new();
        let symbols = symbols
            .iter()
            .map(|symbol| crate::models::market::canonical_symbol(symbol))
            .collect::<AppResult<std::collections::BTreeSet<_>>>()?;
        for symbol in symbols {
            let service = self.clone();
            tasks.spawn(async move { service.check(&symbol, day, deadline).await });
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
        if previous.attempted_at != before.attempted_at || fresh(&previous) {
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
    // Persisted attempts throttle source requests, including failures and restarts.
    let ttl = 24 * 60 * 60;
    snapshot
        .attempted_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|date| (Utc::now() - date.with_timezone(&Utc)).num_seconds() < ttl)
}
impl DividendPlanReader for PublicDividendPlanService {
    fn snapshot(&self, symbol: &str) -> AppResult<PlanSnapshot> {
        self.store
            .calendar_snapshot(&crate::models::market::canonical_symbol(symbol)?)
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::application::dividend_calendar::today;
    use crate::models::market::dividend_plans::{tests::plan, DividendPlan, PlanStage};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    pub(crate) struct Source {
        pub(crate) calls: AtomicUsize,
        pub(crate) fail: AtomicBool,
    }
    #[async_trait::async_trait]
    impl DividendPlanProvider for Source {
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
    }
    #[tokio::test]
    async fn fetches_wait_a_day_after_success_or_failure_across_restarts() {
        let store = Arc::new(PublicDividendPlans::test());
        let source = Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
        });
        for (symbol, hours, failed) in [
            ("600000.SS", 23, false),
            ("600001.SS", 23, true),
            ("600002.SS", 25, false),
            ("600003.SS", 25, true),
        ] {
            let attempted = (Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();
            store
                .save_calendar_snapshot(
                    symbol,
                    &PlanSnapshot {
                        plans: vec![plan(symbol, PlanStage::Scheduled)],
                        attempted_at: Some(attempted.clone()),
                        fetched_at: (!failed).then_some(attempted),
                        refresh_failed: failed,
                    },
                )
                .unwrap();
        }
        // A new service must respect the persisted attempt times, without any warm memory cache.
        let service = Arc::new(PublicDividendPlanService::new(
            store.clone(),
            source.clone(),
        ));
        let recent = vec!["600000.SS".into(), "600001.SS".into()];
        service.refresh(&recent).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
        let expired = vec!["600002.SS".into(), "600003.SS".into()];
        service.refresh(&expired).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 2);
        service.refresh(&expired).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 2);
        let restarted = Arc::new(PublicDividendPlanService::new(store, source.clone()));
        restarted.refresh(&recent).await.unwrap();
        restarted.refresh(&expired).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 2);
        let new_company = vec!["600004.SS".into()];
        source.fail.store(true, Ordering::SeqCst);
        restarted.refresh(&new_company).await.unwrap();
        restarted.refresh(&new_company).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 3);
    }
    #[tokio::test]
    async fn aliases_share_one_public_plan_cache_and_reads_never_fetch() {
        let source = Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
        });
        let store = Arc::new(PublicDividendPlans::test());
        let service = Arc::new(PublicDividendPlanService::new(
            store.clone(),
            source.clone(),
        ));
        let symbols = vec!["600000.SH".into(), "600000.SS".into()];
        assert!(service.snapshot("600000.SH").unwrap().plans.is_empty());
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
        let (first, second) = tokio::join!(service.refresh(&symbols), service.refresh(&symbols));
        first.unwrap();
        second.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            service.snapshot("600000.SH").unwrap().plans[0].symbol,
            "600000.SS"
        );
        let restarted = Arc::new(PublicDividendPlanService::new(store, source.clone()));
        restarted.refresh(&symbols).await.unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    }
}
