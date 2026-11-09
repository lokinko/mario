use super::MarketServices;
use crate::{
    db::{Database, TestAccount},
    models::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
    ports::market::{FxRateProvider, SecurityPriceProvider},
    AppError, AppResult,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{oneshot, Notify};

struct Quotes {
    name: &'static str,
    entered: Notify,
    gate: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
    dropped: AtomicBool,
    fail: bool,
}
impl Quotes {
    fn new(name: &'static str, gate: Option<oneshot::Receiver<()>>, fail: bool) -> Arc<Self> {
        Arc::new(Self {
            name,
            entered: Notify::new(),
            gate: tokio::sync::Mutex::new(gate),
            dropped: AtomicBool::new(false),
            fail,
        })
    }
}
#[async_trait::async_trait]
impl FxRateProvider for Quotes {
    async fn quote(&self, _: &FxRateQuery) -> AppResult<FxRateQuote> {
        Err(AppError::MarketData(
            "FX must not be requested by security preview tests".into(),
        ))
    }
}
#[async_trait::async_trait]
impl SecurityPriceProvider for Quotes {
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote> {
        struct Dropped<'a>(&'a AtomicBool);
        impl Drop for Dropped<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _dropped = Dropped(&self.dropped);
        if let Some(gate) = self.gate.lock().await.take() {
            self.entered.notify_one();
            let _ = gate.await; // A failed assertion also releases the provider when its sender drops.
        }
        if self.fail {
            return Err(AppError::MarketData("synthetic quote failure".into()));
        }
        Ok(serde_json::from_value(serde_json::json!({
            "symbol": query.symbol, "currency": "USD", "close": 123.45,
            "requestedOn": query.on_date, "observedOn": query.on_date, "stalenessDays": 0,
            "providerCode": self.name, "providerName": "Yahoo Finance",
            "exchange": "TEST", "micCode": "XNAS", "instrumentType": "stock",
            "priceBasis": "split_adjusted_daily_close",
            "sourceUrl": format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d", query.symbol),
            "methodologyUrl": crate::models::market::METHODOLOGY_URL,
            "disclaimer": "Synthetic data; no live quote requested"
        }))
        .unwrap())
    }
}
fn query() -> SecurityPriceQuery {
    SecurityPriceQuery {
        symbol: "AAPL".into(),
        on_date: "2026-01-02".into(),
        action_from: None,
    }
}
fn databases() -> (Database, Database) {
    let account = TestAccount::new();
    // Connection setup also takes the account lock, so open both before the request starts.
    (account.open(), account.open())
}
fn peer_can_lock(db: &Database) -> bool {
    let conn = db.conn().unwrap();
    let acquired = conn
        .query_row(
            "SELECT pg_try_advisory_lock(hashtextextended(current_schema(),0))",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap();
    if acquired {
        conn.execute_batch("SELECT pg_advisory_unlock(hashtextextended(current_schema(),0))")
            .unwrap();
    }
    acquired
}
fn fingerprint(db: &Database) -> (String, String) {
    (
        db.revision().unwrap(),
        db.export_sync_data().unwrap().content_hash().unwrap(),
    )
}
async fn wait_for_provider<T: std::fmt::Debug>(
    work: Pin<&mut impl Future<Output = AppResult<T>>>,
    provider: &Quotes,
) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::select! {
            _ = provider.entered.notified() => {},
            result = work => panic!("quote completed before the provider gate: {result:?}"),
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preview_releases_and_reacquires_the_real_account_lock_on_success_and_error() {
    for fail in [false, true] {
        let (db, peer) = databases();
        let before = fingerprint(&db);
        let (release, gate) = oneshot::channel();
        let provider = Quotes::new("public-fixture", Some(gate), fail);
        let market = MarketServices::new(provider.clone(), provider.clone());
        let service = market.for_account(&db);
        let query = query();
        let outer = db.lock_account().unwrap();
        let mut work = Box::pin(service.preview_security_quote(&query));
        wait_for_provider(work.as_mut(), &provider).await;
        assert!(
            peer_can_lock(&peer),
            "the independent session must acquire the released lock"
        );
        assert!(peer.snapshot().is_ok());
        release.send(()).unwrap();
        let result = work.await;
        if fail {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("synthetic quote failure"));
        } else {
            assert_eq!(result.unwrap().provider_code, "public-fixture");
        }
        assert!(
            !peer_can_lock(&peer),
            "normal success and errors must reacquire before returning"
        );
        assert_eq!(fingerprint(&db), before);
        drop(outer);
        assert!(
            peer_can_lock(&peer),
            "the completed preview must not leak an extra lock level"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_pending_preview_drops_the_provider_and_leaves_the_account_unlocked() {
    let (db, peer) = databases();
    let before = fingerprint(&db);
    let (_release, gate) = oneshot::channel();
    let provider = Quotes::new("public-fixture", Some(gate), false);
    let market = MarketServices::new(provider.clone(), provider.clone());
    let service = market.for_account(&db);
    let query = query();
    let outer = db.lock_account().unwrap();
    let mut work = Box::pin(service.preview_security_quote(&query));
    wait_for_provider(work.as_mut(), &provider).await;
    drop(work);
    assert!(provider.dropped.load(Ordering::SeqCst));
    assert!(peer_can_lock(&peer)); // Cancellation intentionally does not reacquire.
    assert_eq!(fingerprint(&db), before);
    drop(outer);
    assert!(peer_can_lock(&peer));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_valuation_rejects_a_concurrent_edit_or_deletion_after_unlocked_query() {
    for delete in [false, true] {
        let (db, peer) = databases();
        let input = holding();
        let id = db.add_holding(&input).unwrap().holdings[0].id.clone();
        let (release, gate) = oneshot::channel();
        let provider = Quotes::new("public-fixture", Some(gate), false);
        let market = MarketServices::new(provider.clone(), provider.clone());
        let service = market.for_account(&db);
        let outer = db.lock_account().unwrap();
        let mut work = Box::pin(service.verify_valuation(
            &id,
            crate::models::VerifiedHoldingValuationInput {
                symbol: "AAPL".into(),
                quantity: 1.0,
                on_date: "2026-01-02".into(),
            },
        ));
        wait_for_provider(work.as_mut(), &provider).await;
        assert!(peer_can_lock(&peer));
        {
            let _peer_lock = peer.lock_account().unwrap();
            if delete {
                // Remove an unused manual holding through its repository path.
                peer.delete_holding(&id, &crate::models::HoldingDeleteInput::default())
                    .unwrap();
            } else {
                let mut changed = input.clone();
                changed.name = "concurrent edit".into();
                peer.update_holding(&id, &changed).unwrap();
            }
        }
        let concurrent = fingerprint(&peer);
        release.send(()).unwrap();
        assert!(matches!(work.await, Err(AppError::Conflict(_))));
        assert!(!peer_can_lock(&peer));
        assert_eq!(fingerprint(&db), concurrent);
        drop(outer);
        assert!(peer_can_lock(&peer));
    }
}

fn holding() -> crate::models::HoldingInput {
    serde_json::from_value(serde_json::json!({
        "name":"synthetic holding", "symbol":"AAPL", "assetClass":"股票",
        "marketValue":100, "costBasis":100, "quantity":1, "currentPrice":100,
        "targetPct":0, "currency":"USD", "valuationDate":"2026-01-02"
    }))
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_valuation_reacquires_before_success_or_provider_failure() {
    for fail in [false, true] {
        let (db, peer) = databases();
        let id = db.add_holding(&holding()).unwrap().holdings[0].id.clone();
        let before = fingerprint(&db);
        let (release, gate) = oneshot::channel();
        let provider = Quotes::new("yahoo_daily_close", Some(gate), fail);
        let market = MarketServices::new(provider.clone(), provider.clone());
        let service = market.for_account(&db);
        let outer = db.lock_account().unwrap();
        let mut work = Box::pin(service.verify_valuation(
            &id,
            crate::models::VerifiedHoldingValuationInput {
                symbol: "AAPL".into(),
                quantity: 1.0,
                on_date: "2026-01-02".into(),
            },
        ));
        wait_for_provider(work.as_mut(), &provider).await;
        assert!(peer_can_lock(&peer));
        release.send(()).unwrap();
        let result = work.await;
        if fail {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("synthetic quote failure"));
            assert_eq!(fingerprint(&db), before);
        } else {
            assert_eq!(result.unwrap().holdings[0].current_price, Some(123.45));
        }
        assert!(!peer_can_lock(&peer));
        drop(outer);
        assert!(peer_can_lock(&peer));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saving_a_holding_releases_the_lock_and_never_overwrites_concurrent_input() {
    let (db, peer) = databases();
    let (release, gate) = oneshot::channel();
    let provider = Quotes::new("public-fixture", Some(gate), false);
    let market = MarketServices::new(provider.clone(), provider.clone());
    let service = market.for_account(&db);
    let mut input = holding();
    input.current_price = None;
    let outer = db.lock_account().unwrap();
    let mut work = Box::pin(service.save_holding(None, input));
    wait_for_provider(work.as_mut(), &provider).await;
    assert!(peer_can_lock(&peer));
    {
        let _peer_lock = peer.lock_account().unwrap();
        let mut profile = peer.snapshot().unwrap().profile;
        profile.monthly_income = 1234.0;
        peer.save_profile(&profile).unwrap();
    }
    let concurrent = fingerprint(&peer);
    release.send(()).unwrap();
    assert!(matches!(work.await, Err(AppError::Conflict(_))));
    assert_eq!(fingerprint(&db), concurrent);
    assert!(db.snapshot().unwrap().holdings.is_empty());
    assert!(!peer_can_lock(&peer));
    drop(outer);
    assert!(peer_can_lock(&peer));
}

struct FxQuotes(Arc<Quotes>);
#[async_trait::async_trait]
impl FxRateProvider for FxQuotes {
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote> {
        SecurityPriceProvider::quote(
            self.0.as_ref(),
            &SecurityPriceQuery {
                symbol: "FX-fixture".into(),
                on_date: query.on_date.clone(),
                action_from: None,
            },
        )
        .await?;
        Ok(serde_json::from_value(serde_json::json!({
            "exactRate":"7.0000", "fromCurrency":query.from_currency,
            "toCurrency":query.to_currency, "rate":7.0,
            "requestedOn":query.on_date, "observedOn":query.on_date, "stalenessDays":0,
            "providerCode":"synthetic", "providerName":"Synthetic FX",
            "sourceUrl":"", "methodologyUrl":"", "disclaimer":""
        }))
        .unwrap())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn readonly_fx_quote_allows_unrelated_writes_but_rejects_changed_rate_settings() {
    for change_rate in [false, true] {
        let (db, peer) = databases();
        let (release, gate) = oneshot::channel();
        let provider = Quotes::new("fx-fixture", Some(gate), false);
        let market = MarketServices::new(Arc::new(FxQuotes(provider.clone())), provider.clone());
        let service = market.for_account(&db);
        let query = FxRateQuery {
            from_currency: "USD".into(),
            to_currency: "CNY".into(),
            on_date: "2026-01-02".into(),
        };
        let outer = db.lock_account().unwrap();
        let mut work = Box::pin(service.fx_quote(&query));
        wait_for_provider(work.as_mut(), &provider).await;
        assert!(peer_can_lock(&peer));
        {
            let _lock = peer.lock_account().unwrap();
            if change_rate {
                peer.set_fx_override(&crate::models::market::FxOverrideInput {
                    exact_rate: None,
                    currency: "USD".into(),
                    rate: Some(8.0),
                })
                .unwrap();
            } else {
                let mut profile = peer.snapshot().unwrap().profile;
                profile.monthly_income = 1234.0;
                peer.save_profile(&profile).unwrap();
            }
        }
        let concurrent = fingerprint(&peer);
        release.send(()).unwrap();
        let result = work.await;
        if change_rate {
            assert!(matches!(result, Err(AppError::Conflict(_))));
        } else {
            assert_eq!(result.unwrap().exact_rate.as_deref(), Some("7.0000"));
        }
        assert_eq!(fingerprint(&db), concurrent);
        assert!(!peer_can_lock(&peer));
        drop(outer);
        assert!(peer_can_lock(&peer));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fx_refresh_does_not_overwrite_a_manual_rate_set_while_waiting() {
    let (db, peer) = databases();
    let (release, gate) = oneshot::channel();
    let provider = Quotes::new("fx-fixture", Some(gate), false);
    let market = MarketServices::new(Arc::new(FxQuotes(provider.clone())), provider.clone());
    let service = market.for_account(&db);
    let outer = db.lock_account().unwrap();
    let mut work = Box::pin(service.refresh_fx(true));
    wait_for_provider(work.as_mut(), &provider).await;
    assert!(peer_can_lock(&peer));
    {
        let _peer_lock = peer.lock_account().unwrap();
        peer.set_fx_override(&crate::models::market::FxOverrideInput {
            exact_rate: Some("8.0100".into()),
            currency: "USD".into(),
            rate: Some(8.01),
        })
        .unwrap();
    }
    let concurrent = fingerprint(&peer);
    release.send(()).unwrap();
    assert!(matches!(work.await, Err(AppError::Conflict(_))));
    assert_eq!(fingerprint(&db), concurrent);
    assert_eq!(
        db.fx_book("CNY").unwrap().manual["USD"]
            .exact_rate
            .as_deref(),
        Some("8.0100")
    );
    assert!(!peer_can_lock(&peer));
    drop(outer);
    assert!(peer_can_lock(&peer));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_pending_fx_refresh_never_commits_and_leaves_the_lock_released() {
    let (db, peer) = databases();
    let before = fingerprint(&db);
    let (_release, gate) = oneshot::channel();
    let provider = Quotes::new("fx-fixture", Some(gate), false);
    let market = MarketServices::new(Arc::new(FxQuotes(provider.clone())), provider.clone());
    let service = market.for_account(&db);
    let outer = db.lock_account().unwrap();
    let mut work = Box::pin(service.refresh_fx(true));
    wait_for_provider(work.as_mut(), &provider).await;
    drop(work);
    assert!(provider.dropped.load(Ordering::SeqCst));
    assert_eq!(fingerprint(&db), before);
    assert!(peer_can_lock(&peer));
    drop(outer);
    assert!(peer_can_lock(&peer));
}
