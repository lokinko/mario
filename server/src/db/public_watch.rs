//! Shared company directory and public latest observations; never opens a tenant schema.
use crate::{
    models::watch::catalog::{Catalog, CatalogSnapshot},
    storage::{params, Connection, OptionalRow},
    AppError, AppResult,
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
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS watch_catalog (
            id INTEGER PRIMARY KEY CHECK (id=1), payload TEXT NOT NULL,
            revision BIGINT NOT NULL, updated_by TEXT, updated_at TEXT
        );",
        )?;
        tx.execute("INSERT INTO watch_catalog(id,payload,revision) VALUES (1,$1,1) ON CONFLICT(id) DO NOTHING",
            [serde_json::to_string(&Catalog::bundled()?)?])?;
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

    pub fn catalog(&self) -> AppResult<(Catalog, i64)> {
        let (payload, revision): (String, i64) = self.conn()?.query_row(
            "SELECT payload,revision FROM watch_catalog WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((Catalog::parse(&payload)?, revision))
    }

    pub fn save_catalog(&self, catalog: &Catalog, revision: i64, user_id: &str) -> AppResult<()> {
        catalog.validate()?;
        let mut connection = self.conn()?;
        let tx = connection.transaction()?;
        let payload: String = tx.query_row(
            "SELECT payload FROM watch_catalog WHERE id=1 FOR UPDATE",
            [],
            |r| r.get(0),
        )?;
        let old = Catalog::parse(&payload)?;
        // Only company membership and metadata are editable; the eight domains stay stable.
        if old
            .industries
            .iter()
            .map(|i| (&i.id, &i.name))
            .collect::<Vec<_>>()
            != catalog
                .industries
                .iter()
                .map(|i| (&i.id, &i.name))
                .collect::<Vec<_>>()
        {
            return Err(AppError::Validation(
                "请保留现有领域，仅维护领域内的公司".into(),
            ));
        }
        let changed = tx.execute(
            "UPDATE watch_catalog SET payload=$1,revision=revision+1,updated_by=$2,updated_at=$3 WHERE id=1 AND revision=$4",
            params![serde_json::to_string(catalog)?, user_id, chrono::Utc::now().to_rfc3339(), revision],
        )?;
        if changed == 0 {
            return Err(AppError::Conflict(
                "公共目录已被更新，请重新读取后核对；当前草稿已保留".into(),
            ));
        }
        tx.commit()?;
        Ok(())
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
    fn catalog_edits_persist_allow_empty_groups_and_reject_conflicts_or_invalid_replacements() {
        let store = PublicWatch::test();
        let (mut catalog, revision) = store.catalog().unwrap();
        let old = catalog.industries[0].companies.remove(0);
        catalog.industries[0].companies.clear();
        catalog.industries[1].companies.push(old);
        store.save_catalog(&catalog, revision, "owner-id").unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        let (saved, next) = reopened.catalog().unwrap();
        assert_eq!(next, revision + 1);
        assert!(saved.industries[0].companies.is_empty());
        assert!(matches!(
            store.save_catalog(&catalog, revision, "stale-editor"),
            Err(AppError::Conflict(_))
        ));
        let actor: String = reopened
            .conn()
            .unwrap()
            .query_row("SELECT updated_by FROM watch_catalog", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actor, "owner-id");
        let mut invalid = saved.clone();
        invalid.industries[0]
            .companies
            .push(saved.industries[1].companies[0].clone());
        assert!(store.save_catalog(&invalid, next, "owner-id").is_err());
        invalid = saved;
        invalid.industries.remove(0);
        assert!(store.save_catalog(&invalid, next, "owner-id").is_err());
        assert_eq!(store.catalog().unwrap().1, next);
    }

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
