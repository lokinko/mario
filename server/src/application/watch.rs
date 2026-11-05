//! Account-scoped observation orchestration; financial records remain read-only.
use crate::{
    db::Database,
    models::watch::{evaluate, WatchBoard},
    ports::market::WatchMarketProvider,
    AppResult,
};
use chrono::{Datelike, NaiveDate, Timelike, Utc};
use std::sync::Arc;

pub fn cutoff() -> NaiveDate {
    let now = Utc::now().with_timezone(&chrono_tz::Asia::Shanghai);
    if now.hour() >= 19 {
        now.date_naive()
    } else {
        now.date_naive() - chrono::Duration::days(1)
    }
}

/// Caller holds one account lock; release it around public network I/O. Recheck
/// each entry before committing so a deleted/edited watch cannot be resurrected.
pub async fn refresh(
    db: &Database,
    provider: Arc<dyn WatchMarketProvider>,
    force: bool,
) -> AppResult<WatchBoard> {
    let day = cutoff();
    let board = db.watch_board(day)?;
    let mut entries = Vec::new();
    for row in board.rows {
        let history = db.watch_dividends(&row.entry.input.symbol)?;
        if force
            || history.as_ref().is_none_or(|h| h.basis.as_of < day)
            || row.refresh_failed
            || row.checked_for.as_deref() != Some(day.to_string().as_str())
            || row.latest.as_ref().is_some_and(|o| {
                o.dividend_basis.is_none()
                    || o.status == crate::models::watch::WatchStatus::Incomplete
                    || (day.weekday().num_days_from_monday() < 5
                        && o.observed_on < day.to_string()
                        && Utc::now().with_timezone(&chrono_tz::Asia::Shanghai).hour() >= 19)
            })
        {
            entries.push((row.entry, history));
        }
    }
    if entries.is_empty() {
        return db.watch_board(
            Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive(),
        );
    }
    let results = db
        .without_account_lock(async {
            if force {
                provider.invalidate().await;
            }
            let mut work = tokio::task::JoinSet::new();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
            for (entry, cached) in entries {
                let provider = provider.clone();
                work.spawn(async move {
                    let checked_at = Utc::now().to_rfc3339();
                    let (market, dividends) = tokio::join!(
                        tokio::time::timeout_at(
                            deadline,
                            provider.observation(&entry.input.symbol, day)
                        ),
                        async {
                            if !force && cached.as_ref().is_some_and(|h| h.basis.as_of >= day) {
                                return cached.clone();
                            }
                            match tokio::time::timeout_at(
                                deadline,
                                provider.dividends(&entry.input.symbol, day),
                            )
                            .await
                            {
                                Ok(Ok(history)) => Some(history),
                                _ => cached,
                            }
                        }
                    );
                    let result = market
                        .unwrap_or_else(|_| {
                            Err(crate::AppError::MarketData("本次收盘检查超时".into()))
                        })
                        .map(|mut data| {
                            if let Some(history) = &dividends {
                                data.dividends = Some(history.events.clone());
                                data.dividend_basis = Some(history.basis.clone());
                                data.sources.extend(history.sources.clone());
                            }
                            evaluate(&entry.input.symbol, &data, day, checked_at.clone())
                        });
                    (entry, checked_at, result, dividends)
                });
            }
            let mut results = Vec::new();
            while let Some(result) = work.join_next().await {
                match result {
                    Ok(value) => results.push(value),
                    Err(error) => tracing::warn!(%error,"observation task interrupted"),
                }
            }
            results
        })
        .await?;
    for (entry, checked_at, result, dividends) in results {
        db.save_watch_observation(
            &entry,
            result.as_ref().ok(),
            &checked_at,
            day,
            dividends.as_ref(),
        )?;
    }
    db.watch_board(
        Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::watch::{tests::market_fixture, WatchInput, WatchMarketData};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Provider {
        calls: AtomicUsize,
        fail: bool,
        dividend_calls: AtomicUsize,
        dividend_fail: bool,
        price: f64,
    }
    #[async_trait::async_trait]
    impl WatchMarketProvider for Provider {
        async fn dividends(
            &self,
            _: &str,
            day: NaiveDate,
        ) -> AppResult<crate::models::watch::DividendHistory> {
            self.dividend_calls.fetch_add(1, Ordering::SeqCst);
            if self.dividend_fail {
                return Err(crate::AppError::MarketData("测试故障".into()));
            }
            let (mut data, end) = market_fixture();
            let mut events = data.dividends.take().unwrap();
            for event in &mut events {
                event.ex_date += day - end;
                event.report_date = Some(event.ex_date - chrono::Duration::days(180));
            }
            Ok(crate::models::watch::DividendHistory {
                basis: crate::models::watch::DividendBasis {
                    as_of: day,
                    fetched_at: Utc::now().to_rfc3339(),
                },
                sources: vec!["https://example.test/dividends".into()],
                events,
            })
        }
        async fn observation(&self, _: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(crate::AppError::MarketData("测试故障".into()));
            }
            let (mut data, end) = market_fixture();
            let shift = day - end;
            for bar in &mut data.bars {
                bar.day += shift;
                bar.close = Some(self.price);
            }
            for day in &mut data.calendar {
                *day += shift;
            }
            data.dividends = None;
            Ok(data)
        }
    }
    #[tokio::test]
    async fn persisted_dividends_reprice_after_restart_without_fetching_again() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        db.save_watch(WatchInput {
            symbol: "600036.SS".into(),
            name: "公司".into(),
            thesis: String::new(),
        })
        .unwrap();
        let provider = Arc::new(Provider {
            calls: AtomicUsize::new(0),
            fail: false,
            dividend_calls: AtomicUsize::new(0),
            dividend_fail: false,
            price: 100.0,
        });
        {
            let _lock = db.lock_account().unwrap();
            refresh(&db, provider.clone(), false).await.unwrap();
        }
        assert_eq!(provider.dividend_calls.load(Ordering::SeqCst), 1);
        let saved = db.watch_dividends("600036.SS").unwrap().unwrap();
        drop(db);
        let db = account.open();
        let _lock = db.lock_account().unwrap();
        db.conn()
            .unwrap()
            .execute_batch("UPDATE watch_entries SET checked_for=NULL")
            .unwrap();
        let provider = Arc::new(Provider {
            calls: AtomicUsize::new(0),
            fail: false,
            dividend_calls: AtomicUsize::new(0),
            dividend_fail: true,
            price: 50.0,
        });
        let board = refresh(&db, provider.clone(), false).await.unwrap();
        assert_eq!(board.rows[0].latest.as_ref().unwrap().yield_pct, Some(10.0));
        assert_eq!(provider.dividend_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            db.watch_dividends("600036.SS").unwrap(),
            Some(saved.clone())
        );
        // An explicit retry attempts the source, preserving the original collection timestamp on failure.
        refresh(&db, provider.clone(), true).await.unwrap();
        assert_eq!(provider.dividend_calls.load(Ordering::SeqCst), 1);
        assert_eq!(db.watch_dividends("600036.SS").unwrap(), Some(saved));
    }

    #[tokio::test]
    async fn scheduled_check_is_idempotent_and_network_failure_keeps_history() {
        let db = Database::test();
        db.save_watch(WatchInput {
            symbol: "600036.SS".into(),
            name: "公司".into(),
            thesis: String::new(),
        })
        .unwrap();
        let provider = Arc::new(Provider {
            calls: AtomicUsize::new(0),
            fail: false,
            dividend_calls: AtomicUsize::new(0),
            dividend_fail: false,
            price: 100.0,
        });
        let _lock = db.lock_account().unwrap();
        let first = refresh(&db, provider.clone(), false).await.unwrap();
        assert!(first.rows[0].latest.is_some());
        assert_eq!(first.rows[0].checked_for, Some(cutoff().to_string()));
        refresh(&db, provider.clone(), false).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let failure = Arc::new(Provider {
            calls: AtomicUsize::new(0),
            fail: true,
            dividend_calls: AtomicUsize::new(0),
            dividend_fail: true,
            price: 100.0,
        });
        let next = refresh(&db, failure, true).await.unwrap();
        assert_eq!(next.rows[0].latest, first.rows[0].latest);
        assert!(next.rows[0].refresh_failed);
    }
}
