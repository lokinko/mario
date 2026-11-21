//! Market-owned statutory announcement acquisition and cache reads.
use crate::{
    db::PublicAnnouncements,
    models::market::{announcements::AnnouncementSnapshot, canonical_symbol, is_a_share_stock},
    ports::market::{AnnouncementProvider, AnnouncementReader},
    AppError, AppResult,
};
use chrono::{Days, NaiveDate, Utc};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::Mutex;

const CACHE_TTL_SECS: i64 = 60 * 60;
const LOOKBACK_DAYS: u64 = 14;

pub(crate) struct PublicAnnouncementService {
    store: Arc<PublicAnnouncements>,
    provider: Arc<dyn AnnouncementProvider>,
    locks: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
}

impl PublicAnnouncementService {
    pub fn new(store: Arc<PublicAnnouncements>, provider: Arc<dyn AnnouncementProvider>) -> Self {
        Self {
            store,
            provider,
            locks: Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn refresh(self: &Arc<Self>, symbols: &[String], force: bool) -> AppResult<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        let today = Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let since = today
            .checked_sub_days(Days::new(LOOKBACK_DAYS))
            .unwrap_or(today);
        let mut targets = BTreeSet::new();
        for raw in symbols {
            if let Ok(normalized) = canonical_symbol(raw) {
                if is_a_share_stock(&normalized) {
                    targets.insert(normalized);
                }
            }
        }
        let mut tasks = tokio::task::JoinSet::new();
        for symbol in targets {
            let service = self.clone();
            tasks.spawn(async move { service.check(&symbol, since, today, force, deadline).await });
        }
        while let Some(result) = tasks.join_next().await {
            result.map_err(|error| AppError::MarketData(error.to_string()))??;
        }
        Ok(())
    }

    async fn check(
        &self,
        symbol: &str,
        since: NaiveDate,
        through: NaiveDate,
        force: bool,
        deadline: tokio::time::Instant,
    ) -> AppResult<()> {
        let before = self.store.snapshot(symbol)?;
        if !force && is_fresh(&before) {
            return Ok(());
        }
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
        let previous = self.store.snapshot(symbol)?;
        if previous.attempted_at != before.attempted_at || (!force && is_fresh(&previous)) {
            return Ok(());
        }
        let attempted_at = Some(Utc::now().to_rfc3339());
        let result = tokio::time::timeout_at(
            deadline,
            self.provider.announcements(symbol, since, through),
        )
        .await;
        let snapshot = match result {
            Ok(Ok(announcements)) if announcements.iter().all(|a| a.symbol == symbol) => {
                AnnouncementSnapshot {
                    announcements,
                    attempted_at,
                    fetched_at: Some(Utc::now().to_rfc3339()),
                    refresh_failed: false,
                }
            }
            other => {
                tracing::warn!(
                    symbol,
                    result = ?other,
                    "official announcement source failed; retaining previous cache"
                );
                AnnouncementSnapshot {
                    attempted_at,
                    refresh_failed: true,
                    ..previous
                }
            }
        };
        self.store.save_snapshot(symbol, &snapshot)
    }
}

fn is_fresh(snapshot: &AnnouncementSnapshot) -> bool {
    snapshot
        .attempted_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_seconds() < CACHE_TTL_SECS)
}

impl AnnouncementReader for PublicAnnouncementService {
    fn snapshot(&self, symbol: &str) -> AppResult<AnnouncementSnapshot> {
        self.store.snapshot(&canonical_symbol(symbol)?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::market::announcements::{AnnouncementImportance, OfficialAnnouncement};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    pub(crate) fn fixture_service() -> Arc<PublicAnnouncementService> {
        Arc::new(PublicAnnouncementService::new(
            Arc::new(PublicAnnouncements::test()),
            Arc::new(FakeProvider {
                calls: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
            }),
        ))
    }

    struct FakeProvider {
        calls: AtomicUsize,
        fail: AtomicBool,
    }

    #[async_trait::async_trait]
    impl AnnouncementProvider for FakeProvider {
        async fn announcements(
            &self,
            symbol: &str,
            _since: NaiveDate,
            through: NaiveDate,
        ) -> AppResult<Vec<OfficialAnnouncement>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(AppError::MarketData("upstream error".into()));
            }
            Ok(vec![OfficialAnnouncement {
                id: format!("cninfo:{symbol}:1"),
                symbol: symbol.into(),
                stock_name: "测试公司".into(),
                title: "关于2026年中期分红派息实施公告".into(),
                category: "分红派息".into(),
                published_on: through.to_string(),
                published_at: Utc::now().to_rfc3339(),
                source_name: "巨潮资讯网".into(),
                source_url: "https://static.cninfo.com.cn/finalpage/test.PDF".into(),
                importance: AnnouncementImportance::High,
            }])
        }
    }

    #[tokio::test]
    async fn caches_announcements_skips_non_a_shares_and_preserves_old_on_failure() {
        let store = Arc::new(PublicAnnouncements::test());
        let provider = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
        });
        let service = Arc::new(PublicAnnouncementService::new(store, provider.clone()));
        service
            .refresh(&["600519.SS".into(), "AAPL".into()], false)
            .await
            .unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        // Cached within TTL: second non-forced refresh makes no provider call.
        service.refresh(&["600519.SS".into()], false).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let snap = service.snapshot("600519.SS").unwrap();
        assert_eq!(snap.announcements.len(), 1);
        assert!(!snap.refresh_failed);

        provider.fail.store(true, Ordering::SeqCst);
        service.refresh(&["600519.SS".into()], true).await.unwrap();
        let after = service.snapshot("600519.SS").unwrap();
        assert_eq!(after.announcements.len(), 1);
        assert!(after.refresh_failed);
    }
}
