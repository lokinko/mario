//! AI-owned contracts for scheduled daily portfolio review and microblog-style feed items.
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub(crate) const DAILY_REVIEW_DEFAULT_TIME: &str = "07:30";
pub(crate) const DAILY_REVIEW_MAX_AI_POSTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewScope {
    #[default]
    HoldingsAndWatch,
    HoldingsOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_schedule_time")]
    pub schedule_time: String,
    #[serde(default)]
    pub scope: DailyReviewScope,
    #[serde(default = "default_true")]
    pub web_search: bool,
}

fn default_true() -> bool {
    true
}

fn default_schedule_time() -> String {
    DAILY_REVIEW_DEFAULT_TIME.into()
}

impl Default for DailyReviewSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            schedule_time: DAILY_REVIEW_DEFAULT_TIME.into(),
            scope: DailyReviewScope::HoldingsAndWatch,
            web_search: true,
        }
    }
}

impl DailyReviewSettings {
    pub fn validate(&self) -> AppResult<()> {
        parse_hhmm(&self.schedule_time)?;
        Ok(())
    }
}

pub(crate) fn parse_hhmm(value: &str) -> AppResult<(u32, u32)> {
    let trimmed = value.trim();
    let Some((h, m)) = trimmed.split_once(':') else {
        return Err(AppError::Validation(
            "触发时间格式必须为 HH:MM（例如 07:30）".into(),
        ));
    };
    if h.len() != 2 || m.len() != 2 {
        return Err(AppError::Validation(
            "触发时间格式必须为 HH:MM（例如 07:30）".into(),
        ));
    }
    let hour: u32 = h
        .parse()
        .map_err(|_| AppError::Validation("触发时间小时无效".into()))?;
    let minute: u32 = m
        .parse()
        .map_err(|_| AppError::Validation("触发时间分钟无效".into()))?;
    if hour > 23 || minute > 59 {
        return Err(AppError::Validation("触发时间超出 00:00–23:59 范围".into()));
    }
    Ok((hour, minute))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewTrigger {
    Scheduled,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewRunStatus {
    Completed,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewAiStatus {
    Completed,
    SkippedNoModel,
    Failed,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewItemKind {
    PortfolioSummary,
    HoldingMove,
    OfficialAnnouncement,
    DividendUpdate,
    WatchSignal,
    AiInsight,
    ExternalClue,
}

impl DailyReviewItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PortfolioSummary => "portfolioSummary",
            Self::HoldingMove => "holdingMove",
            Self::OfficialAnnouncement => "officialAnnouncement",
            Self::DividendUpdate => "dividendUpdate",
            Self::WatchSignal => "watchSignal",
            Self::AiInsight => "aiInsight",
            Self::ExternalClue => "externalClue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewItemOrigin {
    DeterministicFact,
    OfficialFiling,
    AiReview,
    WebClue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DailyReviewImportance {
    High,
    Medium,
    Normal,
}

impl DailyReviewImportance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Normal => "normal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricTone {
    Positive,
    Negative,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewMetric {
    pub label: String,
    pub value: String,
    #[serde(default = "default_metric_tone")]
    pub tone: MetricTone,
    #[serde(default)]
    pub sensitive: bool,
}

fn default_metric_tone() -> MetricTone {
    MetricTone::Neutral
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewSource {
    pub label: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub published_on: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewFeedItem {
    pub id: String,
    pub run_id: String,
    pub review_day: String,
    pub kind: DailyReviewItemKind,
    pub origin: DailyReviewItemOrigin,
    pub importance: DailyReviewImportance,
    pub title: String,
    pub summary: String,
    pub body: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub asset_name: Option<String>,
    #[serde(default)]
    pub metrics: Vec<DailyReviewMetric>,
    #[serde(default)]
    pub sources: Vec<DailyReviewSource>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ask_prompt: String,
    #[serde(default)]
    pub read_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewRun {
    pub id: String,
    pub review_day: String,
    pub trigger: DailyReviewTrigger,
    pub status: DailyReviewRunStatus,
    pub ai_status: DailyReviewAiStatus,
    #[serde(default)]
    pub ai_model: Option<String>,
    #[serde(default)]
    pub degraded_reason: Option<String>,
    pub item_count: usize,
    pub announcement_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewFeedQuery {
    #[serde(default)]
    pub day: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub unread_only: Option<bool>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewFeedResponse {
    pub settings: DailyReviewSettings,
    pub timezone: String,
    pub latest_review_day: Option<String>,
    pub unread_count: usize,
    pub runs: Vec<DailyReviewRun>,
    pub items: Vec<DailyReviewFeedItem>,
    pub has_more: bool,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewRunInput {
    #[serde(default)]
    pub review_day: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewRunResult {
    pub run: DailyReviewRun,
    pub feed: DailyReviewFeedResponse,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewReadInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewReadResult {
    pub ok: bool,
    pub unread_count: usize,
}

// --- Compact read-only user projection for daily review (never full ledger Snapshot) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTrackedAsset {
    pub symbol: String,
    pub name: String,
    pub asset_class: String,
    pub in_portfolio: bool,
    pub in_watchlist: bool,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub market_value: Option<f64>,
    #[serde(default)]
    pub weight_pct: Option<f64>,
    #[serde(default)]
    pub unrealized_pnl_pct: Option<f64>,
    #[serde(default)]
    pub day_change_amount: Option<f64>,
    #[serde(default)]
    pub day_change_pct: Option<f64>,
    #[serde(default)]
    pub pct_point_change: Option<f64>,
    #[serde(default)]
    pub change_kind: String,
    #[serde(default)]
    pub valuation_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWatchSignal {
    pub symbol: String,
    pub name: String,
    pub day: String,
    pub alert: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRuleBrief {
    pub category: String,
    pub statement: String,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewPortfolioSummary {
    pub review_day: String,
    pub base_currency: String,
    pub total_assets: Option<f64>,
    pub net_assets: Option<f64>,
    pub previous_day_change: Option<f64>,
    pub previous_day_change_pct: Option<f64>,
    pub carried: bool,
    pub missing_fx: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewUserContext {
    pub timezone: String,
    pub today: String,
    pub review_day: String,
    pub portfolio: Option<DailyReviewPortfolioSummary>,
    pub tracked_assets: Vec<ReviewTrackedAsset>,
    pub watch_signals: Vec<ReviewWatchSignal>,
    pub active_rules: Vec<ReviewRuleBrief>,
    pub methodology_prompt: String,
}

// --- Frozen input & structured output for AI daily review execution ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAnnouncementBrief {
    pub symbol: String,
    pub stock_name: String,
    pub title: String,
    pub category: String,
    pub published_on: String,
    pub source_name: String,
    pub source_url: String,
    pub importance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDividendPlanBrief {
    pub symbol: String,
    pub stock_name: String,
    pub report_period: String,
    pub stage: String,
    pub cash_per_10: Option<String>,
    pub ex_dividend_date: Option<String>,
    pub announced_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReviewPromptInput {
    pub review_day: String,
    pub web_search: bool,
    pub portfolio: Option<DailyReviewPortfolioSummary>,
    pub assets: Vec<ReviewTrackedAsset>,
    pub announcements: Vec<ReviewAnnouncementBrief>,
    pub dividend_plans: Vec<ReviewDividendPlanBrief>,
    pub watch_signals: Vec<ReviewWatchSignal>,
    pub active_rules: Vec<ReviewRuleBrief>,
    pub methodology_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewPostDraft {
    pub kind: DailyReviewItemKind,
    pub importance: DailyReviewImportance,
    pub title: String,
    pub summary: String,
    pub body: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub asset_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ask_prompt: String,
    #[serde(default)]
    pub sources: Vec<DailyReviewSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewOutcome {
    pub posts: Vec<AiReviewPostDraft>,
    pub model_name: String,
    pub warnings: Vec<String>,
    pub web_sources_used: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_schedule_time_format() {
        assert_eq!(parse_hhmm("07:30").unwrap(), (7, 30));
        assert_eq!(parse_hhmm("00:00").unwrap(), (0, 0));
        assert_eq!(parse_hhmm("23:59").unwrap(), (23, 59));
        assert!(parse_hhmm("7:30").is_err());
        assert!(parse_hhmm("24:00").is_err());
        assert!(parse_hhmm("12:60").is_err());
        assert!(parse_hhmm("bad").is_err());
    }
}
