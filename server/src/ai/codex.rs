//! Official Codex App Server adapter. Credentials remain owned and refreshed by
//! Codex; no OAuth token or auth.json content is returned to the frontend.
use super::provider::{
    ChatMessage, ModelCompletion, ModelProvider, SearchCompletion, SearchSource,
};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct CodexProvider {
    model: String,
}
impl CodexProvider {
    pub fn new(model: String) -> Self {
        Self { model }
    }
    pub async fn detect() -> AppResult<String> {
        tokio::time::timeout(Duration::from_secs(25), async {
            let mut rpc = Rpc::start().await?;
            rpc.account().await?;
            let models = rpc.call("model/list", json!({})).await?;
            models["data"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|model| model["isDefault"] == true)
                        .or_else(|| items.first())
                })
                .and_then(|model| model["model"].as_str())
                .map(str::to_string)
                .ok_or_else(|| AppError::Model("已读取 Codex 登录，但没有可用模型".into()))
        })
        .await
        .map_err(|_| {
            AppError::Model("读取 Codex 凭证超时，请打开 Codex 检查登录状态后重试".into())
        })?
    }
    async fn generate(
        &self,
        messages: Vec<ChatMessage>,
        search: bool,
    ) -> AppResult<SearchCompletion> {
        tokio::time::timeout(Duration::from_secs(180), async {
            let mut rpc = Rpc::start().await?;
            rpc.account().await?;
            let work = WorkDir::new()?;
            let instructions = messages.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n");
            let prompt = messages.iter().filter(|m| m.role != "system").map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n");
            let thread = rpc.call("thread/start", json!({
                "model":self.model, "modelProvider":"openai", "ephemeral":true,
                "cwd":work.0, "approvalPolicy":"never", "sandbox":"read-only",
                "environments":[], "selectedCapabilityRoots":[],
                "baseInstructions":format!("你是 mario 的问答模型。仅依据本次输入回答。禁止调用文件、终端、应用、MCP 或代理工具。{}\n{}", if search { "只允许内置网页搜索；来源必须提供链接。" } else { "禁止调用工具。" }, instructions),
                "config": {"web_search":if search { "live" } else { "disabled" },
                    "features.shell_tool":false, "features.multi_agent":false,
                    "features.apps":false, "features.skills":false, "features.hooks":false,
                    "mcp_servers":{}}
            })).await?;
            let id = thread["thread"]["id"].as_str().ok_or_else(|| AppError::Model("Codex 未创建临时会话".into()))?;
            rpc.call("turn/start", json!({"threadId":id,"input":[{"type":"text","text":prompt}],"effort":"low","environments":[]})).await?;
            let mut output = SearchCompletion { api_calls:1, ..Default::default() };
            loop {
                let event = rpc.read().await?;
                if event.get("id").is_some() && event.get("method").is_some() {
                    // No interactive tool requests are permitted in this model adapter.
                    return Err(AppError::Model("Codex 请求了问答以外的工具操作，已停止本次调用".into()));
                }
                match event["method"].as_str() {
                    Some("item/completed") => ingest_item(&event["params"]["item"], &mut output)?,
                    Some("thread/tokenUsage/updated") => {
                        let usage = &event["params"]["tokenUsage"]["total"];
                        output.usage.input_tokens = usage["inputTokens"].as_u64();
                        output.usage.output_tokens = usage["outputTokens"].as_u64();
                    }
                    Some("turn/completed") => {
                        if event["params"]["turn"]["status"] != "completed" {
                            return Err(AppError::Model("Codex 调用失败或额度不足，请在 Codex 中检查登录与额度后重试".into()));
                        }
                        if output.content.trim().is_empty() { return Err(AppError::Model("Codex 没有返回有效回答".into())); }
                        return Ok(output);
                    }
                    _ => {}
                }
            }
        }).await.map_err(|_| AppError::Model("Codex 调用超时，请缩小问题后重试".into()))?
    }
}
#[async_trait]
impl ModelProvider for CodexProvider {
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
        "codex"
    }
    fn model_name(&self) -> &str {
        &self.model
    }
}

