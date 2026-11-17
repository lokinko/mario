//! Public disclosed plans and cache state, independent of account watch scopes.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStage {
    Preannounced,
    Proposed,
    Approved,
    Scheduled,
    Cancelled,
    NoDistribution,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanIssue {
    InvalidDates,
    InvalidAmount,
    UnknownProgress,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendPlan {
    pub id: String,
    pub symbol: String,
    pub report_date: NaiveDate,
    pub announced_on: NaiveDate,
    pub stage: PlanStage,
    pub source_progress: String,
    pub profile: String,
    pub cash_per_ten: Option<String>,
    pub bonus_per_ten: Option<String>,
    pub transfer_per_ten: Option<String>,
    pub record_date: Option<NaiveDate>,
    pub ex_date: Option<NaiveDate>,
    pub payment_date: Option<NaiveDate>,
    pub issues: Vec<PlanIssue>,
    pub sources: Vec<String>,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanSnapshot {
    pub plans: Vec<DividendPlan>,
    pub attempted_at: Option<String>,
    pub fetched_at: Option<String>,
    pub refresh_failed: bool,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn plan(symbol: &str, stage: PlanStage) -> DividendPlan {
        DividendPlan {
            id: format!("{symbol}:2026-06-30"),
            symbol: symbol.into(),
            report_date: "2026-06-30".parse().unwrap(),
            announced_on: "2026-11-06".parse().unwrap(),
            stage,
            source_progress: "实施分配".into(),
            profile: "10派1元（含税）".into(),
            cash_per_ten: Some("1".into()),
            bonus_per_ten: None,
            transfer_per_ten: None,
            record_date: Some("2026-11-10".parse().unwrap()),
            ex_date: Some("2026-11-11".parse().unwrap()),
            payment_date: None,
            issues: vec![],
            sources: vec!["https://example.test/dividend-plan".into()],
        }
    }
}
