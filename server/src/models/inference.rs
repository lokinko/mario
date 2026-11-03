//! Execution limits apply to a whole workflow, not to every stage combined.
pub(crate) const ANALYSIS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25 * 60);
pub(crate) const AGENT_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(26 * 60);
pub(crate) const MODEL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
/// Codex turns are retried only before any assistant output was emitted.
pub(crate) const CODEX_MAX_ATTEMPTS: usize = 3;
pub(crate) const ANALYSIS_TIMEOUT_MESSAGE: &str = "问答达到 25 分钟执行上限，已停止模型调用；本次未保存完整结果。可缩小问题或关闭部分分析步骤后重试";

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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum AnalysisEvent {
    Status {
        message: String,
    },
    Draft {
        answer: String,
    },
    Complete {
        result: Box<crate::models::AnalysisResult>,
    },
    Error {
        message: String,
    },
}
