use super::{params, AppResult, Database, OptionalRow};
mod rules;
use crate::models::watch::grid::GridPlan;
use crate::models::watch::{
    dividend_prices, DividendHistory, Observation, WatchBoard, WatchEntry, WatchInput, WatchRow,
    WatchStatus, MAX_WATCHES,
};
use crate::AppError;
use chrono::{Datelike, Duration, NaiveDate, Utc};

/// One query for entry state, the latest observation and unread status. The list
/// projection discards heavy evidence in PostgreSQL before transfer and JSON parsing.
fn watch_rows(
    conn: &crate::storage::Connection,
    symbol: Option<&str>,
    rules: &crate::models::watch::rules::WatchRules,
) -> AppResult<Vec<WatchRow>> {
    let values = conn.prepare(
        "SELECT e.payload,e.checked_at,e.checked_for,e.refresh_failed,
          CASE WHEN $1::text IS NULL THEN (o.payload::jsonb - 'trends' - 'dividendHistory')::text ELSE o.payload END,
          EXISTS(SELECT 1 FROM watch_observations a WHERE a.symbol=e.symbol AND a.alert=1 AND a.read_at IS NULL)
         FROM watch_entries e
         LEFT JOIN LATERAL (SELECT payload FROM watch_observations WHERE symbol=e.symbol ORDER BY day DESC LIMIT 1) o ON TRUE
         WHERE ($1::text IS NULL OR e.symbol=$1) ORDER BY e.symbol"
    )?.query_map([symbol], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,i64>(3)?!=0,r.get::<_,Option<String>>(4)?,r.get::<_,bool>(5)?)))?
    .collect::<Result<Vec<_>,_>>()?;
    values
        .into_iter()
        .map(
            |(entry, checked_at, checked_for, refresh_failed, latest, alert)| {
                let entry: WatchEntry = serde_json::from_str(&entry)?;
                let mut latest: Option<Observation> =
                    latest.as_deref().map(serde_json::from_str).transpose()?;
                if let Some(observation) = &mut latest {
                    observation.apply_rules(rules);
                }
                let dividend_prices = dividend_prices(latest.as_ref());
                let grid_projection = entry.grid_plan.as_ref().map(GridPlan::project);
                Ok(WatchRow {
                    entry,
                    latest,
                    dividend_prices,
                    grid_projection,
                    alert,
                    checked_at,
                    checked_for,
                    refresh_failed,
                })
            },
        )
        .collect()
}

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
        self.save_watch_entry(input, false)
    }

    pub fn add_watch_if_missing(&self, input: WatchInput) -> AppResult<WatchEntry> {
        self.save_watch_entry(input, true)
    }

    fn save_watch_entry(&self, input: WatchInput, if_absent: bool) -> AppResult<WatchEntry> {
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
        let old = old
            .as_deref()
            .map(serde_json::from_str::<WatchEntry>)
            .transpose()?;
        if if_absent {
            if let Some(entry) = old {
                return Ok(entry);
            }
        }
        let entry = match old {
            Some(entry) => WatchEntry { input, ..entry },
            None => WatchEntry {
                input,
                created_at: Utc::now().to_rfc3339(),
                pinned: false,
                grid_plan: None,
            },
        };
        tx.execute("INSERT INTO watch_entries(symbol,payload,checked_at,refresh_failed) VALUES ($1,$2,NULL,0) ON CONFLICT(symbol) DO UPDATE SET payload=excluded.payload",params![entry.input.symbol,serde_json::to_string(&entry)?])?;
        tx.commit()?;
        Ok(entry)
    }

    pub fn set_watch_pinned(&self, symbol: &str, pinned: bool) -> AppResult<WatchEntry> {
        self.update_watch_entry(symbol, |entry| entry.pinned = pinned)
    }

    pub fn set_watch_grid(&self, symbol: &str, plan: Option<GridPlan>) -> AppResult<()> {
        if let Some(plan) = &plan {
            plan.validate()?;
        }
        self.update_watch_entry(symbol, |entry| entry.grid_plan = plan)?;
        Ok(())
    }

    fn update_watch_entry(
        &self,
        symbol: &str,
        update: impl FnOnce(&mut WatchEntry),
    ) -> AppResult<WatchEntry> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let payload: String = tx
            .query_row(
                "SELECT payload FROM watch_entries WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("观察公司不存在".into()))?;
        let original: WatchEntry = serde_json::from_str(&payload)?;
        let mut entry = original.clone();
        update(&mut entry);
        if entry != original {
            tx.execute(
                "UPDATE watch_entries SET payload=$2 WHERE symbol=$1",
                params![symbol, serde_json::to_string(&entry)?],
            )?;
        }
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
        let rules = self.watch_rules()?;
        let conn = self.conn()?;
        let rows = watch_rows(&conn, None, &rules)?;
        let mut stmt = conn.prepare("SELECT (payload::jsonb - 'trends' - 'dividendHistory')::text FROM watch_observations WHERE day >= $1 AND day <= $2 ORDER BY day,symbol")?;
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
            default_rules: Default::default(),
            rows,
            weekly,
            week_start: start.to_string(),
            week_end: end.to_string(),
            rules,
        })
    }

    /// Full evidence is fetched for one existing account entry, never for the whole list.
    pub fn watch_row(&self, symbol: &str) -> AppResult<WatchRow> {
        let symbol = crate::models::market::canonical_symbol(symbol)?;
        let rules = self.watch_rules()?;
        watch_rows(&*self.conn()?, Some(&symbol), &rules)?
            .pop()
            .ok_or_else(|| AppError::NotFound("观察公司不存在".into()))
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
            .is_none_or(|entry| {
                entry.input != expected.input || entry.created_at != expected.created_at
            })
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
            let rules = rules::load(&tx)?;
            let mut current_observation = observation.clone();
            current_observation.apply_rules(&rules);
            let observation = &current_observation;
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
            let mut stmt = tx.prepare(
                "SELECT payload FROM watch_observations WHERE symbol=$1 ORDER BY day DESC",
            )?;
            let payloads = stmt
                .query_map([symbol], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            let mut known = None;
            for payload in payloads {
                let mut prior: Observation = serde_json::from_str(&payload)?;
                prior.apply_rules(&rules);
                if prior.status != WatchStatus::Incomplete {
                    known = Some(prior);
                    break;
                }
            }
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

/// Only for tenants/backups with the retired two-lower-band payload. Remove when
/// those backups are no longer supported. The caller owns the migration/import transaction.
pub(super) fn migrate_rules(conn: &crate::storage::Connection) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT symbol,day,payload FROM watch_observations WHERE (payload::jsonb->'daily' ? 'nearLower') OR (payload::jsonb->'weekly' ? 'nearLower')")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (symbol, day, payload) in rows {
        let mut observation: Observation = serde_json::from_str(&payload)?;
        observation.apply_rules(&Default::default());
        conn.execute("UPDATE watch_observations SET payload=$3,alert=0,read_at=NULL WHERE symbol=$1 AND day=$2", params![symbol, day, serde_json::to_string(&observation)?])?;
        conn.execute(
            "UPDATE watch_entries SET checked_for=NULL WHERE symbol=$1",
            [symbol],
        )?;
    }
    Ok(())
}

