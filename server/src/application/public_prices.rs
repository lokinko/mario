//! One public database price revision per security; never reads account data.
use crate::{
    db::PublicPrices,
    models::market::{
        canonical_symbol, close,
        history::PriceHistory,
        quotes::{SecurityPriceQuery, SecurityPriceQuote},
    },
    ports::market::{SecurityHistoryProvider, SecurityPriceProvider},
    AppError, AppResult,
};
use chrono::{NaiveDate, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore};

pub(crate) struct PublicPriceService {
    store: Arc<PublicPrices>,
    source: Arc<dyn SecurityHistoryProvider>,
    locks: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
    slots: Semaphore,
}
impl PublicPriceService {
    pub fn new(store: Arc<PublicPrices>, source: Arc<dyn SecurityHistoryProvider>) -> Self {
        Self {
            store,
            source,
            locks: Mutex::new(BTreeMap::new()),
            slots: Semaphore::new(4),
        }
    }
    async fn symbol_lock(&self, symbol: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.retain(|_, v| v.strong_count() > 0);
        if let Some(lock) = locks.get(symbol).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(symbol.into(), Arc::downgrade(&lock));
        lock
    }
    pub async fn refresh_registered(&self) {
        if let Err(error) = self.save_collection_day() {
            tracing::warn!(%error,"price collection daily snapshot failed");
        }
        let symbols = match self.store.symbols() {
            Ok(v) => v,
            Err(error) => {
                tracing::warn!(%error,"public price enumeration failed");
                return;
            }
        };
        self.refresh_symbols(&symbols, Duration::from_secs(120))
            .await;
        if let Err(error) = self.save_collection_day() {
            tracing::warn!(%error,"price collection daily snapshot failed");
        }
    }
    async fn refresh_symbols(&self, symbols: &[String], budget: Duration) {
        let deadline = tokio::time::Instant::now() + budget;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let worker = || async {
            while tokio::time::Instant::now() < deadline {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(symbol) = symbols.get(index) else {
                    break;
                };
                if let Err(error) = self.load_before(symbol, false, deadline).await {
                    tracing::warn!(symbol,%error,"public price refresh failed; retaining database revision");
                }
            }
        };
        tokio::join!(worker(), worker(), worker(), worker());
    }
    async fn load(&self, symbol: &str, touch: bool) -> AppResult<PriceHistory> {
        self.load_before(
            symbol,
            touch,
            tokio::time::Instant::now() + Duration::from_secs(90),
        )
        .await
    }
    async fn load_before(
        &self,
        symbol: &str,
        touch: bool,
        deadline: tokio::time::Instant,
    ) -> AppResult<PriceHistory> {
        let symbol = canonical_symbol(symbol)?;
        if touch {
            self.store.register(&symbol)?;
        }
        let lock = self.symbol_lock(&symbol).await;
        let timeout_error =
            || AppError::MarketData("公共十年行情更新达到本轮时限，等待自动重试".into());
        let _guard = tokio::time::timeout_at(deadline, lock.lock())
            .await
            .map_err(|_| timeout_error())?;
        let now = Utc::now();
        let today = now.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
        let end = close::completed_day(&symbol, today, now);
        let existing = self
            .store
            .state(&symbol)?
            .ok_or_else(|| AppError::MarketData("公共行情注册记录不存在".into()))?;
        if let Some(h) = &existing.history {
            let age = now.timestamp_millis()
                - chrono::DateTime::parse_from_rfc3339(&h.fetched_at)
                    .map_err(|_| AppError::MarketData("公共行情采集日期无效".into()))?
                    .timestamp_millis();
            let ttl = if h.adjustment_anchor == end {
                3_600_000
            } else {
                60_000
            };
            if h.window_start == crate::models::market::history::history_start(end)
                && h.completed_through == end
                && (0..ttl).contains(&age)
            {
                return Ok(h.clone());
            }
        }
        if existing.lease_until > now.timestamp_millis() {
            if let Some(history) = existing.history {
                return Ok(history);
            }
            return Err(AppError::MarketData(
                "该证券公共行情正在更新，请稍后重试".into(),
            ));
        }
        if now.timestamp_millis() - existing.attempted_at < 60_000 {
            if let Some(history) = existing.history {
                return Ok(history);
            }
            return Err(AppError::MarketData(if existing.last_error.is_empty() {
                "公共行情正在准备，请稍后重试".into()
            } else {
                existing.last_error
            }));
        }
        let _slot = tokio::time::timeout_at(deadline, self.slots.acquire())
            .await
            .map_err(|_| timeout_error())?
            .map_err(|_| AppError::MarketData("公共行情服务已停止".into()))?;
        if tokio::time::Instant::now() >= deadline {
            return Err(timeout_error());
        }
        let token = uuid::Uuid::new_v4().to_string();
        if !self
            .store
            .claim(&symbol, Utc::now().timestamp_millis(), &token)?
        {
            return self
                .store
                .state(&symbol)?
                .and_then(|s| s.history)
                .ok_or_else(|| AppError::MarketData("该证券公共行情正在更新，请稍后重试".into()));
        }
        let result = tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(90)),
            self.source.history(&symbol, end),
        )
        .await
        .map_err(|_| timeout_error())
        .and_then(|r| r)
        .and_then(|h| {
            h.validate()?;
            if h.symbol != symbol
                || h.completed_through != end
                || h.window_start != crate::models::market::history::history_start(end)
            {
                return Err(AppError::MarketData(
                    "行情来源返回了不同证券或收盘截止日".into(),
                ));
            }
            Ok(h)
        });
        self.store.finish(&symbol, &token, &result)?;
        if let Err(error) = self.save_collection_day() {
            tracing::warn!(%error,"price collection progress snapshot failed");
        }
        if let Err(error) = &result {
            tracing::warn!(%symbol,error=?error,"public ten-year price source failed");
        }
        result?;
        self.store
            .state(&symbol)?
            .and_then(|s| s.history)
            .ok_or_else(|| AppError::MarketData("公共行情未完成持久保存".into()))
    }
}
#[async_trait::async_trait]
impl SecurityHistoryProvider for PublicPriceService {
    fn cached_revision(&self, symbol: &str) -> AppResult<Option<String>> {
        self.store
            .state(&canonical_symbol(symbol)?)?
            .and_then(|s| s.history)
            .map(|h| h.revision())
            .transpose()
    }
    async fn history(&self, symbol: &str, _through: NaiveDate) -> AppResult<PriceHistory> {
        self.load(symbol, true).await
    }
}
#[async_trait::async_trait]
impl SecurityPriceProvider for PublicPriceService {
    fn cached_quote(&self, query: &SecurityPriceQuery) -> AppResult<Option<SecurityPriceQuote>> {
        let symbol = canonical_symbol(&query.symbol)?;
        self.store.register_throttled(&symbol)?;
        let requested = query
            .on_date
            .parse::<NaiveDate>()
            .map_err(|_| AppError::Validation("证券价格日期无效".into()))?;
        let cutoff = close::completed_day(&symbol, requested, Utc::now());
        if let Some(latest) = self.store.latest_quote(&symbol)? {
            if let Some(quote) = latest.to_quote(requested, cutoff, &query.symbol)? {
                return Ok(Some(quote));
            }
            return self
                .store
                .state(&symbol)?
                .and_then(|s| s.history)
                .map(|h| h.quote(requested, cutoff, &query.symbol))
                .transpose();
        }
        Ok(None)
    }
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote> {
        let symbol = canonical_symbol(&query.symbol)?;
        let requested = query
            .on_date
            .parse::<NaiveDate>()
            .map_err(|_| AppError::Validation("证券价格日期必须使用 YYYY-MM-DD".into()))?;
        let now = Utc::now();
        let today = now.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
        if requested > today {
            return Err(AppError::Validation("不能查询未来收盘价".into()));
        }
        if query
            .action_from
            .as_deref()
            .is_some_and(|d| d.parse::<NaiveDate>().map_or(true, |d| d > requested))
        {
            return Err(AppError::Validation("公司行动起始日期无效".into()));
        }
        self.load(&symbol, true).await?.quote(
            requested,
            close::completed_day(&symbol, requested, now),
            &query.symbol,
        )
    }
}

mod monitor;
#[cfg(test)]
mod tests;
