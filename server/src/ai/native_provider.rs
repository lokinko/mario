use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use super::provider::{
    ChatMessage, ModelCompletion, ModelProvider, SearchCompletion, SearchSource,
};
use crate::error::{AppError, AppResult};

/// Both tools execute at the provider. This client never fetches search-result URLs.
pub struct NativeModelProvider {
    client: Client,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
}

impl NativeModelProvider {
    pub fn new(
        protocol: String,
        base_url: String,
        model: String,
        api_key: String,
    ) -> AppResult<Self> {
        let protocol = match protocol.as_str() {
            "openai-compatible" | "openai-responses" => "openai-responses",
            "anthropic" => "anthropic",
            _ => {
                return Err(AppError::Validation(
                    "请选择 OpenAI Responses 或 Anthropic Messages".into(),
                ))
            }
        };
        if api_key.trim().is_empty() || model.trim().is_empty() {
            return Err(AppError::Validation("请先配置模型名称和 API Key".into()));
        }
        Ok(Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            protocol: protocol.into(),
            base_url: base_url.trim_end_matches('/').into(),
            model,
            api_key,
        })
    }

    async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
    ) -> AppResult<SearchCompletion> {
        let anthropic = self.protocol == "anthropic";
        let mut body = if anthropic {
            let system = messages
                .iter()
                .filter(|m| m.role == "system")
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let turns = messages
                .into_iter()
                .filter(|m| m.role != "system")
                .collect::<Vec<_>>();
            json!({"model":self.model,"system":system,"messages":turns,"max_tokens":8192})
        } else {
            json!({"model":self.model,"input":messages,"store":false})
        };
        if search {
            body["tools"] = if anthropic {
                json!([{"type":"web_search_20250305","name":"web_search","max_uses":5}])
            } else {
                json!([{"type":"web_search"}])
            };
        }
        let mut result = SearchCompletion::default();
        for _ in 0..3 {
            result.api_calls += 1;
            let url = format!(
                "{}/{}",
                self.base_url,
                if anthropic { "messages" } else { "responses" }
            );
            let request = self.client.post(url).json(&body);
            let response = if anthropic {
                request
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
            } else {
                request.bearer_auth(&self.api_key)
            }
            .send()
            .await?;
            let status = response.status();
            if !status.is_success() {
                // Do not echo upstream bodies: proxies may include credentials or private prompts.
                return Err(AppError::Model(format!(
                    "{} 服务返回 HTTP {}，请检查协议、模型、密钥和网页搜索权限",
                    self.protocol, status
                )));
            }
            let parsed: Value = response.json().await?;
            if parsed.get("error").is_some_and(|v| !v.is_null()) {
                return Err(AppError::Model("模型服务返回错误，未完成本次请求".into()));
            }
            add_usage(
                &mut result.usage.input_tokens,
                parsed["usage"]["input_tokens"].as_u64(),
            );
            add_usage(
                &mut result.usage.output_tokens,
                parsed["usage"]["output_tokens"].as_u64(),
            );
            if anthropic {
                parse_anthropic(&parsed, &mut result);
                match parsed["stop_reason"].as_str() {
                    Some("end_turn" | "stop_sequence") => return finish(result, search),
                    Some("pause_turn") if search => {
                        let blocks = parsed["content"]
                            .as_array()
                            .ok_or_else(|| AppError::Model("搜索暂停响应缺少内容".into()))?;
                        // Preserve encrypted_content and encrypted_index byte-for-byte as JSON values.
                        body["messages"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!({"role":"assistant","content":blocks}));
                    }
                    _ => {
                        return Err(AppError::Model(
                            "模型输出未完成（可能达到长度上限），请缩小问题后重试".into(),
                        ))
                    }
                }
            } else {
                if parsed["status"] != "completed" {
                    return Err(AppError::Model(
                        "Responses 输出未完成，请重试或缩小问题".into(),
                    ));
                }
                parse_openai(&parsed, &mut result);
                return finish(result, search);
            }
        }
        Err(AppError::Model(
            "网页搜索多次暂停仍未完成，请缩小问题后重试".into(),
        ))
    }
}

