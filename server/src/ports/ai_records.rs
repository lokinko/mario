//! AI history and analysis preset capabilities, separate from user record stores.
use crate::{
    models::analysis::{AnalysisHistoryItem, AnalysisSettings, StoredAnalysis},
    AppResult,
};

pub(crate) trait AnalysisArchive: Send + Sync {
    fn analysis_history(&self) -> AppResult<Vec<AnalysisHistoryItem>>;
    fn analysis(&self, id: &str) -> AppResult<StoredAnalysis>;
}

pub(crate) trait AnalysisPresets: Send + Sync {
    fn analysis_settings(&self) -> AppResult<AnalysisSettings>;
    fn save_analysis_settings(&self, settings: AnalysisSettings) -> AppResult<AnalysisSettings>;
}
