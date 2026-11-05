use super::{params, AppResult, Database, OptionalRow};
use crate::models::watch::{
    DividendHistory, Observation, WatchBoard, WatchEntry, WatchInput, WatchRow, WatchStatus,
    BAND_DISTANCE_PCT, MAX_WATCHES, MIN_YIELD_PCT,
};
use crate::AppError;
use chrono::{Datelike, Duration, NaiveDate, Utc};

impl Database {
    pub fn watch_dividends(&self, symbol: &str) -> AppResult<Option<DividendHistory>> {
        let payload: Option<String> = self
            .conn()?
            .query_row(
                "SELECT payload FROM watch_dividends WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .as_deref()
            .map(|p| Ok(serde_json::from_str(p)?))
            .transpose()
    }

    pub fn watch_entries(&self) -> AppResult<Vec<WatchEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT payload FROM watch_entries ORDER BY symbol")?;
        let payloads = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        payloads
            .iter()
            .map(|p| Ok(serde_json::from_str(p)?))
            .collect()
    }

    pub fn save_watch(&self, input: WatchInput) -> AppResult<WatchEntry> {
        let input = input.normalize()?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let old: Option<String> = tx
            .query_row(
                "SELECT payload FROM watch_entries WHERE symbol=$1",
                [&input.symbol],
                |r| r.get(0),
            )
            .optional()?;
        if old.is_none()
            && tx.query_row("SELECT count(*) FROM watch_entries", [], |r| {
                r.get::<_, i64>(0)
            })? >= MAX_WATCHES as i64
        {
            return Err(AppError::Validation("观察名单最多保留 50 家公司".into()));
        }
        let created_at = old
            .as_deref()
            .map(serde_json::from_str::<WatchEntry>)
            .transpose()?
            .map_or_else(|| Utc::now().to_rfc3339(), |v| v.created_at);
        let entry = WatchEntry { input, created_at };
        tx.execute("INSERT INTO watch_entries(symbol,payload,checked_at,refresh_failed) VALUES ($1,$2,NULL,0) ON CONFLICT(symbol) DO UPDATE SET payload=excluded.payload",params![entry.input.symbol,serde_json::to_string(&entry)?])?;
        tx.commit()?;
        Ok(entry)
    }

    pub fn delete_watch(&self, symbol: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM watch_entries WHERE symbol=$1", [symbol])?;
        Ok(())
    }

