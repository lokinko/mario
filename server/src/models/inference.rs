//! Execution limits apply to a whole workflow, not to every stage combined.
pub(crate) const ANALYSIS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25 * 60);
pub(crate) const AGENT_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(26 * 60);
pub(crate) const MODEL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
/// Codex turns are retried only before any assistant output was emitted.
pub(crate) const CODEX_MAX_ATTEMPTS: usize = 3;
pub(crate) const ANALYSIS_TIMEOUT_MESSAGE: &str =
    "问答达到 25 分钟执行上限，已停止模型调用；本次未保存完整结果。可缩小问题或关闭网页搜索后重试";
/// Dreaming is one call plus at most one repair.
pub(crate) const DREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2 * 180 + 30);
pub(crate) const DREAM_TIMEOUT_MESSAGE: &str =
    "记忆整理超过执行上限，已停止；记忆和整理进度均未改变";
/// Daily review runs optional public search plus structured generation and at most one repair.
pub(crate) const DAILY_REVIEW_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(2 * 180 + 30);
pub(crate) const DAILY_REVIEW_TIMEOUT_MESSAGE: &str =
    "每日 AI 复盘超过执行上限，已停止；本次已保留昨日持仓事实与官方公告动态";

/// Shared local-model failure categories keep their historical wire names.
/// Upstream bodies and account details never cross the bridge.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
#[serde(rename_all = "camelCase")]
pub enum CodexFailure {
    #[error("本机模型服务暂时繁忙，请稍后再试")]
    Busy,
    #[error("本机模型上游连接暂时不可用，请检查网络后再试")]
    Unavailable,
    #[error("本机模型回复连接中断，未取得完整结果；请检查本机网络后重试")]
    Disconnected,
    #[error("本机模型单次调用超过 180 秒，已停止；请缩小问题或稍后重试")]
    Timeout,
    #[error("本机模型登录已失效或未授权，请在本机重新登录或授权后重试")]
    Unauthorized,
    #[error("本机模型已报告用量限制（包括订阅使用限额），请到 ChatGPT 设置的 Usage 或模型提供方用量页面查看；自动重试无法恢复额度，也不会自动切换计费方式")]
    UsageLimit,
    #[error("本机模型本次会话的执行预算已用尽，请缩小问题后重试")]
    SessionBudget,
    #[error("本机模型上下文超出模型上限，请减少本次发送的资料")]
    ContextLimit,
    #[error("本机模型拒绝了请求，请检查所选模型、账号与工作区权限")]
    Rejected,
    #[error("本机模型调用失败且未提供可识别的错误分类，请检查本机登录和授权状态后重试")]
    Other,
}
impl CodexFailure {
    pub(crate) fn retryable(self) -> bool {
        matches!(self, Self::Busy | Self::Unavailable | Self::Disconnected)
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum AnalysisEvent {
    Status {
        message: String,
    },
    /// Unvalidated live text of one proposal agent; replaced by the voted report on completion.
    Draft {
        proposal: String,
        answer: String,
    },
    Complete {
        result: Box<crate::models::analysis::AnalysisResult>,
    },
    Error {
        message: String,
    },
}

// Internally tagged derived Deserialize buffers into serde's private Content enum,
// which turns floats into arbitrary-precision maps under `serde_json/arbitrary_precision`
// and rejects `f64` fields inside `AnalysisResult`. Dispatching through a JSON map keeps
// the exact wire contract while decoding numbers directly.
impl<'de> serde::Deserialize<'de> for AnalysisEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind = fields
            .remove("type")
            .ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let kind = kind.as_str().ok_or_else(|| {
            serde::de::Error::invalid_type(
                serde::de::Unexpected::Other("non-string"),
                &"event type string",
            )
        })?;
        let take = |fields: &mut serde_json::Map<String, serde_json::Value>,
                    name: &'static str|
         -> Result<serde_json::Value, D::Error> {
            fields
                .remove(name)
                .ok_or_else(|| serde::de::Error::missing_field(name))
        };
        match kind {
            "status" => Ok(Self::Status {
                message: serde_json::from_value(take(&mut fields, "message")?)
                    .map_err(serde::de::Error::custom)?,
            }),
            "draft" => Ok(Self::Draft {
                proposal: serde_json::from_value(take(&mut fields, "proposal")?)
                    .map_err(serde::de::Error::custom)?,
                answer: serde_json::from_value(take(&mut fields, "answer")?)
                    .map_err(serde::de::Error::custom)?,
            }),
            "complete" => Ok(Self::Complete {
                result: serde_json::from_value(take(&mut fields, "result")?)
                    .map_err(serde::de::Error::custom)?,
            }),
            "error" => Ok(Self::Error {
                message: serde_json::from_value(take(&mut fields, "message")?)
                    .map_err(serde::de::Error::custom)?,
            }),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["status", "draft", "complete", "error"],
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        analysis::{AnalysisResult, AnalysisTransparency, AnalysisWorkflowTrace},
        portfolio::HoldingValuationEvidence,
    };

    #[test]
    fn analysis_event_round_trips_complete_with_floats_and_any_field_order() {
        let result = AnalysisResult {
            id: "event-float-test".into(),
            answer: "结论".into(),
            stages: vec!["研究代理".into()],
            transparency: AnalysisTransparency {
                provider: "openai-chat".into(),
                model: "test-model".into(),
                context_groups: vec!["当前持仓".into()],
                payload_bytes: 256,
                context_revision: "rev-1".into(),
                evidence_items_used: 0,
                citations_required: false,
                model_calls: 7,
                total_latency_ms: 1234,
                input_tokens: Some(100),
                output_tokens: Some(50),
                structured_output_validated: true,
                output_repairs: 0,
                memory_notes_used: 0,
                external_data_used: false,
                api_key_sent: false,
            },
            workflow_trace: AnalysisWorkflowTrace {
                valuation_evidence: vec![HoldingValuationEvidence {
                    holding_id: "h1".into(),
                    symbol: "600036.SS".into(),
                    quantity: 100.5,
                    unit_price: 38.5,
                    market_value: 3869.25,
                    currency: "CNY".into(),
                    requested_on: "2026-11-20".into(),
                    observed_on: "2026-11-20".into(),
                    staleness_days: 0,
                    provider_code: "test".into(),
                    provider_name: "Test".into(),
                    exchange: "SSE".into(),
                    mic_code: "XSHG".into(),
                    instrument_type: "equity".into(),
                    price_basis: "close".into(),
                    source_url: "https://example.test/quote".into(),
                    methodology_url: "https://example.test/method".into(),
                    disclaimer: String::new(),
                    captured_at: "2026-11-20T08:00:00Z".into(),
                }],
                personal_context: Some(serde_json::json!({
                    "exact": 99999999999999.02,
                    "ratio": 0.375
                })),
                version: "investment-workflow-v1".into(),
                ..AnalysisWorkflowTrace::default()
            },
            created_at: "2026-11-20T08:00:00Z".into(),
            disclaimer: "test".into(),
        };

        let encoded = serde_json::to_string(&AnalysisEvent::Complete {
            result: Box::new(result.clone()),
        })
        .unwrap();
        let decoded: AnalysisEvent = serde_json::from_str(&encoded).unwrap();
        let AnalysisEvent::Complete { result: decoded } = decoded else {
            panic!("expected complete event");
        };
        assert_eq!(decoded.id, "event-float-test");
        assert_eq!(decoded.workflow_trace.valuation_evidence[0].quantity, 100.5);
        assert_eq!(
            decoded.workflow_trace.valuation_evidence[0].market_value,
            3869.25
        );

        let reordered = serde_json::json!({
            "result": result,
            "type": "complete"
        })
        .to_string();
        assert!(matches!(
            serde_json::from_str::<AnalysisEvent>(&reordered).unwrap(),
            AnalysisEvent::Complete { .. }
        ));
        assert!(serde_json::from_str::<AnalysisEvent>(r#"{"type":"unknown"}"#).is_err());
        assert!(serde_json::from_str::<AnalysisEvent>(r#"{"type":"status"}"#).is_err());
    }
}
