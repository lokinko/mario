use super::{
    normalized_fx_provenance, validate_holding_valuation_evidence, AppError, AppResult,
    FinancialProfile, HashMap, HashSet, SecurityPriceQuote, SyncDataset, SyncValue,
};

pub(super) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
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
    let effective_on = sync_text(row, effective_index, "估值日期")?;
    if currency.eq_ignore_ascii_case(base_currency) && rate.is_some() {
        return Err(AppError::Validation(
            "同步数据中的同币种记录不能携带折算汇率".into(),
        ));
    }
    let effective_on = if source_index == 10 && matches!(source, "central_daily" | "central_manual")
    {
        observed_on
    } else {
        effective_on
    };
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
        if !sync_text(row, 7, "持仓币种")?.eq_ignore_ascii_case(&profile.base_currency)
            && matches!(row.get(8), Some(SyncValue::Null))
            && sync_text(row, 10, "汇率来源")?.is_empty()
            && sync_text(row, 11, "汇率日期")?.is_empty()
        {
            continue;
        }
        validate_synced_fx_row(row, 7, 8, 10, 11, &profile.base_currency, 9)?;
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
        .filter(|table| {
            table.name == "analyses" || (dataset.schema_version < 16 && table.name == "decisions")
        })
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
    if dataset.schema_version >= 14 {
        for row in &holdings.rows {
            super::portfolio::validate_detail_totals(
                sync_optional_real(row, 14, "成本价")?,
                sync_optional_real(row, 15, "持仓数")?,
                sync_optional_real(row, 16, "当前价格")?,
                sync_real(row, 4, "市值")?,
                sync_real(row, 5, "总成本")?,
            )?;
        }
    }
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
            exact_close: None,
            splits: Vec::new(),
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

    Ok(())
}

fn sync_real(row: &[SyncValue], index: usize, label: &str) -> AppResult<f64> {
    match row.get(index) {
        Some(SyncValue::Real(value)) => Ok(*value),
        Some(SyncValue::Integer(value)) => Ok(*value as f64),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

pub(super) fn validate_synced_attachments(dataset: &SyncDataset) -> AppResult<()> {
    use crate::{
        attachments::{decode, MAX_BYTES, MAX_FILES, MAX_TEXT_CHARS},
        models::ResearchAttachmentInput,
    };
    let Some(table) = dataset
        .tables
        .iter()
        .find(|t| t.name == "research_attachments")
    else {
        return Ok(());
    };
    let evidence = dataset
        .tables
        .iter()
        .find(|t| t.name == "research_evidence")
        .ok_or_else(|| AppError::Validation("快照缺少资料表".into()))?;
    let ids = evidence
        .rows
        .iter()
        .filter_map(|row| match &row[0] {
            SyncValue::Text(id) => Some(id.as_str()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let mut seen = std::collections::HashSet::new();
    let mut totals = std::collections::HashMap::<String, (usize, usize)>::new();
    for row in &table.rows {
        let string = |i| match &row[i] {
            SyncValue::Text(s) => Ok(s.as_str()),
            _ => Err(AppError::Validation("附件快照字段无效".into())),
        };
        let id = string(0)?;
        let parent = string(1)?;
        if uuid::Uuid::parse_str(id).is_err() || !seen.insert(id) || !ids.contains(parent) {
            return Err(AppError::Validation("附件快照的资料关联无效".into()));
        }
        let bytes = decode(&ResearchAttachmentInput {
            name: string(2)?.into(),
            data_base64: string(4)?.into(),
        })?;
        if row[3] != SyncValue::Integer(bytes.len() as i64)
            || string(5)?.chars().count() > MAX_TEXT_CHARS
            || string(6)?.chars().count() > 500
        {
            return Err(AppError::Validation("附件快照内容无效".into()));
        }
        let total = totals.entry(parent.into()).or_default();
        total.0 += 1;
        total.1 += bytes.len();
        if total.0 > MAX_FILES || total.1 > MAX_BYTES {
            return Err(AppError::Validation("附件快照超过数量或大小限制".into()));
        }
    }
    Ok(())
}
