//! Local ChatGPT-plan provider. OAuth credentials never cross the Mario bridge.
//! Only the two documented public API endpoints below receive the bearer token.
use super::provider::{ChatMessage, SearchCompletion, SearchSource};
use crate::{
    error::{AppError, AppResult},
    models::{inference::CodexFailure, inference::MODEL_CALL_TIMEOUT, CodexModel},
};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::time::Duration;

const MODELS_URL: &str = "https://api.openai.com/v1/models";
const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
const MAX_EVENT: usize = 4 * 1024 * 1024;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_WIRE: usize = 32 * 1024 * 1024;
const MAX_EVENTS: usize = 20_000;
const MAX_SOURCES: usize = 100;

pub struct ChatGptProvider {
    model: String,
    // An opaque local registration ID, pinned when the worker is paired. Never
    // consult the mutable active account again while serving that worker.
    account: String,
    session: String,
}

impl ChatGptProvider {
    pub fn new(model: String, account: String, session: String) -> Self {
        Self {
            model,
            account,
            session,
        }
    }

    pub async fn models(account: &str, session: &str) -> AppResult<Vec<CodexModel>> {
        tokio::time::timeout(MODEL_CALL_TIMEOUT, async {
            let token = crate::chatgpt_auth::access_token(account, session).await?;
            let response = client()?
                .get(MODELS_URL)
                .bearer_auth(token)
                .send()
                .await
                .map_err(network_error)?;
            let status = response.status().as_u16();
            if !response.status().is_success() {
                return Err(http_error(response, status).await);
            }
            let bytes = read_bounded(response, MAX_EVENT).await?;
            let value = serde_json::from_slice(&bytes).map_err(|_| invalid_response())?;
            parse_models(&value)
        })
        .await
        .map_err(|_| failure(CodexFailure::Timeout))?
    }

    pub async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
        on_delta: Option<super::streaming::DeltaCallback>,
    ) -> AppResult<SearchCompletion> {
        tokio::time::timeout(MODEL_CALL_TIMEOUT, async {
            let body = request_body(&self.model, messages, search)?;
            let token = crate::chatgpt_auth::access_token(&self.account, &self.session).await?;
            // There is deliberately no retry loop, even before the first delta.
            // Dropping this future drops both the HTTP response and its parser.
            let mut response = client()?
                .post(RESPONSES_URL)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(&body)
                .send()
                .await
                .map_err(network_error)?;
            let status = response.status().as_u16();
            if !response.status().is_success() {
                return Err(http_error(response, status).await);
            }
            let mut decoder = EventDecoder::default();
            let mut turn = Turn::new(search);
            while let Some(bytes) = response.chunk().await.map_err(network_error)? {
                for data in decoder.feed(&bytes)? {
                    if let Some(result) = turn.event(&data, on_delta.as_deref())? {
                        return Ok(result);
                    }
                }
            }
            // Neither EOF nor [DONE] is a successful Responses terminal event.
            Err(failure(CodexFailure::Disconnected))
        })
        .await
        .map_err(|_| failure(CodexFailure::Timeout))?
    }
}

fn client() -> AppResult<Client> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(10))
        .timeout(MODEL_CALL_TIMEOUT)
        .build()
        .map_err(|_| failure(CodexFailure::Unavailable))
}

fn request_body(model: &str, messages: Vec<ChatMessage>, search: bool) -> AppResult<Value> {
    if model.trim().is_empty() || model.len() > 200 {
        return Err(AppError::Validation("请选择有效的 ChatGPT 模型".into()));
    }
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for message in messages {
        match message.role.as_str() {
            "system" => instructions.push(message.content),
            "user" | "developer" => {
                input.push(json!({"role":message.role,"content":message.content}));
            }
            _ => {
                return Err(AppError::Validation(
                    "ChatGPT 请求包含不支持的消息角色".into(),
                ))
            }
        }
    }
    if input.is_empty() {
        return Err(AppError::Validation("ChatGPT 请求缺少问答输入".into()));
    }
    let mut body = json!({
        "model":model,"input":input,"store":false,"stream":true,
        "instructions":instructions.join("\n\n")
    });
    if search {
        body["tools"] = json!([{"type":"web_search"}]);
    }
    Ok(body)
}

