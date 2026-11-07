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
        let seed = Catalog::bundled()?;
        tx.execute("INSERT INTO watch_catalog(id,payload,revision) VALUES (1,$1,1) ON CONFLICT(id) DO NOTHING",
            [serde_json::to_string(&seed)?])?;
        let payload: String = tx.query_row(
            "SELECT payload FROM watch_catalog WHERE id=1 FOR UPDATE",
            [],
            |r| r.get(0),
        )?;
        let mut current = Catalog::parse(&payload)?;
        if current.upgrade(&seed)? {
            tx.execute(
                "UPDATE watch_catalog SET payload=$1, revision=revision+1 WHERE id=1",
                [serde_json::to_string(&current)?],
            )?;
        }
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
        // Only company membership and metadata are editable; the current domains stay stable.
        if old.version != catalog.version
            || old
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

    /// Public-source enrichment never restores removed companies or replaces edits.
    pub fn complete_sources(
        &self,
        expected: &crate::models::watch::catalog::Company,
        resolved: &crate::models::watch::catalog::Company,
    ) -> AppResult<()> {
        if expected.symbol != resolved.symbol || resolved.sources_checked_on.is_none() {
            return Ok(());
        }
        let mut connection = self.conn()?;
        let tx = connection.transaction()?;
        let payload: String = tx.query_row(
            "SELECT payload FROM watch_catalog WHERE id=1 FOR UPDATE",
            [],
            |r| r.get(0),
        )?;
        let mut catalog = Catalog::parse(&payload)?;
        if let Some(company) = catalog
            .industries
            .iter_mut()
            .flat_map(|i| &mut i.companies)
            .find(|c| *c == expected)
        {
            company.sources.clone_from(&resolved.sources);
            company
                .sources_checked_on
                .clone_from(&resolved.sources_checked_on);
            catalog.validate()?;
            tx.execute(
                "UPDATE watch_catalog SET payload=$1,revision=revision+1 WHERE id=1",
                [serde_json::to_string(&catalog)?],
            )?;
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
    fn requested_company_migration_preserves_custom_entries_and_only_runs_once() {
        use crate::models::watch::catalog::{Company, Industry};
        let store = PublicWatch::test();
        let seed = Catalog::bundled().unwrap();
        let custom = Company {
            symbol: "600000.SS".into(),
            name: "浦发银行".into(),
            sources: vec![],
            sources_checked_on: None,
        };
        let mut old = seed.clone();
        old.version = 1;
        old.industries = vec![
            Industry {
                id: "banks".into(),
                name: "银行".into(),
                companies: vec![custom.clone()],
            },
            Industry {
                id: "coal".into(),
                name: "煤炭".into(),
                companies: vec![seed.industry("energy").unwrap().companies[0].clone()],
            },
            Industry {
                id: "oil".into(),
                name: "石油石化".into(),
                companies: vec![],
            },
        ];
        store
            .conn()
            .unwrap()
            .execute(
                "UPDATE watch_catalog SET payload=$1",
                [serde_json::to_string(&old).unwrap()],
            )
            .unwrap();
        let upgraded = PublicWatch::open(&store.url, &store.schema).unwrap();
        let (mut current, revision) = upgraded.catalog().unwrap();
        assert_eq!(current.version, 4);
        assert!(current.industry("coal").is_err());
        assert!(current
            .industries
            .iter()
            .flat_map(|i| &i.companies)
            .any(|c| c == &custom));
        assert!(
            !current
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .any(|c| c.symbol == "600036.SS"),
            "unrequested prior removal remains removed"
        );
        for symbol in serde_json::from_str::<Vec<String>>(include_str!(
            "../../data/watch-catalog-additions-20261107.json"
        ))
        .unwrap()
        {
            assert!(current
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .any(|c| c.symbol == symbol));
        }
        current
            .industries
            .iter_mut()
            .for_each(|i| i.companies.retain(|c| c.symbol != "600795.SS"));
        upgraded.save_catalog(&current, revision, "owner").unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        assert!(
            !reopened
                .catalog()
                .unwrap()
                .0
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .any(|c| c.symbol == "600795.SS"),
            "restarts do not undo later removals"
        );
    }

    #[test]
    fn screened_release_adds_only_selected_companies_once_and_preserves_curated_v2_data() {
        let store = PublicWatch::test();
        let (mut old, _) = store.catalog().unwrap();
        let selected: std::collections::BTreeSet<String> = serde_json::from_str(include_str!(
            "../../data/watch-catalog-screened-additions-20261107.json"
        ))
        .unwrap();
        old.version = 2;
        for industry in &mut old.industries {
            industry
                .companies
                .retain(|c| !selected.contains(&c.symbol) && c.symbol != "600036.SS");
        }
        let mut custom = old.industries[0].companies[0].clone();
        custom.symbol = "600000.SS".into();
        custom.name = "自定义公司名".into();
        old.industries[0].companies.push(custom.clone());
        store
            .conn()
            .unwrap()
            .execute(
                "UPDATE watch_catalog SET payload=$1",
                [serde_json::to_string(&old).unwrap()],
            )
            .unwrap();
        let migrated = PublicWatch::open(&store.url, &store.schema).unwrap();
        let (mut current, revision) = migrated.catalog().unwrap();
        let companies: Vec<_> = current
            .industries
            .iter()
            .flat_map(|i| &i.companies)
            .collect();
        assert!(selected
            .iter()
            .all(|s| companies.iter().any(|c| &c.symbol == s)));
        assert!(companies.contains(&&custom));
        assert!(
            !companies.iter().any(|c| c.symbol == "600036.SS"),
            "v2 removals are not replayed as v1 additions"
        );
        current.industries[0]
            .companies
            .retain(|c| c.symbol != "601166.SS");
        migrated.save_catalog(&current, revision, "owner").unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        assert_eq!(reopened.catalog().unwrap().1, revision + 1);
        assert!(!reopened.catalog().unwrap().0.industries[0]
            .companies
            .iter()
            .any(|c| c.symbol == "601166.SS"));
    }

    #[test]
    fn broker_release_upgrades_v3_once_preserving_edits_removals_and_global_uniqueness() {
        let store = PublicWatch::test();
        let (mut old, revision) = store.catalog().unwrap();
        old.version = 3;
        let brokers = old.industries.pop().unwrap();
        assert_eq!(brokers.id, "brokers");
        let mut existing = brokers.companies[0].clone();
        existing.name = "人工保留名称".into();
        existing.sources.clear();
        existing.sources_checked_on = None;
        old.industries[0].companies.push(existing.clone());
        old.industries[0]
            .companies
            .retain(|c| c.symbol != "601166.SS" && c.symbol != "600036.SS");
        store
            .conn()
            .unwrap()
            .execute(
                "UPDATE watch_catalog SET payload=$1",
                [serde_json::to_string(&old).unwrap()],
            )
            .unwrap();
        let migrated = PublicWatch::open(&store.url, &store.schema).unwrap();
        let (mut current, next) = migrated.catalog().unwrap();
        assert_eq!(current.version, 4);
        assert_eq!(next, revision + 1);
        assert_eq!(current.industries[0].companies, old.industries[0].companies);
        let added = current.industry("brokers").unwrap();
        assert_eq!(added.companies.len(), brokers.companies.len() - 1);
        assert!(!added.companies.iter().any(|c| c.symbol == existing.symbol));
        assert!(current.industries[0].companies.contains(&existing));
        current
            .industries
            .iter_mut()
            .find(|i| i.id == "brokers")
            .unwrap()
            .companies
            .clear();
        migrated.save_catalog(&current, next, "owner").unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        let (saved, final_revision) = reopened.catalog().unwrap();
        assert_eq!(final_revision, next + 1);
        assert!(saved.industry("brokers").unwrap().companies.is_empty());
        assert_eq!(saved.industries[0].companies, old.industries[0].companies);
    }

    #[test]
    fn source_completion_preserves_manual_edits_and_cannot_restore_removed_members() {
        let store = PublicWatch::test();
        let (mut catalog, revision) = store.catalog().unwrap();
        let resolved = catalog.industries[0].companies[0].clone();
        let mut pending = resolved.clone();
        pending.sources.clear();
        pending.sources_checked_on = None;
        catalog.industries[0].companies[0] = pending.clone();
        store.save_catalog(&catalog, revision, "owner").unwrap();
        let before = store.catalog().unwrap().1;
        store.complete_sources(&pending, &resolved).unwrap();
        let (mut current, after) = store.catalog().unwrap();
        assert_eq!(after, before + 1);
        assert_eq!(current.industries[0].companies[0], resolved);
        assert!(matches!(
            store.save_catalog(&catalog, before, "stale"),
            Err(AppError::Conflict(_))
        ));
        current.industries[0].companies.remove(0);
        store.save_catalog(&current, after, "owner").unwrap();
        let removed_revision = store.catalog().unwrap().1;
        store.complete_sources(&pending, &resolved).unwrap();
        assert_eq!(store.catalog().unwrap().1, removed_revision);
        assert!(!store.catalog().unwrap().0.industries[0]
            .companies
            .iter()
            .any(|c| c.symbol == pending.symbol));
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
