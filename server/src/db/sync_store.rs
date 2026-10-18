use super::{
    quote_identifier, sync_specs_for_version, validate_synced_fx_provenance,
    validate_synced_holding_valuations, AppError, AppResult, Database, SyncDataset, SyncTable,
    SyncValue, Utc, ValueRef, SYNC_DATASET_SCHEMA_VERSION, SYNC_TABLES,
};

impl Database {
    pub fn export_sync_data(&self) -> AppResult<SyncDataset> {
        let conn = self.conn()?;
        let mut tables = Vec::with_capacity(SYNC_TABLES.len());
        for spec in SYNC_TABLES {
            let columns = spec
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>();
            let sql = format!(
                "SELECT {} FROM {} {} ORDER BY {}",
                columns.join(","),
                quote_identifier(spec.name),
                if spec.name == "settings" {
                    "WHERE key LIKE 'fx.book.%' OR key LIKE 'fx.quote.%'"
                } else {
                    ""
                },
                spec.order_by
            );
            let mut statement = conn.prepare(&sql)?;
            let rows = statement
                .query_map([], |row| {
                    let mut values = Vec::with_capacity(spec.columns.len());
                    for index in 0..spec.columns.len() {
                        values.push(match row.get_ref(index)? {
                            ValueRef::Null => SyncValue::Null,
                            ValueRef::Integer(value) => SyncValue::Integer(value),
                            ValueRef::Real(value) => SyncValue::Real(value),
                            ValueRef::Text(value) => SyncValue::Text(
                                String::from_utf8(value.to_vec()).map_err(|error| {
                                    rusqlite::Error::FromSqlConversionFailure(
                                        index,
                                        rusqlite::types::Type::Text,
                                        Box::new(error),
                                    )
                                })?,
                            ),
                            ValueRef::Blob(_) => {
                                return Err(rusqlite::Error::InvalidColumnType(
                                    index,
                                    spec.columns[index].into(),
                                    rusqlite::types::Type::Blob,
                                ));
                            }
                        });
                    }
                    Ok(values)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            tables.push(SyncTable {
                name: spec.name.into(),
                columns: spec.columns.iter().copied().map(str::to_owned).collect(),
                rows,
            });
        }
        let dataset = SyncDataset {
            schema_version: SYNC_DATASET_SCHEMA_VERSION,
            exported_at: Utc::now().to_rfc3339(),
            tables,
        };
        dataset.validate()?;
        Ok(dataset)
    }

    pub fn import_sync_data(&self, dataset: &SyncDataset) -> AppResult<()> {
        dataset.validate()?;
        if dataset.schema_version < 18
            && dataset
                .tables
                .iter()
                .any(|t| t.name == "holding_trades" && !t.rows.is_empty())
        {
            return Err(AppError::Validation(
                "旧交易记录格式已停用，请清空旧交易后按成交单重新录入；本次未覆盖任何数据".into(),
            ));
        }
        validate_synced_fx_provenance(dataset)?;
        validate_synced_holding_valuations(dataset)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute_batch("PRAGMA defer_foreign_keys=ON;")?;

        for spec in SYNC_TABLES.iter().rev() {
            let filter = if spec.name == "settings" {
                " WHERE key LIKE 'fx.book.%' OR key LIKE 'fx.quote.%'"
            } else {
                ""
            };
            transaction.execute(
                &format!("DELETE FROM {}{filter}", quote_identifier(spec.name)),
                [],
            )?;
        }
        for spec in sync_specs_for_version(dataset.schema_version)? {
            if !SYNC_TABLES.iter().any(|current| current.name == spec.name) {
                continue;
            }
            let table = dataset
                .tables
                .iter()
                .find(|table| table.name == spec.name)
                .ok_or_else(|| AppError::Validation(format!("数据快照缺少 {}", spec.name)))?;
            let columns = spec
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>();
            let placeholders = (1..=spec.columns.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>();
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                quote_identifier(spec.name),
                columns.join(","),
                placeholders.join(",")
            );
            let mut statement = transaction.prepare(&sql)?;
            for row in &table.rows {
                let values = row
                    .iter()
                    .map(|value| match value {
                        SyncValue::Null => rusqlite::types::Value::Null,
                        SyncValue::Integer(value) => rusqlite::types::Value::Integer(*value),
                        SyncValue::Real(value) => rusqlite::types::Value::Real(*value),
                        SyncValue::Text(value) => rusqlite::types::Value::Text(value.clone()),
                    })
                    .collect::<Vec<_>>();
                statement.execute(values)?;
            }
        }
        super::schema::remove_retired_analysis_metadata(&transaction)?;
        super::daily::validate(&transaction)?;
        super::trades::validate_stored(&transaction)?;
        super::cash::consolidate(&transaction)?;
        super::cash::initialize_balances(&transaction)?;
        super::amounts::initialize(&transaction)?;
        super::amounts::validate(&transaction)?;
        super::cash::validate(&transaction)?;
        super::corporate::validate(&transaction)?;
        transaction.execute("DELETE FROM settings WHERE key LIKE 'trading.reset.%'", [])?;
        transaction.commit()?;
        Ok(())
    }
}
