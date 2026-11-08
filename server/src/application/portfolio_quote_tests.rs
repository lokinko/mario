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
            "providerCode": self.name, "providerName": "Synthetic quote",
            "exchange": "TEST", "micCode": "XNAS", "instrumentType": "stock",
            "priceBasis": "split_adjusted_close", "sourceUrl": "https://example.test/quote",
            "methodologyUrl": "", "disclaimer": "Synthetic data"
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
async fn wait_for_provider(
    work: Pin<&mut impl Future<Output = AppResult<SecurityPriceQuote>>>,
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
async fn the_shared_quote_used_by_verified_valuation_keeps_the_write_account_lock() {
    let (db, peer) = databases();
    let before = fingerprint(&db);
    let (release, gate) = oneshot::channel();
    let provider = Quotes::new("public-fixture", Some(gate), false);
    let market = MarketServices::new(provider.clone(), provider.clone());
    let service = market.for_account(&db);
    let query = query();
    let outer = db.lock_account().unwrap();
    let mut work = Box::pin(service.security_quote(&query));
    wait_for_provider(work.as_mut(), &provider).await;
    assert!(
        !peer_can_lock(&peer),
        "the shared write-side quote must remain serialized"
    );
    release.send(()).unwrap();
    work.await.unwrap();
    assert!(!peer_can_lock(&peer));
    assert_eq!(fingerprint(&db), before);
    drop(outer);
    assert!(peer_can_lock(&peer));
}
