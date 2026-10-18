use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use super::provider::{
    ChatMessage, ModelCompletion, ModelProvider, SearchCompletion, SearchSource,
};
use crate::error::{AppError, AppResult};

/// Native search tools execute at the provider; Chat Completions supports generation only.
/// This client never fetches search-result URLs.
pub struct NativeModelProvider {
    client: Client,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
    events: Option<crate::ports::analysis::EventSender>,
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
            "openai-chat" => "openai-chat",
            "anthropic" => "anthropic",
            _ => {
                return Err(AppError::Validation(
                    "请选择 OpenAI Chat Completions、OpenAI Responses 或 Anthropic Messages".into(),
                ))
            }
        };
        if api_key.trim().is_empty() || model.trim().is_empty() {
            return Err(AppError::Validation("请先配置模型名称和 API Key".into()));
        }
        crate::models::validate_endpoint(&base_url)?;
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
            events: None,
        })
    }

    pub(crate) fn with_events(
        mut self,
        events: Option<crate::ports::analysis::EventSender>,
    ) -> Self {
        self.events = events;
        self
    }

    // Retry only explicit HTTP rejections before accepting a response. Network
    // failures and errors after streaming begins never replay the model call.
    async fn send_request(&self, body: &Value) -> AppResult<(reqwest::Response, usize)> {
        let url = format!(
            "{}/{}",
            self.base_url,
            match self.protocol.as_str() {
                "anthropic" => "messages",
                "openai-chat" => "chat/completions",
                _ => "responses",
            }
        );
        let started = tokio::time::Instant::now();
        for attempt in 0..3 {
            let request = self.client.post(&url).json(body);
            let request = if self.protocol == "anthropic" {
                request
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
            } else {
                request.bearer_auth(&self.api_key)
            };
            let response = if attempt == 0 {
                request.send().await?
            } else {
                tokio::time::timeout(
                    std::time::Duration::from_secs(45).saturating_sub(started.elapsed()),
                    request.send(),
                )
                .await
                .map_err(|_| {
                    AppError::Model("模型服务重试等待超时（45 秒），请稍后重试".into())
                })??
            };
            if response.status().is_success() {
                if attempt > 0 {
                    super::streaming::status(&self.events, "模型服务已恢复，正在继续处理…");
                }
                return Ok((response, attempt + 1));
            }
            let failure = UpstreamFailure::read(response).await;
            if !failure.retryable() || attempt == 2 {
                return Err(failure.error(&self.protocol, attempt + 1));
            }
            let delay = failure
                .retry_after
                .unwrap_or_else(|| std::time::Duration::from_secs(2_u64 << attempt))
                .saturating_add(std::time::Duration::from_millis(
                    rand::random::<u64>() % 251,
                ));
            // A long server delay is never shortened. Return a visible failure
            // instead of retrying earlier or exceeding the retry budget.
            if delay > std::time::Duration::from_secs(30)
                || started.elapsed().saturating_add(delay) > std::time::Duration::from_secs(45)
            {
                return Err(failure.error(&self.protocol, attempt + 1));
            }
            super::streaming::status(
                &self.events,
                format!(
                    "{}，等待约 {} 秒后自动重试（{}/2），可停止生成…",
                    if failure.status == 429 {
                        "模型服务暂时限流"
                    } else {
                        "模型服务暂时繁忙"
                    },
                    delay.as_secs().saturating_add(1),
                    attempt + 1
                ),
            );
            tokio::time::sleep(delay).await;
        }
        unreachable!("bounded retries always return")
    }

    async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
    ) -> AppResult<SearchCompletion> {
        tokio::time::timeout(
            std::time::Duration::from_secs(120),
            self.generate_inner(messages, search),
        )
        .await
        .map_err(|_| AppError::Model("模型请求超时（含限流等待），请稍后重试或缩小问题".into()))?
    }

    async fn generate_inner(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
    ) -> AppResult<SearchCompletion> {
        let anthropic = self.protocol == "anthropic";
        let chat = self.protocol == "openai-chat";
        if chat && search {
            return Err(AppError::Validation(
                "Chat Completions 接口不支持原生网页搜索，请关闭联网查找或使用支持搜索的 Responses / Messages 接口".into(),
            ));
        }
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
        } else if chat {
            json!({"model":self.model,"messages":messages,"stream":false})
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
            let (response, attempts) = self.send_request(&body).await?;
            result.api_calls += attempts;
            let parsed: Value = response.json().await?;
            if parsed.get("error").is_some_and(|v| !v.is_null()) {
                return Err(event_error(&parsed, &self.protocol, result.api_calls));
            }
            add_usage(
                &mut result.usage.input_tokens,
                parsed["usage"][if chat {
                    "prompt_tokens"
                } else {
                    "input_tokens"
                }]
                .as_u64(),
            );
            add_usage(
                &mut result.usage.output_tokens,
                parsed["usage"][if chat {
                    "completion_tokens"
                } else {
                    "output_tokens"
                }]
                .as_u64(),
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
            } else if chat {
                let choice = &parsed["choices"][0];
                if choice["finish_reason"] != "stop" {
                    return Err(AppError::Model(
                        "Chat Completions 输出未完成，请重试或缩小问题".into(),
                    ));
                }
                result.content = choice["message"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .into();
                return finish(result, false);
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

struct UpstreamFailure {
    status: u16,
    quota: bool,
    known_rate_limit: bool,
    retry_after: Option<std::time::Duration>,
}
impl UpstreamFailure {
    async fn read(mut response: reqwest::Response) -> Self {
        let mut failure = Self {
            status: response.status().as_u16(),
            quota: false,
            known_rate_limit: false,
            retry_after: retry_after(response.headers(), std::time::SystemTime::now()),
        };
        let mut bytes = Vec::new();
        // Inspect only bounded error metadata; never log or forward a provider's
        // message/body, which can contain credentials or the user's prompt.
        while let Ok(Some(chunk)) = response.chunk().await {
            if bytes.len() + chunk.len() > 64 * 1024 {
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            failure.observe(&value);
        }
        failure
    }
    fn observe(&mut self, value: &Value) {
        for code in [
            &value["error"]["code"],
            &value["error"]["type"],
            &value["code"],
            &value["type"],
        ]
        .into_iter()
        .filter_map(Value::as_str)
        {
            let code = code.to_ascii_lowercase();
            self.quota |= matches!(
                code.as_str(),
                "insufficient_quota"
                    | "credit_balance_exhausted"
                    | "organization_spend_limit_exceeded"
                    | "project_spend_limit_exceeded"
                    | "organization_usage_limit_exceeded"
                    | "billing_hard_limit_reached"
                    | "billing_not_active"
                    | "quota_exceeded"
                    | "insufficient_balance"
            );
            self.known_rate_limit |= matches!(
                code.as_str(),
                "rate_limit_exceeded" | "rate_limit_error" | "slow_down"
            );
        }
    }
    fn retryable(&self) -> bool {
        !self.quota && matches!(self.status, 429 | 503)
    }
    fn error(&self, protocol: &str, attempts: usize) -> AppError {
        let message = if self.quota {
            format!("{protocol} 模型服务的余额、额度或消费上限不足（HTTP {}）。请在模型服务商账户中检查余额、API 额度及项目/组织限额；自动重试无法恢复额度。", self.status)
        } else if self.status == 429 {
            let reason = if self.known_rate_limit {
                "模型服务请求或 token 频率受限"
            } else {
                "模型服务拒绝请求，可能是限流或额度限制"
            };
            let wait = self
                .retry_after
                .map(|delay| {
                    format!(
                        "服务要求至少等待 {} 秒。",
                        delay
                            .as_secs()
                            .saturating_add(u64::from(delay.subsec_nanos() > 0))
                    )
                })
                .unwrap_or_default();
            format!("{protocol} {reason}（HTTP 429，已请求 {attempts} 次）。{wait}请稍后再试；也可在问答预设中使用快速分析、减少联网查找，或检查服务商 API 额度与限额。")
        } else if self.status == 503 {
            format!("{protocol} 模型服务暂时繁忙（HTTP 503，已请求 {attempts} 次），请稍后重试。")
        } else {
            format!(
                "{protocol} 服务返回 HTTP {}，请检查协议、模型、密钥和网页搜索权限",
                self.status
            )
        };
        if self.status == 429 {
            AppError::RateLimit(message)
        } else {
            AppError::Model(message)
        }
    }
}
fn retry_after(
    headers: &reqwest::header::HeaderMap,
    now: std::time::SystemTime,
) -> Option<std::time::Duration> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<f64>() {
        if seconds.is_finite() && seconds >= 0.0 {
            return Some(
                std::time::Duration::try_from_secs_f64(seconds).unwrap_or(std::time::Duration::MAX),
            );
        }
        return None;
    }
    httpdate::parse_http_date(value)
        .ok()
        .map(|date| date.duration_since(now).unwrap_or_default())
}

fn event_error(value: &Value, protocol: &str, attempts: usize) -> AppError {
    let mut failure = UpstreamFailure {
        status: 502,
        quota: false,
        known_rate_limit: false,
        retry_after: None,
    };
    failure.observe(value);
    failure.observe(&value["response"]);
    if failure.quota || failure.known_rate_limit {
        failure.status = 429;
        failure.error(protocol, attempts)
    } else {
        AppError::Model("模型服务返回错误或未完成输出，请重试或缩小问题".into())
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
    async fn complete_stream(
        &self,
        messages: Vec<ChatMessage>,
        on_delta: super::streaming::DeltaCallback,
    ) -> AppResult<ModelCompletion> {
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let anthropic = self.protocol == "anthropic";
        let chat = self.protocol == "openai-chat";
        let body = if anthropic {
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
            json!({"model":self.model,"system":system,"messages":turns,"max_tokens":8192,"stream":true})
        } else if chat {
            json!({"model":self.model,"messages":messages,"stream":true,"stream_options":{"include_usage":true}})
        } else {
            json!({"model":self.model,"input":messages,"store":false,"stream":true})
        };
        let (mut response, request_count) = self.send_request(&body).await?;
        let mut decoder = super::streaming::SseDecoder::default();
        let mut content = String::new();
        let mut usage = super::provider::ModelUsage::default();
        let mut stopped = false;
        while let Some(bytes) = response.chunk().await? {
            for data in decoder.feed(&bytes)? {
                if data == "[DONE]" {
                    if chat && stopped && !content.trim().is_empty() {
                        return Ok(ModelCompletion { content, usage, request_count });
                    }
                    return Err(AppError::Model("模型流未正常完成".into()));
                }
                let event: Value = serde_json::from_str(&data)
                    .map_err(|_| AppError::Model("模型流式事件无效".into()))?;
                if event.get("error").is_some_and(|e| !e.is_null())
                    || matches!(
                        event["type"].as_str(),
                        Some("error" | "response.failed" | "response.incomplete")
                    )
                {
                    return Err(event_error(&event, &self.protocol, request_count));
                }
                let delta = if chat {
                    let choice = event["choices"]
                        .as_array()
                        .and_then(|choices| choices.iter().find(|c| c["index"] == 0));
                    if let Some(choice) = choice {
                        if let Some(reason) = choice["finish_reason"].as_str() {
                            if reason != "stop" {
                                return Err(AppError::Model(
                                    "模型流式输出未完成，请重试或缩小问题".into(),
                                ));
                            }
                            stopped = true;
                        }
                    }
                    usage.input_tokens = event["usage"]["prompt_tokens"]
                        .as_u64()
                        .or(usage.input_tokens);
                    usage.output_tokens = event["usage"]["completion_tokens"]
                        .as_u64()
                        .or(usage.output_tokens);
                    choice.and_then(|c| c["delta"]["content"].as_str())
                } else if anthropic {
                    match event["type"].as_str() {
                        Some("message_start") => {
                            usage.input_tokens = event["message"]["usage"]["input_tokens"].as_u64();
                            usage.output_tokens =
                                event["message"]["usage"]["output_tokens"].as_u64();
                            None
                        }
                        Some("content_block_start") if event["content_block"]["type"] == "text" => {
                            event["content_block"]["text"].as_str()
                        }
                        Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                            event["delta"]["text"].as_str()
                        }
                        Some("message_delta") => {
                            usage.input_tokens = event["usage"]["input_tokens"]
                                .as_u64()
                                .or(usage.input_tokens);
                            usage.output_tokens = event["usage"]["output_tokens"]
                                .as_u64()
                                .or(usage.output_tokens);
                            if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                                if !matches!(reason, "end_turn" | "stop_sequence") {
                                    return Err(AppError::Model(
                                        "模型流式输出未完成，请重试或缩小问题".into(),
                                    ));
                                }
                                stopped = true;
                            }
                            None
                        }
                        Some("message_stop") => {
                            if stopped && !content.trim().is_empty() {
                                return Ok(ModelCompletion { content, usage, request_count });
                            }
                            return Err(AppError::Model("模型流未正常完成".into()));
                        }
                        _ => None,
                    }
                } else {
                    match event["type"].as_str() {
                        Some("response.output_text.delta") => event["delta"].as_str(),
                        Some("response.completed") => {
                            if event["response"]["status"] != "completed" {
                                return Err(AppError::Model("模型流未正常完成".into()));
                            }
                            let mut result = SearchCompletion::default();
                            parse_openai(&event["response"], &mut result);
                            if !result.content.is_empty() {
                                content = result.content;
                            }
                            usage.input_tokens =
                                event["response"]["usage"]["input_tokens"].as_u64();
                            usage.output_tokens =
                                event["response"]["usage"]["output_tokens"].as_u64();
                            if content.trim().is_empty() {
                                return Err(AppError::Model("模型没有返回有效内容".into()));
                            }
                            return Ok(ModelCompletion { content, usage, request_count });
                        }
                        _ => None,
                    }
                };
                if let Some(delta) = delta {
                    if content.len() + delta.len() > 128_000 {
                        return Err(AppError::Model("模型输出过大".into()));
                    }
                    content.push_str(delta);
                    on_delta(delta);
                }
            }
        }
        Err(AppError::Model("模型流连接中断，回答尚未完成".into()))
        }).await.map_err(|_| AppError::Model("模型流式请求超时（含限流等待），请稍后重试或缩小问题".into()))?
    }
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion> {
        let result = self.generate(messages, false).await?;
        Ok(ModelCompletion {
            content: result.content,
            usage: result.usage,
            request_count: result.api_calls.max(1),
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

    type HttpReply = (u16, Option<String>, String, &'static str);
    async fn rejection_fixture(
        protocol: &str,
        replies: Vec<HttpReply>,
    ) -> (
        NativeModelProvider,
        Arc<Mutex<Vec<Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let replies = Arc::new(Mutex::new(VecDeque::from(replies)));
        let app = Router::new().route(
            "/v1/{*path}",
            post(move |Json(body): Json<Value>| {
                let captured = captured.clone();
                let replies = replies.clone();
                async move {
                    captured.lock().unwrap().push(body);
                    let (status, retry, body, content_type) = replies
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected retry");
                    let mut builder = axum::response::Response::builder()
                        .status(status)
                        .header("content-type", content_type);
                    if let Some(retry) = retry {
                        builder = builder.header("retry-after", retry);
                    }
                    builder.body(axum::body::Body::from(body)).unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = NativeModelProvider::new(
            protocol.into(),
            format!("http://{address}/v1"),
            "gpt-6.1-sol".into(),
            "fixture-key".into(),
        )
        .unwrap();
        (provider, requests, server)
    }
    fn rejection(status: u16, code: &str, after: &str) -> HttpReply {
        (
            status,
            Some(after.into()),
            json!({"error":{"code":code,"message":"sk-private-secret original private prompt"}})
                .to_string(),
            "application/json",
        )
    }
    fn successful_reply() -> HttpReply {
        (200, None, openai_reply().to_string(), "application/json")
    }

    #[tokio::test]
    async fn temporary_limit_recovers_for_search_and_tracks_every_attempt() {
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![
                rejection(429, "rate_limit_exceeded", "0"),
                rejection(503, "server_is_overloaded", "0"),
                successful_reply(),
            ],
        )
        .await;
        let result = provider.search(messages()).await.unwrap();
        assert_eq!(result.api_calls, 3);
        assert!(!result.sources.is_empty());
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests.windows(2).all(|r| r[0] == r[1]));
        assert!(requests[0]["tools"].is_array());
        server.abort();
    }

    #[tokio::test]
    async fn retries_before_streaming_without_repeating_deltas_and_counts_attempts() {
        let text = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({"type":"response.output_text.delta","delta":"中文正文"}),
            json!({"type":"response.completed","response":{"status":"completed"}})
        );
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![
                rejection(429, "slow_down", "0"),
                (200, None, text, "text/event-stream"),
            ],
        )
        .await;
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let captured = deltas.clone();
        let result = provider
            .complete_stream(
                messages(),
                Box::new(move |delta| {
                    captured.lock().unwrap().push(delta.to_string());
                }),
            )
            .await
            .unwrap();
        assert_eq!(result.content, "中文正文");
        assert_eq!(result.request_count, 2);
        assert_eq!(*deltas.lock().unwrap(), vec!["中文正文"]);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0], requests[1]);
        assert_eq!(requests[0]["stream"], true);
        server.abort();
    }

    #[tokio::test]
    async fn quota_billing_and_auth_failures_are_not_retried_or_echoed() {
        for (status, code) in [
            (429, "insufficient_quota"),
            (429, "credit_balance_exhausted"),
            (429, "project_spend_limit_exceeded"),
            (429, "organization_usage_limit_exceeded"),
            (401, "invalid_api_key"),
        ] {
            let (provider, requests, server) =
                rejection_fixture("openai-responses", vec![rejection(status, code, "0")]).await;
            let error = provider.complete(messages()).await.unwrap_err().to_string();
            assert!(!error.contains("sk-private-secret"));
            assert!(!error.contains("original private prompt"));
            if status == 429 {
                assert!(error.contains("余额、额度或消费上限"));
            }
            assert_eq!(requests.lock().unwrap().len(), 1);
            server.abort();
        }
    }

    #[tokio::test]
    async fn retries_are_bounded_and_long_server_delay_is_not_shortened() {
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![
                rejection(429, "rate_limit_exceeded", "0"),
                rejection(429, "rate_limit_exceeded", "0"),
                rejection(429, "rate_limit_exceeded", "0"),
            ],
        )
        .await;
        let error = provider.complete(messages()).await.unwrap_err().to_string();
        assert!(error.contains("已请求 3 次"));
        assert_eq!(requests.lock().unwrap().len(), 3);
        server.abort();
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![rejection(429, "rate_limit_exceeded", "120")],
        )
        .await;
        let error = provider.complete(messages()).await.unwrap_err().to_string();
        assert!(error.contains("至少等待 120 秒"));
        assert_eq!(requests.lock().unwrap().len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn dropping_execution_cancels_backoff_before_another_request() {
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![rejection(429, "rate_limit_exceeded", "5")],
        )
        .await;
        let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
        let provider = provider.with_events(Some(sender));
        let pending =
            tokio::spawn(
                async move { provider.complete_stream(messages(), Box::new(|_| {})).await },
            );
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(event, crate::models::inference::AnalysisEvent::Status { message } if message.contains("自动重试"))
        );
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert_eq!(requests.lock().unwrap().len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn quota_or_limit_events_after_output_never_replay_generation() {
        let body = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({"type":"response.output_text.delta","delta":"已收到的草稿"}),
            json!({"type":"error","error":{"code":"insufficient_quota","message":"sk-private-secret"}})
        );
        let (provider, requests, server) = rejection_fixture(
            "openai-responses",
            vec![(200, None, body, "text/event-stream")],
        )
        .await;
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let captured = deltas.clone();
        let error = provider
            .complete_stream(
                messages(),
                Box::new(move |delta| {
                    captured.lock().unwrap().push(delta.to_string());
                }),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("余额、额度或消费上限"));
        assert!(!error.contains("sk-private-secret"));
        assert_eq!(*deltas.lock().unwrap(), vec!["已收到的草稿"]);
        assert_eq!(requests.lock().unwrap().len(), 1);
        server.abort();
    }

    #[test]
    fn parses_retry_after_seconds_http_dates_and_invalid_values() {
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let mut headers = reqwest::header::HeaderMap::new();
        for (input, expected) in [
            ("0".to_string(), Some(std::time::Duration::ZERO)),
            ("1.5".into(), Some(std::time::Duration::from_millis(1500))),
            (
                httpdate::fmt_http_date(now + std::time::Duration::from_secs(10)),
                Some(std::time::Duration::from_secs(10)),
            ),
            ("invalid".into(), None),
            ("NaN".into(), None),
            ("-1".into(), None),
            ("1e100".into(), Some(std::time::Duration::MAX)),
        ] {
            headers.insert("retry-after", input.parse().unwrap());
            assert_eq!(retry_after(&headers, now), expected);
        }
        let mut failure = UpstreamFailure {
            status: 429,
            quota: false,
            known_rate_limit: false,
            retry_after: None,
        };
        failure
            .observe(&json!({"error":{"type":"insufficient_quota","code":"unknown_gateway_code"}}));
        assert!(!failure.retryable());
        assert!(failure
            .error("openai-responses", 1)
            .to_string()
            .contains("额度"));
    }

    async fn stream_fixture(
        protocol: &str,
        first: String,
        last: String,
    ) -> (
        NativeModelProvider,
        Arc<tokio::sync::Notify>,
        tokio::task::JoinHandle<()>,
    ) {
        let release = Arc::new(tokio::sync::Notify::new());
        let gate = release.clone();
        let app = Router::new().route("/v1/{*path}", post(move |Json(body): Json<Value>| {
            let gate = gate.clone(); let first = first.clone(); let last = last.clone();
            async move {
                assert_eq!(body["stream"], true);
                let stream = async_stream::stream! {
                    for byte in first.bytes() { yield Ok::<_, std::convert::Infallible>(bytes::Bytes::from(vec![byte])); }
                    gate.notified().await;
                    yield Ok(bytes::Bytes::from(last));
                };
                axum::response::Response::builder().header("content-type", "text/event-stream").body(axum::body::Body::from_stream(stream)).unwrap()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = NativeModelProvider::new(
            protocol.into(),
            format!("http://{address}/v1"),
            "fixture".into(),
            "fixture-key".into(),
        )
        .unwrap();
        (provider, release, server)
    }

    #[tokio::test]
    async fn streams_all_native_protocols_before_terminal_event_and_keeps_usage() {
        for protocol in ["openai-chat", "openai-responses", "anthropic"] {
            let (first, last) = match protocol {
                "openai-chat" => (
                    json!({"choices":[{"index":0,"delta":{"content":"中文"},"finish_reason":null}]}).to_string(),
                    format!("data: {}\n\ndata: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}), json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":2}})),
                ),
                "openai-responses" => (
                    json!({"type":"response.output_text.delta","delta":"中文"}).to_string(),
                    format!("data: {}\n\n", json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":10,"output_tokens":2}}})),
                ),
                _ => (
                    json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"中文"}}).to_string(),
                    format!("data: {}\n\ndata: {}\n\n", json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":10,"output_tokens":2}}), json!({"type":"message_stop"})),
                ),
            };
            let (provider, release, server) = stream_fixture(
                protocol,
                format!(": ping\r\n\r\ndata: {first}\r\n\r\n"),
                last,
            )
            .await;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let pending = tokio::spawn(async move {
                provider
                    .complete_stream(
                        vec![ChatMessage::user("问题")],
                        Box::new(move |delta| {
                            tx.send(delta.to_string()).unwrap();
                        }),
                    )
                    .await
            });
            assert_eq!(
                tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                "中文"
            );
            assert!(!pending.is_finished());
            release.notify_one();
            let result = pending.await.unwrap().unwrap();
            assert_eq!(result.content, "中文");
            assert_eq!(result.usage.input_tokens, Some(10));
            assert_eq!(result.usage.output_tokens, Some(2));
            server.abort();
        }
    }

    #[tokio::test]
    async fn rejects_stream_disconnect_truncation_and_in_band_errors() {
        for (protocol, last) in [
            ("openai-chat", String::new()),
            ("openai-chat", format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"index":0,"delta":{},"finish_reason":"length"}]}))),
            ("openai-responses", "data: {\"type\":\"response.incomplete\"}\n\n".into()),
            ("anthropic", "data: {\"type\":\"error\",\"error\":{\"message\":\"private provider failure\"}}\n\n".into()),
        ] {
            let (provider, release, server) = stream_fixture(protocol, ": ping\n\n".into(), last).await;
            release.notify_one();
            let error = provider.complete_stream(vec![ChatMessage::user("test")], Box::new(|_| {})).await.unwrap_err();
            assert!(!error.to_string().contains("private provider failure"));
            server.abort();
        }
    }

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
                } else if protocol == "openai-chat" {
                    "/custom/v1/chat/completions"
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
            format!(
                "http://{address}{}/",
                if protocol == "openai-chat" {
                    "/custom/v1"
                } else {
                    "/v1"
                }
            ),
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
    async fn chat_completions_uses_custom_endpoint_messages_auth_and_usage() {
        let (provider, state, server) = mock("openai-chat", vec![json!({
            "choices":[{"message":{"role":"assistant","content":"第三方回复"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":15,"completion_tokens":6}
        })]).await;
        let result = provider.complete(messages()).await.unwrap();
        assert_eq!(result.content, "第三方回复");
        assert_eq!(result.usage.input_tokens, Some(15));
        assert_eq!(result.usage.output_tokens, Some(6));
        assert_eq!(provider.provider_name(), "openai-chat");
        assert!(provider
            .search(messages())
            .await
            .unwrap_err()
            .to_string()
            .contains("不支持原生网页搜索"));
        server.abort();
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0["authorization"], "Bearer test-key");
        assert_eq!(requests[0].1["model"], "test-model");
        assert_eq!(requests[0].1["messages"][0]["role"], "system");
        assert_eq!(requests[0].1["messages"][1]["content"], "research");
        assert_eq!(requests[0].1["stream"], false);
        assert!(requests[0].1.get("input").is_none());
        assert!(requests[0].1.get("tools").is_none());
    }

    #[tokio::test]
    async fn chat_completions_rejects_truncated_empty_and_error_outputs() {
        for reply in [
            json!({"choices":[{"message":{"content":"partial"},"finish_reason":"length"}]}),
            json!({"choices":[{"message":{"content":""},"finish_reason":"stop"}]}),
            json!({"error":{"message":"secret upstream details"}}),
        ] {
            let (provider, _, server) = mock("openai-chat", vec![reply]).await;
            let error = provider.complete(messages()).await.unwrap_err().to_string();
            assert!(!error.contains("secret upstream details"));
            server.abort();
        }
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
