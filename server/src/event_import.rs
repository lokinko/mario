use std::collections::HashMap;

use csv::{ReaderBuilder, StringRecord, Trim};

use crate::{
    error::{AppError, AppResult},
    models::{PortfolioEventImportRow, PortfolioEventInput, PortfolioEventType},
};

const MAX_CSV_BYTES: usize = 2 * 1024 * 1024;
const MAX_ROWS: usize = 1_000;

pub struct ParsedEventRow {
    pub row_number: usize,
    pub input: PortfolioEventInput,
}

pub struct ParsedEventCsv {
    pub rows: Vec<ParsedEventRow>,
    pub issues: Vec<PortfolioEventImportRow>,
}

pub fn parse(csv_text: &str) -> AppResult<ParsedEventCsv> {
    if csv_text.trim().is_empty() {
        return Err(AppError::Validation("CSV 内容不能为空".into()));
    }
    if csv_text.len() > MAX_CSV_BYTES {
        return Err(AppError::Validation("CSV 不能超过 2 MB".into()));
    }

    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .flexible(false)
        .from_reader(csv_text.as_bytes());
    let raw_headers = reader
        .headers()
        .map_err(|error| AppError::Validation(format!("无法读取 CSV 表头：{error}")))?
        .clone();
    let mut header_positions = HashMap::new();
    for (index, header) in raw_headers.iter().enumerate() {
        let normalized = header
            .trim_start_matches('\u{feff}')
            .trim()
            .to_ascii_lowercase();
        if header_positions.insert(normalized.clone(), index).is_some() {
            return Err(AppError::Validation(format!("CSV 表头重复：{normalized}")));
        }
    }
    let allowed_headers = [
        "source",
        "external_id",
        "event_type",
        "occurred_on",
        "amount",
        "currency",
        "fx_rate_to_base",
        "fx_rate_source",
        "fx_rate_observed_on",
        "asset_name",
        "note",
    ];
    if let Some(unexpected) = header_positions
        .keys()
        .find(|header| !allowed_headers.contains(&header.as_str()))
    {
        return Err(AppError::Validation(format!(
            "CSV 包含未知列：{unexpected}"
        )));
    }
    for required in [
        "source",
        "external_id",
        "event_type",
        "occurred_on",
        "amount",
        "currency",
        "note",
    ] {
        if !header_positions.contains_key(required) {
            return Err(AppError::Validation(format!("CSV 缺少必需列：{required}")));
        }
    }

    let mut rows = Vec::new();
    let mut issues = Vec::new();
    for (index, result) in reader.records().enumerate() {
        let row_number = index + 2;
        if rows.len() + issues.len() >= MAX_ROWS {
            return Err(AppError::Validation("CSV 单次最多导入 1000 行".into()));
        }
        let record = match result {
            Ok(record) => record,
            Err(error) => {
                issues.push(issue(row_number, format!("CSV 行结构无效：{error}")));
                continue;
            }
        };
        if record.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        match parse_record(row_number, &record, &header_positions) {
            Ok(row) => rows.push(row),
            Err(row) => issues.push(*row),
        }
    }
    if rows.is_empty() && issues.is_empty() {
        return Err(AppError::Validation("CSV 没有数据行".into()));
    }
    Ok(ParsedEventCsv { rows, issues })
}

