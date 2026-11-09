use super::{authenticate, HostedState, AUTH_SLOTS, REQUEST_SLOTS};
use crate::{
    accounts::{Accounts, Credentials},
    application::portfolio::MarketServices,
    db::Database,
    http::api_router,
    models::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
    ports::market::{FxRateProvider, SecurityPriceProvider, WatchMarketProvider},
    AppError, AppResult,
};
use axum::{http::StatusCode, middleware};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{oneshot, Notify};

struct Prices {
    entered: Notify,
    gate: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
    calls: AtomicUsize,
}
#[async_trait::async_trait]
impl SecurityPriceProvider for Prices {
    async fn quote(&self, query: &SecurityPriceQuery) -> AppResult<SecurityPriceQuote> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = self.gate.lock().await.take() {
            self.entered.notify_one();
            let _ = gate.await;
        }
        Ok(serde_json::from_value(serde_json::json!({
            "symbol": query.symbol, "currency": "USD", "close": 123.45,
            "requestedOn": query.on_date, "observedOn": query.on_date, "stalenessDays": 0,
            "providerCode": "http-fixture", "providerName": "Synthetic quote",
            "exchange": "TEST", "micCode": "XNAS", "instrumentType": "stock",
            "priceBasis": "split_adjusted_close", "sourceUrl": "https://example.test/quote",
            "methodologyUrl": "", "disclaimer": "Synthetic data"
        }))
        .unwrap())
    }
}
#[async_trait::async_trait]
impl FxRateProvider for Prices {
    async fn quote(&self, _: &FxRateQuery) -> AppResult<FxRateQuote> {
        Err(AppError::MarketData("unused FX provider".into()))
    }
}
#[async_trait::async_trait]
impl WatchMarketProvider for Prices {
    async fn dividends(
        &self,
        _: &str,
        _: chrono::NaiveDate,
    ) -> AppResult<crate::models::watch::DividendHistory> {
        Err(AppError::MarketData("unused watch provider".into()))
    }
    async fn observation(
        &self,
        _: &str,
        _: chrono::NaiveDate,
    ) -> AppResult<crate::models::watch::WatchMarketData> {
        Err(AppError::MarketData("unused watch provider".into()))
    }
}
struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_security_preview_allows_reads_and_profile_writes_without_acknowledging_stale_data(
) {
    crate::secrets::use_test_master_key();
    let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
    let accounts = Arc::new(Accounts::open(url.clone()).unwrap());
    let register = || {
        accounts
            .register(&Credentials {
                email: format!("{}@example.test", uuid::Uuid::new_v4()),
                password: "synthetic-quote-password-123".into(),
            })
            .unwrap()
    };
    let a = register();
    let b = register();
    let db = Database::postgres(&url, &a.user_id).unwrap();
    let before = db.revision().unwrap();
    let (release, gate) = oneshot::channel();
    let provider = Arc::new(Prices {
        entered: Notify::new(),
        gate: tokio::sync::Mutex::new(Some(gate)),
        calls: AtomicUsize::new(0),
    });
    // These constructors only assemble test services; MarketRuntime::new would start network tasks.
    let host = Arc::new(HostedState {
        tenant_connections: Arc::new(crate::storage::ConnectionPool::new(accounts.url.clone(), 8)),
        model_services: crate::composition::model_services(),
        accounts,
        openai: None,
        slots: tokio::sync::Semaphore::new(REQUEST_SLOTS),
        auth_slots: tokio::sync::Semaphore::new(AUTH_SLOTS),
        market: Arc::new(MarketServices::new(provider.clone(), provider.clone())),
        watch_provider: provider.clone(),
        watch_catalog: crate::application::watch_catalog::tests::fixture_service(),
        dividend_calendar: crate::application::dividend_calendar::tests::fixture_service(),
        analysis_executor: Arc::new(crate::agent::AgentExecutor),
    });
    // Authenticate creates a real account Database for each request, as in production.
    let app = api_router().route_layer(middleware::from_fn_with_state(host, authenticate));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base}/snapshot"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut quote = Box::pin(
        client
            .get(format!(
                "{base}/market-data/security-price?symbol=AAPL&onDate=2026-01-02"
            ))
            .bearer_auth(&a.token)
            .send(),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::select! {
            _ = provider.entered.notified() => {},
            result = &mut quote => panic!("preview returned before provider release: {result:?}"),
        }
    })
    .await
    .unwrap();
    let same_account = client
        .get(format!("{base}/snapshot"))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(same_account.status(), StatusCode::OK);
    assert_eq!(
        same_account.headers()["x-data-revision"].to_str().unwrap(),
        before
    );
    let snapshot: serde_json::Value = same_account.json().await.unwrap();
    let other_account = client
        .get(format!("{base}/snapshot"))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(other_account.status(), StatusCode::OK);
    let mut profile = snapshot["profile"].clone();
    profile["monthlyIncome"] = serde_json::json!(98765);
    let saved = client
        .put(format!("{base}/profile"))
        .bearer_auth(&a.token)
        .header("If-Match", &before)
        .json(&profile)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let after = saved.headers()["x-data-revision"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(after, before);
    let saved_hash = db.export_sync_data().unwrap().content_hash().unwrap();
    release.send(()).unwrap();
    let quote = quote.await.unwrap();
    assert_eq!(quote.status(), StatusCode::OK); // An unrelated write does not create a new preview conflict.
    assert_eq!(quote.headers()["x-data-revision"].to_str().unwrap(), after);
    let result: SecurityPriceQuote = quote.json().await.unwrap();
    assert_eq!(result.provider_code, "http-fixture");
    assert_eq!(result.close, 123.45);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(db.revision().unwrap(), after);
    assert_eq!(
        db.export_sync_data().unwrap().content_hash().unwrap(),
        saved_hash
    );
    let stale = client
        .put(format!("{base}/profile"))
        .bearer_auth(&a.token)
        .header("If-Match", &before)
        .json(&profile)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        stale.json::<serde_json::Value>().await.unwrap()["revision"],
        after
    );
    assert_eq!(db.snapshot().unwrap().profile.monthly_income, 98765.0);
    assert_eq!(db.revision().unwrap(), after);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_watch_detail_reads_stay_account_scoped_and_preserve_stored_evidence() {
    crate::secrets::use_test_master_key();
    let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
    let accounts = Arc::new(Accounts::open(url.clone()).unwrap());
    let register = || {
        accounts
            .register(&Credentials {
                email: format!("{}@example.test", uuid::Uuid::new_v4()),
                password: "isolated-watch-password-123".into(),
            })
            .unwrap()
    };
    let a = register();
    let b = register();
    let db = Database::postgres(&url, &a.user_id).unwrap();
    let entry = db
        .save_watch(crate::models::watch::WatchInput {
            symbol: "600036.SS".into(),
            name: "私有公司".into(),
            thesis: "账户 A 的理由".into(),
        })
        .unwrap();
    let (data, day) = crate::models::watch::tests::market_fixture();
    let observation = crate::models::watch::evaluate(
        &Default::default(),
        "600036.SS",
        &data,
        day,
        "2026-07-03T12:00:00Z".into(),
    );
    db.save_watch_observation(
        &entry,
        Some(&observation),
        &observation.checked_at,
        day,
        None,
    )
    .unwrap();
    let rules = crate::models::watch::rules::WatchRules {
        min_yield_bps: 725,
        ..Default::default()
    };
    db.save_watch_rules(rules, Default::default()).unwrap();
    let revision = db.revision().unwrap();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    let provider = Arc::new(Prices {
        entered: Notify::new(),
        gate: tokio::sync::Mutex::new(None),
        calls: AtomicUsize::new(0),
    });
    let catalog = crate::application::watch_catalog::tests::fixture_service();
    catalog.refresh(Some("banks"), true).await.unwrap();
    let host = Arc::new(HostedState {
        tenant_connections: Arc::new(crate::storage::ConnectionPool::new(accounts.url.clone(), 8)),
        model_services: crate::composition::model_services(),
        accounts,
        openai: None,
        slots: tokio::sync::Semaphore::new(REQUEST_SLOTS),
        auth_slots: tokio::sync::Semaphore::new(AUTH_SLOTS),
        market: Arc::new(MarketServices::new(provider.clone(), provider.clone())),
        watch_provider: provider.clone(),
        watch_catalog: catalog.clone(),
        dividend_calendar: crate::application::dividend_calendar::tests::fixture_service(),
        analysis_executor: Arc::new(crate::agent::AgentExecutor),
    });
    let app = api_router().route_layer(middleware::from_fn_with_state(host, authenticate));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let client = reqwest::Client::new();
    let path = format!("{base}/watch/600036.SS/observation");
    assert_eq!(
        client.get(&path).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&path)
            .bearer_auth(&b.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let list: serde_json::Value = client
        .get(format!("{base}/watch"))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["rows"][0]["latest"].get("trends").is_none());
    let response = client
        .get(&path)
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("x-data-revision").unwrap(),
        revision.as_str()
    );
    let detail: serde_json::Value = response.json().await.unwrap();
    assert!(detail["latest"]["trends"].as_array().is_some());
    assert!(detail["latest"]["dividendHistory"].is_object());
    assert_eq!(detail["entry"]["thesis"], "账户 A 的理由");
    assert_eq!(detail["latest"]["rules"]["minYieldBps"], 725);
    let public_symbol = catalog
        .board()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.snapshot.latest.is_some())
        .unwrap()
        .company
        .symbol;
    let public: serde_json::Value = client
        .get(format!("{base}/watch/catalog/{public_symbol}/observation"))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(public["latest"]["trends"].is_array());
    assert_eq!(public["latest"]["rules"]["minYieldBps"], 725);
    assert_eq!(public["latest"]["yieldMatchesRule"], false);
    assert_eq!(db.revision().unwrap(), revision);
    assert_eq!(
        db.export_sync_data().unwrap().content_hash().unwrap(),
        before
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_json_import_preserves_authentication_revision_tenant_and_restore_boundaries() {
    use axum::routing::{get, post};
    use serde_json::json;
    crate::secrets::use_test_master_key();
    let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
    let accounts = Arc::new(Accounts::open(url.clone()).unwrap());
    let register = || {
        accounts
            .register(&Credentials {
                email: format!("{}@example.test", uuid::Uuid::new_v4()),
                password: "synthetic-import-password-123".into(),
            })
            .unwrap()
    };
    let a = register();
    let b = register();
    let db = Database::postgres(&url, &a.user_id).unwrap();
    let other = Database::postgres(&url, &b.user_id).unwrap();
    let before = db.revision().unwrap();
    let provider = Arc::new(Prices {
        entered: Notify::new(),
        gate: tokio::sync::Mutex::new(None),
        calls: AtomicUsize::new(0),
    });
    let host = Arc::new(HostedState {
        tenant_connections: Arc::new(crate::storage::ConnectionPool::new(accounts.url.clone(), 8)),
        model_services: crate::composition::model_services(),
        accounts,
        openai: None,
        slots: tokio::sync::Semaphore::new(REQUEST_SLOTS),
        auth_slots: tokio::sync::Semaphore::new(AUTH_SLOTS),
        market: Arc::new(MarketServices::new(provider.clone(), provider.clone())),
        watch_provider: provider,
        watch_catalog: crate::application::watch_catalog::tests::fixture_service(),
        dividend_calendar: crate::application::dividend_calendar::tests::fixture_service(),
        analysis_executor: Arc::new(crate::agent::AgentExecutor),
    });
    let app = api_router()
        .route("/api/data/import", post(super::import))
        .route("/api/data/export", get(super::export))
        .route_layer(middleware::from_fn_with_state(host, authenticate));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api", listener.local_addr().unwrap());
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let data = json!({"version":"1.0","exportedAt":"2026-01-06T01:00:00Z","data":{"accounts":[{
        "id":"external-user", "name":"导入测试账户", "watchlist":[{
            "code":"600900","name":"测试证券","price":12,"shares":100,"transactions":[
                {"type":"buy","qty":100,"price":10,"ts":1767225600000_i64}
            ]}], "feeConfig":{"enabled":false},"cashTrackingEnabled":false
    }]}});
    assert_eq!(
        client
            .post(format!("{base}/data/import"))
            .json(&data)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/data/import"))
            .bearer_auth(&a.token)
            .json(&data)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert!(db.watch_entries().unwrap().is_empty());
    let response = client
        .post(format!("{base}/data/import"))
        .bearer_auth(&a.token)
        .header("If-Match", &before)
        .json(&data)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let after = response.headers()["x-data-revision"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(before, after);
    assert_eq!(db.snapshot().unwrap().holding_trades.len(), 1);
    assert!(other.snapshot().unwrap().holdings.is_empty());
    assert!(other.watch_entries().unwrap().is_empty());
    assert_eq!(
        client
            .post(format!("{base}/data/import"))
            .bearer_auth(&a.token)
            .header("If-Match", &after)
            .json(&data)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let backup = client
        .get(format!("{base}/data/export"))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap()
        .json::<crate::db::SyncDataset>()
        .await
        .unwrap();
    let hash = backup.content_hash().unwrap();
    assert_eq!(
        client
            .post(format!("{base}/data/import?replace=true"))
            .bearer_auth(&a.token)
            .header("If-Match", &before)
            .json(&data)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut invalid = data.clone();
    invalid["data"]["accounts"][0]["watchlist"][0]["shares"] = json!(200);
    assert_eq!(
        client
            .post(format!("{base}/data/import?replace=true"))
            .bearer_auth(&a.token)
            .header("If-Match", &after)
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(hash, db.export_sync_data().unwrap().content_hash().unwrap());
    assert_eq!(
        client
            .post(format!("{base}/data/import?replace=true"))
            .bearer_auth(&a.token)
            .header("If-Match", &after)
            .json(&backup)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(hash, db.export_sync_data().unwrap().content_hash().unwrap());
}
