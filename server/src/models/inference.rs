//! Execution limits apply to a whole workflow, not to every stage combined.
pub(crate) const ANALYSIS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25 * 60);
pub(crate) const AGENT_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(26 * 60);
pub(crate) const MODEL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
pub(crate) const ANALYSIS_TIMEOUT_MESSAGE: &str = "问答达到 25 分钟执行上限，已停止模型调用；本次未保存完整结果。可缩小问题或关闭部分分析步骤后重试";

/// Safe error categories cross the bridge; upstream bodies and account details do not.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
#[serde(rename_all = "camelCase")]
pub enum CodexFailure {
    #[error("Codex 服务暂时繁忙，请稍后再试")]
    Busy,
    #[error("Codex 上游连接暂时不可用，请检查网络后再试")]
    Unavailable,
    #[error("Codex 回复连接中断，未取得完整结果；请检查本机网络后重试")]
    Disconnected,
    #[error("Codex 单次模型调用超过 180 秒，已停止；请缩小问题或稍后重试")]
    Timeout,
    #[error("Codex 登录已失效或未授权，请在本机 Codex 重新登录后重试")]
    Unauthorized,
    #[error("Codex 已报告账号用量限制，请在 Codex 中查看额度及恢复时间；自动重试无法恢复额度")]
    UsageLimit,
    #[error("Codex 本次会话的执行预算已用尽，请缩小问题后重试")]
    SessionBudget,
    #[error("Codex 上下文超出模型上限，请减少本次发送的资料")]
    ContextLimit,
    #[error("Codex 拒绝了请求，请检查所选模型权限或更新本机 Codex")]
    Rejected,
    #[error("Codex 调用失败且未提供可识别的错误分类，请检查本机 Codex 状态后重试")]
    Other,
}
impl CodexFailure {
    pub(crate) fn retryable(self) -> bool {
        matches!(self, Self::Busy | Self::Unavailable | Self::Disconnected)
    }
}
