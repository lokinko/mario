use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialProfile {
    pub monthly_income: f64,
    pub monthly_expense: f64,
    pub emergency_fund: f64,
    pub liabilities: f64,
    pub investable_assets: f64,
    pub horizon_years: i64,
    pub max_drawdown_pct: f64,
    pub risk_level: String,
}

impl Default for FinancialProfile {
    fn default() -> Self {
        Self {
            monthly_income: 0.0,
            monthly_expense: 0.0,
            emergency_fund: 0.0,
            liabilities: 0.0,
            investable_assets: 0.0,
            horizon_years: 5,
            max_drawdown_pct: 15.0,
            risk_level: "稳健".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    pub name: String,
    pub target_amount: f64,
    pub target_date: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalInput {
    pub name: String,
    pub target_amount: f64,
    pub target_date: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub asset_class: String,
    pub market_value: f64,
    pub cost_basis: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingInput {
    pub symbol: String,
    pub name: String,
    pub asset_class: String,
    pub market_value: f64,
    pub cost_basis: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskFinding {
    pub level: String,
    pub title: String,
    pub detail: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub profile: FinancialProfile,
    pub goals: Vec<Goal>,
    pub holdings: Vec<Holding>,
    pub findings: Vec<RiskFinding>,
    pub total_value: f64,
    pub emergency_months: f64,
    pub concentration_pct: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigInput {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConnectionTest {
    pub ok: bool,
    pub model: String,
    pub latency_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub question: String,
    pub workflow: String,
    pub use_memory: bool,
    pub reflect: bool,
    pub explore_alternatives: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub id: String,
    pub answer: String,
    pub stages: Vec<String>,
    pub created_at: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEntry {
    pub id: Option<String>,
    pub asset_name: String,
    pub thesis: String,
    pub counter_thesis: String,
    pub expected_return_pct: f64,
    pub downside_pct: f64,
    pub confidence_pct: f64,
    pub position_pct: f64,
    pub invalidation: String,
    pub review_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionReview {
    pub outcome_summary: String,
    pub actual_return_pct: Option<f64>,
    pub thesis_status: String,
    pub process_rating: i64,
    pub lessons: String,
    pub reviewed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionReviewInput {
    pub outcome_summary: String,
    pub actual_return_pct: Option<f64>,
    pub thesis_status: String,
    pub process_rating: i64,
    pub lessons: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRecord {
    pub id: String,
    pub asset_name: String,
    pub thesis: String,
    pub counter_thesis: String,
    pub expected_return_pct: f64,
    pub downside_pct: f64,
    pub confidence_pct: f64,
    pub position_pct: f64,
    pub invalidation: String,
    pub review_date: String,
    pub created_at: String,
    pub review: Option<DecisionReview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
}
