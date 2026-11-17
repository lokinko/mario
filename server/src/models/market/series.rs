//! Public price and dividend inputs, independent of observation preferences.
use super::dividends::{DividendBasis, DividendEvent};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceBar {
    pub day: NaiveDate,
    pub close: Option<f64>,
    pub adjusted_close: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SecurityMarketData {
    pub price_revision: Option<String>,
    pub bars: Vec<PriceBar>,
    pub calendar: Vec<NaiveDate>,
    pub dividends: Option<Vec<DividendEvent>>,
    pub dividend_basis: Option<DividendBasis>,
    pub sources: Vec<String>,
}