fn parse_record(
    row_number: usize,
    record: &StringRecord,
    positions: &HashMap<String, usize>,
) -> Result<ParsedEventRow, Box<PortfolioEventImportRow>> {
    let value = |name: &str| {
        positions
            .get(name)
            .and_then(|index| record.get(*index))
            .unwrap_or_default()
            .trim()
    };
    let source = value("source").to_owned();
    let external_id = value("external_id").to_owned();
    let event_type_text = value("event_type").to_owned();
    let occurred_on = value("occurred_on").to_owned();
    let currency = value("currency").to_owned();
    let asset_name = value("asset_name").to_owned();
    let note = value("note").to_owned();
    let fx_rate_source = value("fx_rate_source").to_owned();
    let fx_rate_observed_on = value("fx_rate_observed_on").to_owned();
    let amount_text = value("amount");

    let amount = match amount_text.parse::<f64>() {
        Ok(amount) => amount,
        Err(_) => {
            return Err(Box::new(issue_with_values(
                row_number,
                "amount 必须是数字".into(),
                &source,
                &external_id,
                &event_type_text,
                &occurred_on,
                None,
                &currency,
                None,
                &fx_rate_source,
                &fx_rate_observed_on,
                &asset_name,
                &note,
            )));
        }
    };
    let event_type = match parse_event_type(&event_type_text) {
        Some(event_type) => event_type,
        None => {
            return Err(Box::new(issue_with_values(
                row_number,
                "event_type 必须是 deposit/withdrawal/dividend/interest/fee/tax/buy/sell 或对应中文".into(),
                &source,
                &external_id,
                &event_type_text,
                &occurred_on,
                Some(amount),
                &currency,
                None,
                &fx_rate_source,
                &fx_rate_observed_on,
                &asset_name,
                &note,
            )));
        }
    };
    let fx_text = value("fx_rate_to_base");
    let fx_rate_to_base = if fx_text.is_empty() {
        None
    } else {
        match fx_text.parse::<f64>() {
            Ok(rate) => Some(rate),
            Err(_) => {
                return Err(Box::new(issue_with_values(
                    row_number,
                    "fx_rate_to_base 必须为空或数字".into(),
                    &source,
                    &external_id,
                    &event_type_text,
                    &occurred_on,
                    Some(amount),
                    &currency,
                    None,
                    &fx_rate_source,
                    &fx_rate_observed_on,
                    &asset_name,
                    &note,
                )));
            }
        }
    };

    Ok(ParsedEventRow {
        row_number,
        input: PortfolioEventInput {
            event_type,
            source,
            external_id,
            asset_name,
            amount,
            currency,
            fx_rate_to_base,
            fx_rate_source,
            fx_rate_observed_on,
            occurred_on,
            note,
        },
    })
}

fn parse_event_type(value: &str) -> Option<PortfolioEventType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "deposit" | "入金" => Some(PortfolioEventType::Deposit),
        "withdrawal" | "出金" => Some(PortfolioEventType::Withdrawal),
        "dividend" | "分红" => Some(PortfolioEventType::Dividend),
        "interest" | "利息" => Some(PortfolioEventType::Interest),
        "fee" | "费用" => Some(PortfolioEventType::Fee),
        "tax" | "税费" => Some(PortfolioEventType::Tax),
        "buy" | "买入" => Some(PortfolioEventType::Buy),
        "sell" | "卖出" => Some(PortfolioEventType::Sell),
        _ => None,
    }
}

fn issue(row_number: usize, message: String) -> PortfolioEventImportRow {
    issue_with_values(
        row_number, message, "", "", "", "", None, "", None, "", "", "", "",
    )
}

#[allow(clippy::too_many_arguments)]
fn issue_with_values(
    row_number: usize,
    message: String,
    source: &str,
    external_id: &str,
    event_type: &str,
    occurred_on: &str,
    amount: Option<f64>,
    currency: &str,
    fx_rate_to_base: Option<f64>,
    fx_rate_source: &str,
    fx_rate_observed_on: &str,
    asset_name: &str,
    note: &str,
) -> PortfolioEventImportRow {
    PortfolioEventImportRow {
        row_number,
        status: "error".into(),
        message,
        source: source.into(),
        external_id: external_id.into(),
        event_type: event_type.into(),
        occurred_on: occurred_on.into(),
        amount,
        currency: currency.into(),
        fx_rate_to_base,
        fx_rate_source: fx_rate_source.into(),
        fx_rate_observed_on: fx_rate_observed_on.into(),
        asset_name: asset_name.into(),
        note: note.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_chinese_and_english_rows() {
        let parsed = parse(
            "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商A,id-1,买入,2026-09-01,1000,CNY,,指数基金,\"定投,自动扣款\"\nbroker,id-2,dividend,2026-09-02,12.5,USD,7.0,指数基金,distribution\n",
        )
        .unwrap();
        assert!(parsed.issues.is_empty());
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[0].input.event_type, PortfolioEventType::Buy);
        assert_eq!(parsed.rows[0].input.note, "定投,自动扣款");
        assert_eq!(parsed.rows[1].input.fx_rate_to_base, Some(7.0));
    }

    #[test]
    fn reports_row_errors_without_losing_valid_rows() {
        let parsed = parse(
            "source,external_id,event_type,occurred_on,amount,currency,note\na,id-1,deposit,2026-09-01,100,CNY,ok\na,id-2,unknown,2026-09-02,nope,CNY,bad\n",
        )
        .unwrap();
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.issues.len(), 1);
        assert_eq!(parsed.issues[0].row_number, 3);
    }

    #[test]
    fn rejects_unknown_columns_instead_of_silently_ignoring_them() {
        assert!(matches!(
            parse("source,external_id,event_type,occurred_on,amount,currency,note,hidden\na,id-1,deposit,2026-09-01,100,CNY,ok,value\n"),
            Err(AppError::Validation(_))
        ));
    }
}