pub(super) fn validate(conn: &crate::storage::Connection) -> AppResult<()> {
    rules::load(conn)?;
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
        if let Some(plan) = entry.grid_plan {
            plan.validate()?;
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
        value.rules.validate()?;
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

    #[test]
    fn catalogue_addition_preserves_an_existing_personal_entry_even_at_the_limit() {
        let db = Database::test();
        let original = entry(&db);
        db.set_watch_pinned(&original.input.symbol, true).unwrap();
        for n in 100..149 {
            db.save_watch(WatchInput {
                symbol: format!("600{n:03}.SS"),
                name: format!("公司{n}"),
                thesis: String::new(),
            })
            .unwrap();
        }
        let before = db.watch_entries().unwrap();
        let version = db.revision().unwrap();
        let added = db
            .add_watch_if_missing(WatchInput {
                symbol: original.input.symbol.clone(),
                name: "目录名称".into(),
                thesis: String::new(),
            })
            .unwrap();
        assert_eq!(added.input, original.input);
        assert!(added.pinned);
        assert_eq!(db.revision().unwrap(), version);
        assert_eq!(db.watch_entries().unwrap(), before);
        assert!(db
            .add_watch_if_missing(WatchInput {
                symbol: "601398.SS".into(),
                name: "工商银行".into(),
                thesis: String::new()
            })
            .is_err());
    }

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
    fn list_and_weekly_reads_remain_small_and_detail_preserves_five_year_evidence() {
        let db = Database::test();
        let (mut data, end) = market_fixture();
        let start = crate::models::market::history::history_start(end);
        data.calendar = (0..=(end - start).num_days())
            .map(|n| start + Duration::days(n))
            .filter(|d| d.weekday().num_days_from_monday() < 5)
            .collect();
        data.bars = data
            .calendar
            .iter()
            .map(|day| crate::models::watch::PriceBar {
                day: *day,
                close: Some(100.0),
                adjusted_close: Some(100.0),
            })
            .collect();
        let observation = evaluate(
            &Default::default(),
            "600036.SS",
            &data,
            end,
            "2026-07-03T12:00:00Z".into(),
        );
        assert!(observation.trends.as_ref().unwrap().len() >= 260);
        for n in 0..50 {
            let entry = db
                .save_watch(WatchInput {
                    symbol: format!("60{n:04}.SS"),
                    name: format!("公司{n}"),
                    thesis: "私有理由".into(),
                })
                .unwrap();
            for offset in (0..5).rev() {
                let day = end - Duration::days(offset);
                let mut stored = observation.clone();
                stored.symbol = entry.input.symbol.clone();
                stored.observed_on = day.to_string();
                db.save_watch_observation(&entry, Some(&stored), &stored.checked_at, day, None)
                    .unwrap();
            }
        }
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        let started = std::time::Instant::now();
        let board = db.watch_board(end).unwrap();
        let summary_bytes = serde_json::to_vec(&board).unwrap().len();
        let elapsed = started.elapsed();
        assert_eq!(board.rows.len(), 50);
        assert!(board
            .rows
            .iter()
            .all(|r| r.latest.as_ref().unwrap().trends.is_none()));
        assert!(board
            .weekly
            .iter()
            .all(|r| r.trends.is_none() && r.dividend_history.is_none()));
        assert_eq!(board.weekly.len(), 250);
        let full = db.watch_row("600000.SH").unwrap();
        assert!(full.latest.as_ref().unwrap().trends.as_ref().unwrap().len() >= 260);
        assert!(full.latest.as_ref().unwrap().dividend_history.is_some());
        let full_evidence_bytes = serde_json::to_vec(&full.latest).unwrap().len()
            * (board.rows.len() + board.weekly.len());
        assert!(summary_bytes * 10 < full_evidence_bytes);
        println!("watch list: 50 entries / 250 weekly observations, summary {} bytes, full observation bodies {} bytes, read+serialize {:?}",summary_bytes,full_evidence_bytes,elapsed);
        let stranger = Database::test();
        assert!(stranger.watch_row("600000.SS").is_err());
        assert!(db.watch_row("600999.SS").is_err());
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
    }

    #[test]
    fn retired_weekly_lower_payload_migrates_once_and_old_backups_cannot_restore_it() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let entry = entry(&db);
        let (mut data, day) = market_fixture();
        for bar in data.bars.iter_mut().take(80) {
            bar.adjusted_close = Some(200.0);
        }
        let observation = evaluate(
            &Default::default(),
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        let mut old = serde_json::to_value(&observation).unwrap();
        old["status"] = serde_json::json!("waiting");
        for name in ["daily", "weekly"] {
            let band = old[name].as_object_mut().unwrap();
            band.remove("reference");
            band.remove("matchesRule");
            band.insert("nearLower".into(), serde_json::json!(name == "daily"));
            band.insert("distancePct".into(), serde_json::json!(999.0));
        }
        db.conn().unwrap().execute("INSERT INTO watch_observations(symbol,day,payload,alert,read_at) VALUES ($1,$2,$3,1,NULL)", params![entry.input.symbol,day.to_string(),old.to_string()]).unwrap();
        let mut backup = db.export_sync_data().unwrap();
        backup.schema_version = 25;
        drop(db);
        let db = account.reopen_from_old_release();
        let board = db.watch_board(day).unwrap();
        let mut summary = observation.clone();
        summary.trends = None;
        summary.dividend_history = None;
        assert_eq!(board.rows[0].latest, Some(summary.clone()));
        assert_eq!(board.weekly, vec![summary]);
        assert_eq!(
            db.watch_row(&entry.input.symbol).unwrap().latest,
            Some(observation.clone())
        );
        assert!(!board.rows[0].alert);
        assert_eq!(board.rows[0].checked_for, None);
        let payload: String = db
            .conn()
            .unwrap()
            .query_row("SELECT payload FROM watch_observations", [], |r| r.get(0))
            .unwrap();
        assert!(!payload.contains("nearLower"));
        let revision = db.revision().unwrap();
        drop(db);
        let db = account.open();
        assert_eq!(db.revision().unwrap(), revision);
        let target = Database::test();
        target.import_sync_data(&backup).unwrap();
        assert_eq!(
            target.watch_row(&entry.input.symbol).unwrap().latest,
            Some(observation)
        );
        assert!(!target.watch_board(day).unwrap().rows[0].alert);
        // A failed migration/import restores the target's existing observations.
        let before = target.export_sync_data().unwrap();
        let table = backup
            .tables
            .iter_mut()
            .find(|t| t.name == "watch_observations")
            .unwrap();
        old["price"] = serde_json::json!("invalid");
        table.rows[0][2] = super::super::SyncValue::Text(old.to_string());
        assert!(target.import_sync_data(&backup).is_err());
        assert_eq!(target.export_sync_data().unwrap().tables, before.tables);
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
            &Default::default(),
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
            &Default::default(),
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
            &Default::default(),
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
        observation.dividend_per_share = Some("4".into());
        observation.yield_pct = Some(4.0);
        save(&observation);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
        db.read_watch_alerts(&entry.input.symbol).unwrap();
        observation.observed_on = (day + Duration::days(5)).to_string();
        observation.status = WatchStatus::Attention;
        observation.dividend_per_share = Some("5".into());
        observation.yield_pct = Some(5.0);
        save(&observation);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
    }
    #[test]
    fn watches_are_isolated_restored_and_deleted_without_financial_writes() {
        use crate::models::watch::grid::{GridLevel, SizeUnit};

        let account = crate::db::TestAccount::new();
        let db = account.open();
        let other = Database::test();
        let before = db.snapshot().unwrap();
        let entry = entry(&db);
        let plan = GridPlan {
            budget: "10000".into(),
            base_price: "10".into(),
            size_unit: SizeUnit::Amount,
            levels: vec![GridLevel {
                price: "9".into(),
                size: "950".into(),
            }],
        };
        db.set_watch_grid(&entry.input.symbol, Some(plan.clone()))
            .unwrap();
        let pinned = db.set_watch_pinned(&entry.input.symbol, true).unwrap();
        assert!(pinned.pinned);
        assert!(matches!(
            other.set_watch_pinned(&entry.input.symbol, true),
            Err(AppError::NotFound(_))
        ));
        let (data, day) = market_fixture();
        let observation = evaluate(
            &Default::default(),
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
        let restored = other.watch_board(day).unwrap().rows.remove(0);
        assert!(restored.entry.pinned);
        assert!(restored.latest.as_ref().unwrap().trends.is_none());
        assert_eq!(
            other.watch_row(&entry.input.symbol).unwrap().latest,
            Some(observation.clone())
        );
        assert_eq!(restored.entry.grid_plan, Some(plan));
        let projection = restored.grid_projection.unwrap();
        assert_eq!(projection.levels[0].shares, 100);
        assert_eq!(projection.scenarios[0].invested_amount, "900.00");
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(db.watch_entries().unwrap(), vec![pinned]);
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
        // Older backups may contain neither a pin preference nor a grid plan.
        let mut unpinned = backup.clone();
        let saved = &mut unpinned
            .tables
            .iter_mut()
            .find(|table| table.name == "watch_entries")
            .unwrap()
            .rows[0][1];
        let super::super::SyncValue::Text(payload) = saved else {
            panic!("watch entry must have a JSON payload");
        };
        let mut payload_value: serde_json::Value = serde_json::from_str(payload).unwrap();
        let fields = payload_value.as_object_mut().unwrap();
        fields.remove("pinned");
        fields.remove("gridPlan");
        *payload = payload_value.to_string();
        other.import_sync_data(&unpinned).unwrap();
        assert!(!other.watch_entries().unwrap()[0].pinned);
        assert!(other.watch_board(day).unwrap().rows[0]
            .grid_projection
            .is_none());
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
            &Default::default(),
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
        assert_eq!(board.rows[0].dividend_prices[1].price, "100.00");
        assert!(board.rows[0].latest.as_ref().unwrap().trends.is_none());
        assert_eq!(
            db.watch_row(&entry.input.symbol).unwrap().latest,
            Some(observation)
        );
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
