//! User-authored rules and reviews.
fn default_currency() -> String {
    "CNY".into()
}
fn default_true() -> bool {
    true
}
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReminderSummary {
    pub periodic_review_due: bool,
    pub checked_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentRuleInput {
    pub category: String,
    pub statement: String,
    pub trigger: String,
    pub rationale: String,
    pub active: bool,
    pub source_review_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentRule {
    pub id: String,
    pub category: String,
    pub statement: String,
    pub trigger: String,
    pub rationale: String,
    pub active: bool,
    pub source_review_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentRuleRevision {
    pub rule_id: String,
    pub revision: i64,
    pub category: String,
    pub statement: String,
    pub trigger: String,
    pub rationale: String,
    pub active: bool,
    pub source_review_id: Option<String>,
    pub changed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemReviewInput {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemReviewSnapshot {
    pub portfolio_value: f64,
    #[serde(default = "default_currency")]
    pub base_currency: String,
    #[serde(default = "default_true")]
    pub portfolio_comparable: bool,
    pub emergency_months: f64,
    pub concentration_pct: f64,
    pub risk_status: String,
    pub high_risk_findings: usize,
    pub active_rules: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemReviewRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub id: String,
    pub period_label: String,
    pub adherence_score: Option<i64>,
    pub process_summary: String,
    pub rule_violations: String,
    pub lessons: String,
    pub next_actions: String,
    pub next_review_date: String,
    pub snapshot: SystemReviewSnapshot,
    pub created_at: String,
}
