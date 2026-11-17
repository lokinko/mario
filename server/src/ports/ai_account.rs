//! Account capabilities used by AI use cases. No SQL, arbitrary settings or user writes.
use super::account::AccountSession;
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
    AppResult,
};
pub(crate) trait ModelConfiguration: Send + Sync {
    fn model_config(&self) -> AppResult<ModelConfig>;
    fn model_source(&self) -> AppResult<Option<String>>;
    fn selected_server_model(&self) -> AppResult<Option<String>>;
    fn model_key_binding(&self) -> AppResult<Option<String>>;
    fn model_credential(&self) -> AppResult<String>;
    fn bridge_id(&self) -> AppResult<Option<String>>;
}

pub(crate) trait ModelSettingsStore: ModelConfiguration + AccountSession {
    fn revision(&self) -> AppResult<String>;
    fn save_model_configuration(
        &self,
        provider: ModelProviderKind,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
    ) -> AppResult<()>;
    fn save_server_model(&self, model: &str) -> AppResult<()>;
    fn set_bridge_id(&self, id: &str) -> AppResult<()>;
    fn clear_bridge_id(&self) -> AppResult<()>;
    fn remove_model_credential(&self) -> AppResult<()>;
}

/// Read-only sources provided by the user domain; never grants ledger mutation.
pub(crate) trait UserAnalysisSources: Send + Sync {
    fn investment_methodology(&self) -> AppResult<InvestmentMethodology>;
    fn financial_context(
        &self,
        selection: &ContextSelection,
    ) -> AppResult<AuthorizedFinancialContext>;
    fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>>;
    fn watch_board(&self, day: chrono::NaiveDate) -> AppResult<WatchBoard>;
}

pub(crate) trait AnalysisAccount:
    ModelConfiguration + AccountSession + UserAnalysisSources
{
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>>;
    fn automatic_research_evidence(&self) -> AppResult<Vec<ResearchEvidence>>;
    fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()>;
}

pub(crate) trait MemoryAccount: ModelConfiguration + AccountSession {
    fn analysis_settings(&self) -> AppResult<AnalysisSettings>;
    fn undreamed_analyses(&self) -> AppResult<(Vec<DreamSourceAnalysis>, usize)>;
    fn memory_notes(&self) -> AppResult<Vec<MemoryNote>>;
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>>;
    fn dream_state(&self) -> AppResult<DreamState>;
    fn set_memory_status(&self, id: &str, active: bool) -> AppResult<MemoryNote>;
    fn delete_memory(&self, id: &str) -> AppResult<()>;
    fn apply_dream(
        &self,
        input: &DreamInput,
        operations: &[AcceptedOperation],
        rejected: Vec<String>,
    ) -> AppResult<DreamSummary>;
}
