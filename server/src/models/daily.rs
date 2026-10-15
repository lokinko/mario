//! Daily asset contracts shared by use cases, HTTP and persistence.
use super::Holding;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAsset {
    #[serde(default)]
    pub accounting: Option<super::accounting::AccountingHolding>,
    #[serde(default)]
    pub exact_base_value: Option<String>,
    pub holding: Holding,
    pub confirmed_on: Option<String>,
    pub carried: bool,
    pub removed: bool,
    pub base_value: Option<f64>,
    #[serde(default)]
    pub previous_day_change: Option<f64>,
    #[serde(default)]
    pub pct_point_change: Option<f64>,
    #[serde(default)]
    pub change_kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyRecord {
    #[serde(default)]
    pub exact_total_assets: Option<String>,
    #[serde(default)]
    pub exact_liabilities: String,
    #[serde(default)]
    pub exact_net_assets: Option<String>,
    pub day: String,
    pub base_currency: String,
    pub total_assets: Option<f64>,
    pub liabilities: f64,
    pub net_assets: Option<f64>,
    pub carried: bool,
    pub assets: Vec<DailyAsset>,
    pub missing_fx: Vec<String>,
    pub previous_day_change: Option<f64>,
    pub last_updated_day: Option<String>,
    pub since_last_update_change: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyHistory {
    pub timezone: String,
    pub today: String,
    pub records: Vec<DailyRecord>,
    pub next_before: Option<String>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub before: Option<String>,
    pub limit: Option<usize>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureInput {
    pub timezone: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AmountInput {
    #[serde(deserialize_with = "super::decimal::text")]
    pub amount: String,
    pub expected_revision: String,
    pub request_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareQuery {
    pub from: String,
    pub to: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetChange {
    pub id: String,
    pub name: String,
    pub status: String,
    pub previous: Option<DailyAsset>,
    pub current: Option<DailyAsset>,
    pub amount_change: Option<f64>,
    pub pct_point_change: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyComparison {
    pub from: DailyRecord,
    pub to: DailyRecord,
    pub amount_change: Option<f64>,
    pub assets: Vec<AssetChange>,
    pub allocation_changes: Vec<crate::models::PortfolioAllocationChange>,
}
