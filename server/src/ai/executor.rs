//! Execution port consumes frozen data and has no storage or HTTP dependency.
use crate::{
    context::BuiltContext,
    models::{AnalysisRequest, AnalysisResult, MemoryItem, ModelConfig},
    AppResult,
};
use async_trait::async_trait;

#[async_trait]
pub(crate) trait AnalysisExecutor: Send + Sync {
    async fn execute(
        &self,
        config: ModelConfig,
        api_key: Option<String>,
        request: AnalysisRequest,
        context: BuiltContext,
        memories: Vec<MemoryItem>,
    ) -> AppResult<AnalysisResult>;
}
