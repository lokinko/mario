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
    let entries: Vec<_> = board
        .rows
        .into_iter()
        .filter(|r| {
            force
                || r.refresh_failed
                || r.checked_for.as_deref() != Some(day.to_string().as_str())
                || r.latest.as_ref().is_some_and(|o| {
                    o.status == crate::models::watch::WatchStatus::Incomplete
                        || (day.weekday().num_days_from_monday() < 5
                            && o.observed_on < day.to_string()
                            && Utc::now().with_timezone(&chrono_tz::Asia::Shanghai).hour() >= 19)
                })
        })
        .map(|r| r.entry)
        .collect();
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
            for entry in entries {
                let provider = provider.clone();
                work.spawn(async move {
                    let checked_at = Utc::now().to_rfc3339();
                    let result = tokio::time::timeout_at(
                        deadline,
                        provider.observation(&entry.input.symbol, day),
                    )
                    .await
                    .unwrap_or_else(|_| Err(crate::AppError::MarketData("本次收盘检查超时".into())))
                    .map(|data| evaluate(&entry.input.symbol, &data, day, checked_at.clone()));
                    (entry, checked_at, result)
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
    for (entry, checked_at, result) in results {
        db.save_watch_observation(&entry, result.as_ref().ok(), &checked_at, day)?;
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
    }
    #[async_trait::async_trait]
    impl WatchMarketProvider for Provider {
        async fn observation(&self, _: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(crate::AppError::MarketData("测试故障".into()));
            }
            let (mut data, end) = market_fixture();
            let shift = day - end;
            for bar in &mut data.bars {
                bar.day += shift;
            }
            for day in &mut data.calendar {
                *day += shift;
            }
            for event in data.dividends.as_mut().unwrap() {
                event.ex_date += shift;
            }
            Ok(data)
        }
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
        });
        let next = refresh(&db, failure, true).await.unwrap();
        assert_eq!(next.rows[0].latest, first.rows[0].latest);
        assert!(next.rows[0].refresh_failed);
    }
}
