use super::{
    event_import, insert_portfolio_event, params, performance, portfolio_event_content_hash,
    portfolio_event_fingerprint, portfolio_event_identity_fingerprint, portfolio_event_import_row,
    portfolio_event_record, portfolio_event_record_from_row, portfolio_import_revision,
    validate_portfolio_event, valuation, AppError, AppResult, Database, HashMap, HashSet, Local,
    NaiveDate, OptionalRow, PortfolioCheckInInput, PortfolioCheckInRecord,
    PortfolioEventImportCommitRequest, PortfolioEventImportPreview, PortfolioEventImportRequest,
    PortfolioEventImportResult, PortfolioEventInput, PortfolioEventRecord,
    PortfolioEventReversalInput, Utc, Uuid,
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

        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        if !latest.base_currency.eq_ignore_ascii_case(&base_currency) {
            return Err(AppError::Validation(
                "基准币种已改变；请先建立新的组合基线，再记录流水".into(),
            ));
        }
        let latest_date = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先明确建立新的比较基线".into())
        })?;
        if input.occurred_on.trim() <= latest_date.as_str() {
            return Err(AppError::Validation(format!(
                "流水日期必须晚于最近一次组合检查点 {latest_date}；已冻结期间不能回填"
            )));
        }

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

        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        let frozen_through = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先明确建立新的比较基线".into())
        })?;
        if input.occurred_on.trim() <= frozen_through.as_str() {
            return Err(AppError::Validation(format!(
                "冲正日期必须晚于最近一次组合检查点 {frozen_through}"
            )));
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
        if source.occurred_on <= frozen_through {
            return Err(AppError::Validation(
                "这笔流水已进入冻结周期；请建立纠正后的新基线并保留说明".into(),
            ));
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
        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        if !latest.base_currency.eq_ignore_ascii_case(&base_currency) {
            return Err(AppError::Validation(
                "基准币种已改变；请先建立新的组合基线，再导入流水".into(),
            ));
        }
        let frozen_through = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先建立新的比较基线".into())
        })?;
        let existing = self.portfolio_event_identities()?;
        let mut seen = HashMap::<String, String>::new();
        let mut rows = parsed.issues;

        for parsed_row in parsed.rows {
            let input = &parsed_row.input;
            let mut status = "ready";
            let mut message = "校验通过，确认后写入".to_owned();
            let validation =
                if input.source.trim().is_empty() || input.external_id.trim().is_empty() {
                    Err(AppError::Validation(
                        "CSV 导入必须填写 source 和 external_id".into(),
                    ))
                } else if input.occurred_on.trim() <= frozen_through.as_str() {
                    Err(AppError::Validation(format!(
                        "发生日期必须晚于冻结边界 {frozen_through}"
                    )))
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
        let preview_revision =
            portfolio_import_revision(&rows, &base_currency, &frozen_through, &request.csv_text)?;
        Ok(PortfolioEventImportPreview {
            rows,
            ready_count,
            duplicate_count,
            error_count,
            preview_revision,
            base_currency,
            frozen_through,
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
                "流水或组合基线已变化，请重新预览 CSV".into(),
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
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let mut inserted_count = 0;
        for row in parsed.rows {
            if !ready_rows.contains(&row.row_number) {
                continue;
            }
            let record = portfolio_event_record(&row.input, &preview.base_currency)?;
            let fingerprint = portfolio_event_fingerprint(&row.input);
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

    pub(super) fn portfolio_events_between(
        &self,
        period_start: &str,
        period_end: &str,
    ) -> AppResult<Vec<PortfolioEventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             WHERE event.occurred_on > ?1 AND event.occurred_on <= ?2
             ORDER BY event.occurred_on, event.created_at, event.id",
        )?;
        let rows = statement.query_map(
            params![period_start, period_end],
            portfolio_event_record_from_row,
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn portfolio_checkins(&self) -> AppResult<Vec<PortfolioCheckInRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT payload FROM portfolio_checkins
             ORDER BY created_at DESC, id DESC LIMIT 100",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(serde_json::from_str::<PortfolioCheckInRecord>(&row?)?);
        }
        Ok(records)
    }

    pub fn save_portfolio_checkin(
        &self,
        input: &PortfolioCheckInInput,
    ) -> AppResult<PortfolioCheckInRecord> {
        if input.period_label.trim().is_empty() {
            return Err(AppError::Validation("组合快照周期为必填项".into()));
        }
        if input.period_label.chars().count() > 100 || input.note.chars().count() > 2_000 {
            return Err(AppError::Validation("组合快照周期或说明过长".into()));
        }
        if !input.external_cash_flow.is_finite() || input.external_cash_flow.abs() > 1e15 {
            return Err(AppError::Validation("期间净入金必须是有效金额".into()));
        }
        if !input.use_ledger_cash_flows
            && input.external_cash_flow.abs() > 0.005
            && input.note.trim().is_empty()
        {
            return Err(AppError::Validation(
                "存在净入金或出金时，必须说明现金流来源".into(),
            ));
        }

        let snapshot = self.snapshot()?;
        if snapshot.holdings.is_empty() {
            return Err(AppError::Validation(
                "请先录入当前持仓，再建立组合变化基线".into(),
            ));
        }
        if !snapshot.valuation_status.comparable {
            return Err(AppError::Validation(format!(
                "请先补齐外币持仓汇率：{}",
                snapshot.valuation_status.missing_fx_holdings.join("、")
            )));
        }
        let valuation_date = snapshot
            .valuation_status
            .aligned_valuation_date
            .clone()
            .ok_or_else(|| {
                AppError::Validation("全部持仓必须使用同一个估值日期，才能冻结组合检查点".into())
            })?;
        if snapshot.total_value <= 0.0 {
            return Err(AppError::Validation("组合折算后的总市值必须大于 0".into()));
        }
        let previous = if input.reset_baseline {
            None
        } else {
            self.portfolio_checkins()?.into_iter().next()
        };
        if previous.is_none() && input.external_cash_flow.abs() > 0.005 {
            return Err(AppError::Validation(
                "第一条记录是组合基线，期间净入金应填写 0".into(),
            ));
        }
        if previous.as_ref().is_some_and(|record| {
            !record
                .base_currency
                .eq_ignore_ascii_case(&snapshot.valuation_status.base_currency)
        }) {
            return Err(AppError::Validation(
                "基准币种已经改变；请明确选择按新币种重新建立基线".into(),
            ));
        }

        let previous_date = previous
            .as_ref()
            .and_then(|record| record.valuation_date.as_deref());
        if previous_date.is_some_and(|date| valuation_date.as_str() <= date) {
            return Err(AppError::Validation(
                "本次估值日期必须晚于上一条组合检查点".into(),
            ));
        }

        let period_events = if input.use_ledger_cash_flows {
            if let Some(period_start) = previous_date {
                let events = self.portfolio_events_between(period_start, &valuation_date)?;
                if events.iter().any(|event| {
                    !event
                        .base_currency
                        .eq_ignore_ascii_case(&snapshot.valuation_status.base_currency)
                }) {
                    return Err(AppError::Validation(
                        "期间流水的基准币种与当前组合不一致，请重新建立基线".into(),
                    ));
                }
                events
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let event_summary = performance::summarize(&period_events);
        let external_cash_flow = if input.use_ledger_cash_flows {
            event_summary.external_cash_flow
        } else {
            input.external_cash_flow
        };

        let total_change = previous
            .as_ref()
            .map(|record| snapshot.total_value - record.total_value);
        let valuation_residual = total_change.map(|change| change - external_cash_flow);
        let modified_dietz_return_pct = if input.use_ledger_cash_flows {
            previous.as_ref().and_then(|record| {
                let period_start =
                    NaiveDate::parse_from_str(record.valuation_date.as_deref()?, "%Y-%m-%d")
                        .ok()?;
                let period_end = NaiveDate::parse_from_str(&valuation_date, "%Y-%m-%d").ok()?;
                performance::modified_dietz_return_pct(
                    record.total_value,
                    snapshot.total_value,
                    period_start,
                    period_end,
                    &period_events,
                )
            })
        } else {
            None
        };
        let allocation_changes = previous
            .as_ref()
            .map(|record| {
                valuation::allocation_changes(
                    &record.holdings,
                    &snapshot.holdings,
                    &snapshot.valuation_status.base_currency,
                )
            })
            .unwrap_or_default();
        let record = PortfolioCheckInRecord {
            id: Uuid::new_v4().to_string(),
            period_label: input.period_label.trim().into(),
            external_cash_flow,
            note: input.note.trim().into(),
            total_value: snapshot.total_value,
            previous_check_in_id: previous.as_ref().map(|record| record.id.clone()),
            previous_total_value: previous.as_ref().map(|record| record.total_value),
            total_change,
            valuation_residual,
            base_currency: snapshot.valuation_status.base_currency,
            valuation_date: Some(valuation_date),
            cash_flow_source: if previous.is_none() {
                "baseline".into()
            } else if input.use_ledger_cash_flows {
                "ledger".into()
            } else {
                "manual".into()
            },
            event_ids: event_summary.event_ids,
            income: event_summary.income,
            costs: event_summary.costs,
            turnover: event_summary.turnover,
            modified_dietz_return_pct,
            holdings: snapshot.holdings,
            holding_valuations: snapshot.holding_valuations,
            allocation_changes,
            created_at: Utc::now().to_rfc3339(),
        };
        self.conn()?.execute(
            "INSERT INTO portfolio_checkins (id, payload, created_at) VALUES (?1,?2,?3)",
            params![
                record.id,
                serde_json::to_string(&record)?,
                record.created_at
            ],
        )?;
        Ok(record)
    }
}
