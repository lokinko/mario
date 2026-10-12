//! Market value contracts and deterministic identity/date policies. No providers or storage.
use super::FxRateQuote;
use crate::{AppError, AppResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub fn today() -> String {
    Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive()
        .to_string()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRate {
    pub rate: f64,
    pub observed_on: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxBook {
    pub base_currency: String,
    pub checked_at: i64,
    pub requested_on: String,
    pub quotes: Vec<FxRateQuote>,
    pub manual: BTreeMap<String, ManualRate>,
    pub error: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FxOverrideInput {
    pub currency: String,
    pub rate: Option<f64>,
}

pub(crate) const METHODOLOGY_URL: &str =
    "https://help.yahoo.com/kb/finance/article-exchanges-data-delays-sln2310.html";
pub(crate) fn canonical_symbol(value: &str) -> AppResult<String> {
    let code = value.trim().to_ascii_uppercase();
    if code.is_empty()
        || code.len() > 40
        || !code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-^=".contains(&b))
    {
        return Err(AppError::Validation(
            "请填写有效证券代码，例如 AAPL、0700.HK、600519.SS、000001.SZ".into(),
        ));
    }
    for (prefix, suffix) in [("SH", "SS"), ("SZ", "SZ")] {
        if let Some(digits) = code
            .strip_prefix(prefix)
            .filter(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit()))
        {
            return Ok(format!("{digits}.{suffix}"));
        }
    }
    if let Some(digits) = code.strip_suffix(".HK") {
        if (1..=5).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(format!("{:04}.HK", digits.parse::<u32>().unwrap()));
        }
    }
    // Bare numeric symbols are ambiguous (stock, fund or another exchange).
    if code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::Validation(
            "数字代码请带交易所后缀：.HK、.SS 或 .SZ".into(),
        ));
    }
    Ok(code)
}

pub(crate) const SUPPORTED_CURRENCIES: &[&str] = &["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
