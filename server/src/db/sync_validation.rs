use super::{
    normalized_fx_provenance, portfolio_event_identity_fingerprint,
    validate_holding_valuation_evidence, validate_stored_holding_valuation, AppError, AppResult,
    FinancialProfile, HashMap, HashSet, PortfolioCheckInRecord, SecurityPriceQuote, SyncDataset,
    SyncValue,
};

pub(super) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn validate_synced_event_identities(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 5 {
        return Ok(());
    }
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    let mut seen = HashSet::new();
    for row in &table.rows {
        let SyncValue::Text(source) = &row[2] else {
            return Err(AppError::Validation("同步流水来源字段类型无效".into()));
        };
        let SyncValue::Text(external_id) = &row[3] else {
            return Err(AppError::Validation("同步流水交易 ID 字段类型无效".into()));
        };
        let SyncValue::Text(fingerprint) = &row[4] else {
            return Err(AppError::Validation("同步流水去重指纹字段类型无效".into()));
        };
        if (!external_id.is_empty() && source.trim().is_empty())
            || source.chars().count() > 120
            || external_id.chars().count() > 200
        {
            return Err(AppError::Validation("同步流水导入标识无效".into()));
        }
        let expected = portfolio_event_identity_fingerprint(source, external_id);
        if fingerprint != &expected {
            return Err(AppError::Validation("同步流水去重指纹不匹配".into()));
        }
        if !fingerprint.is_empty() && !seen.insert(fingerprint) {
            return Err(AppError::Validation("同步数据包含重复流水交易 ID".into()));
        }
    }
    Ok(())
}

