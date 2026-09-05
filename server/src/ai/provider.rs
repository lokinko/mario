use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ModelCompletion {
    pub content: String,
    pub usage: ModelUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion>;
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
}