    pub fn read_watch_alerts(&self, symbol: &str) -> AppResult<()> {
        self.conn()?.execute("UPDATE watch_observations SET read_at=$2 WHERE symbol=$1 AND alert=1 AND read_at IS NULL",params![symbol,Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn watch_board(&self, today: NaiveDate) -> AppResult<WatchBoard> {
        // Weekends show the week just completed; weekdays show the current week so far.
        let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
        let end = start + Duration::days(6);
        let entries = self.watch_entries()?;
        let conn = self.conn()?;
        let mut rows = Vec::new();
        for entry in entries {
            let symbol = &entry.input.symbol;
            let latest: Option<String> = conn.query_row("SELECT payload FROM watch_observations WHERE symbol=$1 ORDER BY day DESC LIMIT 1",[symbol],|r|r.get(0)).optional()?;
            let (checked_at, refresh_failed) = conn.query_row(
                "SELECT checked_at,refresh_failed FROM watch_entries WHERE symbol=$1",
                [symbol],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)? != 0)),
            )?;
            let checked_for = conn.query_row(
                "SELECT checked_for FROM watch_entries WHERE symbol=$1",
                [symbol],
                |r| r.get::<_, Option<String>>(0),
            )?;
            let alert = conn.query_row("SELECT EXISTS(SELECT 1 FROM watch_observations WHERE symbol=$1 AND alert=1 AND read_at IS NULL)",[symbol],|r|r.get(0))?;
            rows.push(WatchRow {
                entry,
                latest: latest.as_deref().map(serde_json::from_str).transpose()?,
                alert,
                checked_at,
                checked_for,
                refresh_failed,
            });
        }
        let mut stmt = conn.prepare("SELECT payload FROM watch_observations WHERE day >= $1 AND day <= $2 ORDER BY day,symbol")?;
        let payloads = stmt
            .query_map(params![start.to_string(), end.to_string()], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let weekly = payloads
            .iter()
            .map(|p| serde_json::from_str(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(WatchBoard {
            rows,
            weekly,
            week_start: start.to_string(),
            week_end: end.to_string(),
            min_yield_pct: MIN_YIELD_PCT,
            band_distance_pct: BAND_DISTANCE_PCT,
        })
    }

    pub fn save_watch_observation(
        &self,
        expected: &WatchEntry,
        result: Option<&Observation>,
        checked_at: &str,
        checked_for: NaiveDate,
        dividends: Option<&DividendHistory>,
    ) -> AppResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let symbol = &expected.input.symbol;
        let current: Option<String> = tx
            .query_row(
                "SELECT payload FROM watch_entries WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?;
        if current
            .as_deref()
            .map(serde_json::from_str::<WatchEntry>)
            .transpose()?
            .as_ref()
            != Some(expected)
        {
            return Ok(());
        }
        let last_checked: Option<String> = tx.query_row(
            "SELECT checked_at FROM watch_entries WHERE symbol=$1",
            [symbol],
            |r| r.get(0),
        )?;
        if last_checked.and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
            > chrono::DateTime::parse_from_rfc3339(checked_at).ok()
        {
            return Ok(());
        }
        if let Some(history) = dividends {
            history.validate()?;
            let stored: Option<String> = tx
                .query_row(
                    "SELECT payload FROM watch_dividends WHERE symbol=$1",
                    [symbol],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(stored) = stored {
                let stored: DividendHistory = serde_json::from_str(&stored)?;
                if stored.is_newer_than(history) {
                    // A later price request may still have read the old cache while
                    // another refresh was fetching a corrected distribution.
                    return Ok(());
                }
            }
            // A slower in-flight request must not overwrite more recent source coverage.
            tx.execute("INSERT INTO watch_dividends(symbol,payload) VALUES ($1,$2) ON CONFLICT(symbol) DO UPDATE SET payload=excluded.payload WHERE (watch_dividends.payload::jsonb->>'asOf') < (excluded.payload::jsonb->>'asOf') OR ((watch_dividends.payload::jsonb->>'asOf') = (excluded.payload::jsonb->>'asOf') AND (watch_dividends.payload::jsonb->>'fetchedAt')::timestamptz < (excluded.payload::jsonb->>'fetchedAt')::timestamptz)",params![symbol,serde_json::to_string(history)?])?;
        }
        if let Some(observation) = result {
            let previous: Option<String> = tx.query_row("SELECT payload FROM watch_observations WHERE symbol=$1 ORDER BY day DESC LIMIT 1",[symbol],|r|r.get(0)).optional()?;
            let previous = previous
                .as_deref()
                .map(serde_json::from_str::<Observation>)
                .transpose()?;
            if previous
                .as_ref()
                .is_some_and(|p| p.observed_on > observation.observed_on)
            {
                return Ok(());
            }
            // Unknown data is not evidence that a price signal ended.
            let known: Option<String> = tx.query_row("SELECT payload FROM watch_observations WHERE symbol=$1 AND payload::jsonb->>'status' <> 'incomplete' ORDER BY day DESC LIMIT 1",[symbol],|r|r.get(0)).optional()?;
            let known = known
                .as_deref()
                .map(serde_json::from_str::<Observation>)
                .transpose()?;
            let alert = observation.status != WatchStatus::Incomplete
                && match known {
                    Some(v) => v.status != observation.status,
                    None => observation.status == WatchStatus::Attention,
                };
            let changed = previous.as_ref().is_none_or(|p| {
                let mut p = p.clone();
                p.checked_at = observation.checked_at.clone();
                p != *observation
            });
            if changed {
                tx.execute("INSERT INTO watch_observations(symbol,day,payload,alert,read_at) VALUES ($1,$2,$3,$4,NULL) ON CONFLICT(symbol,day) DO UPDATE SET payload=excluded.payload, alert=GREATEST(watch_observations.alert,excluded.alert), read_at=CASE WHEN excluded.alert=1 THEN NULL ELSE watch_observations.read_at END",params![symbol,observation.observed_on,serde_json::to_string(observation)?,i64::from(alert)])?;
            }
        }
        tx.execute(
            "UPDATE watch_entries SET checked_at=$2,refresh_failed=$3,checked_for=$4 WHERE symbol=$1",
            params![symbol, checked_at, i64::from(result.is_none()),checked_for.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn validate(conn: &crate::storage::Connection) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT symbol,payload FROM watch_entries")?;
    let entries = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    if entries.len() > MAX_WATCHES {
        return Err(AppError::Validation("观察名单超出上限".into()));
    }
    for (symbol, payload) in entries {
        let entry: WatchEntry = serde_json::from_str(&payload)?;
        if entry.input.clone().normalize()? != entry.input
            || symbol != entry.input.symbol
            || chrono::DateTime::parse_from_rfc3339(&entry.created_at).is_err()
        {
            return Err(AppError::Validation("观察名单备份无效".into()));
        }
    }
    let mut stmt =
        conn.prepare("SELECT symbol,day,payload,alert,read_at FROM watch_observations")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (symbol, day, payload, alert, read_at) in rows {
        let value: Observation = serde_json::from_str(&payload)?;
        if symbol != value.symbol
            || day != value.observed_on
            || NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_err()
            || ![0, 1].contains(&alert)
            || read_at.is_some_and(|v| chrono::DateTime::parse_from_rfc3339(&v).is_err())
            || chrono::DateTime::parse_from_rfc3339(&value.checked_at).is_err()
        {
            return Err(AppError::Validation("观察历史备份无效".into()));
        }
    }
    let mut stmt = conn.prepare("SELECT payload FROM watch_dividends")?;
    let histories = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for payload in histories {
        serde_json::from_str::<DividendHistory>(&payload)?.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::watch::{evaluate, tests::market_fixture, WatchIssue};
    fn entry(db: &Database) -> WatchEntry {
        db.save_watch(WatchInput {
            symbol: "600036.SS".into(),
            name: "测试公司".into(),
            thesis: "长期研究".into(),
        })
        .unwrap()
    }
    fn history() -> DividendHistory {
        let (mut data, day) = market_fixture();
        for e in data.dividends.as_mut().unwrap() {
            e.report_date = Some(e.ex_date - Duration::days(180));
        }
        DividendHistory {
            basis: crate::models::watch::DividendBasis {
                as_of: day,
                fetched_at: "2026-07-03T12:00:00Z".into(),
            },
            sources: vec!["https://example.test/dividends".into()],
            events: data.dividends.unwrap(),
        }
    }
    #[test]
    fn dividend_history_survives_migration_backup_and_corrections_without_duplicates() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let entry = entry(&db);
        let mut history = history();
        let day = history.basis.as_of;
        db.save_watch_observation(&entry, None, &history.basis.fetched_at, day, Some(&history))
            .unwrap();
        assert_eq!(
            db.watch_dividends(&entry.input.symbol).unwrap(),
            Some(history.clone())
        );
        assert!(Database::test()
            .watch_dividends(&entry.input.symbol)
            .unwrap()
            .is_none());
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(
            db.watch_dividends(&entry.input.symbol).unwrap(),
            Some(history.clone())
        );
        let target = Database::test();
        let backup = db.export_sync_data().unwrap();
        target.import_sync_data(&backup).unwrap();
        assert_eq!(
            target.watch_dividends(&entry.input.symbol).unwrap(),
            Some(history.clone())
        );
        history.basis.fetched_at = "2026-07-03T13:00:00Z".into();
        history.events[0].cash_per_ten = "60".into();
        db.save_watch_observation(&entry, None, &history.basis.fetched_at, day, Some(&history))
            .unwrap();
        assert_eq!(
            db.watch_dividends(&entry.input.symbol)
                .unwrap()
                .unwrap()
                .events
                .len(),
            1
        );
        // A later validated empty result corrects the source; failure is never an empty result.
        history.events.clear();
        history.basis.fetched_at = "2026-07-03T14:00:00Z".into();
        db.save_watch_observation(&entry, None, &history.basis.fetched_at, day, Some(&history))
            .unwrap();
        assert!(db
            .watch_dividends(&entry.input.symbol)
            .unwrap()
            .unwrap()
            .events
            .is_empty());
        let older = self::history();
        let (data, _) = market_fixture();
        let stale = evaluate(
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T15:00:00Z".into(),
        );
        db.save_watch_observation(&entry, Some(&stale), &stale.checked_at, day, Some(&older))
            .unwrap();
        assert!(db.watch_board(day).unwrap().rows[0].latest.is_none());
        db.save_watch_observation(&entry, None, &older.basis.fetched_at, day, Some(&older))
            .unwrap();
        assert_eq!(
            db.watch_dividends(&entry.input.symbol).unwrap(),
            Some(history)
        );
        // v24 observations cannot prove a complete raw event history: fetch it again, never infer zero.
        let mut old = backup.clone();
        old.schema_version = 24;
        old.tables.retain(|t| t.name != "watch_dividends");
        target.import_sync_data(&old).unwrap();
        assert!(target
            .watch_dividends(&entry.input.symbol)
            .unwrap()
            .is_none());
        assert_eq!(target.watch_entries().unwrap(), vec![entry.clone()]);
        let mut corrupt = backup;
        let row = &mut corrupt
            .tables
            .iter_mut()
            .find(|t| t.name == "watch_dividends")
            .unwrap()
            .rows[0];
        let mut invalid = older.clone();
        invalid.events[0].cash_per_ten = "-1".into();
        row[1] = super::super::SyncValue::Text(serde_json::to_string(&invalid).unwrap());
        assert!(target.import_sync_data(&corrupt).is_err());
        assert!(target
            .watch_dividends(&entry.input.symbol)
            .unwrap()
            .is_none());
        db.delete_watch(&entry.input.symbol).unwrap();
        db.save_watch_observation(&entry, None, &older.basis.fetched_at, day, Some(&older))
            .unwrap();
        assert!(db.watch_dividends(&entry.input.symbol).unwrap().is_none());
    }

    #[test]
    fn observation_failure_rolls_back_dividend_update_too() {
        let db = Database::test();
        let entry = entry(&db);
        let history = history();
        let (data, day) = market_fixture();
        let observation = evaluate(
            &entry.input.symbol,
            &data,
            day,
            history.basis.fetched_at.clone(),
        );
        let revision = db.revision().unwrap();
        super::super::fail_inserts(
            &db.conn().unwrap(),
            "watch_observations",
            "injected failure",
        );
        assert!(db
            .save_watch_observation(
                &entry,
                Some(&observation),
                &history.basis.fetched_at,
                day,
                Some(&history)
            )
            .is_err());
        assert!(db.watch_dividends(&entry.input.symbol).unwrap().is_none());
        assert_eq!(db.revision().unwrap(), revision);
    }

    #[test]
    fn alerts_deduplicate_recover_from_gaps_and_rearm_after_leaving() {
        let db = Database::test();
        let entry = entry(&db);
        let (data, day) = market_fixture();
        let mut observation = evaluate(
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        let save = |o: &Observation| {
            db.save_watch_observation(&entry, Some(o), &o.checked_at, day, None)
                .unwrap()
        };
        save(&observation);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
        db.read_watch_alerts(&entry.input.symbol).unwrap();
        save(&observation);
        assert!(!db.watch_board(day).unwrap().rows[0].alert);
        observation.observed_on = (day + Duration::days(3)).to_string();
        observation.status = WatchStatus::Incomplete;
        observation.issues = vec![WatchIssue::DividendUnavailable];
        save(&observation);
        assert!(!db.watch_board(day).unwrap().rows[0].alert);
        observation.status = WatchStatus::Attention;
        observation.issues.clear();
        save(&observation);
        assert!(!db.watch_board(day).unwrap().rows[0].alert);
        observation.observed_on = (day + Duration::days(4)).to_string();
        observation.status = WatchStatus::Waiting;
        save(&observation);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
        db.read_watch_alerts(&entry.input.symbol).unwrap();
        observation.observed_on = (day + Duration::days(5)).to_string();
        observation.status = WatchStatus::Attention;
        save(&observation);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
    }
    #[test]
    fn watches_are_isolated_restored_and_deleted_without_financial_writes() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let other = Database::test();
        let before = db.snapshot().unwrap();
        let entry = entry(&db);
        let (data, day) = market_fixture();
        let observation = evaluate(
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        db.save_watch_observation(
            &entry,
            Some(&observation),
            &observation.checked_at,
            day,
            None,
        )
        .unwrap();
        assert!(other.watch_entries().unwrap().is_empty());
        let backup = db.export_sync_data().unwrap();
        other.import_sync_data(&backup).unwrap();
        assert_eq!(
            other.watch_board(day).unwrap().rows[0].latest,
            Some(observation.clone())
        );
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(db.watch_entries().unwrap(), vec![entry.clone()]);
        db.delete_watch(&entry.input.symbol).unwrap();
        db.save_watch_observation(
            &entry,
            Some(&observation),
            &observation.checked_at,
            day,
            None,
        )
        .unwrap();
        assert!(db.watch_entries().unwrap().is_empty());
        let after = db.snapshot().unwrap();
        assert_eq!(
            serde_json::to_value(before.holdings).unwrap(),
            serde_json::to_value(after.holdings).unwrap()
        );
        assert_eq!(before.holding_trades, after.holding_trades);
        // v23 backups have no watch tables; restoring them deliberately restores an empty list.
        let mut old = backup;
        old.schema_version = 23;
        old.tables.retain(|t| !t.name.starts_with("watch_"));
        other.import_sync_data(&old).unwrap();
        assert!(other.watch_entries().unwrap().is_empty());
    }
    #[test]
    fn corrupt_restore_rolls_back_and_failure_preserves_previous_observation() {
        let db = Database::test();
        let entry = entry(&db);
        let (data, day) = market_fixture();
        let observation = evaluate(
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        db.save_watch_observation(
            &entry,
            Some(&observation),
            &observation.checked_at,
            day,
            None,
        )
        .unwrap();
        db.save_watch_observation(
            &entry,
            None,
            "2026-07-04T12:00:00Z",
            day + Duration::days(1),
            None,
        )
        .unwrap();
        let board = db.watch_board(day).unwrap();
        assert!(board.rows[0].refresh_failed);
        assert_eq!(board.rows[0].latest, Some(observation));
        let mut backup = db.export_sync_data().unwrap();
        let row = &mut backup
            .tables
            .iter_mut()
            .find(|t| t.name == "watch_entries")
            .unwrap()
            .rows[0];
        row[1] = super::super::SyncValue::Text("{}".into());
        assert!(db.import_sync_data(&backup).is_err());
        assert_eq!(db.watch_entries().unwrap(), vec![entry]);
    }
}