fn add_usage(total: &mut Option<u64>, next: Option<u64>) {
    if let Some(next) = next {
        *total = Some(total.unwrap_or(0).saturating_add(next));
    }
}
fn finish(result: SearchCompletion, search: bool) -> AppResult<SearchCompletion> {
    if result.content.trim().is_empty() && !search {
        return Err(AppError::Model("模型没有返回有效内容".into()));
    }
    Ok(result)
}
fn push_source(
    result: &mut SearchCompletion,
    url: &str,
    title: &str,
    claim: String,
    original: bool,
) {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return;
    };
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || claim.trim().is_empty()
    {
        return;
    }
    if !result
        .sources
        .iter()
        .any(|s| s.url == url && s.claim == claim)
    {
        result.sources.push(SearchSource {
            url: url.into(),
            title: title.into(),
            claim,
            original,
        });
    }
}
fn parse_openai(value: &Value, result: &mut SearchCompletion) {
    for item in value["output"].as_array().into_iter().flatten() {
        if item["type"] == "web_search_call" {
            if item["status"] == "completed" {
                result.search_calls += 1;
            } else {
                result.warnings.push("部分网页搜索未完成".into());
            }
        }
        if item["type"] != "message" {
            continue;
        }
        for block in item["content"].as_array().into_iter().flatten() {
            if block["type"] != "output_text" {
                continue;
            }
            let text = block["text"].as_str().unwrap_or_default();
            result.content.push_str(text);
            result.content.push('\n');
            for citation in block["annotations"].as_array().into_iter().flatten() {
                if citation["type"] != "url_citation" {
                    continue;
                }
                // OpenAI supplies citation locations, not verbatim source excerpts.
                let end = citation["start_index"].as_u64().unwrap_or(0) as usize;
                let prefix = text.chars().take(end).collect::<String>();
                let paragraph = prefix.rsplit('\n').next().unwrap_or_default().trim();
                let claimed_span = text
                    .chars()
                    .skip(end)
                    .take(
                        citation["end_index"]
                            .as_u64()
                            .unwrap_or(end as u64)
                            .saturating_sub(end as u64) as usize,
                    )
                    .collect::<String>();
                let paragraph = if paragraph.is_empty() {
                    claimed_span.trim()
                } else {
                    paragraph
                };
                push_source(
                    result,
                    citation["url"].as_str().unwrap_or_default(),
                    citation["title"].as_str().unwrap_or_default(),
                    paragraph.to_string(),
                    false,
                );
            }
        }
    }
}
fn parse_anthropic(value: &Value, result: &mut SearchCompletion) {
    for block in value["content"].as_array().into_iter().flatten() {
        if block["type"] == "web_search_tool_result" {
            if block["content"]["type"] == "web_search_tool_result_error" {
                result.warnings.push(format!(
                    "网页搜索未完成：{}",
                    block["content"]["error_code"].as_str().unwrap_or("unknown")
                ));
            } else {
                result.search_calls += 1;
            }
        }
        if block["type"] != "text" {
            continue;
        }
        result
            .content
            .push_str(block["text"].as_str().unwrap_or_default());
        result.content.push('\n');
        for citation in block["citations"].as_array().into_iter().flatten() {
            if citation["type"] == "web_search_result_location" {
                push_source(
                    result,
                    citation["url"].as_str().unwrap_or_default(),
                    citation["title"].as_str().unwrap_or_default(),
                    citation["cited_text"].as_str().unwrap_or_default().into(),
                    true,
                );
            }
        }
    }
}

