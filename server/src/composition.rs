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
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl MarketRuntime {
    pub fn new() -> AppResult<Self> {
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
        Ok(Self {
            services,
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
        let runtime = MarketRuntime::new().unwrap();
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
