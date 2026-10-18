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
