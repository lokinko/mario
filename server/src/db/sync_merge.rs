use super::*;
use std::collections::BTreeMap;

pub(super) fn row_key(table: &SyncTable, row: &[SyncValue]) -> AppResult<String> {
    // Most records use the first column as their stable primary key. Rule revisions
    // are immutable children identified by (rule_id, revision), never by row order.
    let count = if table.name == "investment_rule_revisions" {
        2
    } else {
        1
    };
    Ok(serde_json::to_string(&row[..count])?)
}
fn same_record(a: Option<&Vec<SyncValue>>, b: Option<&Vec<SyncValue>>, columns: &[String]) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a
            .iter()
            .zip(b)
            .zip(columns)
            .all(|((a, b), column)| column == "updated_at" || a == b),
        _ => false,
    }
}
fn rows(table: &SyncTable) -> AppResult<BTreeMap<String, Vec<SyncValue>>> {
    let mut output = BTreeMap::new();
    for row in &table.rows {
        if output.insert(row_key(table, row)?, row.clone()).is_some() {
            return Err(AppError::Validation(format!(
                "同步数据存在重复主键：{}",
                table.name
            )));
        }
    }
    Ok(output)
}

impl Database {
    pub fn normalize_sync_dataset(dataset: &SyncDataset) -> AppResult<SyncDataset> {
        let connection = Connection::open_in_memory()?;
        schema::initialize(&connection)?;
        let db = Self {
            connection: Mutex::new(connection),
        };
        db.import_sync_data(dataset)?;
        db.export_sync_data()
    }

    pub fn merge_sync_data(
        base: Option<&SyncDataset>,
        local: &SyncDataset,
        remote: &SyncDataset,
    ) -> AppResult<SyncDataset> {
        let local = Self::normalize_sync_dataset(local)?;
        let remote = Self::normalize_sync_dataset(remote)?;
        let base = base.map(Self::normalize_sync_dataset).transpose()?;
        let mut merged = local.clone();
        let mut conflicts = Vec::new();
        for (index, table) in merged.tables.iter_mut().enumerate() {
            let local_rows = rows(&local.tables[index])?;
            let remote_rows = rows(&remote.tables[index])?;
            let base_rows = base
                .as_ref()
                .map(|b| rows(&b.tables[index]))
                .transpose()?
                .unwrap_or_default();
            let keys = local_rows
                .keys()
                .chain(remote_rows.keys())
                .chain(base_rows.keys())
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            table.rows.clear();
            for key in keys {
                let l = local_rows.get(&key);
                let r = remote_rows.get(&key);
                let b = base_rows.get(&key);
                let chosen =
                    if same_record(l, r, &table.columns) || same_record(l, b, &table.columns) {
                        r
                    } else if same_record(r, b, &table.columns) {
                        l
                    } else {
                        conflicts.push(format!("{} {}", table.name, key));
                        continue;
                    };
                if let Some(row) = chosen {
                    table.rows.push(row.clone());
                }
            }
        }
        if !conflicts.is_empty() {
            return Err(AppError::Conflict(format!("同一记录存在双向修改或删除与修改冲突，未覆盖任何数据：{}。请先在一台设备核对并统一这些记录，再重试同步；也可明确选择用云端恢复本机。", conflicts.into_iter().take(8).collect::<Vec<_>>().join("；"))));
        }
        // Canonical SQL ordering, foreign keys and domain invariants are checked
        // before either the remote write or any mutation of the real database.
        Self::normalize_sync_dataset(&merged).map_err(|error| {
            AppError::Conflict(format!("合并后记录关联不一致，未覆盖任何数据：{error}"))
        })
    }

