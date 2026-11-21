//! Persistence adapters for the narrow capabilities consumed by AI use cases.
use crate::{
    models::{
        accounting::analysis::AuthorizedFinancialContext,
        analysis::{AnalysisResult, AnalysisSettings, ContextSelection},
        connection::ModelConfig,
        daily::HistoryQuery,
        daily_review::{
            DailyReviewFeedItem, DailyReviewFeedQuery, DailyReviewPortfolioSummary, DailyReviewRun,
            DailyReviewScope, DailyReviewSettings, DailyReviewUserContext, ReviewRuleBrief,
            ReviewTrackedAsset, ReviewWatchSignal,
        },
        memory::{
            AcceptedOperation, DreamInput, DreamSourceAnalysis, DreamState, DreamSummary,
            MemoryNote,
        },
        methodology::InvestmentMethodology,
        provider::ModelProviderKind,
        research::ResearchEvidence,
        watch::WatchBoard,
    },
    ports::ai_account::{
        AnalysisAccount, DailyReviewAccount, MemoryAccount, ModelConfiguration, ModelSettingsStore,
        UserAnalysisSources,
    },
    AppResult,
};
use {super::settings::ai::AiSetting, crate::db::Database};

impl ModelConfiguration for Database {
    fn model_config(&self) -> AppResult<ModelConfig> {
        Database::model_config(self)
    }
    fn model_source(&self) -> AppResult<Option<String>> {
        self.ai_setting(AiSetting::Source)
    }
    fn selected_server_model(&self) -> AppResult<Option<String>> {
        self.ai_setting(AiSetting::ServerModel)
    }
    fn model_key_binding(&self) -> AppResult<Option<String>> {
        self.ai_setting(AiSetting::KeyBinding)
    }
    fn model_credential(&self) -> AppResult<String> {
        self.model_secret()
    }
    fn bridge_id(&self) -> AppResult<Option<String>> {
        self.ai_setting(AiSetting::Bridge)
    }
}
impl ModelSettingsStore for Database {
    fn revision(&self) -> AppResult<String> {
        Database::revision(self)
    }
    fn save_model_configuration(
        &self,
        provider: ModelProviderKind,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
    ) -> AppResult<()> {
        Database::save_model_configuration(self, provider, base_url, model, api_key)
    }
    fn save_server_model(&self, model: &str) -> AppResult<()> {
        Database::save_server_model(self, model)
    }
    fn set_bridge_id(&self, id: &str) -> AppResult<()> {
        self.set_ai_setting(AiSetting::Bridge, id)
    }
    fn clear_bridge_id(&self) -> AppResult<()> {
        self.delete_ai_setting(AiSetting::Bridge)
    }
    fn remove_model_credential(&self) -> AppResult<()> {
        self.remove_model_secret()
    }
}
impl UserAnalysisSources for Database {
    fn investment_methodology(&self) -> AppResult<InvestmentMethodology> {
        Database::investment_methodology(self)
    }
    fn financial_context(
        &self,
        selection: &ContextSelection,
    ) -> AppResult<AuthorizedFinancialContext> {
        Ok(AuthorizedFinancialContext::from_snapshot(
            &Database::snapshot(self)?,
            selection.include_profile,
            selection.include_holdings,
        ))
    }
    fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        Database::research_evidence(self)
    }
    fn watch_board(&self, day: chrono::NaiveDate) -> AppResult<WatchBoard> {
        Database::watch_board(self, day)
    }
}
impl AnalysisAccount for Database {
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::active_memory_notes(self)
    }
    fn automatic_research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        Database::automatic_research_evidence(self)
    }
    fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()> {
        Database::save_analysis(self, result, question)
    }
}
impl MemoryAccount for Database {
    fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        Database::analysis_settings(self)
    }
    fn undreamed_analyses(&self) -> AppResult<(Vec<DreamSourceAnalysis>, usize)> {
        Database::undreamed_analyses(self)
    }
    fn memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::memory_notes(self)
    }
    fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Database::active_memory_notes(self)
    }
    fn dream_state(&self) -> AppResult<DreamState> {
        Database::dream_state(self)
    }
    fn set_memory_status(&self, id: &str, active: bool) -> AppResult<MemoryNote> {
        Database::set_memory_status(self, id, active)
    }
    fn delete_memory(&self, id: &str) -> AppResult<()> {
        Database::delete_memory(self, id)
    }
    fn apply_dream(
        &self,
        input: &DreamInput,
        operations: &[AcceptedOperation],
        rejected: Vec<String>,
    ) -> AppResult<DreamSummary> {
        Database::apply_dream(self, input, operations, rejected)
    }
}

