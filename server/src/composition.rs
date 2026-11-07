//! Process dependency assembly and ownership of public quote refresh tasks.
use crate::{
    application::portfolio::MarketServices,
    market_data::{EcbFxRateProvider, TwelveDataSecurityPriceProvider},
    models::{market::today, FxRateQuery},
    ports::market::FxRateProvider,
    prices::FreeSecurityPriceProvider,
    AppResult,
};
use std::sync::Arc;

pub(crate) struct MarketRuntime {
    pub services: Arc<MarketServices>,
    pub watch_provider: Arc<dyn crate::ports::market::WatchMarketProvider>,
    pub watch_catalog: Arc<crate::application::watch_catalog::WatchCatalogService>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl MarketRuntime {
    pub fn start_watch(&mut self, accounts: Arc<crate::accounts::Accounts>) {
        let catalog = self.watch_catalog.clone();
        self.tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(error) = catalog.refresh(None, false).await {
                    tracing::warn!(%error, "public catalogue refresh failed");
                }
            }
        }));
        let provider = self.watch_provider.clone();
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
        let prices = Arc::new(FreeSecurityPriceProvider::new()?);
        let keyed = Arc::new(TwelveDataSecurityPriceProvider::new()?);
        let services = Arc::new(MarketServices::new(fx.clone(), prices.clone(), keyed));
        let price_task = crate::prices::start_refresh(prices);
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
            ));
        let watch_catalog = Arc::new(crate::application::watch_catalog::WatchCatalogService::new(
            Arc::new(crate::db::PublicWatch::postgres(database_url)?),
            watch_provider.clone(),
            Arc::new(crate::market_data::watch_catalog::PublicCatalogSources::new()?),
        ));
        Ok(Self {
            services,
            watch_provider,
            watch_catalog,
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
