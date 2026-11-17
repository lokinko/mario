//! Persistence adapters for owned record capabilities; transactions remain in their repositories.
use crate::db::Database;
use crate::{
    models::analysis::{AnalysisHistoryItem, AnalysisSettings, StoredAnalysis},
    AppResult,
};

use crate::ports::ai_records::{AnalysisArchive, AnalysisPresets};

impl AnalysisArchive for Database {
    fn analysis_history(&self) -> AppResult<Vec<AnalysisHistoryItem>> {
        Database::analysis_history(self)
    }
    fn analysis(&self, id: &str) -> AppResult<StoredAnalysis> {
        Database::analysis(self, id)
    }
}

impl AnalysisPresets for Database {
    fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        Database::analysis_settings(self)
    }
    fn save_analysis_settings(&self, settings: AnalysisSettings) -> AppResult<AnalysisSettings> {
        Database::save_analysis_settings(self, settings)
    }
}