#[async_trait]
impl ModelProvider for NativeModelProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
        })
    }
    async fn search(&self, messages: Vec<ChatMessage>) -> AppResult<SearchCompletion> {
        self.generate(messages, true).await
    }
    fn provider_name(&self) -> &str {
        &self.protocol
    }
    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct MockState {
        replies: Mutex<VecDeque<Value>>,
        requests: Mutex<Vec<(HeaderMap, Value)>>,
    }
    async fn respond(
        State(state): State<Arc<MockState>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        state.requests.lock().unwrap().push((headers, body));
        Json(
            state
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra API call"),
        )
    }
    async fn mock(
        protocol: &str,
        replies: Vec<Value>,
    ) -> (
        NativeModelProvider,
        Arc<MockState>,
        tokio::task::JoinHandle<()>,
    ) {
        let state = Arc::new(MockState {
            replies: Mutex::new(replies.into()),
            ..Default::default()
        });
        let app = Router::new()
            .route(
                if protocol == "anthropic" {
                    "/v1/messages"
                } else {
                    "/v1/responses"
                },
                post(respond),
            )
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = NativeModelProvider::new(
            protocol.into(),
            format!("http://{address}/v1"),
            "test-model".into(),
            "test-key".into(),
        )
        .unwrap();
        (provider, state, server)
    }
    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage::system("policy"), ChatMessage::user("research")]
    }
    fn openai_reply() -> Value {
        json!({"status":"completed","output":[
            {"type":"web_search_call","status":"completed"},
            {"type":"message","content":[{"type":"output_text","text":"利率为2%。[来源]","annotations":[{"type":"url_citation","start_index":6,"end_index":10,"url":"https://example.com/rate","title":"Rate"}]}]}
        ],"usage":{"input_tokens":12,"output_tokens":4}})
    }
    #[tokio::test]
    async fn responses_uses_native_search_auth_and_annotations() {
        let (provider, state, server) =
            mock("openai-responses", vec![openai_reply(), openai_reply()]).await;
        let result = provider.search(messages()).await.unwrap();
        assert_eq!(result.sources.len(), 1);
        assert_eq!(result.sources[0].claim, "利率为2%。");
        assert!(!result.sources[0].original);
        assert_eq!(result.search_calls, 1);
        assert_eq!(result.usage.input_tokens, Some(12));
        provider.complete(messages()).await.unwrap();
        server.abort();
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests[0].0["authorization"], "Bearer test-key");
        assert!(!requests[0].0.contains_key("x-api-key"));
        assert_eq!(requests[0].1["tools"][0]["type"], "web_search");
        assert_eq!(requests[0].1["store"], false);
        assert_eq!(requests[0].1["input"][0]["role"], "system");
        assert!(requests[0].1.get("messages").is_none());
        assert!(requests[1].1.get("tools").is_none());
    }
    #[tokio::test]
    async fn anthropic_preserves_paused_blocks_and_native_cited_text() {
        let blocks = json!([
            {"type":"server_tool_use","id":"tool1","name":"web_search","input":{"query":"rate"}},
            {"type":"web_search_tool_result","tool_use_id":"tool1","content":[{"type":"web_search_result","url":"https://example.com","encrypted_content":"opaque-unaltered"}]}
        ]);
        let (provider, state, server) = mock("anthropic", vec![
            json!({"content":blocks,"stop_reason":"pause_turn","usage":{"input_tokens":10,"output_tokens":2}}),
            json!({"content":[{"type":"text","text":"利率为2%。","citations":[{"type":"web_search_result_location","url":"https://example.com","title":"官方公告","cited_text":"政策利率为2%。","encrypted_index":"opaque-index"}]}],"stop_reason":"end_turn","usage":{"input_tokens":20,"output_tokens":3}}),
        ]).await;
        let result = provider.search(messages()).await.unwrap();
        server.abort();
        assert_eq!(result.sources[0].claim, "政策利率为2%。");
        assert!(result.sources[0].original);
        assert_eq!(result.api_calls, 2);
        assert_eq!(result.usage.input_tokens, Some(30));
        assert_eq!(result.usage.output_tokens, Some(5));
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests[0].0["x-api-key"], "test-key");
        assert_eq!(requests[0].0["anthropic-version"], "2023-06-01");
        assert!(!requests[0].0.contains_key("authorization"));
        assert_eq!(requests[0].1["system"], "policy");
        assert_eq!(requests[0].1["messages"].as_array().unwrap().len(), 1);
        assert_eq!(requests[0].1["tools"][0]["type"], "web_search_20250305");
        assert_eq!(requests[1].1["messages"][1]["content"], blocks);
    }
    #[tokio::test]
    async fn tool_errors_remain_visible_even_with_http_success() {
        let (provider, _, server) = mock("anthropic", vec![json!({"stop_reason":"end_turn","content":[{"type":"web_search_tool_result","content":{"type":"web_search_tool_result_error","error_code":"unavailable"}},{"type":"text","text":"无法搜索"}]})]).await;
        let result = provider.search(messages()).await.unwrap();
        server.abort();
        assert!(result.sources.is_empty());
        assert!(result.warnings[0].contains("unavailable"));
        assert_eq!(result.search_calls, 0);
    }
    #[tokio::test]
    async fn rejects_incomplete_outputs_and_bounds_paused_turns() {
        let (provider, _, server) = mock(
            "openai-responses",
            vec![json!({"status":"incomplete","output":[]})],
        )
        .await;
        assert!(provider.complete(messages()).await.is_err());
        server.abort();
        let (provider, state, server) = mock(
            "anthropic",
            vec![json!({"stop_reason":"pause_turn","content":[]}); 3],
        )
        .await;
        assert!(provider.search(messages()).await.is_err());
        server.abort();
        assert_eq!(state.requests.lock().unwrap().len(), 3);
    }
    #[test]
    fn ignores_model_written_links_and_unsafe_native_urls() {
        let mut result = SearchCompletion::default();
        parse_openai(
            &json!({"output":[{"type":"message","content":[{"type":"output_text","text":"https://example.com"}]}]}),
            &mut result,
        );
        assert!(result.sources.is_empty());
        push_source(
            &mut result,
            "javascript:alert(1)",
            "unsafe",
            "quote".into(),
            true,
        );
        assert!(result.sources.is_empty());
    }
}
