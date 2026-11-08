//! Process dependency assembly and ownership of public quote refresh tasks.
use crate::{
    application::portfolio::MarketServices,
    market_data::{EcbFxRateProvider, PublicMarketSource},
    models::{market::today, FxRateQuery},
    ports::market::FxRateProvider,
    AppResult,
};
use std::sync::Arc;

pub(crate) struct MarketRuntime {
    pub services: Arc<MarketServices>,
    pub watch_provider: Arc<dyn crate::ports::market::WatchMarketProvider>,
    pub watch_catalog: Arc<crate::application::watch_catalog::WatchCatalogService>,
    pub dividend_calendar: Arc<crate::application::dividend_calendar::DividendCalendarService>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl MarketRuntime {
    pub fn start_account_tasks(&mut self, accounts: Arc<crate::accounts::Accounts>) {
        let services = self.services.clone();
        let price_accounts = accounts.clone();
        self.tasks.push(tokio::spawn(async move {
            tracing::info!("holding closing price background checks started; running immediately and every 5 minutes");
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let users = match price_accounts.user_ids() {
                    Ok(users) => users,
                    Err(error) => {
                        tracing::warn!(%error, "closing price account enumeration failed");
                        continue;
                    }
                };
                for user in users {
                    let result = async {
                        let db = crate::db::Database::postgres(&price_accounts.url, &user)?;
                        services.for_account(&db).refresh_closing_prices().await
                    }.await;
                    match result {
                        Ok(result) => {
                            tracing::info!(%user, updated = result.updated_count, failed = result.errors.len(), "holding closing price refresh completed");
                            for error in result.errors {
                                tracing::warn!(%user, %error, "holding closing price refresh failed; keeping prior valuation");
                            }
                        }
                        Err(error) => tracing::warn!(%user, %error, "account closing price refresh failed"),
                    }
                }
            }
        }));
        let catalog = self.watch_catalog.clone();
        let calendar = self.dividend_calendar.clone();
        self.tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let (market, plans) =
                    tokio::join!(catalog.refresh(None, false), calendar.refresh_public());
                if let Err(error) = market {
                    tracing::warn!(%error, "public catalogue refresh failed");
                }
                if let Err(error) = plans {
                    tracing::warn!(%error, "public dividend calendar refresh failed");
                }
            }
        }));
        let provider = self.watch_provider.clone();
        let calendar = self.dividend_calendar.clone();
        self.tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let users = match accounts.user_ids() {
                    Ok(users) => users,
                    Err(error) => {
                        tracing::warn!(%error, "observation account enumeration failed");
                        continue;
                    }
                };
                for user in users {
                    let result = async {
                        let db = crate::db::Database::postgres(&accounts.url, &user)?;
                        let _lock = db.lock_account()?;
                        let symbols = db
                            .watch_entries()?
                            .into_iter()
                            .map(|entry| entry.input.symbol)
                            .collect::<Vec<_>>();
                        if let Err(error) = db.without_account_lock(calendar.refresh(&symbols, false)).await? {
                            tracing::warn!(%error, "personal calendar refresh failed; price checks continue");
                        }
                        crate::application::watch::refresh(&db, provider.clone(), false).await?;
                        Ok::<_, crate::AppError>(())
                    }
                    .await;
                    if let Err(error) = result {
                        tracing::warn!(%error, "account observation refresh failed");
                    }
                }
            }
        }));
    }
    pub fn new(database_url: &str) -> AppResult<Self> {
        let public_dividends = Arc::new(crate::db::PublicDividends::postgres(
            database_url,
            &crate::market_data::watch_archive::initial_histories()?,
        )?);
        let fx = Arc::new(EcbFxRateProvider::new()?);
        let prices = Arc::new(crate::application::public_prices::PublicPriceService::new(
            Arc::new(crate::db::PublicPrices::postgres(database_url)?),
            Arc::new(PublicMarketSource::new()?),
        ));
        let services = Arc::new(MarketServices::new(fx.clone(), prices.clone()));
        let refresh_prices = prices.clone();
        let price_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                refresh_prices.refresh_registered().await;
            }
        });
        let fx_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let query = FxRateQuery {
                    from_currency: "USD".into(),
                    to_currency: "CNY".into(),
                    on_date: today(),
                };
                if let Err(error) = fx.quote(&query).await {
                    tracing::warn!(%error, "automatic FX refresh failed");
                }
            }
        });
        let watch_provider: Arc<dyn crate::ports::market::WatchMarketProvider> =
            Arc::new(crate::application::watch_market::SharedWatchProvider::new(
                Arc::new(crate::market_data::watch::PublicWatchProvider::new()?),
                public_dividends,
                prices,
            ));
        let public_watch = Arc::new(crate::db::PublicWatch::postgres(database_url)?);
        let dividend_calendar = Arc::new(
            crate::application::dividend_calendar::DividendCalendarService::new(
                public_watch.clone(),
                watch_provider.clone(),
            ),
        );
        let watch_catalog = Arc::new(crate::application::watch_catalog::WatchCatalogService::new(
            public_watch,
            watch_provider.clone(),
            Arc::new(crate::market_data::watch_catalog::PublicCatalogSources::new()?),
        ));
        Ok(Self {
            services,
            watch_provider,
            watch_catalog,
            dividend_calendar,
            tasks: vec![price_task, fx_task],
        })
    }
}
impl Drop for MarketRuntime {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn ending_the_server_lifetime_cancels_both_refresh_tasks() {
        let runtime =
            MarketRuntime::new(&std::env::var("MARIO_TEST_DATABASE_URL").unwrap()).unwrap();
        let handles: Vec<_> = runtime
            .tasks
            .iter()
            .map(|task| task.abort_handle())
            .collect();
        drop(runtime);
        tokio::task::yield_now().await;
        assert!(handles.iter().all(|handle| handle.is_finished()));
    }
}
