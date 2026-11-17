//! User financial facts, ledger operations and deterministic portfolio results.
use super::{accounting, market::quotes::SplitEvent, options};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileField {
    MonthlyIncome,
    MonthlyExpense,
    EmergencyFund,
    Liabilities,
    HorizonYears,
    MaxDrawdownPct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialProfile {
    #[serde(default)]
    pub provided_fields: Option<Vec<ProfileField>>,
    #[serde(default)]
    pub exact_amounts: Option<accounting::ProfileAmounts>,
    pub monthly_income: f64,
    pub monthly_expense: f64,
    pub emergency_fund: f64,
    pub liabilities: f64,
    pub horizon_years: i64,
    pub max_drawdown_pct: f64,
    #[serde(default = "default_currency")]
    pub base_currency: String,
}

impl Default for FinancialProfile {
    fn default() -> Self {
        Self {
            provided_fields: None,
            exact_amounts: None,
            monthly_income: 0.0,
            monthly_expense: 0.0,
            emergency_fund: 0.0,
            liabilities: 0.0,
            horizon_years: 5,
            max_drawdown_pct: 15.0,
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
