//! User record capabilities for independent business entry points.
use super::user_account::{PortfolioAccount, WatchAccount};
use crate::{
    models::{
        daily::{AmountInput, CompareQuery, DailyComparison},
        methodology::{InvestmentMethodology, InvestmentMethodologyInput},
        options::OptionPriceInput,
        portfolio::{
            ConfirmCorporateActionInput, FinancialProfile, HoldingDeleteInput,
            HoldingTradeDeleteInput, HoldingTradeInput, HoldingTradeSummary,
            HoldingTradeUpdateInput, ManualCorporateActionInput, Snapshot, TradeRepairInput,
        },
        research::{
            ResearchAttachmentDownload, ResearchEvidence, ResearchEvidenceInput,
            ResearchEvidenceTagsInput,
        },
        watch::{grid::GridPlan, rules::WatchRules, WatchEntry, WatchInput, WatchRow},
    },
    AppResult,
};

pub(crate) trait LedgerAccount: PortfolioAccount {
    fn save_profile(&self, profile: &FinancialProfile) -> AppResult<Snapshot>;
    fn delete_holding(&self, id: &str, input: &HoldingDeleteInput) -> AppResult<Snapshot>;
    fn save_holding_trade(&self, input: &HoldingTradeInput) -> AppResult<Snapshot>;
    fn update_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeUpdateInput,
    ) -> AppResult<Snapshot>;
    fn delete_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeDeleteInput,
    ) -> AppResult<Snapshot>;
    fn update_holding_amount(&self, id: &str, input: &AmountInput) -> AppResult<Snapshot>;
    fn update_option_price(&self, id: &str, input: &OptionPriceInput) -> AppResult<Snapshot>;
    fn daily_compare(&self, query: &CompareQuery) -> AppResult<DailyComparison>;
    fn financial_history(&self, id: &str) -> AppResult<serde_json::Value>;
    fn preview_trade_repair(
        &self,
        id: &str,
        input: &TradeRepairInput,
    ) -> AppResult<HoldingTradeSummary>;
    fn repair_trades(&self, id: &str, input: &TradeRepairInput) -> AppResult<Snapshot>;
    fn declare_corporate_action(
        &self,
        id: &str,
        input: &ManualCorporateActionInput,
    ) -> AppResult<Snapshot>;
    fn review_corporate_action(
        &self,
        id: &str,
        input: &ConfirmCorporateActionInput,
        mode: &str,
    ) -> AppResult<Snapshot>;
    fn replay_financial_request(
        &self,
        method: &str,
        path: &str,
        input: &serde_json::Value,
    ) -> AppResult<Option<Snapshot>>;
}

pub(crate) trait WatchManagement: WatchAccount {
    fn watch_entries(&self) -> AppResult<Vec<WatchEntry>>;
    fn watch_rules(&self) -> AppResult<WatchRules>;
    fn save_watch_rules(&self, rules: WatchRules, expected: WatchRules) -> AppResult<()>;
    fn watch_row(&self, symbol: &str) -> AppResult<WatchRow>;
    fn save_watch(&self, input: WatchInput) -> AppResult<WatchEntry>;
    fn add_watch_if_missing(&self, input: WatchInput) -> AppResult<WatchEntry>;
    fn delete_watch(&self, symbol: &str) -> AppResult<()>;
    fn read_watch_alerts(&self, symbol: &str) -> AppResult<()>;
    fn set_watch_pinned(&self, symbol: &str, pinned: bool) -> AppResult<WatchEntry>;
    fn set_watch_grid(&self, symbol: &str, plan: Option<GridPlan>) -> AppResult<()>;
}

pub(crate) trait EvidenceAccount: Send + Sync {
    fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>>;
    fn research_evidence_summary(&self) -> AppResult<Vec<ResearchEvidence>> {
        self.research_evidence()
    }
    fn add_research_evidence(&self, input: &ResearchEvidenceInput) -> AppResult<ResearchEvidence>;
    fn research_attachment(
        &self,
        id: &str,
        attachment_id: &str,
    ) -> AppResult<ResearchAttachmentDownload>;
    fn set_research_evidence_status(&self, id: &str, active: bool) -> AppResult<ResearchEvidence>;
    fn set_research_evidence_tags(
        &self,
        id: &str,
        input: &ResearchEvidenceTagsInput,
    ) -> AppResult<ResearchEvidence>;
}

pub(crate) trait MethodologyAccount: Send + Sync {
    fn investment_methodology(&self) -> AppResult<InvestmentMethodology>;
    fn save_investment_methodology(
        &self,
        input: InvestmentMethodologyInput,
    ) -> AppResult<InvestmentMethodology>;
}
