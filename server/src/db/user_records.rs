//! Persistence adapters for owned record capabilities; transactions remain in their repositories.
use super::Database;
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
        reviews::{
            InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, ReviewReminderSummary,
            SystemReviewInput, SystemReviewRecord,
        },
        watch::{grid::GridPlan, rules::WatchRules, WatchEntry, WatchInput, WatchRow},
    },
    AppResult,
};

use crate::ports::user_records::{
    EvidenceAccount, LedgerAccount, MethodologyAccount, ReviewAccount, WatchManagement,
};

impl LedgerAccount for Database {
    fn save_profile(&self, profile: &FinancialProfile) -> AppResult<Snapshot> {
        Database::save_profile(self, profile)
    }
    fn delete_holding(&self, id: &str, input: &HoldingDeleteInput) -> AppResult<Snapshot> {
        Database::delete_holding(self, id, input)
    }
    fn save_holding_trade(&self, input: &HoldingTradeInput) -> AppResult<Snapshot> {
        Database::save_holding_trade(self, input)
    }
    fn update_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeUpdateInput,
    ) -> AppResult<Snapshot> {
        Database::update_holding_trade(self, id, input)
    }
    fn delete_holding_trade(
        &self,
        id: &str,
        input: &HoldingTradeDeleteInput,
    ) -> AppResult<Snapshot> {
        Database::delete_holding_trade(self, id, input)
    }
    fn update_holding_amount(&self, id: &str, input: &AmountInput) -> AppResult<Snapshot> {
        Database::update_holding_amount(self, id, input)
    }
    fn update_option_price(&self, id: &str, input: &OptionPriceInput) -> AppResult<Snapshot> {
        Database::update_option_price(self, id, input)
    }
    fn daily_compare(&self, query: &CompareQuery) -> AppResult<DailyComparison> {
        Database::daily_compare(self, query)
    }
    fn financial_history(&self, id: &str) -> AppResult<serde_json::Value> {
        Database::financial_history(self, id)
    }
    fn preview_trade_repair(
        &self,
        id: &str,
        input: &TradeRepairInput,
    ) -> AppResult<HoldingTradeSummary> {
        Database::preview_trade_repair(self, id, input)
    }
    fn repair_trades(&self, id: &str, input: &TradeRepairInput) -> AppResult<Snapshot> {
        Database::repair_trades(self, id, input)
    }
    fn declare_corporate_action(
        &self,
        id: &str,
        input: &ManualCorporateActionInput,
    ) -> AppResult<Snapshot> {
        Database::declare_corporate_action(self, id, input)
    }
    fn review_corporate_action(
        &self,
        id: &str,
        input: &ConfirmCorporateActionInput,
        mode: &str,
    ) -> AppResult<Snapshot> {
        Database::review_corporate_action(self, id, input, mode)
    }
    fn replay_financial_request(
        &self,
        method: &str,
        path: &str,
        input: &serde_json::Value,
    ) -> AppResult<Option<Snapshot>> {
        Database::replay_financial_request(self, method, path, input)
    }
}

impl WatchManagement for Database {
    fn watch_entries(&self) -> AppResult<Vec<WatchEntry>> {
        Database::watch_entries(self)
    }
    fn watch_rules(&self) -> AppResult<WatchRules> {
        Database::watch_rules(self)
    }
    fn save_watch_rules(&self, rules: WatchRules, expected: WatchRules) -> AppResult<()> {
        Database::save_watch_rules(self, rules, expected)
    }
    fn watch_row(&self, symbol: &str) -> AppResult<WatchRow> {
        Database::watch_row(self, symbol)
    }
    fn save_watch(&self, input: WatchInput) -> AppResult<WatchEntry> {
        Database::save_watch(self, input)
    }
    fn add_watch_if_missing(&self, input: WatchInput) -> AppResult<WatchEntry> {
        Database::add_watch_if_missing(self, input)
    }
    fn delete_watch(&self, symbol: &str) -> AppResult<()> {
        Database::delete_watch(self, symbol)
    }
    fn read_watch_alerts(&self, symbol: &str) -> AppResult<()> {
        Database::read_watch_alerts(self, symbol)
    }
    fn set_watch_pinned(&self, symbol: &str, pinned: bool) -> AppResult<WatchEntry> {
        Database::set_watch_pinned(self, symbol, pinned)
    }
    fn set_watch_grid(&self, symbol: &str, plan: Option<GridPlan>) -> AppResult<()> {
        Database::set_watch_grid(self, symbol, plan)
    }
}

impl EvidenceAccount for Database {
    fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        Database::research_evidence(self)
    }
    fn add_research_evidence(&self, input: &ResearchEvidenceInput) -> AppResult<ResearchEvidence> {
        Database::add_research_evidence(self, input)
    }
    fn research_attachment(
        &self,
        id: &str,
        attachment_id: &str,
    ) -> AppResult<ResearchAttachmentDownload> {
        Database::research_attachment(self, id, attachment_id)
    }
    fn set_research_evidence_status(&self, id: &str, active: bool) -> AppResult<ResearchEvidence> {
        Database::set_research_evidence_status(self, id, active)
    }
    fn set_research_evidence_tags(
        &self,
        id: &str,
        input: &ResearchEvidenceTagsInput,
    ) -> AppResult<ResearchEvidence> {
        Database::set_research_evidence_tags(self, id, input)
    }
}

impl MethodologyAccount for Database {
    fn investment_methodology(&self) -> AppResult<InvestmentMethodology> {
        Database::investment_methodology(self)
    }
    fn save_investment_methodology(
        &self,
        input: InvestmentMethodologyInput,
    ) -> AppResult<InvestmentMethodology> {
        Database::save_investment_methodology(self, input)
    }
}

impl ReviewAccount for Database {
    fn investment_rules(&self) -> AppResult<Vec<InvestmentRule>> {
        Database::investment_rules(self)
    }
    fn add_investment_rule(&self, input: &InvestmentRuleInput) -> AppResult<InvestmentRule> {
        Database::add_investment_rule(self, input)
    }
    fn update_investment_rule(
        &self,
        id: &str,
        input: &InvestmentRuleInput,
    ) -> AppResult<InvestmentRule> {
        Database::update_investment_rule(self, id, input)
    }
    fn investment_rule_history(&self, id: &str) -> AppResult<Vec<InvestmentRuleRevision>> {
        Database::investment_rule_history(self, id)
    }
    fn system_reviews(&self) -> AppResult<Vec<SystemReviewRecord>> {
        Database::system_reviews(self)
    }
    fn save_system_review(&self, input: &SystemReviewInput) -> AppResult<SystemReviewRecord> {
        Database::save_system_review(self, input)
    }
    fn review_reminder_summary(
        &self,
        today: chrono::NaiveDate,
    ) -> AppResult<ReviewReminderSummary> {
        Database::review_reminder_summary(self, today)
    }
}
