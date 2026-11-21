//! Daily automated review use case: aggregates prior-day portfolio moves, statutory
//! CNINFO announcements, dividend plans, watch signals and AI microblog-style insights.
use super::{
    analysis::frozen_credentials,
    model_settings::{configuration, ServerOpenAi},
};
use crate::{
    models::{
        daily_review::{
            parse_hhmm, DailyReviewAiStatus, DailyReviewFeedItem, DailyReviewFeedQuery,
            DailyReviewFeedResponse, DailyReviewImportance, DailyReviewItemKind,
            DailyReviewItemOrigin, DailyReviewMetric, DailyReviewPromptInput,
            DailyReviewReadResult, DailyReviewRun, DailyReviewRunResult, DailyReviewRunStatus,
            DailyReviewSettings, DailyReviewSource, DailyReviewTrigger, DailyReviewUserContext,
            MetricTone, ReviewAnnouncementBrief, ReviewDividendPlanBrief, ReviewTrackedAsset,
        },
        inference::{DAILY_REVIEW_TIMEOUT, DAILY_REVIEW_TIMEOUT_MESSAGE},
        market::{announcements::AnnouncementImportance, dividend_plans::PlanStage},
    },
    ports::{
        ai_account::DailyReviewAccount,
        analysis::AnalysisExecutor,
        market::{AnnouncementReader, DividendPlanReader},
    },
    AppError, AppResult,
};
use chrono::{DateTime, Days, NaiveDate, Timelike, Utc};
use std::collections::BTreeSet;
use uuid::Uuid;

pub(crate) struct DailyReviewService<'a, A: DailyReviewAccount> {
    db: &'a A,
    server: Option<&'a ServerOpenAi>,
}

impl<'a, A: DailyReviewAccount> DailyReviewService<'a, A> {
    pub fn new(db: &'a A, server: Option<&'a ServerOpenAi>) -> Self {
        Self { db, server }
    }

    pub fn feed(&self, query: &DailyReviewFeedQuery) -> AppResult<DailyReviewFeedResponse> {
        let settings = self.db.daily_review_settings()?;
        let timezone = self.db.daily_review_timezone()?;
        let runs = self.db.daily_review_runs(14)?;
        let latest_review_day = runs.first().map(|r| r.review_day.clone());
        let unread_count = self.db.daily_review_unread_count()?;
        let (items, has_more, next_before) = self.db.daily_review_items(query)?;
        Ok(DailyReviewFeedResponse {
            settings,
            timezone,
            latest_review_day,
            unread_count,
            runs,
            items,
            has_more,
            next_before,
        })
    }

    pub fn save_settings(&self, settings: &DailyReviewSettings) -> AppResult<DailyReviewSettings> {
        self.db.save_daily_review_settings(settings)
    }

    pub fn mark_read(&self, id: Option<&str>, all: bool) -> AppResult<DailyReviewReadResult> {
        let unread_count = self.db.mark_daily_review_read(id, all)?;
        Ok(DailyReviewReadResult {
            ok: true,
            unread_count,
        })
    }

