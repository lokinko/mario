use super::*;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn prices() -> Value {
    json!({"rc":0,"data":{"code":"600941","market":1,"klines":[
        "2026-11-04,103.08,101.20", "2026-11-05,101.11,100.24", "2026-11-06,100.17,103.49"
    ]}})
}
fn actions(page: usize) -> Value {
    let mut row = json!({"SECUCODE":"600941.SH","SECURITY_CODE":"600941","ASSIGN_PROGRESS":"实施分配","EX_DIVIDEND_DATE":"2026-09-30 00:00:00","REPORT_DATE":"2026-06-30 00:00:00","BONUS_RATIO":2,"IT_RATIO":1,"PRETAX_BONUS_RMB":10});
    if page == 2 {
        row["EX_DIVIDEND_DATE"] = "2026-06-05 00:00:00".into();
        row["REPORT_DATE"] = "2025-12-31 00:00:00".into();
        row["BONUS_RATIO"] = Value::Null;
        row["IT_RATIO"] = Value::Null;
    }
    json!({"success":true,"result":{"pages":2,"count":2,"data":[row]}})
}

#[test]
fn daily_pairs_use_exact_close_and_reject_wrong_identity_duplicates_and_bad_values() {
    let day = "2026-11-06".parse().unwrap();
    let before = "2026-11-05".parse().unwrap();
    assert_eq!(
        parse_close(&prices(), "600941.SS", day, day).unwrap(),
        (day, Decimal::from_str_exact("103.49").unwrap())
    );
    assert_eq!(
        parse_close(&prices(), "600941.SS", day, before)
            .unwrap()
            .1
            .to_string(),
        "100.24"
    );
    for symbol in [
        "600941.SZ",
        "600519.SS",
        "510300.SS",
        "000001.SS",
        "159330.SZ",
    ] {
        assert!(parse_close(&prices(), symbol, day, day).is_err());
    }
    for row in [
        "2026-11-06,1,NaN",
        "2026-11-06,1,-1",
        "2026-11-06,1,0",
        "invalid,1,10",
        "2026-11-06,1,2,3",
    ] {
        let mut invalid = prices();
        invalid["data"]["klines"] = json!([row]);
        assert!(parse_close(&invalid, "600941.SS", day, day).is_err());
    }
    let mut duplicate = prices();
    duplicate["data"]["klines"] = json!(["2026-11-06,1,2", "2026-11-06,1,3"]);
    assert!(parse_close(&duplicate, "600941.SS", day, day).is_err());
}

#[tokio::test]
async fn domestic_quotes_collect_all_action_pages_and_never_guess_when_actions_are_incomplete() {
    for failed in [false, true] {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let recorded = paths.clone();
        let router = axum::Router::new()
            .route(
                "/prices",
                axum::routing::get(|| async { axum::Json(prices()) }),
            )
            .route(
                "/actions",
                axum::routing::get(
                    move |axum::extract::Query(query): axum::extract::Query<
                        BTreeMap<String, String>,
                    >| {
                        let recorded = recorded.clone();
                        async move {
                            let page = query["pageNumber"].parse::<usize>().unwrap();
                            recorded.lock().unwrap().push(page);
                            let mut payload = actions(page);
                            if failed && page == 2 {
                                payload["success"] = false.into();
                            }
                            axum::Json(payload)
                        }
                    },
                ),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let provider = AshareSecurityPriceProvider {
            client: Client::new(),
            price_url: format!("{url}/prices"),
            actions_url: format!("{url}/actions"),
        };
        let result = provider
            .quote(
                &SecurityPriceQuery {
                    symbol: "600941.SS".into(),
                    on_date: "2026-11-08".into(),
                    action_from: Some("2026-01-01".into()),
                },
                "",
            )
            .await;
        assert_eq!(*paths.lock().unwrap(), vec![1, 2]);
        if failed {
            assert!(result.is_err());
        } else {
            let mut quote = result.unwrap();
            assert_eq!(quote.observed_on, "2026-11-06");
            assert_eq!(quote.exact_close.as_deref(), Some("103.49"));
            assert_eq!(quote.price_basis, "unadjusted_daily_close");
            assert_eq!(quote.splits.len(), 1);
            assert_eq!(
                (&*quote.splits[0].numerator, &*quote.splits[0].denominator),
                ("13", "10")
            );
            // Persisting the canonical source and backup recovery use the same validator.
            quote.source_url = "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=1.600941&klt=101&fqt=0".into();
            let db = crate::db::Database::test();
            let input=serde_json::from_value(json!({"symbol":"600941.SS","name":"测试持仓","assetClass":"股票","currency":"CNY","quantity":10,"currentPrice":100.49,"marketValue":1004.9,"costBasis":1000,"targetPct":0,"valuationDate":"2026-01-01"})).unwrap();
            let id = db.add_holding(&input).unwrap().holdings[0].id.clone();
            let pending_db = crate::db::Database::test();
            let pending_id = pending_db.add_holding(&input).unwrap().holdings[0]
                .id
                .clone();
            assert!(pending_db
                .apply_verified_holding_valuation(&pending_id, 10.0, &quote)
                .is_err());
            let pending = pending_db.snapshot().unwrap();
            assert_eq!(pending.corporate_actions.len(), 1);
            assert_eq!(pending.holdings[0].current_price, Some(100.49));
            quote.splits.clear();
            let saved = db
                .apply_verified_holding_valuation(&id, 10.0, &quote)
                .unwrap();
            assert_eq!(saved.holdings[0].current_price, Some(103.49));
            let restored = crate::db::Database::test();
            restored
                .import_sync_data(&db.export_sync_data().unwrap())
                .unwrap();
            assert_eq!(
                restored.snapshot().unwrap().holding_valuations[0].provider_code,
                "eastmoney_daily_close"
            );
            for forged in [
                quote
                    .source_url
                    .replace("push2his.eastmoney.com", "attacker.test"),
                quote.source_url.replace("fqt=0", "fqt=1"),
                quote.source_url.replace("1.600941", "0.600941"),
                format!("{}&fqt=1", quote.source_url),
            ] {
                let mut invalid = quote.clone();
                invalid.source_url = forged;
                assert!(db
                    .apply_verified_holding_valuation(&id, 10.0, &invalid)
                    .is_err());
            }
        }
        task.abort();
    }
}
