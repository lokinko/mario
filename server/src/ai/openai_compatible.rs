use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{ChatMessage, ModelProvider};
use crate::error::{AppError, AppResult};

pub struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, model: String, api_key: String) -> AppResult<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::Validation("请先配置模型 API Key".into()));
        }
        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').into(),
            model,
            api_key,
        })
    }
}

#[derive(Serialize)]
struct CompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<String> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&CompletionRequest {
                model: self.model.clone(),
                messages,
                temperature: 0.2,
            })
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(AppError::Model(format!(
                "模型服务返回 {}：{}",
                status,
                truncate(&body, 500)
            )));
        }
        let parsed: CompletionResponse = serde_json::from_str(&body)?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| AppError::Model("模型没有返回有效内容".into()))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
