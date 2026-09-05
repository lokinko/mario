use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{ChatMessage, ModelCompletion, ModelProvider, ModelUsage};
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
    #[serde(default)]
    usage: Option<CompletionUsage>,
}

#[derive(Deserialize)]
struct CompletionUsage {
    #[serde(alias = "input_tokens")]
    prompt_tokens: Option<u64>,
    #[serde(alias = "output_tokens")]
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion> {
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
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| AppError::Model("模型没有返回有效内容".into()))?;
        let usage = parsed
            .usage
            .map_or_else(ModelUsage::default, |usage| ModelUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            });
        Ok(ModelCompletion { content, usage })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn provider_name(&self) -> &str {
        "openai-compatible"
    }
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_standard_and_responses_style_usage_names() {
        let standard: CompletionResponse = serde_json::from_str(
            r#"{"choices":[{"message":{"role":"assistant","content":"OK"}}],"usage":{"prompt_tokens":12,"completion_tokens":3}}"#,
        )
        .unwrap();
        assert_eq!(standard.usage.as_ref().unwrap().prompt_tokens, Some(12));
        assert_eq!(standard.usage.as_ref().unwrap().completion_tokens, Some(3));

        let aliases: CompletionResponse = serde_json::from_str(
            r#"{"choices":[{"message":{"role":"assistant","content":"OK"}}],"usage":{"input_tokens":8,"output_tokens":2}}"#,
        )
        .unwrap();
        assert_eq!(aliases.usage.as_ref().unwrap().prompt_tokens, Some(8));
        assert_eq!(aliases.usage.as_ref().unwrap().completion_tokens, Some(2));
    }
}
