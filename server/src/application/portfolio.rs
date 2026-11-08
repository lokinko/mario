//! Portfolio workflows coordinate repositories and replaceable market providers.
use super::{fx, prices};
use crate::models::market::{FxBook, FxOverrideInput};
use crate::{
    db::Database,
    models,
    models::{FxRateQuery, HoldingInput, SecurityPriceQuery, SecurityPriceQuote, Snapshot},
    ports::market::{FxRateProvider, SecurityPriceProvider},
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
    pub fn for_account<'a>(&'a self, db: &'a Database) -> PortfolioService<'a> {
        PortfolioService { db, market: self }
    }
}
pub(crate) struct PortfolioService<'a> {
    db: &'a Database,
    market: &'a MarketServices,
}
impl PortfolioService<'_> {
    pub async fn refresh_fx(&self, force: bool) -> AppResult<FxBook> {
        fx::refresh(self.db, self.market.fx.as_ref(), force).await
    }
    pub async fn refresh_prices(&self) -> AppResult<prices::RefreshResult> {
        prices::refresh(self.db, self.market.prices.as_ref()).await
    }
    pub async fn refresh_closing_prices(&self) -> AppResult<prices::RefreshResult> {
        prices::refresh_after_close(self.db, self.market.prices.as_ref(), chrono::Utc::now()).await
    }
    pub async fn fx_quote(&self, query: &FxRateQuery) -> AppResult<crate::models::FxRateQuote> {
        fx::historical_quote(self.db, self.market.fx.as_ref(), query).await
    }
    pub fn snapshot(&self) -> AppResult<Snapshot> {
        prices::apply_cached(self.db, self.market.prices.as_ref())
    }
    pub async fn security_quote(
        &self,
        query: &SecurityPriceQuery,
    ) -> AppResult<SecurityPriceQuote> {
        self.market.prices.quote(query).await
    }
    /// The caller holds one account lock; only the frozen provider query runs unlocked.
    pub async fn preview_security_quote(
        &self,
        query: &SecurityPriceQuery,
    ) -> AppResult<SecurityPriceQuote> {
        self.db
            .without_account_lock(self.market.prices.quote(query))
            .await?
    }
    pub async fn verify_valuation(
        &self,
        id: &str,
        input: crate::models::VerifiedHoldingValuationInput,
    ) -> AppResult<Snapshot> {
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
        let quote = self.security_quote(&query).await?;
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
        self.refresh_fx(false).await?;
        let input = prices::complete_input(input, self.market.prices.as_ref()).await?;
        if let Some(id) = id {
            self.db.update_holding(id, &input)?;
        } else {
            self.db.add_holding(&input)?;
        }
        self.snapshot()
    }
    pub async fn ensure_daily(
        &self,
        input: &models::daily::EnsureInput,
    ) -> AppResult<models::daily::DailyHistory> {
        self.db.ensure_daily(input)?;
        self.db
            .daily_history(&models::daily::HistoryQuery::default())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::HoldingInput;
    use crate::{application::fx::refresh, models::market::FxOverrideInput, models::FxRateQuote};
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
#[path = "portfolio_quote_tests.rs"]
mod quote_preview_tests;