    /// Returns the canonical A-share symbols tracked by this account for pre-fetching official filings.
    pub fn tracked_symbols(&self) -> AppResult<Vec<String>> {
        let settings = self.db.daily_review_settings()?;
        let ctx = self.db.daily_review_context(None, settings.scope)?;
        Ok(ctx
            .tracked_assets
            .into_iter()
            .map(|a| a.symbol)
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Checks whether the scheduled daily review is due at `now` for this account, and runs it once.
    pub async fn run_if_due(
        &self,
        now: DateTime<Utc>,
        announcements: &dyn AnnouncementReader,
        dividend_plans: &dyn DividendPlanReader,
        executor: &dyn AnalysisExecutor,
    ) -> AppResult<Option<DailyReviewRun>> {
        let settings = self.db.daily_review_settings()?;
        if !settings.enabled {
            return Ok(None);
        }
        let tz_name = self.db.daily_review_timezone()?;
        let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::Asia::Shanghai);
        let local_now = now.with_timezone(&tz);
        let (target_hour, target_minute) = parse_hhmm(&settings.schedule_time)?;
        if (local_now.hour(), local_now.minute()) < (target_hour, target_minute) {
            return Ok(None);
        }
        let review_day = local_now
            .date_naive()
            .checked_sub_days(Days::new(1))
            .unwrap_or_else(|| local_now.date_naive());
        let review_day_str = review_day.to_string();
        if self.db.has_daily_review_for_day(&review_day_str)? {
            return Ok(None);
        }
        let ctx = self
            .db
            .daily_review_context(Some(review_day), settings.scope)?;
        if ctx.portfolio.is_none() && ctx.tracked_assets.is_empty() && ctx.watch_signals.is_empty()
        {
            return Ok(None);
        }
        let outcome = self
            .run(
                Some(review_day),
                DailyReviewTrigger::Scheduled,
                announcements,
                dividend_plans,
                executor,
            )
            .await?;
        Ok(Some(outcome.run))
    }

    /// Executes a daily review for `review_day` (defaults to yesterday in the account's timezone).
    pub async fn run(
        &self,
        requested_day: Option<NaiveDate>,
        trigger: DailyReviewTrigger,
        announcements: &dyn AnnouncementReader,
        dividend_plans: &dyn DividendPlanReader,
        executor: &dyn AnalysisExecutor,
    ) -> AppResult<DailyReviewRunResult> {
        let settings = self.db.daily_review_settings()?;
        let ctx = self
            .db
            .daily_review_context(requested_day, settings.scope)?;
        let review_date = NaiveDate::parse_from_str(&ctx.review_day, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("复盘日期格式无效".into()))?;
        let ann_since = review_date
            .checked_sub_days(Days::new(3))
            .unwrap_or(review_date);
        let ann_through = review_date
            .checked_add_days(Days::new(1))
            .unwrap_or(review_date);

        let mut official_announcements = Vec::new();
        let mut seen_ann = BTreeSet::new();
        let mut recent_plans = Vec::new();
        let mut seen_plans = BTreeSet::new();

        for asset in &ctx.tracked_assets {
            if asset.symbol.is_empty() {
                continue;
            }
            if let Ok(snap) = announcements.snapshot(&asset.symbol) {
                for ann in snap.announcements {
                    if let Ok(pub_day) = NaiveDate::parse_from_str(&ann.published_on, "%Y-%m-%d") {
                        if pub_day >= ann_since
                            && pub_day <= ann_through
                            && seen_ann.insert(ann.id.clone())
                        {
                            official_announcements.push((asset.clone(), ann));
                        }
                    }
                }
            }
            if let Ok(plan_snap) = dividend_plans.snapshot(&asset.symbol) {
                for plan in plan_snap.plans {
                    let announced_recent =
                        plan.announced_on >= ann_since && plan.announced_on <= ann_through;
                    let ex_soon = plan.ex_date.is_some_and(|d| {
                        d >= review_date
                            && d <= review_date
                                .checked_add_days(Days::new(7))
                                .unwrap_or(review_date)
                    });
                    if (announced_recent || ex_soon)
                        && !matches!(plan.stage, PlanStage::Cancelled | PlanStage::NoDistribution)
                        && seen_plans.insert(format!("{}:{}", plan.symbol, plan.report_date))
                    {
                        recent_plans.push((asset.clone(), plan));
                    }
                }
            }
        }

        let run_id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let mut items = build_deterministic_items(
            &run_id,
            &created_at,
            &ctx,
            &official_announcements,
            &recent_plans,
        );
        let announcement_count = official_announcements.len();

        let prompt_input = DailyReviewPromptInput {
            review_day: ctx.review_day.clone(),
            web_search: settings.web_search,
            portfolio: ctx.portfolio.clone(),
            assets: ctx.tracked_assets.iter().take(25).cloned().collect(),
            announcements: official_announcements
                .iter()
                .take(15)
                .map(|(_, ann)| ReviewAnnouncementBrief {
                    symbol: ann.symbol.clone(),
                    stock_name: ann.stock_name.clone(),
                    title: ann.title.clone(),
                    category: ann.category.clone(),
                    published_on: ann.published_on.clone(),
                    source_name: ann.source_name.clone(),
                    source_url: ann.source_url.clone(),
                    importance: format!("{:?}", ann.importance),
                })
                .collect(),
            dividend_plans: recent_plans
                .iter()
                .take(10)
                .map(|(asset, plan)| ReviewDividendPlanBrief {
                    symbol: plan.symbol.clone(),
                    stock_name: asset.name.clone(),
                    report_period: plan.report_date.to_string(),
                    stage: format!("{:?}", plan.stage),
                    cash_per_10: plan.cash_per_ten.clone(),
                    ex_dividend_date: plan.ex_date.map(|d| d.to_string()),
                    announced_on: plan.announced_on.to_string(),
                })
                .collect(),
            watch_signals: ctx.watch_signals.iter().take(10).cloned().collect(),
            active_rules: ctx.active_rules.clone(),
            methodology_prompt: ctx.methodology_prompt.clone(),
        };

        let (ai_status, ai_model, degraded_reason, ai_posts) =
            self.run_ai_stage(&prompt_input, executor).await;

        for (index, post) in ai_posts.into_iter().enumerate() {
            let origin = if matches!(post.kind, DailyReviewItemKind::ExternalClue) {
                DailyReviewItemOrigin::WebClue
            } else {
                DailyReviewItemOrigin::AiReview
            };
            items.push(DailyReviewFeedItem {
                id: format!("ai:{}:{index}", ctx.review_day),
                run_id: run_id.clone(),
                review_day: ctx.review_day.clone(),
                kind: post.kind,
                origin,
                importance: post.importance,
                title: post.title,
                summary: post.summary,
                body: post.body,
                symbol: post.symbol,
                asset_name: post.asset_name,
                metrics: Vec::new(),
                sources: post.sources,
                tags: post.tags,
                ask_prompt: post.ask_prompt,
                read_at: None,
                created_at: created_at.clone(),
            });
        }

        let status = if matches!(ai_status, DailyReviewAiStatus::Completed) {
            DailyReviewRunStatus::Completed
        } else {
            DailyReviewRunStatus::Degraded
        };

        let run = DailyReviewRun {
            id: run_id,
            review_day: ctx.review_day.clone(),
            trigger,
            status,
            ai_status,
            ai_model,
            degraded_reason,
            item_count: items.len(),
            announcement_count,
            created_at,
        };

        self.db.save_daily_review_run(&run, &items)?;
        let feed = self.feed(&DailyReviewFeedQuery::default())?;
        Ok(DailyReviewRunResult { run, feed })
    }

    async fn run_ai_stage(
        &self,
        prompt_input: &DailyReviewPromptInput,
        executor: &dyn AnalysisExecutor,
    ) -> (
        DailyReviewAiStatus,
        Option<String>,
        Option<String>,
        Vec<crate::models::daily_review::AiReviewPostDraft>,
    ) {
        let config = match configuration(self.db, self.server) {
            Ok(config) => config,
            Err(error) => {
                return (
                    DailyReviewAiStatus::SkippedNoModel,
                    None,
                    Some(format!(
                        "尚未配置可用的 AI 模型（{error}），本次已保留昨日持仓事实与官方公告动态。"
                    )),
                    Vec::new(),
                );
            }
        };
        let api_key = match frozen_credentials(self.db, self.server, &config) {
            Ok(key) => key,
            Err(error) => {
                return (
                    DailyReviewAiStatus::SkippedNoModel,
                    Some(config.model),
                    Some(format!(
                        "模型凭据暂不可用（{error}），本次已保留昨日持仓事实与官方公告动态。"
                    )),
                    Vec::new(),
                );
            }
        };
        let model_name = config.model.clone();
        let execution = self
            .db
            .without_account_lock(tokio::time::timeout(
                DAILY_REVIEW_TIMEOUT,
                executor.daily_review(config, api_key, prompt_input.clone()),
            ))
            .await;

        match execution {
            Ok(Ok(Ok(outcome))) => {
                let reason = if outcome.warnings.is_empty() {
                    None
                } else {
                    Some(outcome.warnings.join("；"))
                };
                if outcome.posts.is_empty() {
                    (
                        DailyReviewAiStatus::Empty,
                        Some(outcome.model_name),
                        Some("AI 未生成额外复盘条目，已保留昨日持仓与官方公告动态。".into()),
                        Vec::new(),
                    )
                } else {
                    (
                        DailyReviewAiStatus::Completed,
                        Some(outcome.model_name),
                        reason,
                        outcome.posts,
                    )
                }
            }
            Ok(Ok(Err(error))) => (
                DailyReviewAiStatus::Failed,
                Some(model_name),
                Some(format!(
                    "AI 每日复盘生成失败（{error}），已保留昨日持仓事实与官方公告动态。"
                )),
                Vec::new(),
            ),
            Ok(Err(_elapsed)) => (
                DailyReviewAiStatus::Failed,
                Some(model_name),
                Some(DAILY_REVIEW_TIMEOUT_MESSAGE.into()),
                Vec::new(),
            ),
            Err(lock_error) => (
                DailyReviewAiStatus::Failed,
                Some(model_name),
                Some(format!("复盘执行协调失败：{lock_error}")),
                Vec::new(),
            ),
        }
    }
}

fn build_deterministic_items(
    run_id: &str,
    created_at: &str,
    ctx: &DailyReviewUserContext,
    official_announcements: &[(
        ReviewTrackedAsset,
        crate::models::market::announcements::OfficialAnnouncement,
    )],
    recent_plans: &[(
        ReviewTrackedAsset,
        crate::models::market::dividend_plans::DividendPlan,
    )],
) -> Vec<DailyReviewFeedItem> {
    let mut items = Vec::new();

    // 1. Portfolio daily wrap-up card
    if let Some(summary) = &ctx.portfolio {
        let mut metrics = Vec::new();
        if let Some(total) = summary.total_assets {
            metrics.push(DailyReviewMetric {
                label: "总资产".into(),
                value: format!("{} {:.2}", summary.base_currency, total),
                tone: MetricTone::Neutral,
                sensitive: true,
            });
        }
        if let Some(change) = summary.previous_day_change {
            let sign = if change > 0.0 { "+" } else { "" };
            let pct_text = summary
                .previous_day_change_pct
                .map(|p| format!(" ({sign}{p:.2}%)"))
                .unwrap_or_default();
            metrics.push(DailyReviewMetric {
                label: "单日变动".into(),
                value: format!("{sign}{change:.2} {}{pct_text}", summary.base_currency),
                tone: if change > 0.0 {
                    MetricTone::Positive
                } else if change < 0.0 {
                    MetricTone::Negative
                } else {
                    MetricTone::Neutral
                },
                sensitive: true,
            });
        }
        if let Some(net) = summary.net_assets {
            metrics.push(DailyReviewMetric {
                label: "净资产".into(),
                value: format!("{} {:.2}", summary.base_currency, net),
                tone: MetricTone::Neutral,
                sensitive: true,
            });
        }
        let headline = match (summary.previous_day_change, summary.previous_day_change_pct) {
            (Some(delta), Some(pct)) if delta > 0.0 => {
                format!("{} 组合收盘小计：上涨 +{pct:.2}%", ctx.review_day)
            }
            (Some(delta), Some(pct)) if delta < 0.0 => {
                format!("{} 组合收盘小计：回落 {pct:.2}%", ctx.review_day)
            }
            (Some(_), _) => format!("{} 组合收盘小计：整体持平", ctx.review_day),
            (None, _) => format!("{} 组合估值状态汇总", ctx.review_day),
        };
        let mut lines = Vec::new();
        if !summary.missing_fx.is_empty() {
            lines.push(format!(
                "汇率缺失提醒：{} 缺少可用汇率，总资产暂未纳入合并计算。",
                summary.missing_fx.join("、")
            ));
        } else if summary.carried {
            lines.push("当日无新的行情或账本确认记录，沿用前一有效估值口径。".into());
        } else {
            lines.push(format!(
                "按 {} 收盘与最新确认估值汇总，已同步纳入昨日复盘基线。",
                ctx.review_day
            ));
        }
        let movers: Vec<String> = ctx
            .tracked_assets
            .iter()
            .filter(|a| a.in_portfolio && a.day_change_pct.is_some())
            .take(5)
            .map(|a| {
                let pct = a.day_change_pct.unwrap_or(0.0);
                let sign = if pct > 0.0 { "+" } else { "" };
                format!("{} {sign}{pct:.2}%", a.name)
            })
            .collect();
        if !movers.is_empty() {
            lines.push(format!("主要持仓表现：{}", movers.join(" · ")));
        }
        let body = lines.join("\n");
        items.push(DailyReviewFeedItem {
            id: format!("fact:portfolio:{}", ctx.review_day),
            run_id: run_id.into(),
            review_day: ctx.review_day.clone(),
            kind: DailyReviewItemKind::PortfolioSummary,
            origin: DailyReviewItemOrigin::DeterministicFact,
            importance: DailyReviewImportance::High,
            title: headline,
            summary: lines.first().cloned().unwrap_or_default(),
            body,
            symbol: None,
            asset_name: None,
            metrics,
            sources: Vec::new(),
            tags: vec!["昨日盘面".into(), "账本事实".into()],
            ask_prompt: format!(
                "请结合我 {} 的持仓变化、仓位分布和投资规则，做一次系统复盘并指出最需要关注的风险点。",
                ctx.review_day
            ),
            read_at: None,
            created_at: created_at.into(),
        });
    }

    // 2. Notable holding moves (>= 2% daily move or added/removed)
    let mut moved_assets: Vec<&ReviewTrackedAsset> = ctx
        .tracked_assets
        .iter()
        .filter(|a| {
            a.in_portfolio
                && (a.day_change_pct.is_some_and(|p| p.abs() >= 2.0)
                    || matches!(a.change_kind.as_str(), "added" | "removed"))
        })
        .collect();
    moved_assets.sort_by(|a, b| {
        b.day_change_pct
            .unwrap_or(0.0)
            .abs()
            .total_cmp(&a.day_change_pct.unwrap_or(0.0).abs())
    });
    for asset in moved_assets.into_iter().take(5) {
        let pct = asset.day_change_pct.unwrap_or(0.0);
        let sign = if pct > 0.0 { "+" } else { "" };
        let title = match asset.change_kind.as_str() {
            "added" => format!("新增持仓：{}", asset.name),
            "removed" => format!("已清仓/移出：{}", asset.name),
            _ => format!("{} 单日波动 {sign}{pct:.2}%", asset.name),
        };
        let mut metrics = Vec::new();
        if let Some(p) = asset.day_change_pct {
            metrics.push(DailyReviewMetric {
                label: "单日涨跌".into(),
                value: format!("{}{p:.2}%", if p > 0.0 { "+" } else { "" }),
                tone: if p > 0.0 {
                    MetricTone::Positive
                } else if p < 0.0 {
                    MetricTone::Negative
                } else {
                    MetricTone::Neutral
                },
                sensitive: false,
            });
        }
        if let Some(delta) = asset.day_change_amount {
            metrics.push(DailyReviewMetric {
                label: "估值变动".into(),
                value: format!("{}{delta:.2}", if delta > 0.0 { "+" } else { "" }),
                tone: if delta > 0.0 {
                    MetricTone::Positive
                } else if delta < 0.0 {
                    MetricTone::Negative
                } else {
                    MetricTone::Neutral
                },
                sensitive: true,
            });
        }
        if let Some(weight) = asset.weight_pct {
            metrics.push(DailyReviewMetric {
                label: "当前仓位".into(),
                value: format!("{weight:.1}%"),
                tone: MetricTone::Neutral,
                sensitive: false,
            });
        }
        let summary = format!(
            "{}（{}）在 {} 估值日记录显著变动，当前组合占比 {}。",
            asset.name,
            if asset.symbol.is_empty() {
                asset.asset_class.as_str()
            } else {
                asset.symbol.as_str()
            },
            ctx.review_day,
            asset
                .weight_pct
                .map(|w| format!("{w:.1}%"))
                .unwrap_or_else(|| "未知".into())
        );
        items.push(DailyReviewFeedItem {
            id: format!(
                "fact:move:{}:{}",
                ctx.review_day,
                if asset.symbol.is_empty() {
                    asset.name.as_str()
                } else {
                    asset.symbol.as_str()
                }
            ),
            run_id: run_id.into(),
            review_day: ctx.review_day.clone(),
            kind: DailyReviewItemKind::HoldingMove,
            origin: DailyReviewItemOrigin::DeterministicFact,
            importance: if pct.abs() >= 4.0 {
                DailyReviewImportance::High
            } else {
                DailyReviewImportance::Medium
            },
            title,
            summary: summary.clone(),
            body: summary,
            symbol: (!asset.symbol.is_empty()).then(|| asset.symbol.clone()),
            asset_name: Some(asset.name.clone()),
            metrics,
            sources: Vec::new(),
            tags: vec!["持仓异动".into(), asset.asset_class.clone()],
            ask_prompt: format!(
                "请分析 {}（{}）在 {} 的波动原因，核对近期公告与财报基本面，并结合我的仓位和投资规则给出观察建议。",
                asset.name, asset.symbol, ctx.review_day
            ),
            read_at: None,
            created_at: created_at.into(),
        });
    }

    // 3. Statutory official announcements (CNINFO)
    let mut anns: Vec<_> = official_announcements.iter().collect();
    anns.sort_by(|a, b| {
        let rank = |imp: AnnouncementImportance| match imp {
            AnnouncementImportance::High => 0,
            AnnouncementImportance::Medium => 1,
            AnnouncementImportance::Normal => 2,
        };
        rank(a.1.importance)
            .cmp(&rank(b.1.importance))
            .then_with(|| b.1.published_at.cmp(&a.1.published_at))
    });
    for (asset, ann) in anns.into_iter().take(12) {
        let importance = match ann.importance {
            AnnouncementImportance::High => DailyReviewImportance::High,
            AnnouncementImportance::Medium => DailyReviewImportance::Medium,
            AnnouncementImportance::Normal => DailyReviewImportance::Normal,
        };
        let relation = if asset.in_portfolio {
            "持仓标的"
        } else {
            "自选标的"
        };
        let summary = format!(
            "{relation} {}（{}）于 {} 通过{}发布「{}」公告。",
            ann.stock_name, ann.symbol, ann.published_on, ann.source_name, ann.category
        );
        items.push(DailyReviewFeedItem {
            id: format!("filing:{}:{}", ctx.review_day, ann.id),
            run_id: run_id.into(),
            review_day: ctx.review_day.clone(),
            kind: DailyReviewItemKind::OfficialAnnouncement,
            origin: DailyReviewItemOrigin::OfficialFiling,
            importance,
            title: format!("{}：{}", ann.stock_name, ann.title),
            summary: summary.clone(),
            body: format!("{summary}\n公告全文见官方披露链接。"),
            symbol: Some(ann.symbol.clone()),
            asset_name: Some(ann.stock_name.clone()),
            metrics: Vec::new(),
            sources: vec![DailyReviewSource {
                label: ann.title.clone(),
                url: ann.source_url.clone(),
                publisher: ann.source_name.clone(),
                published_on: ann.published_on.clone(),
            }],
            tags: vec![relation.into(), ann.category.clone(), ann.source_name.clone()],
            ask_prompt: format!(
                "请解读 {}（{}）于 {} 发布的官方公告《{}》，说明对公司基本面、分红或估值的实质影响，并对照我的投资原则评估。",
                ann.stock_name, ann.symbol, ann.published_on, ann.title
            ),
            read_at: None,
            created_at: ann.published_at.clone(),
        });
    }

    // 4. Dividend plan updates / upcoming ex-dividend dates
    for (asset, plan) in recent_plans.iter().take(6) {
        let stage_label = match plan.stage {
            PlanStage::Preannounced => "预披露",
            PlanStage::Proposed => "董事会预案",
            PlanStage::Approved => "股东大会通过",
            PlanStage::Scheduled => "即将实施分配",
            PlanStage::Cancelled => "已取消",
            PlanStage::NoDistribution => "不分配",
            PlanStage::Unknown => "分红动态",
        };
        let cash_text = plan
            .cash_per_ten
            .as_deref()
            .map(|c| format!("每 10 股派 {c} 元（税前）"))
            .unwrap_or_else(|| "金额待披露".into());
        let ex_text = plan
            .ex_date
            .map(|d| format!("，除权除息日 {d}"))
            .unwrap_or_default();
        let report_period = plan.report_date.to_string();
        let summary = format!(
            "{}（{}）{report_period} 分红计划处于「{stage_label}」阶段：{cash_text}{ex_text}。",
            asset.name, plan.symbol
        );
        items.push(DailyReviewFeedItem {
            id: format!("dividend:{}:{}:{report_period}", ctx.review_day, plan.symbol),
            run_id: run_id.into(),
            review_day: ctx.review_day.clone(),
            kind: DailyReviewItemKind::DividendUpdate,
            origin: DailyReviewItemOrigin::OfficialFiling,
            importance: if matches!(plan.stage, PlanStage::Scheduled) {
                DailyReviewImportance::High
            } else {
                DailyReviewImportance::Medium
            },
            title: format!("{} 分红动态：{stage_label}", asset.name),
            summary: summary.clone(),
            body: summary,
            symbol: Some(plan.symbol.clone()),
            asset_name: Some(asset.name.clone()),
            metrics: vec![DailyReviewMetric {
                label: "方案".into(),
                value: cash_text,
                tone: MetricTone::Positive,
                sensitive: false,
            }],
            sources: plan
                .sources
                .iter()
                .take(2)
                .map(|url| DailyReviewSource {
                    label: format!("{} 分红方案数据源", asset.name),
                    url: url.clone(),
                    publisher: "公开分红计划".into(),
                    published_on: plan.announced_on.to_string(),
                })
                .collect(),
            tags: vec!["分红计划".into(), stage_label.into()],
            ask_prompt: format!(
                "请核对 {}（{}）的 {report_period} 分红方案（{stage_label}），结合历史派息率与现金流评估分红可持续性。",
                asset.name, plan.symbol
            ),
            read_at: None,
            created_at: created_at.into(),
        });
    }

    // 5. Watch signal alerts
    for signal in ctx.watch_signals.iter().filter(|s| s.alert).take(6) {
        items.push(DailyReviewFeedItem {
            id: format!("watch:{}:{}", ctx.review_day, signal.symbol),
            run_id: run_id.into(),
            review_day: ctx.review_day.clone(),
            kind: DailyReviewItemKind::WatchSignal,
            origin: DailyReviewItemOrigin::DeterministicFact,
            importance: DailyReviewImportance::High,
            title: format!("买点观察触发：{}（{}）", signal.name, signal.symbol),
            summary: signal.summary.clone(),
            body: format!("观测日期 {}：{}。观察信号仅用于研究提醒，不构成自动交易决定。", signal.day, signal.summary),
            symbol: Some(signal.symbol.clone()),
            asset_name: Some(signal.name.clone()),
            metrics: Vec::new(),
            sources: Vec::new(),
            tags: vec!["买点观察".into(), "信号触发".into()],
            ask_prompt: format!(
                "{}（{}）在 {} 触发了我的买点观察条件（{}），请帮我评估当前是否具备介入价值以及主要证伪风险。",
                signal.name, signal.symbol, signal.day, signal.summary
            ),
            read_at: None,
            created_at: created_at.into(),
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{
        connection::ModelConfig,
        daily_review::{AiReviewOutcome, AiReviewPostDraft},
        market::{
            announcements::{AnnouncementSnapshot, OfficialAnnouncement},
            dividend_plans::PlanSnapshot,
        },
        portfolio::HoldingInput,
        provider::ModelProviderKind,
    };
    use chrono::TimeZone;

    struct FakeAnnouncements {
        snap: AnnouncementSnapshot,
    }
    impl AnnouncementReader for FakeAnnouncements {
        fn snapshot(&self, symbol: &str) -> AppResult<AnnouncementSnapshot> {
            if symbol == "600519.SS" {
                Ok(self.snap.clone())
            } else {
                Ok(AnnouncementSnapshot::default())
            }
        }
    }

    struct EmptyPlans;
    impl DividendPlanReader for EmptyPlans {
        fn snapshot(&self, _symbol: &str) -> AppResult<PlanSnapshot> {
            Ok(PlanSnapshot::default())
        }
    }

    struct FakeExecutor {
        fail: bool,
    }
    #[async_trait::async_trait]
    impl AnalysisExecutor for FakeExecutor {
        async fn execute(
            &self,
            _: ModelConfig,
            _: Option<String>,
            _: crate::models::analysis::AnalysisRequest,
            _: crate::context::BuiltContext,
        ) -> AppResult<crate::models::analysis::AnalysisResult> {
            unreachable!()
        }
        async fn daily_review(
            &self,
            _config: ModelConfig,
            _api_key: Option<String>,
            input: DailyReviewPromptInput,
        ) -> AppResult<AiReviewOutcome> {
            if self.fail {
                return Err(AppError::Model("model down".into()));
            }
            assert_eq!(input.review_day, "2026-11-20");
            assert_eq!(input.announcements.len(), 1);
            Ok(AiReviewOutcome {
                posts: vec![AiReviewPostDraft {
                    kind: DailyReviewItemKind::AiInsight,
                    importance: DailyReviewImportance::High,
                    title: "茅台分红公告与仓位回顾".into(),
                    summary: "中期利润分配公告兑现，符合红利策略要求。".into(),
                    body: "详细分析...".into(),
                    symbol: Some("600519.SS".into()),
                    asset_name: Some("贵州茅台".into()),
                    tags: vec!["AI复盘".into()],
                    ask_prompt: "请进一步拆解茅台派息率变化".into(),
                    sources: Vec::new(),
                }],
                model_name: "fixture-model".into(),
                warnings: Vec::new(),
                web_sources_used: 0,
            })
        }
    }

    fn seed_db_with_holding() -> Database {
        crate::secrets::use_test_master_key();
        let db = Database::test();
        db.ensure_daily(&crate::models::daily::EnsureInput {
            timezone: "Asia/Shanghai".into(),
        })
        .unwrap();
        let input: HoldingInput = serde_json::from_value(serde_json::json!({
            "symbol": "600519.SS",
            "name": "贵州茅台",
            "assetClass": "股票",
            "marketValue": 150000.0,
            "costBasis": 140000.0,
            "targetPct": 20.0,
            "currency": "CNY",
            "fxRateToBase": null,
            "valuationDate": "2026-11-20",
            "fxRateSource": "",
            "fxRateObservedOn": ""
        }))
        .unwrap();
        db.add_holding(&input).unwrap();
        db.save_model_configuration(
            ModelProviderKind::OpenaiResponses,
            "https://example.test/v1",
            "fixture-model",
            Some("sk-test"),
        )
        .unwrap();
        db
    }

    fn sample_announcements() -> FakeAnnouncements {
        FakeAnnouncements {
            snap: AnnouncementSnapshot {
                announcements: vec![OfficialAnnouncement {
                    id: "cninfo:888".into(),
                    symbol: "600519.SS".into(),
                    stock_name: "贵州茅台".into(),
                    title: "关于2026年中期利润分配方案的公告".into(),
                    category: "分红派息".into(),
                    published_on: "2026-11-20".into(),
                    published_at: "2026-11-20T10:30:00+00:00".into(),
                    source_name: "巨潮资讯网".into(),
                    source_url: "https://static.cninfo.com.cn/finalpage/888.PDF".into(),
                    importance: AnnouncementImportance::High,
                }],
                attempted_at: Some("2026-11-21T00:00:00+00:00".into()),
                fetched_at: Some("2026-11-21T00:00:00+00:00".into()),
                refresh_failed: false,
            },
        }
    }

    #[tokio::test]
    async fn scheduled_run_respects_time_generates_mixed_feed_and_runs_once_per_day() {
        let db = seed_db_with_holding();
        let service = DailyReviewService::new(&db, None);
        let ann = sample_announcements();
        let exec = FakeExecutor { fail: false };

        // 07:10 Shanghai time is before default 07:30 schedule -> should not run.
        let before = chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2026, 11, 21, 7, 10, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(service
            .run_if_due(before, &ann, &EmptyPlans, &exec)
            .await
            .unwrap()
            .is_none());

        // 07:35 Shanghai time -> triggers review for yesterday (2026-11-20).
        let after = chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2026, 11, 21, 7, 35, 0)
            .unwrap()
            .with_timezone(&Utc);
        let run = service
            .run_if_due(after, &ann, &EmptyPlans, &exec)
            .await
            .unwrap()
            .expect("should run once after 07:30");
        assert_eq!(run.review_day, "2026-11-20");
        assert_eq!(run.status, DailyReviewRunStatus::Completed);
        assert_eq!(run.announcement_count, 1);

        // Second check on the same day does not duplicate.
        assert!(service
            .run_if_due(after, &ann, &EmptyPlans, &exec)
            .await
            .unwrap()
            .is_none());

        let feed = service.feed(&DailyReviewFeedQuery::default()).unwrap();
        assert!(feed
            .items
            .iter()
            .any(|i| i.kind == DailyReviewItemKind::OfficialAnnouncement));
        assert!(feed
            .items
            .iter()
            .any(|i| i.kind == DailyReviewItemKind::AiInsight));
    }

    #[tokio::test]
    async fn degrades_gracefully_when_ai_fails_while_keeping_official_filings() {
        let db = seed_db_with_holding();
        let service = DailyReviewService::new(&db, None);
        let ann = sample_announcements();
        let exec = FakeExecutor { fail: true };

        let result = service
            .run(
                Some(NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()),
                DailyReviewTrigger::Manual,
                &ann,
                &EmptyPlans,
                &exec,
            )
            .await
            .unwrap();
        assert_eq!(result.run.status, DailyReviewRunStatus::Degraded);
        assert_eq!(result.run.ai_status, DailyReviewAiStatus::Failed);
        assert!(result.run.degraded_reason.is_some());
        assert!(result
            .feed
            .items
            .iter()
            .any(|i| i.kind == DailyReviewItemKind::OfficialAnnouncement));
    }

    #[tokio::test]
    async fn scheduled_run_skips_brand_new_empty_accounts_without_mutating_revision() {
        let db = Database::test();
        let before_rev = db.revision().unwrap();
        let service = DailyReviewService::new(&db, None);
        let ann = sample_announcements();
        let exec = FakeExecutor { fail: false };
        let after = chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2026, 11, 21, 9, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(service
            .run_if_due(after, &ann, &EmptyPlans, &exec)
            .await
            .unwrap()
            .is_none());
        assert_eq!(db.revision().unwrap(), before_rev);
    }
}
