//! Central account FX book; current balances use today's book, ledger events use dated quotes.
use crate::{
    db::Database,
    market_data::{FxRateProvider, SUPPORTED_CURRENCIES},
    models::{FxRateQuery, FxRateQuote},
    AppError, AppResult,
};
use chrono::Utc;

pub use crate::models::market::{today, FxBook, FxOverrideInput};

pub async fn refresh(
    db: &Database,
    provider: &dyn FxRateProvider,
    force: bool,
) -> AppResult<FxBook> {
    let base = db.snapshot()?.profile.base_currency;
    let mut book = db.fx_book(&base)?;
    let day = today();
    if !force
        && book.requested_on == day
        && Utc::now().timestamp() - book.checked_at < if book.error.is_empty() { 3600 } else { 60 }
    {
        return Ok(book);
    }
    if force {
        crate::market_data::invalidate_fx_cache().await;
    }
    let mut errors = Vec::new();
    for currency in SUPPORTED_CURRENCIES.iter().filter(|c| **c != base) {
        let query = FxRateQuery {
            from_currency: currency.to_string(),
            to_currency: base.clone(),
            on_date: day.clone(),
        };
        match provider.quote(&query).await {
            Ok(quote) => {
                book.quotes.retain(|q| q.from_currency != *currency);
                book.quotes.push(quote);
            }
            Err(error) => errors.push(format!("{currency}：{error}")),
        }
    }
    book.requested_on = day;
    book.checked_at = Utc::now().timestamp();
    book.error = errors.join("；");
    db.save_fx_book(&book)?;
    Ok(book)
}

pub async fn historical_quote(
    db: &Database,
    provider: &dyn FxRateProvider,
    query: &FxRateQuery,
) -> AppResult<FxRateQuote> {
    let from = query.from_currency.trim().to_ascii_uppercase();
    let to = query.to_currency.trim().to_ascii_uppercase();
    let date = chrono::NaiveDate::parse_from_str(&query.on_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("汇率日期必须使用 YYYY-MM-DD".into()))?;
    if date.to_string() > today()
        || !SUPPORTED_CURRENCIES.contains(&from.as_str())
        || !SUPPORTED_CURRENCIES.contains(&to.as_str())
    {
        return Err(AppError::Validation("汇率币种或日期无效".into()));
    }
    let book = db.fx_book(&to)?;
    if from != to {
        if let Some(manual) = book
            .manual
            .get(&from)
            .filter(|m| m.observed_on.as_str() <= query.on_date.as_str())
        {
            return Ok(FxRateQuote {
                from_currency: from,
                to_currency: to,
                rate: manual.rate,
                requested_on: query.on_date.clone(),
                observed_on: manual.observed_on.clone(),
                staleness_days: 0,
                provider_code: "user_declared".into(),
                provider_name: "手动汇率".into(),
                source_url: String::new(),
                methodology_url: String::new(),
                disclaimer: "统一汇率设置中手动指定的值".into(),
            });
        }
    }
    let key = format!("fx.quote.{from}.{to}.{}", query.on_date);
    if let Some(stored) = db.setting(&key)? {
        let quote: FxRateQuote = serde_json::from_str(&stored)?;
        // Today's table is refreshed hourly; finalized historical quotes stay frozen.
        if query.on_date != today() {
            return Ok(quote);
        }
    }
    let quote = if query.on_date == today() {
        let current = refresh(db, provider, false).await?;
        current
            .quotes
            .into_iter()
            .find(|q| q.from_currency == from && q.requested_on == query.on_date)
    } else {
        None
    };
    let quote = match quote {
        Some(q) => q,
        None => provider.quote(query).await?,
    };
    db.set_setting(&key, &serde_json::to_string(&quote)?)?;
    Ok(quote)
}

