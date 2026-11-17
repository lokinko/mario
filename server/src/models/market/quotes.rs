//! Public market query and quote contracts.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRateQuery {
    pub from_currency: String,
    pub to_currency: String,
    pub on_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRateQuote {
    #[serde(default)]
    pub exact_rate: Option<String>,
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub requested_on: String,
    pub observed_on: String,
    pub staleness_days: i64,
    pub provider_code: String,
    pub provider_name: String,
    pub source_url: String,
    pub methodology_url: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPriceQuery {
    pub symbol: String,
    pub on_date: String,
    #[serde(default)]
    pub action_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPriceQuote {
    #[serde(default)]
    pub exact_close: Option<String>,
    #[serde(default)]
    pub splits: Vec<SplitEvent>,
    pub symbol: String,
    pub currency: String,
    pub close: f64,
    pub requested_on: String,
    pub observed_on: String,
    pub staleness_days: i64,
    pub provider_code: String,
    pub provider_name: String,
    pub exchange: String,
    pub mic_code: String,
    pub instrument_type: String,
    pub price_basis: String,
    pub source_url: String,
    pub methodology_url: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SplitEvent {
    pub effective_on: String,
    pub numerator: String,
    pub denominator: String,
}