fn sync_text<'a>(row: &'a [SyncValue], index: usize, label: &str) -> AppResult<&'a str> {
    match row.get(index) {
        Some(SyncValue::Text(value)) => Ok(value),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

fn sync_optional_real(row: &[SyncValue], index: usize, label: &str) -> AppResult<Option<f64>> {
    match row.get(index) {
        Some(SyncValue::Null) => Ok(None),
        Some(SyncValue::Real(value)) => Ok(Some(*value)),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

fn validate_synced_fx_row(
    row: &[SyncValue],
    currency_index: usize,
    rate_index: usize,
    source_index: usize,
    observed_index: usize,
    base_currency: &str,
    effective_index: usize,
) -> AppResult<()> {
    let currency = sync_text(row, currency_index, "币种")?;
    let rate = sync_optional_real(row, rate_index, "汇率")?;
    let source = sync_text(row, source_index, "汇率来源")?;
    let observed_on = sync_text(row, observed_index, "汇率观察日")?;
    let effective_on = sync_text(row, effective_index, "估值或流水日期")?;
    if currency.eq_ignore_ascii_case(base_currency) && rate.is_some() {
        return Err(AppError::Validation(
            "同步数据中的同币种记录不能携带折算汇率".into(),
        ));
    }
    let canonical = normalized_fx_provenance(
        currency,
        base_currency,
        rate,
        source,
        observed_on,
        effective_on,
    )?;
    if canonical.0 != source || canonical.1 != observed_on {
        return Err(AppError::Validation("同步汇率来源不是规范形式".into()));
    }
    Ok(())
}

pub(super) fn validate_synced_fx_provenance(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 6 {
        return Ok(());
    }
    let profile_table = dataset
        .tables
        .iter()
        .find(|table| table.name == "profile")
        .ok_or_else(|| AppError::Validation("数据快照缺少 profile".into()))?;
    let profile = match profile_table.rows.first() {
        Some(profile_row) => {
            serde_json::from_str::<FinancialProfile>(sync_text(profile_row, 1, "财务档案")?)
                .map_err(|_| AppError::Validation("同步财务档案格式无效".into()))?
        }
        None => FinancialProfile::default(),
    };

    let holdings = dataset
        .tables
        .iter()
        .find(|table| table.name == "holdings")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holdings".into()))?;
    for row in &holdings.rows {
        validate_synced_fx_row(row, 7, 8, 10, 11, &profile.base_currency, 9)?;
    }

    let events = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    for row in &events.rows {
        let base_currency = sync_text(row, 11, "流水基准币种")?;
        validate_synced_fx_row(row, 7, 8, 9, 10, base_currency, 13)?;
    }
    Ok(())
}

pub(super) fn validate_synced_event_reversals(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 7 {
        return Ok(());
    }
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    let rows_by_id = table
        .rows
        .iter()
        .map(|row| Ok((sync_text(row, 0, "流水 ID")?, row)))
        .collect::<AppResult<HashMap<_, _>>>()?;
    let mut reversed_targets = HashSet::new();

    for row in &table.rows {
        let target_id = match row.get(16) {
            Some(SyncValue::Null) => continue,
            Some(SyncValue::Text(value)) if !value.trim().is_empty() => value,
            _ => return Err(AppError::Validation("同步流水冲正关联无效".into())),
        };
        let event_id = sync_text(row, 0, "流水 ID")?;
        if target_id == event_id || !reversed_targets.insert(target_id.as_str()) {
            return Err(AppError::Validation(
                "同步数据包含重复或自引用的流水冲正".into(),
            ));
        }
        let target = rows_by_id
            .get(target_id.as_str())
            .ok_or_else(|| AppError::Validation("同步流水冲正找不到原记录".into()))?;
        if !matches!(target.get(16), Some(SyncValue::Null)) {
            return Err(AppError::Validation(
                "同步数据不能对冲正记录再次冲正".into(),
            ));
        }
        let amount = sync_real(row, 6, "冲正金额")?;
        let target_amount = sync_real(target, 6, "原流水金额")?;
        let base_amount = sync_real(row, 12, "冲正折算金额")?;
        let target_base_amount = sync_real(target, 12, "原流水折算金额")?;
        if amount >= 0.0
            || amount != -target_amount
            || base_amount != -target_base_amount
            || row[1] != target[1]
            || row[5] != target[5]
            || row[7] != target[7]
            || row[8] != target[8]
            || row[9] != target[9]
            || row[10] != target[10]
            || row[11] != target[11]
            || sync_text(row, 13, "冲正日期")? < sync_text(target, 13, "原流水日期")?
        {
            return Err(AppError::Validation(
                "同步流水冲正内容与原记录不匹配".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_synced_memory_preferences(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 8 {
        return Ok(());
    }
    let source_ids = dataset
        .tables
        .iter()
        .filter(|table| matches!(table.name.as_str(), "decisions" | "analyses"))
        .flat_map(|table| table.rows.iter())
        .map(|row| sync_text(row, 0, "记忆来源 ID"))
        .collect::<AppResult<HashSet<_>>>()?;
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "memory_preferences")
        .ok_or_else(|| AppError::Validation("数据快照缺少 memory_preferences".into()))?;
    let mut seen = HashSet::new();
    for row in &table.rows {
        let memory_id = sync_text(row, 0, "长期记忆 ID")?;
        let preference = sync_text(row, 1, "长期记忆偏好")?;
        let note = sync_text(row, 2, "长期记忆备注")?;
        let updated_at = sync_text(row, 3, "长期记忆更新时间")?;
        if memory_id.trim().is_empty()
            || memory_id.chars().count() > 128
            || !seen.insert(memory_id)
            || !source_ids.contains(memory_id)
            || !matches!(preference, "pinned" | "hidden")
            || note.chars().count() > 1_000
            || chrono::DateTime::parse_from_rfc3339(updated_at).is_err()
        {
            return Err(AppError::Validation("同步长期记忆偏好无效".into()));
        }
    }
    Ok(())
}

pub(super) fn validate_synced_holding_valuations(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 9 {
        return Ok(());
    }
    let holdings = dataset
        .tables
        .iter()
        .find(|table| table.name == "holdings")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holdings".into()))?;
    let holdings_by_id = holdings
        .rows
        .iter()
        .map(|row| Ok((sync_text(row, 0, "持仓 ID")?, row)))
        .collect::<AppResult<HashMap<_, _>>>()?;
    let valuations = dataset
        .tables
        .iter()
        .find(|table| table.name == "holding_valuations")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holding_valuations".into()))?;
    let mut seen = HashSet::new();
    for row in &valuations.rows {
        let holding_id = sync_text(row, 0, "估值持仓 ID")?;
        if !seen.insert(holding_id) {
            return Err(AppError::Validation("同步数据包含重复持仓估值".into()));
        }
        let quantity = sync_real(row, 2, "持仓数量")?;
        let market_value = sync_real(row, 4, "持仓估值")?;
        let staleness = sync_real(row, 8, "价格回退天数")?;
        if staleness.fract() != 0.0 {
            return Err(AppError::Validation("同步价格回退天数无效".into()));
        }
        let quote = SecurityPriceQuote {
            symbol: sync_text(row, 1, "行情代码")?.into(),
            currency: sync_text(row, 5, "行情币种")?.into(),
            close: sync_real(row, 3, "单位价格")?,
            requested_on: sync_text(row, 6, "请求日期")?.into(),
            observed_on: sync_text(row, 7, "观察日期")?.into(),
            staleness_days: staleness as i64,
            provider_code: sync_text(row, 9, "价格来源代码")?.into(),
            provider_name: sync_text(row, 10, "价格来源名称")?.into(),
            exchange: sync_text(row, 11, "交易所")?.into(),
            mic_code: sync_text(row, 12, "MIC")?.into(),
            instrument_type: sync_text(row, 13, "证券类型")?.into(),
            price_basis: sync_text(row, 14, "价格口径")?.into(),
            source_url: sync_text(row, 15, "价格来源地址")?.into(),
            methodology_url: sync_text(row, 16, "价格方法地址")?.into(),
            disclaimer: sync_text(row, 17, "价格限制")?.into(),
        };
        validate_holding_valuation_evidence(quantity, &quote)?;
        let expected_market_value = quantity * quote.close;
        if (market_value - expected_market_value).abs()
            > 0.005_f64.max(expected_market_value.abs() * 1e-10)
        {
            return Err(AppError::Validation(
                "同步持仓估值与数量、单位价格不一致".into(),
            ));
        }
        chrono::DateTime::parse_from_rfc3339(sync_text(row, 18, "行情采集时间")?)
            .map_err(|_| AppError::Validation("同步行情采集时间无效".into()))?;
        let holding = holdings_by_id
            .get(holding_id)
            .ok_or_else(|| AppError::Validation("同步持仓估值找不到对应持仓".into()))?;
        if sync_text(holding, 1, "持仓代码")? != quote.symbol
            || !sync_text(holding, 7, "持仓币种")?.eq_ignore_ascii_case(&quote.currency)
            || sync_text(holding, 9, "持仓估值日")? != quote.requested_on
            || (sync_real(holding, 4, "持仓市值")? - market_value).abs() > 0.005
        {
            return Err(AppError::Validation("同步持仓与价格估值证据不一致".into()));
        }
    }

    let checkins = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_checkins")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_checkins".into()))?;
    for row in &checkins.rows {
        let record: PortfolioCheckInRecord = serde_json::from_str(sync_text(row, 1, "组合检查点")?)
            .map_err(|_| AppError::Validation("同步组合检查点格式无效".into()))?;
        for evidence in &record.holding_valuations {
            validate_stored_holding_valuation(evidence)?;
            let holding = record
                .holdings
                .iter()
                .find(|holding| holding.id == evidence.holding_id)
                .ok_or_else(|| AppError::Validation("检查点行情证据找不到对应持仓".into()))?;
            if holding.symbol != evidence.symbol
                || !holding.currency.eq_ignore_ascii_case(&evidence.currency)
                || holding.valuation_date != evidence.requested_on
                || (holding.market_value - evidence.market_value).abs() > 0.005
            {
                return Err(AppError::Validation("检查点持仓与行情证据不一致".into()));
            }
        }
    }
    Ok(())
}

fn sync_real(row: &[SyncValue], index: usize, label: &str) -> AppResult<f64> {
    match row.get(index) {
        Some(SyncValue::Real(value)) => Ok(*value),
        Some(SyncValue::Integer(value)) => Ok(*value as f64),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}