/// Keep the provider cache warm even with no client connected. Account observations
/// are applied on the next read, under that account's existing transaction/lock.
pub fn start_daily_refresh() {
    tokio::spawn(async {
        let Ok(provider) = crate::market_data::EcbFxRateProvider::new() else {
            return;
        };
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let query = FxRateQuery {
                from_currency: "USD".into(),
                to_currency: "CNY".into(),
                on_date: today(),
            };
            if let Err(error) = provider.quote(&query).await {
                tracing::warn!(%error, "automatic FX refresh failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Provider {
        calls: AtomicUsize,
        fail: bool,
        rate: f64,
    }
    #[async_trait::async_trait]
    impl FxRateProvider for Provider {
        async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(AppError::MarketData("offline".into()));
            }
            Ok(FxRateQuote {
                from_currency: query.from_currency.clone(),
                to_currency: query.to_currency.clone(),
                rate: self.rate,
                requested_on: query.on_date.clone(),
                observed_on: query.on_date.clone(),
                staleness_days: 0,
                provider_code: "ecb_reference".into(),
                provider_name: "ECB".into(),
                source_url: "https://data-api.ecb.europa.eu".into(),
                methodology_url: String::new(),
                disclaimer: String::new(),
            })
        }
    }
    fn db() -> Database {
        Database::open(std::path::Path::new(":memory:")).unwrap()
    }
    fn provider(rate: f64) -> Provider {
        Provider {
            calls: AtomicUsize::new(0),
            fail: false,
            rate,
        }
    }
    fn input(currency: &str) -> crate::models::HoldingInput {
        serde_json::from_value(serde_json::json!({"symbol":"","name":"账户","assetClass":"现金", "marketValue":100,"costBasis":0,"targetPct":0,"currency":currency,"valuationDate":"2026-01-01","fxRateToBase":999})).unwrap()
    }
    #[tokio::test]
    async fn refresh_is_cached_rolls_over_and_never_guesses_on_failure() {
        let db = db();
        let provider = provider(7.0);
        refresh(&db, &provider, false).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
        assert_eq!(
            db.conn()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM daily_settings", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.ensure_daily(&crate::models::daily::EnsureInput {
                timezone: "Asia/Shanghai".into()
            })
            .unwrap()
            .timezone,
            "Asia/Shanghai"
        );
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        refresh(&db, &provider, false).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
        assert_eq!(
            db.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
        let mut book = db.fx_book("CNY").unwrap();
        book.requested_on = "2026-01-01".into();
        db.save_fx_book(&book).unwrap();
        refresh(&db, &provider, false).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 10);
        let failed = Provider {
            calls: AtomicUsize::new(0),
            fail: true,
            rate: 1.0,
        };
        let result = refresh(&db, &failed, true).await.unwrap();
        assert!(!result.error.is_empty());
        assert!(result.quotes.iter().all(|q| q.rate == 7.0));
        let empty = super::tests::db();
        refresh(&empty, &failed, true).await.unwrap();
        let snap = empty.add_holding(&input("USD")).unwrap();
        assert!(!snap.valuation_status.comparable);
        assert_eq!(snap.holdings[0].fx_rate_to_base, None);
    }
    #[tokio::test]
    async fn manual_rate_applies_to_all_holdings_survives_refresh_restart_and_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fx.db");
        let db = Database::open(&path).unwrap();
        let source = provider(7.0);
        refresh(&db, &source, false).await.unwrap();
        db.add_holding(&input("USD")).unwrap();
        db.add_holding(&input("USD")).unwrap();
        assert_eq!(db.snapshot().unwrap().total_value, 1400.0);
        db.set_fx_override(&FxOverrideInput {
            currency: "USD".into(),
            rate: Some(8.0),
        })
        .unwrap();
        refresh(&db, &provider(6.0), true).await.unwrap();
        assert_eq!(db.snapshot().unwrap().total_value, 1600.0);
        let data = db.export_sync_data().unwrap();
        let restored = super::tests::db();
        restored.import_sync_data(&data).unwrap();
        assert_eq!(restored.fx_book("CNY").unwrap().manual["USD"].rate, 8.0);
        drop(db);
        let reopened = Database::open(&path).unwrap();
        assert_eq!(reopened.fx_book("CNY").unwrap().manual["USD"].rate, 8.0);
        reopened
            .set_fx_override(&FxOverrideInput {
                currency: "USD".into(),
                rate: None,
            })
            .unwrap();
        assert_eq!(reopened.snapshot().unwrap().total_value, 1200.0);
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e10] {
            assert!(reopened
                .set_fx_override(&FxOverrideInput {
                    currency: "USD".into(),
                    rate: Some(invalid)
                })
                .is_err());
        }
    }
    #[tokio::test]
    async fn historical_quotes_are_cached_and_manual_settings_do_not_rewrite_the_past() {
        let db = db();
        let source = provider(7.0);
        let query = FxRateQuery {
            from_currency: "USD".into(),
            to_currency: "CNY".into(),
            on_date: "2026-01-01".into(),
        };
        let quote = historical_quote(&db, &source, &query).await.unwrap();
        db.set_fx_override(&FxOverrideInput {
            currency: "USD".into(),
            rate: Some(8.0),
        })
        .unwrap();
        let cached = historical_quote(&db, &source, &query).await.unwrap();
        assert_eq!(quote.rate, cached.rate);
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        let mut current = query;
        current.on_date = today();
        assert_eq!(
            historical_quote(&db, &source, &current).await.unwrap().rate,
            8.0
        );
    }
    #[test]
    fn imports_cannot_write_secrets_through_currency_settings() {
        let db = db();
        let mut data = db.export_sync_data().unwrap();
        data.tables
            .iter_mut()
            .find(|t| t.name == "settings")
            .unwrap()
            .rows
            .push(vec![
                crate::db::SyncValue::Text("model.name".into()),
                crate::db::SyncValue::Text("injected".into()),
            ]);
        assert!(db.import_sync_data(&data).is_err());
        assert!(db.setting("model.name").unwrap().is_none());
    }
}
