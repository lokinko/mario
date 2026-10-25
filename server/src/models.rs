pub(crate) mod analysis_history;
pub use analysis_history::StoredAnalysisReport;
mod endpoint;
pub(crate) mod inference;
pub(crate) mod information;
pub(crate) mod market;
pub(crate) mod methodology;
mod provider;
pub(crate) use endpoint::validate_endpoint;
pub use provider::ModelProviderKind;
pub(crate) mod accounting;
pub(crate) mod daily;
pub(crate) mod decimal;
pub(crate) mod options;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorporateActionRecord {
    pub id: String,
    pub holding_id: String,
    pub split: SplitEvent,
    pub applied: bool,
    #[serde(default)]
    pub dismissed: bool,
    #[serde(default)]
    pub user_declared: bool,
    #[serde(default)]
    pub symbol: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmCorporateActionInput {
    pub expected_revision: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualCorporateActionInput {
    pub request_id: String,
    pub expected_revision: String,
    pub split: SplitEvent,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPriceConfig {
    pub provider: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPriceConfigInput {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedHoldingValuationInput {
    pub symbol: String,
    pub quantity: f64,
    pub on_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingValuationEvidence {
    pub holding_id: String,
    pub symbol: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub market_value: f64,
    pub currency: String,
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
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialProfile {
    #[serde(default)]
    pub exact_amounts: Option<accounting::ProfileAmounts>,
    pub monthly_income: f64,
    pub monthly_expense: f64,
    pub emergency_fund: f64,
    pub liabilities: f64,
    pub investable_assets: f64,
    pub horizon_years: i64,
    pub max_drawdown_pct: f64,
    pub risk_level: String,
    #[serde(default = "default_currency")]
    pub base_currency: String,
}

impl Default for FinancialProfile {
    fn default() -> Self {
        Self {
            exact_amounts: None,
            monthly_income: 0.0,
            monthly_expense: 0.0,
            emergency_fund: 0.0,
            liabilities: 0.0,
            investable_assets: 0.0,
            horizon_years: 5,
            max_drawdown_pct: 15.0,
            risk_level: "稳健".into(),
            base_currency: default_currency(),
        }
    }
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
    #[serde(default)]
    pub cost_price: Option<f64>,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub current_price: Option<f64>,
    pub target_pct: f64,
    pub currency: String,
    #[serde(default)]
    pub fx_rate_to_base: Option<f64>,
    #[serde(default)]
    pub valuation_date: String,
    #[serde(default)]
    pub fx_rate_source: String,
    #[serde(default)]
    pub fx_rate_observed_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingInput {
    #[serde(default)]
    pub exact_values: Option<accounting::HoldingAmounts>,
    #[serde(default)]
    pub cash_amount: Option<String>,
    pub symbol: String,
    pub name: String,
    pub asset_class: String,
    pub market_value: f64,
    pub cost_basis: f64,
    #[serde(default)]
    pub cost_price: Option<f64>,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub current_price: Option<f64>,
    pub target_pct: f64,
    pub currency: String,
    #[serde(default)]
    pub fx_rate_to_base: Option<f64>,
    #[serde(default)]
    pub valuation_date: String,
    #[serde(default)]
    pub fx_rate_source: String,
    #[serde(default)]
    pub fx_rate_observed_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioAllocationChange {
    pub asset_class: String,
    pub previous_value: f64,
    pub current_value: f64,
    pub value_change: f64,
    pub previous_pct: f64,
    pub current_pct: f64,
    pub pct_point_change: f64,
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
    #[serde(default)]
    pub option_contracts: std::collections::BTreeMap<String, options::OptionContract>,
    #[serde(default)]
    pub stock_option_totals: std::collections::BTreeMap<String, options::StockOptionTotals>,
    #[serde(default)]
    pub accounting: Option<accounting::AccountingSnapshot>,
    #[serde(default)]
    pub corporate_actions: Vec<CorporateActionRecord>,
    #[serde(default)]
    pub cash_balances: Vec<CashBalanceState>,
    #[serde(default)]
    pub price_refresh_errors: Vec<String>,
    #[serde(default)]
    pub holding_trades: Vec<HoldingTrade>,
    #[serde(default)]
    pub holding_trade_summaries: std::collections::BTreeMap<String, HoldingTradeSummary>,
    #[serde(default)]
    pub cleared_trade_holdings: Vec<String>,
    #[serde(default)]
    pub holding_revisions: std::collections::BTreeMap<String, String>,
    pub profile: FinancialProfile,
    pub holdings: Vec<Holding>,
    #[serde(default)]
    pub holding_valuations: Vec<HoldingValuationEvidence>,
    pub findings: Vec<RiskFinding>,
    pub total_value: f64,
    pub emergency_months: f64,
    pub concentration_pct: f64,
    pub valuation_status: PortfolioValuationStatus,
    pub plan: PortfolioPlan,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashBalanceState {
    pub currency: String,
    pub holding_id: String,
    pub amount: String,
    pub baseline_id: String,
    pub confirmed_on: String,
    pub confirmed_at: String,
    pub needs_reconciliation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingTrade {
    pub id: String,
    pub holding_id: String,
    pub kind: TradeKind,
    pub occurred_on: String,
    pub sequence: u32,
    pub quantity: String,
    pub price: String,
    pub gross_amount: String,
    pub fees: String,
    pub taxes: String,
    pub currency: String,
    pub account: String,
    pub external_id: String,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeSecurityInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<options::OptionContract>,
    pub name: String,
    pub symbol: String,
    pub asset_class: String,
    pub currency: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingTradeSummary {
    pub account: String,
    pub quantity: String,
    pub cost_basis: String,
    pub average_cost: String,
    pub realized_profit: String,
    pub dividend_income: String,
    pub settled_profit: String,
    pub total_fees: String,
    pub total_taxes: String,
    pub net_cash_flow: String,
    pub profits: std::collections::BTreeMap<String, String>,
    pub cash_flows: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingTradeInput {
    #[serde(default)]
    pub expected_cash_baseline: String,
    pub request_id: String,
    #[serde(default)]
    pub holding_id: String,
    pub expected_revision: Option<String>,
    pub new_holding: Option<TradeSecurityInput>,
    pub kind: TradeKind,
    pub occurred_on: String,
    pub sequence: u32,
    pub quantity: String,
    pub price: String,
    pub gross_amount: String,
    pub fees: String,
    pub taxes: String,
    pub account: String,
    #[serde(default)]
    pub external_id: String,
    #[serde(default)]
    pub note: String,
    pub settlement_mode: SettlementMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    Buy,
    Sell,
    /// Cash dividend before tax; carries no quantity or price.
    Dividend,
    /// Explicit broker-confirmed expiry without exercise or assignment.
    Expire,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMode {
    History,
    Settle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingTradeDeleteInput {
    pub expected_revision: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingDeleteInput {
    #[serde(default)]
    pub expected_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeRepairInput {
    pub expected_revision: String,
    pub trades: Vec<HoldingTrade>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoldingTradeUpdateInput {
    pub expected_revision: String,
    pub kind: TradeKind,
    pub occurred_on: String,
    pub sequence: u32,
    pub quantity: String,
    pub price: String,
    pub gross_amount: String,
    pub fees: String,
    pub taxes: String,
    pub account: String,
    #[serde(default)]
    pub external_id: String,
    #[serde(default)]
    pub note: String,
    pub settlement_mode: SettlementMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioValuationStatus {
    pub base_currency: String,
    pub comparable: bool,
    pub missing_fx_holdings: Vec<String>,
    pub undated_holding_count: usize,
    pub valuation_dates: Vec<String>,
    pub aligned_valuation_date: Option<String>,
    pub issues: Vec<DataIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueCode {
    MissingFxRate,
    PendingCorporateAction,
    QuarantinedTrades,
    CashNeedsReconciliation,
    InvalidTargetWeights,
}

impl IssueCode {
    pub fn blocks_portfolio_value(self) -> bool {
        matches!(
            self,
            Self::MissingFxRate | Self::PendingCorporateAction | Self::QuarantinedTrades
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataIssue {
    pub code: IssueCode,
    pub holding_ids: Vec<String>,
    pub currency: Option<String>,
    pub message: String,
}

fn default_currency() -> String {
    "CNY".into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioPlan {
    #[serde(default)]
    pub exact_monthly_surplus: Option<String>,
    pub monthly_surplus: f64,
    pub modeled_annual_return_pct: f64,
    pub modeled_annual_volatility_pct: f64,
    pub stress_loss_pct: f64,
    pub risk_capacity_pct: f64,
    pub risk_status: String,
    pub rebalancing: Vec<RebalanceAction>,
    pub rebalancing_blocked_by: Vec<IssueCode>,
    pub excluded_holding_ids: Vec<String>,
    pub as_of: Option<String>,
    pub target_total_pct: f64,
    pub assumptions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalanceAction {
    #[serde(default)]
    pub exact_amount: Option<String>,
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
    pub provider: ModelProviderKind,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModel {
    pub model: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigInput {
    pub provider: ModelProviderKind,
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
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub user_message: Option<String>,
    pub question: String,
    pub workflow: String,
    pub reflect: bool,
    pub explore_alternatives: bool,
    #[serde(default)]
    pub context_selection: ContextSelection,
    #[serde(default)]
    pub preview_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisSettings {
    pub workflow: String,
    pub reflect: bool,
    pub explore_alternatives: bool,
    pub web_search: bool,
    pub context_selection: ContextSelection,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            workflow: "quick".into(),
            reflect: false,
            explore_alternatives: false,
            web_search: false,
            context_selection: ContextSelection::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSelection {
    #[serde(default = "enabled")]
    pub include_profile: bool,
    #[serde(default = "enabled")]
    pub include_holdings: bool,
    #[serde(default = "enabled")]
    pub include_evidence: bool,
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            include_profile: true,
            include_holdings: true,
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
    pub provider: ModelProviderKind,
    pub model: String,
    pub workflow: String,
    pub groups: Vec<ContextGroup>,
    pub evidence_candidates: Vec<ResearchEvidence>,
    pub payload: serde_json::Value,
    pub payload_bytes: usize,
    pub context_revision: String,
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
    pub answer: String,
    pub suggestions: Vec<AnalysisSuggestion>,
    pub facts: Vec<AnalysisClaim>,
    pub question: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisSuggestion {
    pub action: String,
    pub why: String,
    pub fact_indices: Vec<usize>,
    pub unless_when: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisClaim {
    pub statement: String,
    pub basis: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
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
    #[serde(default)]
    pub attachments: Vec<ResearchAttachment>,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub evidence_type: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub stance: String,
    #[serde(default)]
    pub captured_at: String,
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
    pub valuation_evidence: Vec<HoldingValuationEvidence>,
    #[serde(default)]
    pub information_selection: Vec<information::SelectedInformation>,
    #[serde(default)]
    pub web_search: Option<WebSearchTrace>,
    #[serde(default)]
    // Read-only historical v7 artifacts; current workflow never produces these.
    pub advice_grounding: Vec<information::AdviceGrounding>,
    #[serde(default)]
    pub personal_context: Option<serde_json::Value>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub research_plan: Option<String>,
    #[serde(default)]
    pub alternatives: Vec<AnalysisAlternative>,
    #[serde(default)]
    pub critique: Option<String>,
    #[serde(default)]
    pub calls: Vec<ModelCallTrace>,
    #[serde(default)]
    pub structured_report: Option<StoredAnalysisReport>,
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
    #[serde(default)]
    pub request_count: Option<usize>,
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
    #[serde(default)]
    pub grounding_status: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ResearchEvidenceInput {
    pub content: String,
    pub attachments: Vec<ResearchAttachmentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidence {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ResearchAttachment>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachmentInput {
    pub name: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachment {
    pub id: String,
    pub name: String,
    pub byte_size: usize,
    #[serde(default, skip_serializing)]
    pub extracted_text: String,
    pub extraction_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachmentDownload {
    pub name: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchTrace {
    pub status: String,
    pub search_calls: usize,
    pub warnings: Vec<String>,
}

impl AnalysisRequest {
    pub fn validate(&self) -> crate::AppResult<()> {
        if self.question.trim().is_empty() {
            return Err(crate::AppError::Validation("分析问题不能为空".into()));
        }
        if !matches!(self.workflow.as_str(), "quick" | "deep") {
            return Err(crate::AppError::Validation("未知的分析工作流".into()));
        }
        Ok(())
    }
}
