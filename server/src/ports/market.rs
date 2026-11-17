use crate::{
    models::market::quotes::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
    AppResult,
};
use async_trait::async_trait;

#[async_trait]
pub(crate) trait SecurityHistoryProvider: Send + Sync {
    fn cached_revision(&self, _symbol: &str) -> AppResult<Option<String>> {
        Ok(None)
    }
    async fn history(
        &self,
        symbol: &str,
        through: chrono::NaiveDate,
    ) -> AppResult<crate::models::market::history::PriceHistory>;
}

#[async_trait]
pub(crate) trait PriceUniverseProvider: Send + Sync {
    async fn constituents(&self) -> AppResult<crate::models::market::monitor::PriceUniverse>;
}

#[async_trait]
pub(crate) trait FxRateProvider: Send + Sync {
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote>;
    /// Invalidate only this provider's public cache before an explicit refresh.
    async fn invalidate(&self) {}
}

#[async_trait]
pub(crate) trait SecurityPriceProvider: Send + Sync {
    /// Read an already persisted public quote. Must not perform source I/O.
    fn cached_quote(&self, _query: &SecurityPriceQuery) -> AppResult<Option<SecurityPriceQuote>> {
        Ok(None)
    }
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote>;
}

#[async_trait]
pub(crate) trait SecurityMarketProvider: Send + Sync {
    fn price_revision(&self, _symbol: &str) -> Option<String> {
        None
    }
    /// The latest dated public history, reusable across accounts without network I/O.
    fn dividend_seed(
        &self,
        _symbol: &str,
        _day: chrono::NaiveDate,
    ) -> Option<crate::models::market::dividends::DividendHistory> {
        None
    }
    async fn dividends(
        &self,
        symbol: &str,
        day: chrono::NaiveDate,
    ) -> AppResult<crate::models::market::dividends::DividendHistory>;
    async fn invalidate(&self) {}
    async fn observation(
        &self,
        _symbol: &str,
        _day: chrono::NaiveDate,
    ) -> AppResult<crate::models::market::series::SecurityMarketData> {
        Err(crate::AppError::MarketData(
            "观察行情统一从公共价格仓储读取".into(),
        ))
    }
}

#[async_trait]
pub(crate) trait SecuritySourceProvider: Send + Sync {
    /// Resolve an exact A-share name or code and collect official public sources.
    /// A verified identity can be returned while its report source is still pending.
    async fn resolve(
        &self,
        query: &str,
    ) -> AppResult<crate::models::market::securities::SecuritySources>;
}

/// Cached plan reads never trigger source I/O or expose refresh/storage capabilities.
pub(crate) trait DividendPlanReader: Send + Sync {
    fn snapshot(
        &self,
        symbol: &str,
    ) -> AppResult<crate::models::market::dividend_plans::PlanSnapshot>;
}
#[async_trait]
pub(crate) trait DividendPlanProvider: Send + Sync {
    async fn dividend_plans(
        &self,
        symbol: &str,
        day: chrono::NaiveDate,
    ) -> AppResult<Vec<crate::models::market::dividend_plans::DividendPlan>>;
}
