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
    pub current_amount: f64,
    pub monthly_contribution: f64,
    pub target_date: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalInput {
    pub name: String,
    pub target_amount: f64,
    pub current_amount: f64,
    pub monthly_contribution: f64,
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
    pub target_pct: f64,
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
    pub target_pct: f64,
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
    pub plan: PortfolioPlan,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioPlan {
    pub monthly_surplus: f64,
    pub committed_monthly: f64,
    pub modeled_annual_return_pct: f64,
    pub modeled_annual_volatility_pct: f64,
    pub stress_loss_pct: f64,
    pub risk_capacity_pct: f64,
    pub risk_status: String,
    pub goal_projections: Vec<GoalProjection>,
    pub rebalancing: Vec<RebalanceAction>,
    pub assumptions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalProjection {
    pub goal_id: String,
    pub name: String,
    pub months_remaining: i64,
    pub funded_pct: f64,
    pub estimated_success_pct: f64,
    pub conservative_amount: f64,
    pub median_amount: f64,
    pub required_monthly_contribution: f64,
    pub monthly_gap: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalanceAction {
    pub holding_id: String,
    pub name: String,
    pub current_pct: f64,
    pub target_pct: f64,
    pub deviation_pct: f64,
    pub amount: f64,
    pub direction: String,
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
    #[serde(default)]
    pub excluded_memory_ids: Vec<String>,
    #[serde(default)]
    pub context_selection: ContextSelection,
    #[serde(default)]
    pub preview_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSelection {
    #[serde(default = "enabled")]
    pub include_profile: bool,
    #[serde(default = "enabled")]
    pub include_goals: bool,
    #[serde(default = "enabled")]
    pub include_holdings: bool,
    #[serde(default = "enabled")]
    pub include_planning: bool,
    #[serde(default = "enabled")]
    pub include_risk_findings: bool,
    #[serde(default = "enabled")]
    pub include_rules: bool,
    #[serde(default = "enabled")]
    pub include_system_reviews: bool,
    #[serde(default = "enabled")]
    pub include_evidence: bool,
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            include_profile: true,
            include_goals: true,
            include_holdings: true,
            include_planning: true,
            include_risk_findings: true,
            include_rules: true,
            include_system_reviews: true,
            include_evidence: true,
        }
    }
}

fn enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextGroup {
    pub key: String,
    pub label: String,
    pub included: bool,
    pub record_count: usize,
    pub sensitivity: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPreview {
    pub provider: String,
    pub model: String,
    pub workflow: String,
    pub groups: Vec<ContextGroup>,
    pub memory_candidates: Vec<MemoryItem>,
    pub evidence_candidates: Vec<ResearchEvidence>,
    pub payload: serde_json::Value,
    pub payload_bytes: usize,
    pub context_revision: String,
    pub memory_policy: String,
    pub system_policy: String,
    pub local_only: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisTransparency {
    pub provider: String,
    pub model: String,
    pub context_groups: Vec<String>,
    pub payload_bytes: usize,
    pub context_revision: String,
    pub memory_items_used: usize,
    #[serde(default)]
    pub reviewed_memory_items_used: usize,
    #[serde(default)]
    pub conflicting_memory_items_used: usize,
    #[serde(default)]
    pub evidence_items_used: usize,
    #[serde(default)]
    pub citations_required: bool,
    #[serde(default)]
    pub model_calls: usize,
    #[serde(default)]
    pub total_latency_ms: u64,
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub structured_output_validated: bool,
    #[serde(default)]
    pub output_repairs: usize,
    pub external_data_used: bool,
    pub api_key_sent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredAnalysis {
    pub verdict: String,
    pub facts: Vec<AnalysisClaim>,
    pub inferences: Vec<AnalysisClaim>,
    pub unknowns: Vec<String>,
    pub options: Vec<AnalysisOption>,
    pub actions: Vec<AnalysisAction>,
    pub review_triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisClaim {
    pub statement: String,
    pub basis: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisOption {
    pub name: String,
    pub suitable_when: String,
    pub tradeoffs: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisAction {
    pub action: String,
    pub rationale: String,
    pub reversible: bool,
    pub review_trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputValidationTrace {
    pub status: String,
    pub attempts: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisEvidenceReference {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub source_url: String,
    pub source_tier: String,
    pub as_of_date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisWorkflowTrace {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub research_plan: Option<String>,
    #[serde(default)]
    pub memory_items: Vec<MemoryItem>,
    #[serde(default)]
    pub alternatives: Vec<AnalysisAlternative>,
    #[serde(default)]
    pub critique: Option<String>,
    #[serde(default)]
    pub calls: Vec<ModelCallTrace>,
    #[serde(default)]
    pub structured_report: Option<StructuredAnalysis>,
    #[serde(default)]
    pub output_validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub evidence_catalog: Vec<AnalysisEvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisAlternative {
    pub id: String,
    pub label: String,
    pub lens: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCallTrace {
    pub stage: String,
    pub label: String,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub id: String,
    pub answer: String,
    pub stages: Vec<String>,
    pub transparency: AnalysisTransparency,
    #[serde(default)]
    pub workflow_trace: AnalysisWorkflowTrace,
    pub created_at: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisHistoryItem {
    pub id: String,
    pub question: String,
    pub created_at: String,
    pub transparency: Option<AnalysisTransparency>,
    #[serde(default)]
    pub workflow_version: Option<String>,
    #[serde(default)]
    pub verdict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAnalysis {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub created_at: String,
    pub transparency: Option<AnalysisTransparency>,
    #[serde(default)]
    pub workflow_trace: Option<AnalysisWorkflowTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEntry {
    pub id: Option<String>,
    #[serde(default)]
    pub source_analysis_id: Option<String>,
    #[serde(default)]
    pub source_action_index: Option<usize>,
    pub asset_name: String,
    pub thesis: String,
    pub counter_thesis: String,
    pub expected_return_pct: f64,
    pub downside_pct: f64,
    pub confidence_pct: f64,
    pub position_pct: f64,
    pub invalidation: String,
    pub review_date: String,
    #[serde(default)]
    pub rule_checks: Vec<DecisionRuleCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRuleCheck {
    pub rule_id: String,
    pub rule_revision: i64,
    pub category: String,
    pub statement: String,
    pub trigger: String,
    pub status: String,
    #[serde(default)]
    pub note: String,
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
    pub source_analysis_id: Option<String>,
    pub source_action_index: Option<usize>,
    pub asset_name: String,
    pub thesis: String,
    pub counter_thesis: String,
    pub expected_return_pct: f64,
    pub downside_pct: f64,
    pub confidence_pct: f64,
    pub position_pct: f64,
    pub invalidation: String,
    pub review_date: String,
    pub rule_checks: Vec<DecisionRuleCheck>,
    pub created_at: String,
    pub review: Option<DecisionReview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEffectivenessSummary {
    pub total_decisions: usize,
    pub evaluated_decisions: usize,
    pub applicable_checks: usize,
    pub followed_checks: usize,
    pub adherence_pct: Option<f64>,
    pub reviewed_checks: usize,
    pub rules: Vec<RuleEffectivenessItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEffectivenessItem {
    pub rule_id: String,
    pub current_revision: i64,
    pub observed_revisions: Vec<i64>,
    pub category: String,
    pub statement: String,
    pub active: bool,
    pub decision_count: usize,
    pub applicable_count: usize,
    pub followed_count: usize,
    pub deviated_count: usize,
    pub reviewed_count: usize,
    pub followed_process_average: Option<f64>,
    pub deviated_process_average: Option<f64>,
    pub signal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSettingsInput {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReminderSummary {
    pub enabled: bool,
    pub due_decision_count: usize,
    pub periodic_review_due: bool,
    pub fingerprint: String,
    pub should_notify: bool,
    pub checked_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReminderAcknowledgeInput {
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub content: serde_json::Value,
    pub created_at: String,
    pub occurred_at: String,
    pub status: String,
    pub reviewed: bool,
    pub contradiction: bool,
    pub tags: Vec<String>,
    #[serde(default = "enabled")]
    pub selected: bool,
    #[serde(default)]
    pub retrieval: Option<MemoryRetrieval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetrieval {
    pub score: f64,
    pub age_days: i64,
    pub reasons: Vec<String>,
    pub passes: Vec<String>,
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
#[serde(rename_all = "camelCase")]
pub struct SystemReviewInput {
    pub period_label: String,
    pub adherence_score: i64,
    pub process_summary: String,
    pub rule_violations: String,
    pub lessons: String,
    pub next_actions: String,
    pub next_review_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemReviewSnapshot {
    pub portfolio_value: f64,
    pub emergency_months: f64,
    pub concentration_pct: f64,
    pub risk_status: String,
    pub high_risk_findings: usize,
    pub goal_total: usize,
    pub goals_on_track: usize,
    pub decision_total: usize,
    pub reviewed_decisions: usize,
    pub active_rules: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemReviewRecord {
    pub id: String,
    pub period_label: String,
    pub adherence_score: i64,
    pub process_summary: String,
    pub rule_violations: String,
    pub lessons: String,
    pub next_actions: String,
    pub next_review_date: String,
    pub snapshot: SystemReviewSnapshot,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidenceInput {
    pub asset_name: String,
    pub title: String,
    pub publisher: String,
    pub source_url: String,
    pub source_tier: String,
    pub evidence_type: String,
    pub stance: String,
    pub as_of_date: String,
    pub claim: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidence {
    pub id: String,
    pub asset_name: String,
    pub title: String,
    pub publisher: String,
    pub source_url: String,
    pub source_tier: String,
    pub evidence_type: String,
    pub stance: String,
    pub as_of_date: String,
    pub claim: String,
    pub notes: String,
    pub active: bool,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidenceStatusInput {
    pub active: bool,
}
