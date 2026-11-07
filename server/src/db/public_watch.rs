//! Only public latest observations; never opens a tenant schema.
use crate::{
    models::watch::catalog::CatalogSnapshot,
    storage::{params, Connection, OptionalRow},
    AppResult,
};
use std::sync::Mutex;

pub(crate) struct PublicWatch {
    connection: Mutex<Connection>,
    url: String,
    schema: String,
}

impl PublicWatch {
    pub fn postgres(url: &str) -> AppResult<Self> {
        Self::open(url, "mario_market")
    }

    fn open(url: &str, schema: &str) -> AppResult<Self> {
        let mut connection = Connection::postgres(url, schema)?;
        let tx = connection.transaction()?;
        tx.execute_batch(
            "SELECT pg_advisory_xact_lock(hashtextextended(current_schema(),0));
            CREATE TABLE IF NOT EXISTS watch_snapshots (
                symbol TEXT PRIMARY KEY, payload TEXT NOT NULL
            );",
        )?;
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(connection),
            url: url.into(),
            schema: schema.into(),
        })
    }

    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if connection
            .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .is_err()
        {
            *connection = Connection::postgres(&self.url, &self.schema)?;
        }
        Ok(connection)
    }

    pub fn get(&self, symbol: &str) -> AppResult<CatalogSnapshot> {
        let payload: Option<String> = self
            .conn()?
            .query_row(
                "SELECT payload FROM watch_snapshots WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?;
        Ok(payload
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default())
    }

    pub fn save(&self, symbol: &str, snapshot: &CatalogSnapshot) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO watch_snapshots(symbol,payload) VALUES ($1,$2)
            ON CONFLICT(symbol) DO UPDATE SET payload=jsonb_set(excluded.payload::jsonb, '{latest}',
                CASE WHEN watch_snapshots.payload::jsonb->'latest' <> 'null'::jsonb
                    AND (excluded.payload::jsonb->'latest' = 'null'::jsonb
                        OR watch_snapshots.payload::jsonb->'latest'->>'observedOn' > excluded.payload::jsonb->'latest'->>'observedOn'
                        OR (watch_snapshots.payload::jsonb->'latest'->>'observedOn' = excluded.payload::jsonb->'latest'->>'observedOn'
                            AND (watch_snapshots.payload::jsonb->'latest'->>'checkedAt')::timestamptz > (excluded.payload::jsonb->'latest'->>'checkedAt')::timestamptz))
                THEN watch_snapshots.payload::jsonb->'latest'
                ELSE excluded.payload::jsonb->'latest' END)::text
            WHERE (watch_snapshots.payload::jsonb->>'checkedAt')::timestamptz
                <= (excluded.payload::jsonb->>'checkedAt')::timestamptz",
            params![symbol, serde_json::to_string(snapshot)?],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self::open(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
            &format!("market_test_{}", uuid::Uuid::new_v4().simple()),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_survive_reopen_and_delayed_writes_cannot_replace_newer_results() {
        let store = PublicWatch::test();
        let newer = CatalogSnapshot {
            checked_at: Some("2026-11-07T12:00:00Z".into()),
            checked_for: Some("2026-11-06".into()),
            ..Default::default()
        };
        store.save("600036.SS", &newer).unwrap();
        let older = CatalogSnapshot {
            checked_at: Some("2026-11-07T11:00:00Z".into()),
            refresh_failed: true,
            ..Default::default()
        };
        store.save("600036.SS", &older).unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        assert_eq!(
            reopened.get("600036.SS").unwrap().checked_at,
            newer.checked_at
        );
        assert!(!reopened.get("600036.SS").unwrap().refresh_failed);
        assert!(reopened.get("601398.SS").unwrap().latest.is_none());
    }

    #[test]
    fn failed_refresh_cannot_restore_the_older_facts_it_read_before_another_writer() {
        let store = PublicWatch::test();
        let (market, day) = crate::models::watch::tests::market_fixture();
        let latest = crate::models::watch::evaluate(
            "600036.SS",
            &market,
            day,
            "2026-11-07T12:00:00Z".into(),
        );
        store
            .save(
                "600036.SS",
                &CatalogSnapshot {
                    latest: Some(latest.clone()),
                    checked_at: Some("2026-11-07T12:00:00Z".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut stale = latest.clone();
        stale.checked_at = "2026-11-07T11:00:00Z".into();
        stale.price = Some(90.0);
        store
            .save(
                "600036.SS",
                &CatalogSnapshot {
                    latest: Some(stale),
                    checked_at: Some("2026-11-07T13:00:00Z".into()),
                    refresh_failed: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let saved = store.get("600036.SS").unwrap();
        assert!(saved.refresh_failed);
        assert_eq!(saved.latest, Some(latest));
    }
}
