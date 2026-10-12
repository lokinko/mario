//! Portfolio workflows coordinate repositories and replaceable market providers.
use crate::{
    db::Database,
    event_import, fx,
    market_data::{
        EcbFxRateProvider, FxRateProvider, SecurityPriceProvider, TwelveDataSecurityPriceProvider,
    },
    models,
    models::{
        FinancialProfile, FxRateQuery, HoldingInput, HoldingTradeInput, HoldingTradeUpdateInput,
        PortfolioEventImportCommitRequest, PortfolioEventImportPreview,
        PortfolioEventImportRequest, PortfolioEventImportResult, PortfolioEventInput,
        PortfolioEventRecord, SecurityPriceConfig, SecurityPriceQuery, SecurityPriceQuote,
        Snapshot,
    },
    prices, AppResult,
};
use std::{collections::HashSet, sync::Arc};
pub(crate) struct MarketServices {
    fx: Arc<dyn FxRateProvider>,
    prices: Arc<dyn SecurityPriceProvider>,
}
impl MarketServices {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            fx: Arc::new(EcbFxRateProvider::new()?),
            prices: Arc::new(prices::FreeSecurityPriceProvider::new()?),
        })
    }
    pub fn for_account<'a>(&'a self, db: &'a Database) -> PortfolioService<'a> {
        PortfolioService { db, market: self }
    }
    #[cfg(test)]
    pub fn with_providers(
        fx: Arc<dyn FxRateProvider>,
        prices: Arc<dyn SecurityPriceProvider>,
    ) -> Self {
        Self { fx, prices }
    }
}
pub(crate) struct PortfolioService<'a> {
    db: &'a Database,
    market: &'a MarketServices,
}
impl PortfolioService<'_> {
    pub async fn refresh_fx(&self, force: bool) -> AppResult<fx::FxBook> {
        fx::refresh(self.db, self.market.fx.as_ref(), force).await
    }
    pub async fn refresh_prices(&self) -> AppResult<prices::RefreshResult> {
        prices::refresh(self.db, self.market.prices.as_ref()).await
    }
    pub async fn snapshot(&self) -> AppResult<Snapshot> {
        self.refresh_fx(false).await?;
        Ok(self.refresh_prices().await?.snapshot)
    }
    pub async fn fx_quote(&self, query: &FxRateQuery) -> AppResult<crate::models::FxRateQuote> {
        fx::historical_quote(self.db, self.market.fx.as_ref(), query).await
    }
    pub async fn security_quote(
        &self,
        query: &SecurityPriceQuery,
    ) -> AppResult<SecurityPriceQuote> {
        if let Ok(key) = self.db.secret("security-price") {
            TwelveDataSecurityPriceProvider::new()?
                .quote(query, &key)
                .await
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
        let quote = self
            .security_quote(&SecurityPriceQuery {
                symbol: input.symbol,
                on_date: input.on_date,
            })
            .await?;
        self.db
            .apply_verified_holding_valuation(id, input.quantity, &quote)
    }
    pub async fn save_fx_override(&self, input: &fx::FxOverrideInput) -> AppResult<fx::FxBook> {
        // A manual value must still be saveable when the provider is unavailable.
        self.db.set_fx_override(input)?;
        self.refresh_fx(false).await
    }
    pub async fn save_profile(&self, input: &FinancialProfile) -> AppResult<Snapshot> {
        self.db.save_profile(input)?;
        self.refresh_fx(false).await?;
        self.db.snapshot()
    }
    pub async fn save_holding(&self, id: Option<&str>, input: HoldingInput) -> AppResult<Snapshot> {
        self.refresh_fx(false).await?;
        let input = prices::complete_input(input, self.market.prices.as_ref()).await?;
        if let Some(id) = id {
            self.db.update_holding(id, &input)?;
        } else {
            self.db.add_holding(&input)?;
        }
        Ok(self.refresh_prices().await?.snapshot)
    }
    pub async fn save_trade(&self, input: &HoldingTradeInput) -> AppResult<Snapshot> {
        self.db.save_holding_trade(input)?;
        self.snapshot().await
    }
    pub async fn update_trade(
        &self,
        id: &str,
        input: &HoldingTradeUpdateInput,
    ) -> AppResult<Snapshot> {
        self.db.update_holding_trade(id, input)?;
        self.snapshot().await
    }
    pub async fn ensure_daily(
        &self,
        input: &models::daily::EnsureInput,
    ) -> AppResult<models::daily::DailyHistory> {
        self.db.ensure_daily(input)?;
        self.snapshot().await?;
        self.db
            .daily_history(&models::daily::HistoryQuery::default())
    }
    pub async fn add_event(&self, input: &PortfolioEventInput) -> AppResult<PortfolioEventRecord> {
        self.cache_event_fx(input).await?;
        let input = self
            .db
            .centralized_event(input, &self.db.snapshot()?.profile.base_currency)?;
        self.db.add_portfolio_event(&input)
    }
    pub async fn preview_import(
        &self,
        input: &PortfolioEventImportRequest,
    ) -> AppResult<PortfolioEventImportPreview> {
        self.cache_import_fx(&input.csv_text).await?;
        self.db.preview_portfolio_event_import(input)
    }
    pub async fn commit_import(
        &self,
        input: &PortfolioEventImportCommitRequest,
    ) -> AppResult<PortfolioEventImportResult> {
        self.cache_import_fx(&input.csv_text).await?;
        self.db.commit_portfolio_event_import(input)
    }
    async fn cache_event_fx(&self, input: &PortfolioEventInput) -> AppResult<()> {
        let base = self.db.snapshot()?.profile.base_currency;
        fx::refresh(self.db, self.market.fx.as_ref(), false).await?;
        if !input.currency.eq_ignore_ascii_case(&base) {
            let query = FxRateQuery {
                from_currency: input.currency.clone(),
                to_currency: base,
                on_date: input.occurred_on.clone(),
            };
            let quote = fx::historical_quote(self.db, self.market.fx.as_ref(), &query).await?;
            let key = format!(
                "fx.quote.{}.{}.{}",
                quote.from_currency, quote.to_currency, query.on_date
            );
            self.db.set_setting(&key, &serde_json::to_string(&quote)?)?;
        }
        Ok(())
    }

    async fn cache_import_fx(&self, csv: &str) -> AppResult<()> {
        let parsed = event_import::parse(csv)?;
        let mut seen = HashSet::new();
        for row in parsed.rows {
            if seen.insert((row.input.currency.clone(), row.input.occurred_on.clone())) {
                // Invalid/missing historical quotes are reported on the affected preview rows.
                if let Err(error) = self.cache_event_fx(&row.input).await {
                    tracing::warn!(%error, "CSV FX lookup failed");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::{
        HoldingInput, PortfolioEventImportCommitRequest, PortfolioEventImportRequest,
        PortfolioEventInput,
    };
    use crate::{
        fx::{refresh, today, FxOverrideInput},
        models::FxRateQuote,
    };
    use chrono::Utc;
    use std::sync::Arc;
    struct Provider;
    #[async_trait::async_trait]
    impl FxRateProvider for Provider {
        async fn quote(&self, q: &FxRateQuery) -> AppResult<FxRateQuote> {
            Ok(FxRateQuote {
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
    async fn ledger_and_csv_without_rate_share_the_source_and_frozen_records_survive_overrides() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        db.add_holding(&holding("CNY")).unwrap();
        let market = MarketServices::with_providers(
            Arc::new(Provider),
            Arc::new(crate::market_data::TwelveDataSecurityPriceProvider::new().unwrap()),
        );
        let service = market.for_account(&db);
        let csv = "source,external_id,event_type,occurred_on,amount,currency,note\nbroker,id-1,deposit,2026-01-02,100,USD,deposit\n";
        let preview = service
            .preview_import(&PortfolioEventImportRequest {
                csv_text: csv.into(),
            })
            .await
            .unwrap();
        assert_eq!(preview.ready_count, 1);
        assert_eq!(preview.rows[0].fx_rate_to_base, Some(7.0));
        let committed = service
            .commit_import(&PortfolioEventImportCommitRequest {
                csv_text: csv.into(),
                preview_revision: preview.preview_revision,
            })
            .await
            .unwrap();
        assert_eq!(committed.inserted_count, 1);
        let frozen = db.portfolio_events().unwrap()[0].clone();
        assert_eq!(frozen.base_amount, 700.0);
        db.set_fx_override(&FxOverrideInput {
            currency: "USD".into(),
            rate: Some(8.0),
        })
        .unwrap();
        let event:PortfolioEventInput=serde_json::from_value(serde_json::json!({"eventType":"deposit","source":"manual","externalId":"id-2","assetName":"","amount":100,"currency":"USD","occurredOn":today(),"note":"new deposit"})).unwrap();
        let saved = service.add_event(&event).await.unwrap();
        assert_eq!(saved.base_amount, 800.0);
        let events = db.portfolio_events().unwrap();
        assert_eq!(
            serde_json::to_value(events.iter().find(|e| e.id == frozen.id).unwrap()).unwrap(),
            serde_json::to_value(&frozen).unwrap()
        );
        let preview = service
            .preview_import(&PortfolioEventImportRequest {
                csv_text: csv.into(),
            })
            .await
            .unwrap();
        assert_eq!(preview.duplicate_count, 1);
        db.set_fx_override(&FxOverrideInput {
            currency: "USD".into(),
            rate: Some(9.0),
        })
        .unwrap();
        let today_csv=format!("source,external_id,event_type,occurred_on,amount,currency,note\nmanual,id-2,deposit,{},100,USD,new deposit\n",today());
        let preview = service
            .preview_import(&PortfolioEventImportRequest {
                csv_text: today_csv,
            })
            .await
            .unwrap();
        assert_eq!(preview.duplicate_count, 1);
        assert_eq!(preview.rows[0].fx_rate_to_base, Some(8.0));
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
