//! Retain old percentage plans as historical evidence; never invent a budget or share count.
//! Remove only when old tenant data and v24-v26 backups are no longer supported.
use super::params;
use crate::{models::watch::grid_archive::GridAllocationArchive, storage::Connection, AppResult};

// The caller owns the tenant-upgrade or backup-import transaction.
pub(super) fn migrate(conn: &Connection) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "SELECT symbol,payload FROM watch_entries WHERE payload::jsonb->'gridPlan' ? 'buyBasis'",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (symbol, payload) in rows {
        let mut entry: serde_json::Value = serde_json::from_str(&payload)?;
        let old = entry
            .as_object_mut()
            .expect("entry with gridPlan is an object")
            .remove("gridPlan")
            .unwrap();
        let archive: GridAllocationArchive = serde_json::from_value(old.clone())?;
        archive.validate()?;
        if entry
            .get("previousGridPlan")
            .is_some_and(|saved| saved != &old)
        {
            return Err(crate::AppError::Validation(
                "旧网格历史参考冲突，迁移未覆盖任何数据".into(),
            ));
        }
        entry["previousGridPlan"] = old;
        conn.execute(
            "UPDATE watch_entries SET payload=$2 WHERE symbol=$1",
            params![symbol, serde_json::to_string(&entry)?],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::{Database, SyncValue, TestAccount};
    use crate::models::watch::{
        grid::{GridLevel, GridPlan},
        WatchInput,
    };
    use serde_json::json;

    fn old_plan() -> serde_json::Value {
        json!({"buyBasis":"remainingCash","sellBasis":"remainingShares","levels":[{"yieldBps":550,"buyBps":2500,"sellBps":1000}]})
    }
    fn create(db: &Database) {
        db.save_watch(WatchInput {
            symbol: "600036.SS".into(),
            name: "测试公司".into(),
            thesis: "保留理由".into(),
        })
        .unwrap();
        db.set_watch_pinned("600036.SS", true).unwrap();
    }
    #[test]
    fn upgrade_and_old_backup_retain_percentages_without_inventing_shares() {
        let account = TestAccount::new();
        let db = account.open();
        create(&db);
        let mut value = serde_json::to_value(&db.watch_entries().unwrap()[0]).unwrap();
        value["gridPlan"] = old_plan();
        db.conn()
            .unwrap()
            .execute("UPDATE watch_entries SET payload=$1", [value.to_string()])
            .unwrap();
        let mut backup = db.export_sync_data().unwrap();
        backup.schema_version = 26;
        drop(db);
        let db = account.reopen_from_old_release();
        let migrated = db.watch_entries().unwrap().remove(0);
        assert!(migrated.grid_plan.is_none());
        assert!(migrated.pinned);
        assert_eq!(migrated.input.thesis, "保留理由");
        assert_eq!(
            serde_json::to_value(&migrated.previous_grid_plan).unwrap(),
            old_plan()
        );
        let target = Database::test();
        target.import_sync_data(&backup).unwrap();
        assert_eq!(target.watch_entries().unwrap(), vec![migrated.clone()]);
        let plan = GridPlan {
            budget: "12345.67".into(),
            generator: None,
            levels: vec![GridLevel {
                yield_bps: 725,
                buy_shares: 123,
                sell_shares: 0,
            }],
        };
        target
            .set_watch_grid("600036.SS", Some(plan.clone()))
            .unwrap();
        assert_eq!(
            target.watch_entries().unwrap()[0].previous_grid_plan,
            migrated.previous_grid_plan
        );
        let latest = target.export_sync_data().unwrap();
        assert_eq!(
            latest.schema_version,
            super::super::SYNC_DATASET_SCHEMA_VERSION
        );
        db.import_sync_data(&latest).unwrap();
        assert_eq!(db.watch_entries().unwrap()[0].grid_plan, Some(plan));
        target.import_sync_data(&backup).unwrap();
        assert_eq!(target.watch_entries().unwrap(), vec![migrated]);
    }
    #[test]
    fn malformed_old_or_new_grid_rolls_back_the_entire_import() {
        let db = Database::test();
        create(&db);
        let original = db.export_sync_data().unwrap();
        for (version, invalid) in [
            (
                26,
                json!({"buyBasis":"invalid","sellBasis":"remainingShares","levels":[]}),
            ),
            (
                27,
                json!({"budget":"0","levels":[{"yieldBps":500,"buyShares":1,"sellShares":0}]}),
            ),
        ] {
            let mut backup = original.clone();
            backup.schema_version = version;
            let table = backup
                .tables
                .iter_mut()
                .find(|t| t.name == "watch_entries")
                .unwrap();
            let SyncValue::Text(payload) = &mut table.rows[0][1] else {
                panic!()
            };
            let mut entry: serde_json::Value = serde_json::from_str(payload).unwrap();
            entry["gridPlan"] = invalid;
            *payload = entry.to_string();
            assert!(db.import_sync_data(&backup).is_err());
            assert_eq!(db.export_sync_data().unwrap().tables, original.tables);
        }
    }
}
