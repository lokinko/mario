//! Input precision and rounding belong to the domain, never to UI display formatting.
use crate::{AppError, AppResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};

pub(crate) fn parse(value: &str) -> AppResult<Decimal> {
    if value.is_empty()
        || value.len() > 32
        || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return Err(AppError::Validation("金额须为普通非负十进制数".into()));
    }
    let amount =
        Decimal::from_str_exact(value).map_err(|_| AppError::Validation("金额格式无效".into()))?;
    if amount.scale() > 8 || amount > Decimal::from(1_000_000_000_000_000_i64) {
        return Err(AppError::Validation("金额超出范围或超过 8 位小数".into()));
    }
    Ok(amount)
}

pub(crate) fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom("金额必须为十进制字符串")),
    }
}
