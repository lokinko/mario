//! Execution port consumes frozen data and has no storage or HTTP dependency.
use crate::{
    context::BuiltContext,
    models::{
        analysis::{AnalysisRequest, AnalysisResult},
        connection::ModelConfig,
        memory::{DreamInput, DreamOutcome},
    },
    AppError, AppResult,
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
    ) -> AppResult<AnalysisResult>;
    async fn execute_stream(
        &self,
        config: ModelConfig,
        api_key: Option<String>,
        request: AnalysisRequest,
        context: BuiltContext,
        _events: EventSender,
    ) -> AppResult<AnalysisResult> {
        self.execute(config, api_key, request, context).await
    }
    /// Propose memory changes from frozen recent analyses; storage stays with the caller.
    async fn dream(
        &self,
        _config: ModelConfig,
        _api_key: Option<String>,
        _input: DreamInput,
    ) -> AppResult<DreamOutcome> {
        Err(AppError::Validation("当前执行器不支持记忆整理".into()))
    }
}

pub(crate) type EventSender = tokio::sync::mpsc::Sender<crate::models::inference::AnalysisEvent>;
