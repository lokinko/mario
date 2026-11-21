//! Public forecast cache only. Account scopes and implemented-dividend storage stay separate.
use crate::{
    models::market::dividend_plans::PlanSnapshot,
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

pub(crate) struct PublicDividendPlans {
    connection: Mutex<PooledConn>,
    url: String,
    schema: String,
}
impl PublicDividendPlans {
    pub fn postgres(url: &str) -> AppResult<Self> {
        Self::open(url, "mario_market")
    }
    fn open(url: &str, schema: &str) -> AppResult<Self> {
        let mut connection = crate::storage::Connection::postgres(url, schema)?;
        let tx = connection.transaction()?;
        tx.execute_batch("SELECT pg_advisory_xact_lock(hashtextextended(current_schema(),0));
            CREATE TABLE IF NOT EXISTS dividend_plan_snapshots (symbol TEXT PRIMARY KEY, payload TEXT NOT NULL);")?;
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
            &format!("plan_test_{}", uuid::Uuid::new_v4().simple()),
        )
        .unwrap()
    }
}
impl PublicDividendPlans {
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

    pub fn calendar_snapshot(&self, symbol: &str) -> AppResult<PlanSnapshot> {
        let payload: Option<String> = self.read_with_recovery(|c| {
            c.query_row(
                "SELECT payload FROM dividend_plan_snapshots WHERE symbol=$1",
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
    pub fn save_calendar_snapshot(&self, symbol: &str, snapshot: &PlanSnapshot) -> AppResult<()> {
        if snapshot
            .attempted_at
            .as_ref()
            .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot
                .fetched_at
                .as_ref()
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot.plans.iter().any(|p| p.symbol != symbol)
        {
            return Err(AppError::Validation("分红计划来源或采集时间无效".into()));
        }
        self.conn()?.execute("INSERT INTO dividend_plan_snapshots(symbol,payload) VALUES ($1,$2)
            ON CONFLICT(symbol) DO UPDATE SET payload=(CASE
              WHEN dividend_plan_snapshots.payload::jsonb->>'fetchedAt' IS NOT NULL
                AND (excluded.payload::jsonb->>'fetchedAt' IS NULL OR
                  (dividend_plan_snapshots.payload::jsonb->>'fetchedAt')::timestamptz > (excluded.payload::jsonb->>'fetchedAt')::timestamptz)
              THEN jsonb_set(jsonb_set(excluded.payload::jsonb,'{plans}',dividend_plan_snapshots.payload::jsonb->'plans'),'{fetchedAt}',dividend_plan_snapshots.payload::jsonb->'fetchedAt')
              ELSE excluded.payload::jsonb END)::text
            WHERE (dividend_plan_snapshots.payload::jsonb->>'attemptedAt')::timestamptz <= (excluded.payload::jsonb->>'attemptedAt')::timestamptz",
            params![symbol, serde_json::to_string(snapshot)?])?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_forecasts_survive_reopen_and_late_failures_cannot_restore_old_plans() {
        let store = PublicDividendPlans::test();
        let good = PlanSnapshot {
            attempted_at: Some("2026-11-07T10:00:00Z".into()),
            fetched_at: Some("2026-11-07T10:00:01Z".into()),
            plans: vec![crate::models::market::dividend_plans::tests::plan(
                "600000.SS",
                crate::models::market::dividend_plans::PlanStage::Scheduled,
            )],
            refresh_failed: false,
        };
        store.save_calendar_snapshot("600000.SS", &good).unwrap();
        let failed = PlanSnapshot {
            attempted_at: Some("2026-11-07T10:00:02Z".into()),
            refresh_failed: true,
            ..Default::default()
        };
        store.save_calendar_snapshot("600000.SS", &failed).unwrap();
        let reopened = PublicDividendPlans::open(&store.url, &store.schema).unwrap();
        let retained = reopened.calendar_snapshot("600000.SS").unwrap();
        assert!(retained.refresh_failed);
        assert_eq!(retained.plans.len(), 1);
        assert_eq!(retained.fetched_at, good.fetched_at);
        store.save_calendar_snapshot("600000.SS", &good).unwrap();
        assert!(store.calendar_snapshot("600000.SS").unwrap().refresh_failed);
        assert!(store
            .calendar_snapshot("600001.SS")
            .unwrap()
            .fetched_at
            .is_none());
    }
    #[test]
    fn opens_existing_plan_table_without_initializing_or_migrating_the_user_catalog() {
        let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
        let schema = format!("legacy_plans_{}", uuid::Uuid::new_v4().simple());
        let connection = crate::storage::Connection::postgres(&url, &schema).unwrap();
        connection.execute_batch("CREATE TABLE dividend_plan_snapshots (symbol TEXT PRIMARY KEY, payload TEXT NOT NULL);").unwrap();
        let stored = PlanSnapshot {
            attempted_at: Some("2026-11-07T10:00:00Z".into()),
            fetched_at: Some("2026-11-07T10:00:01Z".into()),
            plans: vec![crate::models::market::dividend_plans::tests::plan(
                "600000.SS",
                crate::models::market::dividend_plans::PlanStage::Scheduled,
            )],
            refresh_failed: false,
        };
        connection
            .execute(
                "INSERT INTO dividend_plan_snapshots VALUES ($1,$2)",
                params!["600000.SS", serde_json::to_string(&stored).unwrap()],
            )
            .unwrap();
        let store = PublicDividendPlans::open(&url, &schema).unwrap();
        assert_eq!(
            serde_json::to_value(store.calendar_snapshot("600000.SS").unwrap()).unwrap(),
            serde_json::to_value(stored).unwrap()
        );
        let catalogs: i64 = connection.query_row("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='watch_catalog'", [], |row| row.get(0)).unwrap();
        assert_eq!(
            catalogs, 0,
            "market plan storage must not initialize the user-owned catalog"
        );
    }
}
