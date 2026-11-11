//! Observation preferences share the account settings table and its backup transaction.
use super::{AppResult, Database, OptionalRow};
use crate::{models::watch::rules::WatchRules, storage::Connection};

pub(super) fn load(conn: &Connection) -> AppResult<WatchRules> {
    let payload: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='watch.rules'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let rules = payload
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    WatchRules::validate(&rules)?;
    Ok(rules)
}

impl Database {
    pub fn watch_rules(&self) -> AppResult<WatchRules> {
        let conn = self.conn()?;
        load(&conn)
    }

    pub fn save_watch_rules(&self, rules: WatchRules, expected: WatchRules) -> AppResult<()> {
        rules.validate()?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let current = load(&tx)?;
        if current != expected {
            return Err(crate::AppError::Conflict(
                "观察规则已被更新，请重新读取后核对；当前草稿已保留".into(),
            ));
        }
        if current == rules {
            return Ok(());
        }
        tx.execute("INSERT INTO settings(key,value) VALUES ('watch.rules',$1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&rules)?])?;
        // Rule edits establish a new baseline; they are not market entry/exit events.
        tx.execute(
            "UPDATE watch_observations SET alert=0,read_at=NULL WHERE alert<>0",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::SyncValue,
        models::watch::{evaluate, tests::market_fixture, WatchInput, WatchStatus},
    };

    #[test]
    fn account_rules_project_current_facts_preserve_history_and_survive_restart_and_backups() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let other = Database::test();
        let entry = db
            .save_watch(WatchInput {
                symbol: "600036.SS".into(),
                name: "测试公司".into(),
                thesis: String::new(),
            })
            .unwrap();
        let grid = crate::models::watch::grid::GridPlan {
            budget: "1000".into(),
            base_price: "10".into(),
            size_unit: crate::models::watch::grid::SizeUnit::BudgetPct,
            levels: vec![crate::models::watch::grid::GridLevel {
                price: "9".into(),
                size: "50".into(),
            }],
        };
        db.set_watch_grid(&entry.input.symbol, Some(grid.clone()))
            .unwrap();
        let (data, day) = market_fixture();
        let original = evaluate(
            &WatchRules::default(),
            &entry.input.symbol,
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        db.save_watch_observation(&entry, Some(&original), &original.checked_at, day, None)
            .unwrap();
        let before = db.watch_board(day).unwrap();
        assert!(before.rows[0].alert);
        let rules = WatchRules {
            min_yield_bps: 600,
            ..WatchRules::default()
        };
        db.save_watch_rules(rules, WatchRules::default()).unwrap();
        let board = db.watch_board(day).unwrap();
        assert_eq!(board.rules, rules);
        assert_eq!(
            board.rows[0].latest.as_ref().unwrap().status,
            WatchStatus::Waiting
        );
        assert!(!board.rows[0].alert);
        assert_eq!(
            board.weekly, before.weekly,
            "rule edits never rewrite historical observations"
        );
        assert_eq!(other.watch_rules().unwrap(), WatchRules::default());
        assert!(other.watch_entries().unwrap().is_empty());
        // An in-flight computation must adopt the current rules when it commits.
        db.save_watch_observation(&entry, Some(&original), &original.checked_at, day, None)
            .unwrap();
        assert_eq!(
            db.watch_board(day).unwrap().rows[0]
                .latest
                .as_ref()
                .unwrap()
                .rules,
            rules
        );
        assert!(!db.watch_board(day).unwrap().rows[0].alert);
        let backup = db.export_sync_data().unwrap();
        assert_eq!(backup.schema_version, 29);
        other.import_sync_data(&backup).unwrap();
        assert_eq!(other.watch_rules().unwrap(), rules);
        drop(db);
        let reopened = account.reopen_from_old_release();
        assert_eq!(reopened.watch_rules().unwrap(), rules);
        // Old-format backups without preferences restore the original defaults.
        let mut old = backup.clone();
        old.schema_version = 27;
        old.tables
            .iter_mut()
            .find(|t| t.name == "settings")
            .unwrap()
            .rows
            .retain(|r| r[0] != SyncValue::Text("watch.rules".into()));
        other.import_sync_data(&old).unwrap();
        assert_eq!(other.watch_rules().unwrap(), WatchRules::default());
        assert_eq!(other.watch_entries().unwrap()[0].grid_plan, Some(grid));
        let mut corrupt = backup;
        let row = corrupt
            .tables
            .iter_mut()
            .find(|t| t.name == "settings")
            .unwrap()
            .rows
            .iter_mut()
            .find(|r| r[0] == SyncValue::Text("watch.rules".into()))
            .unwrap();
        row[1] = SyncValue::Text("{}".into());
        assert!(other.import_sync_data(&corrupt).is_err());
        assert_eq!(other.watch_rules().unwrap(), WatchRules::default());
    }

    #[test]
    fn conflict_invalid_values_and_write_failure_keep_configuration_and_alerts_atomic() {
        let db = Database::test();
        let changed = WatchRules {
            min_yield_bps: 325,
            ..WatchRules::default()
        };
        db.save_watch_rules(changed, WatchRules::default()).unwrap();
        let revision = db.revision().unwrap();
        assert!(matches!(
            db.save_watch_rules(WatchRules::default(), WatchRules::default()),
            Err(crate::AppError::Conflict(_))
        ));
        assert!(db
            .save_watch_rules(
                WatchRules {
                    min_yield_bps: 0,
                    ..changed
                },
                changed
            )
            .is_err());
        assert_eq!(db.revision().unwrap(), revision);
        let entry = db
            .save_watch(WatchInput {
                symbol: "600036.SS".into(),
                name: "测试公司".into(),
                thesis: String::new(),
            })
            .unwrap();
        let (data, day) = market_fixture();
        let observation = evaluate(
            &changed,
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
        assert!(db.watch_board(day).unwrap().rows[0].alert);
        let revision = db.revision().unwrap();
        crate::db::fail_inserts(
            &db.conn().unwrap(),
            "settings",
            "injected rule save failure",
        );
        assert!(db.save_watch_rules(WatchRules::default(), changed).is_err());
        assert_eq!(db.watch_rules().unwrap(), changed);
        assert_eq!(db.revision().unwrap(), revision);
        assert!(db.watch_board(day).unwrap().rows[0].alert);
    }
}
