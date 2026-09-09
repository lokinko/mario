use super::{
    normalized_fx_provenance, params, AppError, AppResult, Connection, Digest,
    PortfolioEventImportRow, PortfolioEventInput, PortfolioEventRecord, PortfolioEventType, Row,
    Sha256, Utc, Uuid,
};

fn portfolio_event_type_to_db(event_type: &PortfolioEventType) -> &'static str {
    match event_type {
        PortfolioEventType::Deposit => "deposit",
        PortfolioEventType::Withdrawal => "withdrawal",
        PortfolioEventType::Dividend => "dividend",
        PortfolioEventType::Interest => "interest",
        PortfolioEventType::Fee => "fee",
        PortfolioEventType::Tax => "tax",
        PortfolioEventType::Buy => "buy",
        PortfolioEventType::Sell => "sell",
    }
}

fn portfolio_event_type_from_db(value: &str) -> AppResult<PortfolioEventType> {
    match value {
        "deposit" => Ok(PortfolioEventType::Deposit),
        "withdrawal" => Ok(PortfolioEventType::Withdrawal),
        "dividend" => Ok(PortfolioEventType::Dividend),
        "interest" => Ok(PortfolioEventType::Interest),
        "fee" => Ok(PortfolioEventType::Fee),
        "tax" => Ok(PortfolioEventType::Tax),
        "buy" => Ok(PortfolioEventType::Buy),
        "sell" => Ok(PortfolioEventType::Sell),
        _ => Err(AppError::Validation("流水类型无效".into())),
    }
}

pub(super) fn portfolio_event_record_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<PortfolioEventRecord> {
    let event_type = portfolio_event_type_from_db(row.get::<_, String>(1)?.as_str())
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(PortfolioEventRecord {
        id: row.get(0)?,
        event_type,
        source: row.get(2)?,
        external_id: row.get(3)?,
        asset_name: row.get(4)?,
        amount: row.get(5)?,
        currency: row.get(6)?,
        fx_rate_to_base: row.get(7)?,
        fx_rate_source: row.get(8)?,
        fx_rate_observed_on: row.get(9)?,
        base_currency: row.get(10)?,
        base_amount: row.get(11)?,
        occurred_on: row.get(12)?,
        note: row.get(13)?,
        reversal_of_event_id: row.get(14)?,
        reversed_by_event_id: row.get(15)?,
        created_at: row.get(16)?,
    })
}

pub(super) fn portfolio_event_record(
    input: &PortfolioEventInput,
    base_currency: &str,
) -> AppResult<PortfolioEventRecord> {
    let currency = input.currency.trim().to_ascii_uppercase();
    let fx_rate_to_base = if currency == base_currency {
        None
    } else {
        input.fx_rate_to_base
    };
    let base_amount = input.amount * fx_rate_to_base.unwrap_or(1.0);
    if !base_amount.is_finite() || base_amount > 1e15 {
        return Err(AppError::Validation("流水折算金额超出有效范围".into()));
    }
    let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
        &currency,
        base_currency,
        fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )?;
    Ok(PortfolioEventRecord {
        id: Uuid::new_v4().to_string(),
        event_type: input.event_type.clone(),
        source: if input.source.trim().is_empty() {
            "manual".into()
        } else {
            input.source.trim().into()
        },
        external_id: input.external_id.trim().into(),
        asset_name: input.asset_name.trim().into(),
        amount: input.amount,
        currency,
        fx_rate_to_base,
        fx_rate_source,
        fx_rate_observed_on,
        base_currency: base_currency.into(),
        base_amount,
        occurred_on: input.occurred_on.trim().into(),
        note: input.note.trim().into(),
        reversal_of_event_id: None,
        reversed_by_event_id: None,
        created_at: Utc::now().to_rfc3339(),
    })
}

pub(super) fn portfolio_event_fingerprint(input: &PortfolioEventInput) -> String {
    portfolio_event_identity_fingerprint(&input.source, &input.external_id)
}

pub(super) fn portfolio_event_identity_fingerprint(source: &str, external_id: &str) -> String {
    if external_id.trim().is_empty() {
        return String::new();
    }
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\u{1f}{}",
                source.trim().to_ascii_lowercase(),
                external_id.trim()
            )
            .as_bytes()
        )
    )
}

pub(super) fn portfolio_event_content_hash(record: &PortfolioEventRecord) -> AppResult<String> {
    let canonical = serde_json::json!({
        "eventType": record.event_type,
        "source": record.source.trim().to_ascii_lowercase(),
        "externalId": record.external_id.trim(),
        "assetName": record.asset_name.trim(),
        "amount": record.amount,
        "currency": record.currency.trim().to_ascii_uppercase(),
        "fxRateToBase": record.fx_rate_to_base,
        "fxRateSource": record.fx_rate_source.trim(),
        "fxRateObservedOn": record.fx_rate_observed_on.trim(),
        "baseCurrency": record.base_currency.trim().to_ascii_uppercase(),
        "baseAmount": record.base_amount,
        "occurredOn": record.occurred_on.trim(),
        "note": record.note.trim(),
        "reversalOfEventId": record.reversal_of_event_id,
    });
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

pub(super) fn portfolio_event_import_row(
    row_number: usize,
    input: &PortfolioEventInput,
    status: &str,
    message: String,
    base_currency: &str,
) -> PortfolioEventImportRow {
    let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )
    .unwrap_or_else(|_| {
        (
            input.fx_rate_source.trim().into(),
            input.fx_rate_observed_on.trim().into(),
        )
    });
    PortfolioEventImportRow {
        row_number,
        status: status.into(),
        message,
        source: input.source.trim().into(),
        external_id: input.external_id.trim().into(),
        event_type: portfolio_event_type_to_db(&input.event_type).into(),
        occurred_on: input.occurred_on.trim().into(),
        amount: Some(input.amount),
        currency: input.currency.trim().to_ascii_uppercase(),
        fx_rate_to_base: input.fx_rate_to_base,
        fx_rate_source,
        fx_rate_observed_on,
        asset_name: input.asset_name.trim().into(),
        note: input.note.trim().into(),
    }
}

pub(super) fn portfolio_import_revision(
    rows: &[PortfolioEventImportRow],
    base_currency: &str,
    frozen_through: &str,
    csv_text: &str,
) -> AppResult<String> {
    let canonical = serde_json::json!({
        "rows": rows,
        "baseCurrency": base_currency,
        "frozenThrough": frozen_through,
        "csvSha256": format!("{:x}", Sha256::digest(csv_text.as_bytes())),
    });
    Ok(format!(
        "import-{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

pub(super) fn insert_portfolio_event(
    connection: &Connection,
    record: &PortfolioEventRecord,
    fingerprint: &str,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO portfolio_events
         (id, event_type, source, external_id, fingerprint, asset_name, amount, currency,
          fx_rate_to_base, fx_rate_source, fx_rate_observed_on, base_currency, base_amount,
          occurred_on, note, created_at, reversal_of_event_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            record.id,
            portfolio_event_type_to_db(&record.event_type),
            record.source,
            record.external_id,
            fingerprint,
            record.asset_name,
            record.amount,
            record.currency,
            record.fx_rate_to_base,
            record.fx_rate_source,
            record.fx_rate_observed_on,
            record.base_currency,
            record.base_amount,
            record.occurred_on,
            record.note,
            record.created_at,
            record.reversal_of_event_id,
        ],
    )?;
    Ok(())
}
