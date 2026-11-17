//! User-owned FX choices and account projection; public quotes remain market contracts.
use super::market::quotes::FxRateQuote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRate {
    #[serde(default)]
    pub exact_rate: Option<String>,
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
    #[serde(default)]
    pub exact_rate: Option<String>,
    pub currency: String,
    pub rate: Option<f64>,
}