impl DailyReviewAccount for Database {
    fn daily_review_settings(&self) -> AppResult<DailyReviewSettings> {
        Database::daily_review_settings(self)
    }
    fn save_daily_review_settings(
        &self,
        settings: &DailyReviewSettings,
    ) -> AppResult<DailyReviewSettings> {
        Database::save_daily_review_settings(self, settings)
    }
    fn daily_review_timezone(&self) -> AppResult<String> {
        Ok(Database::daily_history(
            self,
            &HistoryQuery {
                limit: Some(1),
                ..Default::default()
            },
        )?
        .timezone)
    }
    fn daily_review_runs(&self, limit: usize) -> AppResult<Vec<DailyReviewRun>> {
        Database::daily_review_runs(self, limit)
    }
    fn has_daily_review_for_day(&self, review_day: &str) -> AppResult<bool> {
        Database::has_daily_review_for_day(self, review_day)
    }
    fn daily_review_unread_count(&self) -> AppResult<usize> {
        Database::daily_review_unread_count(self)
    }
    fn daily_review_items(
        &self,
        query: &DailyReviewFeedQuery,
    ) -> AppResult<(Vec<DailyReviewFeedItem>, bool, Option<String>)> {
        Database::daily_review_items(self, query)
    }
    fn save_daily_review_run(
        &self,
        run: &DailyReviewRun,
        items: &[DailyReviewFeedItem],
    ) -> AppResult<()> {
        Database::save_daily_review_run(self, run, items)
    }
    fn mark_daily_review_read(&self, id: Option<&str>, all: bool) -> AppResult<usize> {
        Database::mark_daily_review_read(self, id, all)
    }
    fn daily_review_context(
        &self,
        requested_day: Option<chrono::NaiveDate>,
        scope: DailyReviewScope,
    ) -> AppResult<DailyReviewUserContext> {
        use chrono::{Days, NaiveDate};
        use std::collections::BTreeMap;

        let history = Database::daily_history(
            self,
            &HistoryQuery {
                limit: Some(14),
                ..Default::default()
            },
        )?;
        let today = NaiveDate::parse_from_str(&history.today, "%Y-%m-%d").unwrap_or_else(|_| {
            chrono::Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive()
        });
        let review_date =
            requested_day.unwrap_or_else(|| today.checked_sub_days(Days::new(1)).unwrap_or(today));
        let review_day = review_date.to_string();

        let target_record = history
            .records
            .iter()
            .find(|r| r.day == review_day)
            .or_else(|| history.records.iter().find(|r| r.day <= review_day))
            .or_else(|| history.records.first());
        let prev_record =
            target_record.and_then(|target| history.records.iter().find(|r| r.day < target.day));

        let portfolio = target_record.map(|record| {
            let prev_total = prev_record
                .and_then(|p| (p.base_currency == record.base_currency).then_some(p.total_assets))
                .flatten()
                .filter(|v| *v > 0.0);
            let change_pct = record
                .previous_day_change
                .zip(prev_total)
                .map(|(delta, prev)| (delta / prev) * 100.0);
            DailyReviewPortfolioSummary {
                review_day: record.day.clone(),
                base_currency: record.base_currency.clone(),
                total_assets: record.total_assets,
                net_assets: record.net_assets,
                previous_day_change: record.previous_day_change,
                previous_day_change_pct: change_pct,
                carried: record.carried,
                missing_fx: record.missing_fx.clone(),
            }
        });

        let snapshot = Database::snapshot(self)?;
        let total_base = target_record
            .and_then(|r| r.total_assets)
            .filter(|v| *v > 0.0)
            .or_else(|| {
                (snapshot.valuation_status.comparable && snapshot.total_value > 0.0)
                    .then_some(snapshot.total_value)
            });

        let mut by_key = BTreeMap::<String, ReviewTrackedAsset>::new();

        if let Some(record) = target_record {
            for asset in &record.assets {
                if asset.holding.asset_class == "现金" && asset.holding.market_value <= 0.0 {
                    continue;
                }
                if asset.removed && asset.change_kind != "removed" {
                    continue;
                }
                let prev_value = prev_record
                    .and_then(|p| {
                        p.assets
                            .iter()
                            .find(|a| a.holding.id == asset.holding.id && !a.removed)
                    })
                    .and_then(|a| a.base_value)
                    .filter(|v| *v > 0.0);
                let day_change_pct = asset
                    .previous_day_change
                    .zip(prev_value)
                    .map(|(delta, prev)| (delta / prev) * 100.0);
                let weight_pct = asset
                    .base_value
                    .zip(total_base)
                    .map(|(v, t)| (v / t) * 100.0);
                let unrealized_pnl_pct = (asset.holding.cost_basis > 0.0).then(|| {
                    ((asset.holding.market_value - asset.holding.cost_basis)
                        / asset.holding.cost_basis)
                        * 100.0
                });
                let key = if !asset.holding.symbol.trim().is_empty() {
                    asset.holding.symbol.trim().to_ascii_uppercase()
                } else {
                    format!("id:{}", asset.holding.id)
                };
                by_key
                    .entry(key)
                    .and_modify(|existing| {
                        existing.in_portfolio = true;
                        if let (Some(a), Some(b)) = (existing.market_value, asset.base_value) {
                            existing.market_value = Some(a + b);
                        }
                        if let (Some(a), Some(b)) = (existing.weight_pct, weight_pct) {
                            existing.weight_pct = Some(a + b);
                        }
                        if let (Some(a), Some(b)) =
                            (existing.day_change_amount, asset.previous_day_change)
                        {
                            existing.day_change_amount = Some(a + b);
                        }
                    })
                    .or_insert(ReviewTrackedAsset {
                        symbol: asset.holding.symbol.clone(),
                        name: asset.holding.name.clone(),
                        asset_class: asset.holding.asset_class.clone(),
                        in_portfolio: true,
                        in_watchlist: false,
                        currency: asset.holding.currency.clone(),
                        market_value: asset.base_value.or(Some(asset.holding.market_value)),
                        weight_pct,
                        unrealized_pnl_pct,
                        day_change_amount: asset.previous_day_change,
                        day_change_pct,
                        pct_point_change: asset.pct_point_change,
                        change_kind: asset.change_kind.clone(),
                        valuation_date: asset.holding.valuation_date.clone(),
                    });
            }
        } else {
            for holding in &snapshot.holdings {
                if holding.market_value <= 0.0 && holding.quantity == Some(0.0) {
                    continue;
                }
                let key = if !holding.symbol.trim().is_empty() {
                    holding.symbol.trim().to_ascii_uppercase()
                } else {
                    format!("id:{}", holding.id)
                };
                let unrealized_pnl_pct = (holding.cost_basis > 0.0).then(|| {
                    ((holding.market_value - holding.cost_basis) / holding.cost_basis) * 100.0
                });
                by_key.entry(key).or_insert(ReviewTrackedAsset {
                    symbol: holding.symbol.clone(),
                    name: holding.name.clone(),
                    asset_class: holding.asset_class.clone(),
                    in_portfolio: true,
                    in_watchlist: false,
                    currency: holding.currency.clone(),
                    market_value: Some(holding.market_value),
                    weight_pct: total_base.map(|t| (holding.market_value / t) * 100.0),
                    unrealized_pnl_pct,
                    day_change_amount: None,
                    day_change_pct: None,
                    pct_point_change: None,
                    change_kind: "baseline".into(),
                    valuation_date: holding.valuation_date.clone(),
                });
            }
        }

        let mut watch_signals = Vec::new();
        if matches!(scope, DailyReviewScope::HoldingsAndWatch) {
            if let Ok(board) = Database::watch_board(self, review_date) {
                for row in board.rows {
                    let symbol = row.entry.input.symbol.clone();
                    let name = row.entry.input.name.clone();
                    by_key
                        .entry(symbol.to_ascii_uppercase())
                        .and_modify(|existing| existing.in_watchlist = true)
                        .or_insert(ReviewTrackedAsset {
                            symbol: symbol.clone(),
                            name: name.clone(),
                            asset_class: "股票".into(),
                            in_portfolio: false,
                            in_watchlist: true,
                            currency: "CNY".into(),
                            market_value: None,
                            weight_pct: None,
                            unrealized_pnl_pct: None,
                            day_change_amount: None,
                            day_change_pct: None,
                            pct_point_change: None,
                            change_kind: "watch".into(),
                            valuation_date: row
                                .latest
                                .as_ref()
                                .map(|o| o.observed_on.clone())
                                .unwrap_or_default(),
                        });
                    if let Some(obs) = row.latest {
                        let status_text = match obs.status {
                            crate::models::watch::WatchStatus::Attention => "满足买点观察条件",
                            crate::models::watch::WatchStatus::Waiting => "观察等待中",
                            crate::models::watch::WatchStatus::Incomplete => "数据待补齐",
                        };
                        let yield_text = obs
                            .yield_pct
                            .map(|y| format!("TTM股息率 {y:.2}%"))
                            .unwrap_or_else(|| "股息率待定".into());
                        let price_text = obs
                            .price
                            .map(|p| format!("收盘价 {p:.2}"))
                            .unwrap_or_default();
                        watch_signals.push(ReviewWatchSignal {
                            symbol,
                            name,
                            day: obs.observed_on,
                            alert: row.alert
                                || matches!(
                                    obs.status,
                                    crate::models::watch::WatchStatus::Attention
                                ),
                            summary: format!("{status_text}（{price_text}，{yield_text}）"),
                        });
                    }
                }
            }
        }

        let mut tracked_assets: Vec<ReviewTrackedAsset> = by_key.into_values().collect();
        tracked_assets.sort_by(|a, b| {
            b.in_portfolio
                .cmp(&a.in_portfolio)
                .then_with(|| {
                    b.weight_pct
                        .unwrap_or(0.0)
                        .total_cmp(&a.weight_pct.unwrap_or(0.0))
                })
                .then_with(|| a.symbol.cmp(&b.symbol))
        });

        let active_rules = Database::investment_rules(self)?
            .into_iter()
            .filter(|r| r.active)
            .take(15)
            .map(|r| ReviewRuleBrief {
                category: r.category,
                statement: r.statement,
                trigger: r.trigger,
            })
            .collect();
        let methodology_prompt = Database::investment_methodology(self)?.prompt;

        Ok(DailyReviewUserContext {
            timezone: history.timezone,
            today: history.today,
            review_day,
            portfolio,
            tracked_assets,
            watch_signals,
            active_rules,
            methodology_prompt,
        })
    }
}
