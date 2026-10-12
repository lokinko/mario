use super::{
    event_import, insert_portfolio_event, portfolio_event_content_hash,
    portfolio_event_fingerprint, portfolio_event_identity_fingerprint, portfolio_event_import_row,
    portfolio_event_record, portfolio_event_record_from_row, portfolio_import_revision,
    validate_portfolio_event, AppError, AppResult, Database, HashMap, HashSet, Local, NaiveDate,
    OptionalRow, PortfolioEventImportCommitRequest, PortfolioEventImportPreview,
    PortfolioEventImportRequest, PortfolioEventImportResult, PortfolioEventInput,
    PortfolioEventRecord, PortfolioEventReversalInput, Utc, Uuid,
};

impl Database {
    pub fn portfolio_events(&self) -> AppResult<Vec<PortfolioEventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             ORDER BY event.occurred_on DESC, event.created_at DESC, event.id DESC LIMIT 500",
        )?;
        let rows = statement.query_map([], portfolio_event_record_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn add_portfolio_event(
        &self,
        input: &PortfolioEventInput,
    ) -> AppResult<PortfolioEventRecord> {
        let snapshot = self.snapshot()?;
        let base_currency = snapshot.profile.base_currency.trim().to_ascii_uppercase();
        validate_portfolio_event(input, &base_currency)?;

        let record = portfolio_event_record(input, &base_currency)?;
        let fingerprint = portfolio_event_fingerprint(input);
        let conn = self.conn()?;
        if !fingerprint.is_empty()
            && conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM portfolio_events WHERE fingerprint=?1)",
                [&fingerprint],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(AppError::Conflict(
                "相同来源与交易 ID 的流水已经存在".into(),
            ));
        }
        insert_portfolio_event(&conn, &record, &fingerprint)?;
        Ok(record)
    }

    pub fn reverse_portfolio_event(
        &self,
        id: &str,
        input: &PortfolioEventReversalInput,
    ) -> AppResult<PortfolioEventRecord> {
        let occurred_on = NaiveDate::parse_from_str(input.occurred_on.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("冲正日期必须使用 YYYY-MM-DD".into()))?;
        if occurred_on > Local::now().date_naive() {
            return Err(AppError::Validation("冲正日期不能晚于今天".into()));
        }
        if input.note.trim().is_empty() {
            return Err(AppError::Validation("冲正说明为必填项".into()));
        }
        if input.note.chars().count() > 2_000 {
            return Err(AppError::Validation("冲正说明不能超过 2000 个字符".into()));
        }

        let conn = self.conn()?;
        let source = conn
            .query_row(
                "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                        event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                        event.fx_rate_observed_on, event.base_currency, event.base_amount,
                        event.occurred_on, event.note, event.reversal_of_event_id,
                        reversal.id, event.created_at
                 FROM portfolio_events event
                 LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
                 WHERE event.id=?1",
                [id],
                portfolio_event_record_from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("找不到要冲正的组合流水".into()))?;

        if source.reversal_of_event_id.is_some() || source.amount <= 0.0 {
            return Err(AppError::Validation("冲正记录不能再次冲正".into()));
        }
        if source.reversed_by_event_id.is_some() {
            return Err(AppError::Conflict("这笔流水已经冲正".into()));
        }
        if input.occurred_on.trim() < source.occurred_on.as_str() {
            return Err(AppError::Validation("冲正日期不能早于原流水日期".into()));
        }

        let reversal_id = Uuid::new_v4().to_string();
        let record = PortfolioEventRecord {
            id: reversal_id.clone(),
            event_type: source.event_type,
            source: "mario-reversal".into(),
            external_id: format!("reversal:{id}"),
            asset_name: source.asset_name,
            amount: -source.amount,
            currency: source.currency,
            fx_rate_to_base: source.fx_rate_to_base,
            fx_rate_source: source.fx_rate_source,
            fx_rate_observed_on: source.fx_rate_observed_on,
            base_currency: source.base_currency,
            base_amount: -source.base_amount,
            occurred_on: input.occurred_on.trim().into(),
            note: input.note.trim().into(),
            reversal_of_event_id: Some(id.into()),
            reversed_by_event_id: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let fingerprint = portfolio_event_identity_fingerprint(&record.source, &record.external_id);
        insert_portfolio_event(&conn, &record, &fingerprint)?;
        Ok(record)
    }

    pub fn preview_portfolio_event_import(
        &self,
        request: &PortfolioEventImportRequest,
    ) -> AppResult<PortfolioEventImportPreview> {
        let parsed = event_import::parse(&request.csv_text)?;
        let snapshot = self.snapshot()?;
        let base_currency = snapshot.profile.base_currency.trim().to_ascii_uppercase();
        let existing = self.portfolio_event_identities()?;
        let mut seen = HashMap::<String, String>::new();
        let mut rows = parsed.issues;

        for parsed_row in parsed.rows {
            let resolved = self.centralized_event(&parsed_row.input, &base_currency)?;
            let input = &resolved;
            let mut status = "ready";
            let mut message = "校验通过，确认后写入".to_owned();
            let validation =
                if input.source.trim().is_empty() || input.external_id.trim().is_empty() {
                    Err(AppError::Validation(
                        "CSV 导入必须填写 source 和 external_id".into(),
                    ))
                } else {
                    validate_portfolio_event(input, &base_currency)
                };

            if let Err(error) = validation {
                status = "error";
                message = error.to_string();
            } else {
                let record = portfolio_event_record(input, &base_currency)?;
                let identity = portfolio_event_fingerprint(input);
                let content = portfolio_event_content_hash(&record)?;
                if let Some(existing_content) = existing.get(&identity) {
                    if existing_content == &content {
                        status = "duplicate";
                        message = "相同来源交易 ID 与内容已经存在，将跳过".into();
                    } else {
                        status = "error";
                        message = "相同来源交易 ID 已存在，但内容不同".into();
                    }
                } else if let Some(previous_content) = seen.get(&identity) {
                    if previous_content == &content {
                        status = "duplicate";
                        message = "CSV 内存在完全相同的重复行，将跳过".into();
                    } else {
                        status = "error";
                        message = "CSV 内同一来源交易 ID 对应不同内容".into();
                    }
                } else {
                    seen.insert(identity, content);
                }
            }

            rows.push(portfolio_event_import_row(
                parsed_row.row_number,
                input,
                status,
                message,
                &base_currency,
            ));
        }
        rows.sort_by_key(|row| row.row_number);
        let ready_count = rows.iter().filter(|row| row.status == "ready").count();
        let duplicate_count = rows.iter().filter(|row| row.status == "duplicate").count();
        let error_count = rows.iter().filter(|row| row.status == "error").count();
        let preview_revision = portfolio_import_revision(&rows, &base_currency, &request.csv_text)?;
        Ok(PortfolioEventImportPreview {
            rows,
            ready_count,
            duplicate_count,
            error_count,
            preview_revision,
            base_currency,
        })
    }

    pub fn commit_portfolio_event_import(
        &self,
        request: &PortfolioEventImportCommitRequest,
    ) -> AppResult<PortfolioEventImportResult> {
        let preview = self.preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: request.csv_text.clone(),
        })?;
        if request.preview_revision != preview.preview_revision {
            return Err(AppError::Conflict(
                "流水或基准币种已变化，请重新预览 CSV".into(),
            ));
        }
        if preview.error_count > 0 {
            return Err(AppError::Validation(
                "CSV 仍有错误行，修正并重新预览后才能导入".into(),
            ));
        }
        let ready_rows = preview
            .rows
            .iter()
            .filter(|row| row.status == "ready")
            .map(|row| row.row_number)
            .collect::<HashSet<_>>();
        let parsed = event_import::parse(&request.csv_text)?;
        let resolved_rows = parsed
            .rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.row_number,
                    self.centralized_event(&row.input, &preview.base_currency)?,
                ))
            })
            .collect::<AppResult<Vec<_>>>()?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let mut inserted_count = 0;
        for (row_number, input) in resolved_rows {
            if !ready_rows.contains(&row_number) {
                continue;
            }
            let record = portfolio_event_record(&input, &preview.base_currency)?;
            let fingerprint = portfolio_event_fingerprint(&input);
            insert_portfolio_event(&transaction, &record, &fingerprint)?;
            inserted_count += 1;
        }
        transaction.commit()?;
        Ok(PortfolioEventImportResult {
            inserted_count,
            duplicate_count: preview.duplicate_count,
            preview_revision: preview.preview_revision,
        })
    }

    pub(super) fn portfolio_event_identities(&self) -> AppResult<HashMap<String, String>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             WHERE event.external_id <> ''",
        )?;
        let rows = statement.query_map([], portfolio_event_record_from_row)?;
        let mut identities = HashMap::new();
        for row in rows {
            let record = row?;
            identities.insert(
                portfolio_event_fingerprint(&PortfolioEventInput {
                    event_type: record.event_type.clone(),
                    source: record.source.clone(),
                    external_id: record.external_id.clone(),
                    asset_name: record.asset_name.clone(),
                    amount: record.amount,
                    currency: record.currency.clone(),
                    fx_rate_to_base: record.fx_rate_to_base,
                    fx_rate_source: record.fx_rate_source.clone(),
                    fx_rate_observed_on: record.fx_rate_observed_on.clone(),
                    occurred_on: record.occurred_on.clone(),
                    note: record.note.clone(),
                }),
                portfolio_event_content_hash(&record)?,
            );
        }
        Ok(identities)
    }
}
