//! User-owned company selection and calendar projection using cached public plans only.
use crate::{
    db::PublicWatch,
    models::watch::{
        calendar::{companies, CalendarBoard, CalendarCompany, CalendarScope},
        WatchInput,
    },
    ports::market::DividendPlanReader,
    AppResult,
};
use chrono::{NaiveDate, Utc};
use std::sync::Arc;
pub(crate) struct DividendCalendarService {
    store: Arc<PublicWatch>,
    plans: Arc<dyn DividendPlanReader>,
}
pub(crate) fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive()
}
impl DividendCalendarService {
    pub fn new(store: Arc<PublicWatch>, plans: Arc<dyn DividendPlanReader>) -> Self {
        Self { store, plans }
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
            let snapshot = self.plans.snapshot(&company.symbol)?;
            snapshots.push((company, snapshot));
        }
        CalendarBoard::build(start, today(), scope, snapshots)
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        application::dividend_plans::{tests::Source, PublicDividendPlanService},
        db::PublicDividendPlans,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    pub(crate) fn fixture_services(
    ) -> (Arc<DividendCalendarService>, Arc<PublicDividendPlanService>) {
        let plans = Arc::new(PublicDividendPlanService::new(
            Arc::new(PublicDividendPlans::test()),
            Arc::new(Source {
                calls: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
            }),
        ));
        (
            Arc::new(DividendCalendarService::new(
                Arc::new(PublicWatch::test()),
                plans.clone(),
            )),
            plans,
        )
    }
    pub(crate) fn fixture_service() -> Arc<DividendCalendarService> {
        fixture_services().0
    }
    #[tokio::test]
    async fn scoped_reads_do_no_network_and_failures_keep_prior_plans_without_advancing_accounts() {
        let store = Arc::new(PublicDividendPlans::test());
        let source = Arc::new(Source {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
        });
        let plans = Arc::new(PublicDividendPlanService::new(
            store.clone(),
            source.clone(),
        ));
        let service = Arc::new(DividendCalendarService::new(
            Arc::new(PublicWatch::test()),
            plans.clone(),
        ));
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
        let (x, y) = tokio::join!(plans.refresh(&symbols), plans.refresh(&symbols));
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
        let stale_input = WatchInput {
            symbol: "600001.SS".into(),
            ..input.clone()
        };
        let mut stale = store.calendar_snapshot(&input.symbol).unwrap();
        stale.attempted_at = Some((Utc::now() - chrono::Duration::hours(25)).to_rfc3339());
        for plan in &mut stale.plans {
            plan.symbol = stale_input.symbol.clone();
        }
        store
            .save_calendar_snapshot(&stale_input.symbol, &stale)
            .unwrap();
        source.fail.store(true, Ordering::SeqCst);
        plans
            .refresh(std::slice::from_ref(&stale_input.symbol))
            .await
            .unwrap();
        assert_eq!(source.calls.load(Ordering::SeqCst), 2);
        let board = service
            .board(&[stale_input], CalendarScope::Favorites, today())
            .unwrap();
        assert_eq!(board.entries.len(), 1);
        assert!(board.coverage[0].refresh_failed);
        assert_eq!(a.revision().unwrap(), revision);
        assert!(b.watch_entries().unwrap().is_empty());
    }
}
