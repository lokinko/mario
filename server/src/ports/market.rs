use crate::{
    models::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
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
pub(crate) trait WatchMarketProvider: Send + Sync {
    fn price_revision(&self, _symbol: &str) -> Option<String> {
        None
    }
    async fn dividend_plans(
        &self,
        _symbol: &str,
        _day: chrono::NaiveDate,
    ) -> AppResult<Vec<crate::models::watch::calendar::DividendPlan>> {
        Err(crate::AppError::MarketData("分红计划来源暂不可用".into()))
    }

    /// The latest dated public history, reusable across accounts without network I/O.
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
        _symbol: &str,
        _day: chrono::NaiveDate,
    ) -> AppResult<crate::models::watch::WatchMarketData> {
        Err(crate::AppError::MarketData(
            "观察行情统一从公共价格仓储读取".into(),
        ))
    }
}

#[async_trait]
pub(crate) trait CatalogSourceProvider: Send + Sync {
    /// Resolve an exact A-share name or code and collect official public sources.
    /// A verified identity can be returned while its report source is still pending.
    async fn resolve(&self, query: &str) -> AppResult<crate::models::watch::catalog::Company>;
}
