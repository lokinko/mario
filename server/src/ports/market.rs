use crate::{
    models::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
    AppResult,
};
use async_trait::async_trait;

#[async_trait]
pub(crate) trait FxRateProvider: Send + Sync {
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote>;
    /// Invalidate only this provider's public cache before an explicit refresh.
    async fn invalidate(&self) {}
}

#[async_trait]
pub(crate) trait SecurityPriceProvider: Send + Sync {
    async fn quote(
        &self,
        query: &SecurityPriceQuery,
        api_key: &str,
    ) -> AppResult<SecurityPriceQuote>;
}

#[async_trait]
pub(crate) trait WatchMarketProvider: Send + Sync {
    /// A dated public bootstrap, used only when the account has no stored history.
    fn dividend_seed(
        &self,
        _symbol: &str,
        _day: chrono::NaiveDate,
    ) -> Option<crate::models::watch::DividendHistory> {
        None
    }
    async fn dividends(
        &self,
        symbol: &str,
        day: chrono::NaiveDate,
    ) -> AppResult<crate::models::watch::DividendHistory>;
    async fn invalidate(&self) {}
    async fn observation(
        &self,
        symbol: &str,
        day: chrono::NaiveDate,
    ) -> AppResult<crate::models::watch::WatchMarketData>;
}
