//! Portfolio workflows coordinate repositories and replaceable market providers.
use super::{fx, prices};
#[cfg(test)]
use crate::db::Database;
use crate::models::fx::{FxBook, FxOverrideInput};
use crate::{
    models::{
        market::quotes::{FxRateQuery, SecurityPriceQuery, SecurityPriceQuote},
        portfolio::{HoldingInput, Snapshot},
    },
    ports::{
        market::{FxRateProvider, SecurityPriceProvider},
        user_account::{PortfolioAccount, PortfolioState},
    },
    AppResult,
};
use std::sync::Arc;
pub(crate) struct MarketServices {
    fx: Arc<dyn FxRateProvider>,
    prices: Arc<dyn SecurityPriceProvider>,
}
impl MarketServices {
    pub fn new(fx: Arc<dyn FxRateProvider>, prices: Arc<dyn SecurityPriceProvider>) -> Self {
        Self { fx, prices }
    }
    pub fn for_account<'a, A: PortfolioAccount>(&'a self, db: &'a A) -> PortfolioService<'a, A> {
        PortfolioService { db, market: self }
    }
}
pub(crate) struct PortfolioService<'a, A: PortfolioAccount> {
    db: &'a A,
    market: &'a MarketServices,
}
impl<A: PortfolioAccount> PortfolioService<'_, A> {
    pub async fn refresh_fx(&self, force: bool) -> AppResult<FxBook> {
        fx::refresh(self.db, self.market.fx.as_ref(), force).await
    }
    pub async fn refresh_prices(&self) -> AppResult<prices::RefreshResult> {
        prices::refresh(self.db, self.market.prices.as_ref()).await
    }
    pub async fn refresh_closing_prices(&self) -> AppResult<prices::RefreshResult> {
        prices::refresh_after_close(self.db, self.market.prices.as_ref(), chrono::Utc::now()).await
    }
    pub async fn fx_quote(
        &self,
        query: &FxRateQuery,
    ) -> AppResult<crate::models::market::quotes::FxRateQuote> {
        fx::historical_quote(self.db, self.market.fx.as_ref(), query).await
    }
    pub fn snapshot(&self) -> AppResult<Snapshot> {
        prices::apply_cached(self.db, self.market.prices.as_ref())
    }
    /// The caller holds one account lock; only the frozen provider query runs unlocked.
    pub async fn preview_security_quote(
        &self,
        query: &SecurityPriceQuery,
    ) -> AppResult<SecurityPriceQuote> {
        let started = std::time::Instant::now();
        let result = self
            .db
            .without_account_lock(self.market.prices.quote(query))
            .await?;
        tracing::debug!(target: "mario::timing", stage = "supplier", kind = "security", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
        result
    }
    pub async fn verify_valuation(
        &self,
        id: &str,
        input: crate::models::portfolio::VerifiedHoldingValuationInput,
    ) -> AppResult<Snapshot> {
        let revision = self.db.revision()?;
        let snapshot = self.db.snapshot()?;
        if snapshot.option_contracts.contains_key(id) {
            return Err(crate::AppError::Validation(
                "请通过期权报价入口核对每单位权利金".into(),
            ));
        }
        let query = SecurityPriceQuery {
            action_from: snapshot
                .holding_trades
                .iter()
                .filter(|t| t.holding_id == id)
                .map(|t| t.occurred_on.clone())
                .min(),
            symbol: input.symbol,
            on_date: input.on_date,
        };
        let quote = self.preview_security_quote(&query).await?;
        self.db.require_revision(&revision)?;
        self.db
            .apply_verified_holding_valuation(id, input.quantity, &quote)
    }
    pub async fn save_fx_override(&self, input: &FxOverrideInput) -> AppResult<FxBook> {
        // A manual value must still be saveable when the provider is unavailable.
        self.db.set_fx_override(input)?;
        self.refresh_fx(false).await
    }
    pub async fn save_holding(&self, id: Option<&str>, input: HoldingInput) -> AppResult<Snapshot> {
        let option_holding = if let Some(id) = id {
            self.db.snapshot()?.option_contracts.contains_key(id)
        } else {
            false
        };
        if input.asset_class == "期权" || option_holding {
            return Err(crate::AppError::Validation(
                "期权资料请通过股票详情中的期权入口管理".into(),
            ));
        }
        if id.is_none() && input.asset_class != "现金" {
            return Err(crate::AppError::Validation(
                "新增投资持仓请通过记录交易完成".into(),
            ));
        }
        self.refresh_fx(false).await?;
        let revision = self.db.revision()?;
        let input = self
            .db
            .without_account_lock(prices::complete_input(input, self.market.prices.as_ref()))
            .await??;
        self.db.require_revision(&revision)?;
        if let Some(id) = id {
            self.db.update_holding(id, &input)?;
        } else {
            self.db.add_holding(&input)?;
        }
        self.snapshot()
    }
    pub async fn ensure_daily(
        &self,
        input: &crate::models::daily::EnsureInput,
    ) -> AppResult<crate::models::daily::DailyHistory> {
        self.db.ensure_daily(input)?;
        self.db
            .daily_history(&crate::models::daily::HistoryQuery::default())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::portfolio::HoldingInput;
    use crate::{
        application::fx::refresh,
        models::{fx::FxOverrideInput, market::quotes::FxRateQuote},
    };
    use chrono::Utc;
    struct Provider;
    #[async_trait::async_trait]
    impl FxRateProvider for Provider {
        async fn quote(&self, q: &FxRateQuery) -> AppResult<FxRateQuote> {
            Ok(FxRateQuote {
                exact_rate: None,
                from_currency: q.from_currency.clone(),
                to_currency: q.to_currency.clone(),
                rate: 7.0,
                requested_on: q.on_date.clone(),
                observed_on: q.on_date.clone(),
                staleness_days: 0,
                provider_code: "ecb_reference".into(),
                provider_name: "ECB".into(),
                source_url: String::new(),
                methodology_url: String::new(),
                disclaimer: String::new(),
            })
        }
    }
    fn holding(currency: &str) -> HoldingInput {
        serde_json::from_value(serde_json::json!({"name":"账户","symbol":"","assetClass":"现金","marketValue":100,"costBasis":0,"targetPct":0,"currency":currency,"valuationDate":"2026-01-01"})).unwrap()
    }
    #[tokio::test]
    async fn changing_today_rate_preserves_prior_daily_totals_and_balance_confirmation() {
        let db = Database::test();
        let _lock = db.lock_account().unwrap();
        refresh(&db, &Provider, false).await.unwrap();
        db.add_holding(&holding("USD")).unwrap();
        let yesterday = (Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive()
            - chrono::Duration::days(1))
        .to_string();
        {
            let c = db.conn().unwrap();
            c.execute(
                "UPDATE daily_entries SET day=$1,id=$1 || ':' || entity_id",
                [&yesterday],
            )
            .unwrap();
            c.execute("DELETE FROM daily_totals", []).unwrap();
        }
        db.set_fx_override(&FxOverrideInput {
            exact_rate: None,
            currency: "USD".into(),
            rate: Some(8.0),
        })
        .unwrap();
        let history = db
            .daily_history(&crate::models::daily::HistoryQuery::default())
            .unwrap();
        assert_eq!(history.records[0].total_assets, Some(800.0));
        assert_eq!(history.records[1].total_assets, Some(700.0));
        assert!(history.records[0].assets[0].carried);
        assert_eq!(
            history.records[0].assets[0].confirmed_on.as_deref(),
            Some("2026-01-01")
        );
    }
}

#[cfg(test)]
mod dividend_income_tests {
    use super::*;
    use crate::models::watch::{DividendBasis, DividendEvent, DividendHistory};
    use chrono::NaiveDate;

    struct Offline(Option<DividendHistory>);
    #[async_trait::async_trait]
    impl crate::ports::market::WatchMarketProvider for Offline {
        fn dividend_seed(&self, _: &str, _: NaiveDate) -> Option<DividendHistory> {
            self.0.clone()
        }
        async fn dividends(&self, _: &str, _: NaiveDate) -> AppResult<DividendHistory> {
            Err(crate::AppError::MarketData("offline".into()))
        }
    }
    fn account() -> Database {
        let db = Database::test();
        let stock = serde_json::json!([{"code":"600900","name":"测试电力","price":12,"shares":100,"costPrice":"10",
            "transactions":[{"type":"buy","qty":100,"price":10,"ts":1735689600000_i64}]}]);
        db.import_account_data(
            &serde_json::json!({"version":"1.0","exportedAt":"2026-01-06T01:00:00Z","data":{
            "watchlist":stock,"accounts":[{"id":"default","name":"测试账户","watchlist":stock,
            "feeConfig":{"enabled":false},"cashTrackingEnabled":false,"cashBalance":{"CNY":0}}]}}),
        )
        .unwrap();
        db
    }

    #[tokio::test]
    async fn failed_refresh_uses_the_stored_history_or_names_the_holding() {
        let db = account();
        let yesterday = super::super::dividend_calendar::today() - chrono::Duration::days(1);
        let stored = DividendHistory {
            basis: DividendBasis {
                as_of: yesterday,
                fetched_at: "2026-01-01T00:00:00Z".into(),
            },
            sources: vec![],
            events: vec![DividendEvent {
                report_date: None,
                ex_date: yesterday - chrono::Duration::days(10),
                cash_per_ten: "8".into(),
                bonus_per_ten: "0".into(),
                transfer_per_ten: "0".into(),
            }],
        };
        let income = dividend_income(&db, Arc::new(Offline(Some(stored))))
            .await
            .unwrap();
        assert_eq!(income.value, "80");
        assert!(income.missing.is_empty());
        let unknown = dividend_income(&db, Arc::new(Offline(None))).await.unwrap();
        assert_eq!(unknown.value, "0");
        assert_eq!(unknown.missing, vec!["测试电力".to_string()]);
    }
}

#[cfg(test)]
#[path = "portfolio_quote_tests.rs"]
mod quote_preview_tests;

/// Today's public ex-dividend history per A-share holding; a failed refresh
/// keeps the last stored history, whose past ex-dates remain valid.
pub(crate) async fn dividend_income(
    db: &impl PortfolioAccount,
    provider: Arc<dyn crate::ports::market::WatchMarketProvider>,
) -> AppResult<crate::models::accounting::DividendIncome> {
    let today = super::dividend_calendar::today();
    let symbols = db
        .snapshot()?
        .holdings
        .iter()
        .filter(|h| crate::models::accounting::has_public_dividends(h))
        .map(|h| h.symbol.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut tasks = tokio::task::JoinSet::new();
    for symbol in symbols {
        let provider = provider.clone();
        tasks.spawn(async move {
            let seed = provider.dividend_seed(&symbol, today);
            let history = match seed {
                Some(seed) if seed.basis.as_of == today => Some(seed),
                seed => match tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    provider.dividends(&symbol, today),
                )
                .await
                {
                    Ok(Ok(history)) => Some(history),
                    Ok(Err(error)) => {
                        tracing::warn!(symbol, %error, "holding dividend history refresh failed");
                        seed
                    }
                    Err(_) => {
                        tracing::warn!(symbol, "holding dividend history refresh timed out");
                        seed
                    }
                },
            };
            (symbol, history)
        });
    }
    let mut histories = std::collections::BTreeMap::new();
    while let Some(joined) = tasks.join_next().await {
        let (symbol, history) = joined.map_err(|e| crate::AppError::MarketData(e.to_string()))?;
        if let Some(history) = history {
            histories.insert(symbol, history);
        }
    }
    db.trailing_dividends(&histories, today)
}

/// Allocation joins account holdings with the public catalogue's industries.
pub(crate) fn allocation(
    db: &impl PortfolioState,
    catalog: &super::watch_catalog::WatchCatalogService,
) -> AppResult<crate::models::accounting::Allocation> {
    let snapshot = db.snapshot()?;
    let accounting = snapshot
        .accounting
        .as_ref()
        .ok_or_else(|| crate::AppError::Validation("缺少精确会计数据".into()))?;
    crate::models::accounting::allocation(
        &snapshot.holdings,
        accounting,
        &snapshot.profile.base_currency,
        &catalog.industries()?,
    )
}