    pub fn apply_sync_update(
        &self,
        dataset: &SyncDataset,
        settings: &[(String, String)],
    ) -> AppResult<()> {
        let target = Self::normalize_sync_dataset(dataset)?;
        let current = self.export_sync_data()?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
        for (old, new) in current.tables.iter().zip(&target.tables).rev() {
            let next_rows = rows(new)?;
            let count = if old.name == "investment_rule_revisions" {
                2
            } else {
                1
            };
            let predicate = old.columns[..count]
                .iter()
                .enumerate()
                .map(|(i, col)| format!("{}=?{}", quote_identifier(col), i + 1))
                .collect::<Vec<_>>()
                .join(" AND ");
            for row in &old.rows {
                if !next_rows.contains_key(&row_key(old, row)?) {
                    tx.execute(
                        &format!(
                            "DELETE FROM {} WHERE {}",
                            quote_identifier(&old.name),
                            predicate
                        ),
                        rusqlite::params_from_iter(sql_values(&row[..count])),
                    )?;
                }
            }
        }
        for (old, new) in current.tables.iter().zip(&target.tables) {
            let old_rows = rows(old)?;
            let count = if new.name == "investment_rule_revisions" {
                2
            } else {
                1
            };
            let columns = new
                .columns
                .iter()
                .map(|c| quote_identifier(c))
                .collect::<Vec<_>>();
            let placeholders = (1..=columns.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let updates = columns[count..]
                .iter()
                .map(|c| format!("{c}=excluded.{c}"))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT ({}) DO UPDATE SET {}",
                quote_identifier(&new.name),
                columns.join(","),
                placeholders,
                columns[..count].join(","),
                updates
            );
            for row in &new.rows {
                if old_rows.get(&row_key(new, row)?) != Some(row) {
                    tx.execute(&sql, rusqlite::params_from_iter(sql_values(row)))?;
                }
            }
        }
        for (key, value) in settings {
            tx.execute("INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        }
        super::daily::validate(&tx)?;
        tx.commit()?;
        Ok(())
    }
}
fn sql_values(row: &[SyncValue]) -> Vec<rusqlite::types::Value> {
    row.iter()
        .map(|v| match v {
            SyncValue::Null => rusqlite::types::Value::Null,
            SyncValue::Integer(v) => (*v).into(),
            SyncValue::Real(v) => (*v).into(),
            SyncValue::Text(v) => v.clone().into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Database {
        let connection = Connection::open_in_memory().unwrap();
        schema::initialize(&connection).unwrap();
        Database {
            connection: Mutex::new(connection),
        }
    }
    fn goals(data: &mut SyncDataset) -> &mut Vec<Vec<SyncValue>> {
        &mut data
            .tables
            .iter_mut()
            .find(|t| t.name == "goals")
            .unwrap()
            .rows
    }
    fn goal(id: &str, name: &str) -> Vec<SyncValue> {
        vec![
            SyncValue::Text(id.into()),
            SyncValue::Text(name.into()),
            SyncValue::Real(100.0),
            SyncValue::Real(0.0),
            SyncValue::Real(1.0),
            SyncValue::Text("2027-01-01".into()),
            SyncValue::Text("高".into()),
            SyncValue::Text("2026-01-01T00:00:00Z".into()),
            SyncValue::Text("2026-01-01T00:00:00Z".into()),
        ]
    }
    #[test]
    fn merges_independent_edits_additions_and_deletions_between_devices() {
        let mut base = db().export_sync_data().unwrap();
        goals(&mut base).extend([
            goal("a", "original-a"),
            goal("b", "original-b"),
            goal("deleted", "remove"),
        ]);
        let mut local = base.clone();
        let mut remote = base.clone();
        goals(&mut local)[0] = goal("a", "local-a");
        goals(&mut local).push(goal("new-local", "new"));
        goals(&mut remote)[1] = goal("b", "remote-b");
        goals(&mut remote).remove(2);
        goals(&mut remote).push(goal("new-remote", "new"));
        let mut merged = Database::merge_sync_data(Some(&base), &local, &remote).unwrap();
        let records = goals(&mut merged);
        assert_eq!(records.len(), 4);
        assert!(records.contains(&goal("a", "local-a")));
        assert!(records.contains(&goal("b", "remote-b")));
        assert!(!records
            .iter()
            .any(|row| row[0] == SyncValue::Text("deleted".into())));
        let reverse = Database::merge_sync_data(Some(&base), &remote, &local).unwrap();
        assert_eq!(
            merged.content_hash().unwrap(),
            reverse.content_hash().unwrap()
        );
    }
    #[test]
    fn conflicts_on_same_record_and_delete_versus_edit_without_writing() {
        let mut base = db().export_sync_data().unwrap();
        goals(&mut base).push(goal("a", "original"));
        let mut local = base.clone();
        let mut remote = base.clone();
        goals(&mut local)[0] = goal("a", "local");
        goals(&mut remote)[0] = goal("a", "remote");
        assert!(Database::merge_sync_data(Some(&base), &local, &remote)
            .unwrap_err()
            .to_string()
            .contains("双向修改"));
        goals(&mut remote).clear();
        assert!(Database::merge_sync_data(Some(&base), &local, &remote).is_err());
        assert_eq!(goals(&mut local)[0], goal("a", "local"));
    }
    #[test]
    fn same_content_with_different_update_times_converges() {
        let base = db().export_sync_data().unwrap();
        let mut local = base.clone();
        let mut remote = base.clone();
        goals(&mut local).push(goal("a", "same"));
        goals(&mut remote).push(goal("a", "same"));
        goals(&mut remote)[0][8] = SyncValue::Text("2026-01-02T00:00:00Z".into());
        let merged = Database::merge_sync_data(Some(&base), &local, &remote).unwrap();
        assert_eq!(
            merged.content_hash().unwrap(),
            remote.content_hash().unwrap()
        );
    }

    #[test]
    fn missing_baseline_never_guesses_deletions_or_overwrites_same_id() {
        let mut local = db().export_sync_data().unwrap();
        let mut remote = local.clone();
        goals(&mut local).push(goal("local", "local"));
        goals(&mut remote).push(goal("remote", "remote"));
        assert_eq!(
            Database::merge_sync_data(None, &local, &remote)
                .unwrap()
                .record_count(),
            2
        );
        goals(&mut remote).push(goal("local", "different"));
        assert!(Database::merge_sync_data(None, &local, &remote).is_err());
    }
    #[test]
    fn identical_edits_converge_and_repeat_apply_does_not_rewrite_rows() {
        let db = db();
        let mut base = db.export_sync_data().unwrap();
        goals(&mut base).push(goal("a", "same"));
        db.apply_sync_update(&base, &[]).unwrap();
        db.conn().unwrap().execute_batch("CREATE TABLE update_audit (id TEXT); CREATE TRIGGER count_goal_updates AFTER UPDATE ON goals BEGIN INSERT INTO update_audit VALUES (NEW.id); END;").unwrap();
        let merged = Database::merge_sync_data(Some(&base), &base, &base).unwrap();
        db.apply_sync_update(&merged, &[("test.baseline".into(), "1".into())])
            .unwrap();
        assert_eq!(
            db.conn()
                .unwrap()
                .query_row("SELECT count(*) FROM update_audit", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(db.setting("test.baseline").unwrap().as_deref(), Some("1"));
        let mut next = merged.clone();
        goals(&mut next)[0] = goal("a", "changed");
        db.apply_sync_update(&next, &[]).unwrap();
        assert_eq!(
            db.conn()
                .unwrap()
                .query_row("SELECT count(*) FROM update_audit", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