fn parse_models(value: &Value) -> AppResult<Vec<CodexModel>> {
    let entries = value["models"].as_array().ok_or_else(invalid_response)?;
    let mut models = Vec::<CodexModel>::new();
    for entry in entries.iter().filter(|entry| entry["visibility"] == "list") {
        let model = entry["slug"].as_str().ok_or_else(invalid_response)?;
        let display_name = entry["display_name"]
            .as_str()
            .ok_or_else(invalid_response)?;
        if model.trim().is_empty()
            || model.len() > 200
            || display_name.trim().is_empty()
            || display_name.len() > 800
        {
            return Err(invalid_response());
        }
        if models.iter().any(|entry| entry.model == model) {
            continue;
        }
        models.push(CodexModel {
            model: model.into(),
            display_name: display_name.into(),
            description: String::new(),
            is_default: models.is_empty(),
        });
        if models.len() > 100 {
            return Err(invalid_response());
        }
    }
    if models.is_empty() {
        return Err(AppError::Model(
            "当前 ChatGPT 授权没有可用的模型，请检查订阅与工作区权限".into(),
        ));
    }
    Ok(models)
}

async fn read_bounded(mut response: Response, limit: usize) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(invalid_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn http_error(response: Response, status: u16) -> AppError {
    let value = read_bounded(response, 64 * 1024)
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    failure(classify_failure(
        value.as_ref().unwrap_or(&Value::Null),
        Some(status),
    ))
}

fn network_error(error: reqwest::Error) -> AppError {
    failure(if error.is_timeout() {
        CodexFailure::Timeout
    } else if error.is_connect() {
        CodexFailure::Unavailable
    } else {
        CodexFailure::Disconnected
    })
}

// Inspect only documented machine-readable fields, never match, retain or echo
// upstream message/detail text (which may contain prompts or credentials).
fn classify_failure(value: &Value, status: Option<u16>) -> CodexFailure {
    for code in [
        &value["response"]["error"]["code"],
        &value["error"]["code"],
        &value["code"],
        &value["response"]["error"]["type"],
        &value["error"]["type"],
    ]
    .into_iter()
    .filter_map(Value::as_str)
    {
        match code {
            "subscription_sharing_usage_limit_exceeded" | "insufficient_quota" => {
                return CodexFailure::UsageLimit;
            }
            "subscription_sharing_invalid_user"
            | "invalid_api_key"
            | "invalid_token"
            | "token_expired"
            | "authentication_error" => return CodexFailure::Unauthorized,
            "subscription_sharing_usage_unavailable"
            | "subscription_sharing_user_unavailable"
            | "server_error" => return CodexFailure::Unavailable,
            "rate_limit_exceeded" | "rate_limit_error" | "server_overloaded" => {
                return CodexFailure::Busy;
            }
            "context_length_exceeded" | "context_window_exceeded" => {
                return CodexFailure::ContextLimit;
            }
            "subscription_sharing_user_not_eligible"
            | "subscription_sharing_unsupported_capability"
            | "subscription_sharing_route_not_supported"
            | "chatpass_v2_scope_not_authorized"
            | "chatpass_v2_invalid_authorization_context"
            | "invalid_request_error"
            | "model_not_found"
            | "permission_denied"
            | "content_policy_violation" => return CodexFailure::Rejected,
            _ => {}
        }
    }
    match status {
        Some(401) => CodexFailure::Unauthorized,
        Some(429) => CodexFailure::Busy,
        Some(408 | 504) => CodexFailure::Timeout,
        Some(500..=599) => CodexFailure::Unavailable,
        Some(300..=499) => CodexFailure::Rejected,
        _ => CodexFailure::Other,
    }
}

fn failure(reason: CodexFailure) -> AppError {
    // Preserve the shared safe failure category through Event::Failure instead
    // of collapsing quota, authorization and transport failures to Event::Error.
    AppError::Codex(reason)
}

fn invalid_response() -> AppError {
    AppError::Model("ChatGPT 返回了无效或超出大小限制的数据，已停止本次调用".into())
}

/// Decode bytes only after complete SSE lines have arrived, so a split UTF-8
/// character never gets replaced or decoded prematurely. Every accumulator is
/// bounded, including comment-only streams and multiline data events.
#[derive(Default)]
struct EventDecoder {
    line: Vec<u8>,
    data: String,
    has_data: bool,
    skip_lf: bool,
    first_line: bool,
    wire_bytes: usize,
    events: usize,
}

impl EventDecoder {
    fn feed(&mut self, bytes: &[u8]) -> AppResult<Vec<String>> {
        self.wire_bytes = self.wire_bytes.saturating_add(bytes.len());
        if self.wire_bytes > MAX_WIRE {
            return Err(invalid_response());
        }
        let mut events = Vec::new();
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if matches!(byte, b'\n' | b'\r') {
                self.skip_lf = byte == b'\r';
                let line = std::mem::take(&mut self.line);
                self.line(&line, &mut events)?;
            } else {
                if self.line.len() >= MAX_EVENT {
                    return Err(invalid_response());
                }
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    fn line(&mut self, bytes: &[u8], events: &mut Vec<String>) -> AppResult<()> {
        let mut line = std::str::from_utf8(bytes).map_err(|_| invalid_response())?;
        if !self.first_line {
            self.first_line = true;
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
        }
        if line.is_empty() {
            if self.has_data {
                self.events += 1;
                if self.events > MAX_EVENTS {
                    return Err(invalid_response());
                }
                events.push(std::mem::take(&mut self.data));
                self.has_data = false;
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            let data = data.strip_prefix(' ').unwrap_or(data);
            let extra = usize::from(self.has_data);
            if data.len().saturating_add(extra) > MAX_EVENT.saturating_sub(self.data.len()) {
                return Err(invalid_response());
            }
            if self.has_data {
                self.data.push('\n');
            }
            self.data.push_str(data);
            self.has_data = true;
        }
        Ok(())
    }
}

struct Turn {
    search: bool,
    draft_bytes: usize,
    finished: bool,
}

impl Turn {
    fn new(search: bool) -> Self {
        Self {
            search,
            draft_bytes: 0,
            finished: false,
        }
    }

    fn event(
        &mut self,
        data: &str,
        on_delta: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> AppResult<Option<SearchCompletion>> {
        if self.finished || data == "[DONE]" {
            return Err(failure(CodexFailure::Disconnected));
        }
        let event: Value = serde_json::from_str(data).map_err(|_| invalid_response())?;
        if event.get("error").is_some_and(|error| !error.is_null())
            || matches!(event["type"].as_str(), Some("error" | "response.failed"))
        {
            return Err(failure(classify_failure(&event, None)));
        }
        let kind = event["type"].as_str().ok_or_else(invalid_response)?;
        match kind {
            "response.incomplete" => return Err(failure(CodexFailure::Other)),
            "response.output_text.delta" => {
                let delta = event["delta"].as_str().ok_or_else(invalid_response)?;
                self.draft_bytes = self.draft_bytes.saturating_add(delta.len());
                if self.draft_bytes > MAX_OUTPUT {
                    return Err(invalid_response());
                }
                if let Some(callback) = on_delta {
                    callback(delta);
                }
            }
            "response.refusal.delta" | "response.refusal.done" => {
                return Err(failure(CodexFailure::Rejected));
            }
            "response.output_item.added" | "response.output_item.done" => {
                validate_item(&event["item"], self.search)?;
            }
            "response.completed" => {
                self.finished = true;
                return parse_completed(&event["response"], self.search).map(Some);
            }
            _ if kind.starts_with("response.web_search_call.") && !self.search => {
                return Err(failure(CodexFailure::Rejected));
            }
            _ => {}
        }
        Ok(None)
    }
}

fn validate_item(item: &Value, search: bool) -> AppResult<()> {
    match item["type"].as_str() {
        Some("message" | "reasoning") => Ok(()),
        Some("web_search_call") if search => Ok(()),
        // No client tool execution path exists. Reject unknown output items too,
        // rather than accidentally accepting a newly introduced tool type.
        _ => Err(failure(CodexFailure::Rejected)),
    }
}

fn parse_completed(response: &Value, search: bool) -> AppResult<SearchCompletion> {
    if response["status"] != "completed"
        || response.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(failure(classify_failure(
            &json!({"response":response}),
            None,
        )));
    }
    let output = response["output"].as_array().ok_or_else(invalid_response)?;
    let mut result = SearchCompletion {
        api_calls: 1,
        ..Default::default()
    };
    let mut citations = 0;
    for item in output {
        validate_item(item, search)?;
        match item["type"].as_str() {
            Some("web_search_call") => {
                if item["status"] == "completed" {
                    result.search_calls += 1;
                } else if result.warnings.is_empty() {
                    result.warnings.push("部分网页搜索未完成".into());
                }
            }
            Some("message") => {
                if item["role"] != "assistant" || item["status"] != "completed" {
                    return Err(invalid_response());
                }
                for block in item["content"].as_array().ok_or_else(invalid_response)? {
                    if block["type"] == "refusal" {
                        return Err(failure(CodexFailure::Rejected));
                    }
                    if block["type"] != "output_text" {
                        return Err(invalid_response());
                    }
                    let text = block["text"].as_str().ok_or_else(invalid_response)?;
                    if text.len() > MAX_OUTPUT.saturating_sub(result.content.len()) {
                        return Err(invalid_response());
                    }
                    result.content.push_str(text);
                    for annotation in block["annotations"].as_array().into_iter().flatten() {
                        if annotation["type"] != "url_citation" {
                            continue;
                        }
                        if !search {
                            return Err(failure(CodexFailure::Rejected));
                        }
                        citations += 1;
                        if citations > MAX_SOURCES {
                            return Err(invalid_response());
                        }
                        citation(annotation, text, &mut result);
                    }
                }
            }
            _ => {}
        }
    }
    if result.content.trim().is_empty() {
        return Err(invalid_response());
    }
    result.usage.input_tokens = response["usage"]["input_tokens"].as_u64();
    result.usage.output_tokens = response["usage"]["output_tokens"].as_u64();
    Ok(result)
}

fn citation(annotation: &Value, text: &str, result: &mut SearchCompletion) {
    let url = annotation["url"].as_str().unwrap_or_default();
    if url.len() > 8192
        || !reqwest::Url::parse(url).is_ok_and(|url| {
            matches!(url.scheme(), "https" | "http")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
    {
        return;
    }
    let start = annotation["start_index"]
        .as_u64()
        .unwrap_or(0)
        .min(MAX_OUTPUT as u64) as usize;
    let end = annotation["end_index"]
        .as_u64()
        .unwrap_or(start as u64)
        .min(MAX_OUTPUT as u64) as usize;
    let prefix = text
        .chars()
        .skip(start.saturating_sub(2000))
        .take(start.min(2000))
        .collect::<String>();
    let mut claim = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if claim.is_empty() {
        claim = text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start).min(2000))
            .collect::<String>()
            .trim()
            .to_owned();
    }
    if claim.is_empty()
        || result
            .sources
            .iter()
            .any(|source| source.url == url && source.claim == claim)
    {
        return;
    }
    result.sources.push(SearchSource {
        url: url.into(),
        title: annotation["title"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(400)
            .collect(),
        claim,
        // URL annotations are attribution, not guaranteed verbatim excerpts.
        original: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn completed(text: &str) -> Value {
        json!({"type":"response.completed","response":{
            "status":"completed","output":[{"type":"message","role":"assistant",
                "status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]}],
            "usage":{"input_tokens":12,"output_tokens":5}
        }})
    }

    #[test]
    fn chatgpt_sse_fragmented_utf8_comments_crlf_and_multiline() {
        let event = completed("中文🙂");
        let wire = format!("\u{feff}: heartbeat\r\n\r\ndata: {}\r\n\r\n", event);
        let mut decoder = EventDecoder::default();
        let mut turn = Turn::new(false);
        let mut result = None;
        for byte in wire.as_bytes() {
            for event in decoder.feed(&[*byte]).unwrap() {
                result = turn.event(&event, None).unwrap();
            }
        }
        assert_eq!(result.unwrap().content, "中文🙂");
        let mut decoder = EventDecoder::default();
        assert_eq!(
            decoder.feed(b"data: one\rdata: two\r\r").unwrap(),
            vec!["one\ntwo"]
        );
    }

    #[test]
    fn chatgpt_delta_then_failure_never_becomes_success_or_echoes_upstream() {
        let draft = Mutex::new(String::new());
        let callback = |delta: &str| draft.lock().unwrap().push_str(delta);
        let mut turn = Turn::new(false);
        turn.event(
            r#"{"type":"response.output_text.delta","delta":"partial"}"#,
            Some(&callback),
        )
        .unwrap();
        let error = turn.event(r#"{"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded","message":"secret prompt and token"}}}"#, None).unwrap_err();
        assert_eq!(*draft.lock().unwrap(), "partial");
        assert!(error.to_string().contains("订阅使用限额"));
        assert!(!error.to_string().contains("secret"));
        assert!(!error.to_string().contains("Codex"));
    }

    #[test]
    fn chatgpt_completed_is_authoritative_and_counts_usage_once() {
        let mut turn = Turn::new(false);
        turn.event(
            r#"{"type":"response.output_text.delta","delta":"draft"}"#,
            None,
        )
        .unwrap();
        let result = turn
            .event(&completed("final").to_string(), None)
            .unwrap()
            .unwrap();
        assert_eq!(result.content, "final");
        assert_eq!(result.api_calls, 1);
        assert_eq!(result.usage.input_tokens, Some(12));
        assert_eq!(result.usage.output_tokens, Some(5));
        assert!(turn
            .event(&completed("duplicate").to_string(), None)
            .is_err());
    }

    #[test]
    fn chatgpt_incomplete_done_bad_json_and_no_terminal_are_not_success() {
        for event in [
            r#"{"type":"response.incomplete","response":{"status":"incomplete"}}"#,
            "[DONE]",
            "not-json",
        ] {
            assert!(Turn::new(false).event(event, None).is_err());
        }
        let mut invalid = completed("text");
        invalid["response"]["status"] = json!("failed");
        assert!(Turn::new(false).event(&invalid.to_string(), None).is_err());
        let mut decoder = EventDecoder::default();
        assert!(decoder
            .feed(b"data: {\"type\":\"response.completed\"")
            .unwrap()
            .is_empty());
        let mut turn = Turn::new(false);
        assert!(turn
            .event(
                r#"{"type":"response.output_text.delta","delta":"partial"}"#,
                None
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn chatgpt_citations_search_count_and_tool_authorization() {
        let mut event = completed("Verified claim.");
        let annotation = json!({"type":"url_citation","url":"https://example.org/source", "title":"Source", "start_index":0,"end_index":15});
        event["response"]["output"][0]["content"][0]["annotations"] = json!([
            annotation, annotation,
            {"type":"url_citation","url":"https://secret:password@example.org/","start_index":0,"end_index":15}
        ]);
        event["response"]["output"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"web_search_call","status":"completed"}));
        let result = Turn::new(true)
            .event(&event.to_string(), None)
            .unwrap()
            .unwrap();
        assert_eq!(result.search_calls, 1);
        assert_eq!(result.sources.len(), 1);
        assert_eq!(result.sources[0].claim, "Verified claim.");
        assert!(!result.sources[0].original);
        assert!(Turn::new(false).event(&event.to_string(), None).is_err());
        for tool in [
            "function_call",
            "mcp_call",
            "computer_call",
            "code_interpreter_call",
            "image_generation_call",
            "future_tool",
        ] {
            let item = json!({"type":"response.output_item.added","item":{"type":tool}});
            assert!(Turn::new(true).event(&item.to_string(), None).is_err());
            let mut event = completed("text");
            event["response"]["output"]
                .as_array_mut()
                .unwrap()
                .push(json!({"type":tool}));
            assert!(Turn::new(true).event(&event.to_string(), None).is_err());
        }
        assert!(Turn::new(false)
            .event(r#"{"type":"response.web_search_call.in_progress"}"#, None)
            .is_err());
    }

    #[test]
    fn chatgpt_models_use_visible_account_catalog_and_preserve_order() {
        let models = parse_models(&json!({"models":[
            {"slug":"hidden","display_name":"Hidden","visibility":"hidden"},
            {"slug":"model-b","display_name":"Model B","visibility":"list"},
            {"slug":"model-a","display_name":"Model A","visibility":"list"},
            {"slug":"model-b","display_name":"duplicate","visibility":"list"}
        ]}))
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model, "model-b");
        assert_eq!(models[0].display_name, "Model B");
        assert!(models[0].is_default);
        assert!(!models[1].is_default);
        assert!(parse_models(&json!({"data":[{"id":"not-the-account-catalog"}]})).is_err());
        assert!(parse_models(&json!({"models":[]})).is_err());
        assert!(parse_models(
            &json!({"models":[{"slug":"","display_name":"Bad","visibility":"list"}]})
        )
        .is_err());
    }

    #[test]
    fn chatgpt_request_uses_instructions_and_only_opt_in_web_search() {
        let messages = vec![
            ChatMessage::system("one"),
            ChatMessage::system("two"),
            ChatMessage::user("question"),
        ];
        let body = request_body("model", messages.clone(), false).unwrap();
        assert_eq!(body["instructions"], "one\n\ntwo");
        assert_eq!(body["input"], json!([{"role":"user","content":"question"}]));
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body.as_object().unwrap().len(), 5);
        assert!(body.get("tools").is_none());
        let body = request_body("model", messages, true).unwrap();
        assert_eq!(body["tools"], json!([{"type":"web_search"}]));
        assert!(request_body(
            "model",
            vec![ChatMessage {
                role: "tool".into(),
                content: "untrusted".into()
            }],
            false
        )
        .is_err());
    }

    #[test]
    fn chatgpt_failures_are_structured_and_never_classify_messages() {
        for (code, expected) in [
            (
                "subscription_sharing_usage_limit_exceeded",
                CodexFailure::UsageLimit,
            ),
            (
                "subscription_sharing_usage_unavailable",
                CodexFailure::Unavailable,
            ),
            (
                "subscription_sharing_invalid_user",
                CodexFailure::Unauthorized,
            ),
            (
                "subscription_sharing_user_not_eligible",
                CodexFailure::Rejected,
            ),
            (
                "subscription_sharing_unsupported_capability",
                CodexFailure::Rejected,
            ),
            ("chatpass_v2_scope_not_authorized", CodexFailure::Rejected),
            ("context_length_exceeded", CodexFailure::ContextLimit),
            ("rate_limit_exceeded", CodexFailure::Busy),
        ] {
            assert_eq!(
                classify_failure(&json!({"error":{"code":code,"message":"private"}}), None),
                expected
            );
        }
        assert_eq!(
            classify_failure(&json!({"detail":"private"}), Some(401)),
            CodexFailure::Unauthorized
        );
        assert_eq!(
            classify_failure(&json!({"error":{"message":"rate_limit_exceeded"}}), None),
            CodexFailure::Other
        );
        assert_eq!(
            classify_failure(&Value::Null, Some(302)),
            CodexFailure::Rejected
        );
    }

    #[test]
    fn chatgpt_stream_bounds_lines_events_output_and_invalid_utf8() {
        let mut decoder = EventDecoder::default();
        assert!(decoder.feed(&vec![b'x'; MAX_EVENT + 1]).is_err());
        let mut decoder = EventDecoder::default();
        assert!(decoder.feed(b"data: \xff\n\n").is_err());
        let mut decoder = EventDecoder {
            events: MAX_EVENTS,
            ..Default::default()
        };
        assert!(decoder.feed(b"data: {}\n\n").is_err());
        let mut decoder = EventDecoder {
            wire_bytes: MAX_WIRE,
            ..Default::default()
        };
        assert!(decoder.feed(b":").is_err());
        let mut decoder = EventDecoder {
            data: "x".repeat(MAX_EVENT),
            has_data: true,
            ..Default::default()
        };
        assert!(decoder.feed(b"data: x\n").is_err());
        let mut turn = Turn::new(false);
        let event = json!({"type":"response.output_text.delta","delta":"x".repeat(MAX_OUTPUT + 1)});
        assert!(turn.event(&event.to_string(), None).is_err());
        assert!(Turn::new(false)
            .event(&completed(&"x".repeat(MAX_OUTPUT + 1)).to_string(), None)
            .is_err());
    }
}