fn binary() -> AppResult<PathBuf> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Err(AppError::Validation(
            "此设备无法读取桌面 Codex 凭证，请使用 OpenAI 或 Anthropic API Key".into(),
        ));
    }
    let mut candidates = vec![
        PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
        PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/ChatGPT.app/Contents/Resources/codex"));
        candidates.push(home.join("Applications/Codex.app/Contents/Resources/codex"));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ]);
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&path)
                .map(|dir| dir.join(if cfg!(windows) { "codex.exe" } else { "codex" })),
        );
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        AppError::Validation("读取失败：没有找到 Codex，请先安装并登录 Codex".into())
    })
}
struct WorkDir(PathBuf);
impl WorkDir {
    fn new() -> AppResult<Self> {
        let path = std::env::temp_dir().join(format!("mario-codex-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Rpc {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
    pending: std::collections::VecDeque<Value>,
}
impl Rpc {
    async fn start() -> AppResult<Self> {
        let mut child = Command::new(binary()?)
            .args(["app-server", "--stdio", "-c", "model_provider=\"openai\""])
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| AppError::Model("无法启动 Codex，请检查 Codex 安装是否完整".into()))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Model("Codex 输入通道不可用".into()))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| AppError::Model("Codex 输出通道不可用".into()))?,
        );
        let mut rpc = Self {
            _child: child,
            input,
            output,
            sequence: 0,
            pending: Default::default(),
        };
        rpc.call("initialize",json!({"clientInfo":{"name":"mario","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        rpc.send(json!({"method":"initialized","params":{}}))
            .await?;
        Ok(rpc)
    }
    async fn send(&mut self, value: Value) -> AppResult<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        Ok(())
    }
    async fn wire_read(&mut self) -> AppResult<Value> {
        let mut bytes = Vec::new();
        // Bound each event before parsing; never log responses or credentials.
        loop {
            let available = self.output.fill_buf().await?;
            if available.is_empty() {
                return Err(AppError::Model("Codex 已退出，请检查安装和登录状态".into()));
            }
            let end = available
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(available.len());
            if bytes.len() + end > 4 * 1024 * 1024 {
                return Err(AppError::Model("Codex 返回的数据过大".into()));
            }
            let finished = available[end - 1] == b'\n';
            bytes.extend_from_slice(&available[..end]);
            self.output.consume(end);
            if finished {
                break;
            }
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Model("Codex 协议响应无效，请更新 Codex 后重试".into()))
    }
    async fn read(&mut self) -> AppResult<Value> {
        if let Some(value) = self.pending.pop_front() {
            Ok(value)
        } else {
            self.wire_read().await
        }
    }
    async fn call(&mut self, method: &str, params: Value) -> AppResult<Value> {
        self.sequence += 1;
        let id = self.sequence;
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let event = self.wire_read().await?;
            if event["id"].as_u64() == Some(id) && event.get("method").is_none() {
                if event.get("error").is_some() {
                    return Err(AppError::Model(format!(
                        "Codex 的 {method} 请求失败，请检查登录、模型权限和 Codex 版本"
                    )));
                }
                return Ok(event["result"].clone());
            }
            if event.get("id").is_some() && event.get("method").is_some() {
                return Err(AppError::Model(
                    "Codex 需要交互确认，请先在 Codex 中完成登录".into(),
                ));
            }
            if self.pending.len() >= 2048 {
                return Err(AppError::Model("Codex 返回事件过多".into()));
            }
            self.pending.push_back(event);
        }
    }
    async fn account(&mut self) -> AppResult<()> {
        let result = self
            .call("account/read", json!({"refreshToken":false}))
            .await?;
        check_account(&result)
    }
}
fn check_account(value: &Value) -> AppResult<()> {
    match value["account"]["type"].as_str() {
        Some("chatgpt" | "apiKey") => Ok(()),
        _ => Err(AppError::Validation(
            "读取失败：未找到已登录的 Codex 凭证，请先在 Codex 中登录".into(),
        )),
    }
}
fn ingest_item(item: &Value, output: &mut SearchCompletion) -> AppResult<()> {
    match item["type"].as_str() {
        Some("agentMessage") => {
            if item["phase"].as_str() != Some("commentary") {
                output.content = item["text"].as_str().unwrap_or_default().to_string();
            }
        }
        Some("webSearch") => {
            output.search_calls += 1;
            // Only structured provider results qualify as evidence, never links
            // invented in an assistant's plain-text answer.
            if let Some(results) = item["results"].as_array() {
                for source in results {
                    collect_source(source, output);
                }
            }
        }
        Some(
            "commandExecution"
            | "fileChange"
            | "mcpToolCall"
            | "dynamicToolCall"
            | "collabAgentToolCall",
        ) => {
            return Err(AppError::Model(
                "Codex 尝试调用问答以外的工具，已停止本次调用".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}
fn collect_source(source: &Value, output: &mut SearchCompletion) {
    let url = source["url"].as_str().unwrap_or_default();
    let title = source["title"].as_str().unwrap_or_default();
    let claim = source["snippet"]
        .as_str()
        .or_else(|| source["text"].as_str())
        .unwrap_or_default();
    if reqwest::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https") && u.username().is_empty() && u.password().is_none()
    }) && !claim.is_empty()
        && !output
            .sources
            .iter()
            .any(|s| s.url == url && s.claim == claim)
    {
        output.sources.push(SearchSource {
            url: url.into(),
            title: title.into(),
            claim: claim.into(),
            original: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_unknown_account_is_a_visible_failure() {
        for account in [json!(null), json!({}), json!({"type":"unknown"})] {
            assert!(check_account(&json!({"account":account}))
                .unwrap_err()
                .to_string()
                .contains("未找到"));
        }
        assert!(check_account(&json!({"account":{"type":"chatgpt"}})).is_ok());
        assert!(check_account(&json!({"account":{"type":"apiKey"}})).is_ok());
    }

    #[test]
    fn only_native_search_results_become_sources() {
        let mut output = SearchCompletion::default();
        ingest_item(
            &json!({"type":"agentMessage","text":"https://invented.example/"}),
            &mut output,
        )
        .unwrap();
        assert!(output.sources.is_empty());
        let result = json!({"type":"webSearch","results":[
            {"url":"https://example.com/docs","title":"Docs","snippet":"Native search excerpt"},
            {"url":"https://example.com/docs","title":"Docs","snippet":"Native search excerpt"},
            {"url":"file:///private/data","snippet":"Invalid scheme"},
            {"url":"https://user:password@example.com","snippet":"Credentials"},
            {"title":"Missing URL","snippet":"Not evidence"}
        ]});
        ingest_item(&result, &mut output).unwrap();
        assert_eq!(output.search_calls, 1);
        assert_eq!(output.sources.len(), 1);
        assert_eq!(output.sources[0].claim, "Native search excerpt");
        assert!(!output.sources[0].original);
        ingest_item(
            &json!({"type":"agentMessage","phase":"commentary","text":"Searching"}),
            &mut output,
        )
        .unwrap();
        assert_eq!(output.content, "https://invented.example/");
        assert!(ingest_item(&json!({"type":"commandExecution"}), &mut output).is_err());
    }

    #[tokio::test]
    #[ignore = "Uses the user's local Codex login and model quota; run explicitly"]
    async fn live_codex_completion_and_native_search() {
        let model = CodexProvider::detect().await.unwrap();
        let provider = CodexProvider::new(model);
        let reply = provider
            .complete(vec![ChatMessage::user("Reply only OK")])
            .await
            .unwrap();
        assert!(reply.content.contains("OK"));
        let search = provider.search(vec![ChatMessage::user("Use native web search to find the official OpenAI Responses API documentation. Search site:platform.openai.com/docs Responses API. Briefly identify the page in one sentence.")]).await.unwrap();
        assert!(search.search_calls > 0, "Expected native web search");
        assert!(
            !search.sources.is_empty(),
            "Expected structured search sources"
        );
        assert!(!search.content.is_empty());
    }
}
