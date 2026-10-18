//! Portfolio workflows coordinate repositories and replaceable market providers.
use super::{fx, prices};
use crate::models::market::{FxBook, FxOverrideInput};
use crate::{
    db::Database,
    models,
    models::{
        FxRateQuery, HoldingInput, SecurityPriceConfig, SecurityPriceQuery, SecurityPriceQuote,
        Snapshot,
    },
    ports::market::{FxRateProvider, SecurityPriceProvider},
    AppResult,
};
use std::sync::Arc;
pub(crate) struct MarketServices {
    fx: Arc<dyn FxRateProvider>,
    prices: Arc<dyn SecurityPriceProvider>,
    keyed_prices: Arc<dyn SecurityPriceProvider>,
}
impl MarketServices {
    pub fn new(
        fx: Arc<dyn FxRateProvider>,
        prices: Arc<dyn SecurityPriceProvider>,
        keyed_prices: Arc<dyn SecurityPriceProvider>,
    ) -> Self {
        Self {
            fx,
            prices,
            keyed_prices,
        }
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
    pub async fn fx_quote(&self, query: &FxRateQuery) -> AppResult<crate::models::FxRateQuote> {
        fx::historical_quote(self.db, self.market.fx.as_ref(), query).await
    }
    pub async fn security_quote(
        &self,
        query: &SecurityPriceQuery,
    ) -> AppResult<SecurityPriceQuote> {
        if let Ok(key) = self.db.secret("security-price") {
            self.market.keyed_prices.quote(query, &key).await
        } else {
            self.market.prices.quote(query, "").await
        }
    }
    pub fn security_config(&self) -> SecurityPriceConfig {
        let has_api_key = self.db.secret("security-price").is_ok();
        SecurityPriceConfig {
            provider: if has_api_key {
                "twelve-data"
            } else {
                "yahoo-finance"
            }
            .into(),
            has_api_key,
        }
    }
    pub fn save_security_config(&self, key: Option<&str>) -> AppResult<SecurityPriceConfig> {
        if let Some(key) = key {
            self.db.save_secret("security-price", key)?;
        }
        Ok(self.security_config())
    }
    pub fn delete_security_key(&self) -> AppResult<SecurityPriceConfig> {
        self.db.remove_secret("security-price")?;
        Ok(self.security_config())
    }
    pub async fn verify_valuation(
        &self,
        id: &str,
        input: crate::models::VerifiedHoldingValuationInput,
    ) -> AppResult<Snapshot> {
        let snapshot = self.db.snapshot()?;
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
        if query.action_from.is_some() && quote.provider_code != "yahoo_daily_close" {
            let actions = self.market.prices.quote(&query, "").await?;
            if self.db.register_corporate_actions(id, &actions)? {
                return Err(crate::AppError::Validation(
                    "存在待确认公司行动，请先核对数量与成本调整".into(),
                ));
            }
            if actions
                .splits
                .iter()
                .any(|s| s.effective_on > quote.observed_on)
            {
                return Err(crate::AppError::Validation(
                    "未复权价格早于公司行动，请等待行动生效后的完整收盘价或人工核对估值".into(),
                ));
            }
        }
        self.db
            .apply_verified_holding_valuation(id, input.quantity, &quote)
    }
    pub async fn save_fx_override(&self, input: &FxOverrideInput) -> AppResult<FxBook> {
        // A manual value must still be saveable when the provider is unavailable.
        self.db.set_fx_override(input)?;
        self.refresh_fx(false).await
    }
    pub async fn save_holding(&self, id: Option<&str>, input: HoldingInput) -> AppResult<Snapshot> {
        self.refresh_fx(false).await?;
        let input = prices::complete_input(input, self.market.prices.as_ref()).await?;
        if let Some(id) = id {
            self.db.update_holding(id, &input)?;
        } else {
            self.db.add_holding(&input)?;
        }
        self.db.snapshot()
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
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
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
                "UPDATE daily_entries SET day=?1,id=?1 || ':' || entity_id",
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
