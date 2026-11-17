//! Persistence adapters for the narrow capabilities consumed by AI use cases.
use super::Database;
use crate::{
    models::{
        accounting::analysis::AuthorizedFinancialContext,
        analysis::{AnalysisResult, AnalysisSettings, ContextSelection},
        connection::ModelConfig,
        memory::{
            AcceptedOperation, DreamInput, DreamSourceAnalysis, DreamState, DreamSummary,
            MemoryNote,
        },
        methodology::InvestmentMethodology,
        provider::ModelProviderKind,
        research::ResearchEvidence,
        watch::WatchBoard,
    },
    ports::ai_account::{
        AnalysisAccount, MemoryAccount, ModelConfiguration, ModelSettingsStore, UserAnalysisSources,
    },
    AppResult,
};

impl ModelConfiguration for Database {
    fn model_config(&self) -> AppResult<ModelConfig> {
        Database::model_config(self)
    }
    fn model_source(&self) -> AppResult<Option<String>> {
        self.setting("model.source")
    }
    fn selected_server_model(&self) -> AppResult<Option<String>> {
        self.setting("model.server_name")
    }
    fn model_key_binding(&self) -> AppResult<Option<String>> {
        self.setting("model.key_binding")
    }
    fn model_credential(&self) -> AppResult<String> {
        self.secret("model")
    }
    fn bridge_id(&self) -> AppResult<Option<String>> {
        self.setting("model.codex_bridge_id")
    }
}
impl ModelSettingsStore for Database {
    fn revision(&self) -> AppResult<String> {
        Database::revision(self)
    }
    fn save_model_configuration(
        &self,
        provider: ModelProviderKind,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
    ) -> AppResult<()> {
        Database::save_model_configuration(self, provider, base_url, model, api_key)
    }
    fn save_server_model(&self, model: &str) -> AppResult<()> {
        Database::save_server_model(self, model)
    }
    fn set_bridge_id(&self, id: &str) -> AppResult<()> {
        self.set_setting("model.codex_bridge_id", id)
    }
    fn clear_bridge_id(&self) -> AppResult<()> {
        self.delete_setting("model.codex_bridge_id")
    }
    fn remove_model_credential(&self) -> AppResult<()> {
        self.remove_secret("model")
    }
}
impl UserAnalysisSources for Database {
    fn investment_methodology(&self) -> AppResult<InvestmentMethodology> {
        Database::investment_methodology(self)
    }
    fn financial_context(
        &self,
        selection: &ContextSelection,
    ) -> AppResult<AuthorizedFinancialContext> {
        Ok(AuthorizedFinancialContext::from_snapshot(
            &Database::snapshot(self)?,
            selection.include_profile,
            selection.include_holdings,
        ))
    }
    fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        Database::research_evidence(self)
    }
    fn watch_board(&self, day: chrono::NaiveDate) -> AppResult<WatchBoard> {
        Database::watch_board(self, day)
    }
}
impl AnalysisAccount for Database {
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::active_memory_notes(self)
    }
    fn automatic_research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        Database::automatic_research_evidence(self)
    }
    fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()> {
        Database::save_analysis(self, result, question)
    }
}
impl MemoryAccount for Database {
    fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        Database::analysis_settings(self)
    }
    fn undreamed_analyses(&self) -> AppResult<(Vec<DreamSourceAnalysis>, usize)> {
        Database::undreamed_analyses(self)
    }
    fn memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::memory_notes(self)
    }
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::active_memory_notes(self)
    }
    fn dream_state(&self) -> AppResult<DreamState> {
        Database::dream_state(self)
    }
    fn set_memory_status(&self, id: &str, active: bool) -> AppResult<MemoryNote> {
        Database::set_memory_status(self, id, active)
    }
    fn delete_memory(&self, id: &str) -> AppResult<()> {
        Database::delete_memory(self, id)
    }
    fn apply_dream(
        &self,
        input: &DreamInput,
        operations: &[AcceptedOperation],
        rejected: Vec<String>,
    ) -> AppResult<DreamSummary> {
        Database::apply_dream(self, input, operations, rejected)
    }
}
