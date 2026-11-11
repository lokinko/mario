//! Drop yield-level grids and their percentage archive; the price grid cannot be inferred from them.
//! Remove only when tenants below schema 16 and backups below v29 are no longer supported.
use crate::{storage::Connection, AppResult};

// The caller owns the tenant-upgrade or backup-import transaction.
pub(super) fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "UPDATE watch_entries
         SET payload=(payload::jsonb - 'gridPlan' - 'previousGridPlan')::text
         WHERE payload::jsonb ? 'previousGridPlan'
            OR (payload::jsonb ? 'gridPlan' AND NOT payload::jsonb->'gridPlan' ? 'basePrice')",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::{Database, SyncValue, TestAccount};
    use crate::models::watch::{
        grid::{GridLevel, GridPlan, SizeUnit},
        WatchInput,
    };
    use serde_json::json;

    fn price_plan() -> GridPlan {
        GridPlan {
            budget: "12345.67".into(),
            base_price: "10".into(),
            size_unit: SizeUnit::Amount,
            levels: vec![GridLevel {
                price: "9".into(),
                size: "900".into(),
            }],
        }
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
    fn with_old_grids(db: &Database) {
        let mut value = serde_json::to_value(&db.watch_entries().unwrap()[0]).unwrap();
        value["gridPlan"] =
            json!({"budget":"10000","levels":[{"yieldBps":500,"buyShares":100,"sellShares":0}]});
        value["previousGridPlan"] =
            json!({"buyBasis":"budget","sellBasis":"totalBought","levels":[]});
        db.conn()
            .unwrap()
            .execute("UPDATE watch_entries SET payload=$1", [value.to_string()])
            .unwrap();
    }

    #[test]
    fn upgrade_and_old_backups_drop_yield_grids_but_keep_the_entry() {
        let account = TestAccount::new();
        let db = account.open();
        create(&db);
        with_old_grids(&db);
        let mut backup = db.export_sync_data().unwrap();
        backup.schema_version = 28;
        drop(db);
        let db = account.reopen_from_old_release();
        let migrated = db.watch_entries().unwrap().remove(0);
        assert!(migrated.grid_plan.is_none());
        assert!(migrated.pinned);
        assert_eq!(migrated.input.thesis, "保留理由");
        let payload: String = db
            .conn()
            .unwrap()
            .query_row("SELECT payload FROM watch_entries", [], |r| r.get(0))
            .unwrap();
        assert!(!payload.contains("previousGridPlan"));

        let target = Database::test();
        target.import_sync_data(&backup).unwrap();
        assert_eq!(target.watch_entries().unwrap(), vec![migrated]);
        target
            .set_watch_grid("600036.SS", Some(price_plan()))
            .unwrap();
        let latest = target.export_sync_data().unwrap();
        assert_eq!(
            latest.schema_version,
            super::super::SYNC_DATASET_SCHEMA_VERSION
        );
        drop(db);
        let db = account.reopen_from_old_release();
        db.import_sync_data(&latest).unwrap();
        assert_eq!(db.watch_entries().unwrap()[0].grid_plan, Some(price_plan()));
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(db.watch_entries().unwrap()[0].grid_plan, Some(price_plan()));
    }

    #[test]
    fn malformed_current_grid_rolls_back_the_entire_import() {
        let db = Database::test();
        create(&db);
        let original = db.export_sync_data().unwrap();
        for invalid in [
            json!({"budget":"0","basePrice":"10","sizeUnit":"amount","levels":[{"price":"9","size":"1"}]}),
            json!({"budget":"1","levels":[{"yieldBps":500,"buyShares":1,"sellShares":0}]}),
        ] {
            let mut backup = original.clone();
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
