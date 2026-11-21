//! Public statutory announcement cache in mario_market. Shared across accounts.
use crate::{
    models::market::announcements::AnnouncementSnapshot,
    storage::{params, OptionalRow},
    AppError, AppResult,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);

struct PooledConn {
    connection: crate::storage::Connection,
    last_ok: Instant,
}
impl std::ops::Deref for PooledConn {
    type Target = crate::storage::Connection;
    fn deref(&self) -> &crate::storage::Connection {
        &self.connection
    }
}
impl std::ops::DerefMut for PooledConn {
    fn deref_mut(&mut self) -> &mut crate::storage::Connection {
        &mut self.connection
    }
}

pub(crate) struct PublicAnnouncements {
    connection: Mutex<PooledConn>,
    url: String,
    schema: String,
}

impl PublicAnnouncements {
    pub fn postgres(url: &str) -> AppResult<Self> {
        Self::open(url, "mario_market")
    }

    fn open(url: &str, schema: &str) -> AppResult<Self> {
        let mut connection = crate::storage::Connection::postgres(url, schema)?;
        let tx = connection.transaction()?;
        tx.execute_batch(
            "SELECT pg_advisory_xact_lock(hashtextextended(current_schema(),0));
            CREATE TABLE IF NOT EXISTS announcement_snapshots (symbol TEXT PRIMARY KEY, payload TEXT NOT NULL);",
        )?;
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(PooledConn {
                connection,
                last_ok: Instant::now(),
            }),
            url: url.into(),
            schema: schema.into(),
        })
    }

    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, PooledConn>> {
        let mut guard = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.last_ok.elapsed() >= HEALTH_CHECK_INTERVAL {
            if guard
                .connection
                .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
                .is_err()
            {
                guard.connection = crate::storage::Connection::postgres(&self.url, &self.schema)?;
            }
            guard.last_ok = Instant::now();
        }
        Ok(guard)
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self::open(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
            &format!("ann_test_{}", uuid::Uuid::new_v4().simple()),
        )
        .unwrap()
    }

    fn read_with_recovery<T>(
        &self,
        mut f: impl FnMut(&crate::storage::Connection) -> crate::storage::Result<T>,
    ) -> crate::storage::Result<T> {
        let mut guard = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match f(&guard.connection) {
            Ok(value) => {
                guard.last_ok = Instant::now();
                Ok(value)
            }
            Err(error) if error.is_connection_broken() => {
                guard.connection = crate::storage::Connection::postgres(&self.url, &self.schema)?;
                let value = f(&guard.connection)?;
                guard.last_ok = Instant::now();
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }

    pub fn snapshot(&self, symbol: &str) -> AppResult<AnnouncementSnapshot> {
        let payload: Option<String> = self.read_with_recovery(|c| {
            c.query_row(
                "SELECT payload FROM announcement_snapshots WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()
        })?;
        Ok(payload
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default())
    }

    pub fn save_snapshot(&self, symbol: &str, snapshot: &AnnouncementSnapshot) -> AppResult<()> {
        if snapshot
            .attempted_at
            .as_ref()
            .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot
                .fetched_at
                .as_ref()
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot.announcements.iter().any(|a| a.symbol != symbol)
        {
            return Err(AppError::Validation("官方公告来源或采集时间无效".into()));
        }
        self.conn()?.execute(
            "INSERT INTO announcement_snapshots(symbol,payload) VALUES ($1,$2)
            ON CONFLICT(symbol) DO UPDATE SET payload=(CASE
              WHEN announcement_snapshots.payload::jsonb->>'fetchedAt' IS NOT NULL
                AND (excluded.payload::jsonb->>'fetchedAt' IS NULL OR
                  (announcement_snapshots.payload::jsonb->>'fetchedAt')::timestamptz > (excluded.payload::jsonb->>'fetchedAt')::timestamptz)
              THEN jsonb_set(jsonb_set(excluded.payload::jsonb,'{announcements}',announcement_snapshots.payload::jsonb->'announcements'),'{fetchedAt}',announcement_snapshots.payload::jsonb->'fetchedAt')
              ELSE excluded.payload::jsonb END)::text
            WHERE (announcement_snapshots.payload::jsonb->>'attemptedAt')::timestamptz <= (excluded.payload::jsonb->>'attemptedAt')::timestamptz",
            params![symbol, serde_json::to_string(snapshot)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::announcements::{AnnouncementImportance, OfficialAnnouncement};

    #[test]
    fn retains_last_successful_announcements_when_later_refresh_fails() {
        let store = PublicAnnouncements::test();
        let first = AnnouncementSnapshot {
            announcements: vec![OfficialAnnouncement {
                id: "cninfo:1".into(),
                symbol: "600519.SS".into(),
                stock_name: "贵州茅台".into(),
                title: "2026年第三季度报告".into(),
                category: "定期报告与业绩".into(),
                published_on: "2026-11-20".into(),
                published_at: "2026-11-20T09:00:00+00:00".into(),
                source_name: "巨潮资讯网".into(),
                source_url: "https://static.cninfo.com.cn/finalpage/1.PDF".into(),
                importance: AnnouncementImportance::High,
            }],
            attempted_at: Some("2026-11-20T10:00:00+00:00".into()),
            fetched_at: Some("2026-11-20T10:00:00+00:00".into()),
            refresh_failed: false,
        };
        store.save_snapshot("600519.SS", &first).unwrap();
        let failed = AnnouncementSnapshot {
            announcements: Vec::new(),
            attempted_at: Some("2026-11-20T11:00:00+00:00".into()),
            fetched_at: None,
            refresh_failed: true,
        };
        store.save_snapshot("600519.SS", &failed).unwrap();
        let loaded = store.snapshot("600519.SS").unwrap();
        assert_eq!(loaded.announcements.len(), 1);
        assert!(loaded.refresh_failed);
        assert_eq!(
            loaded.fetched_at.as_deref(),
            Some("2026-11-20T10:00:00+00:00")
        );
    }
}
