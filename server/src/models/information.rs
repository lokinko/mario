//! Immutable analysis annotations shared with storage, UI and workflow validation.
use serde::{Deserialize, Serialize};
// Model annotations are internal, query-specific aids. Original source fields
// remain immutable and are stored separately in the analysis's frozen catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedInformation {
    pub source_id: String,
    pub topic: String,
    pub entities: Vec<String>,
    pub information_type: String,
    pub event_date: Option<String>,
    pub date_quote: Option<String>,
    pub time_sensitivity: String,
    pub excerpt: String,
    pub relevance: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundingCheck {
    pub fact_index: usize,
    pub source_kind: String,
    pub source_ref: String,
    pub quote: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdviceGrounding {
    pub action_index: usize,
    pub status: String,
    pub reason: String,
    pub checks: Vec<GroundingCheck>,
}
