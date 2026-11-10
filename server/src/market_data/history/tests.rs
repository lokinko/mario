use super::*;
use serde_json::json;
#[test]
fn domestic_daily_rows_keep_exact_prices_and_match_by_date() {
    let day = "2026-11-06".parse().unwrap();
    let parse = |payload: &Value, symbol: &str, day, adjusted| {
        let security = crate::models::market::eastmoney::security(
            symbol,
            if symbol.ends_with(".SS") {
                "SHH"
            } else {
                "SHZ"
            },
        )
        .unwrap();
        parse_rows(payload, &security, day, adjusted)
    };
    let value = json!({"rc":0,"data":{"code":"600941","market":1,"klines":["2026-11-05,1,100.24","2026-11-06,1,103.49"]}});
    let rows = parse(&value, "600941.SS", day, false).unwrap();
    assert_eq!(rows[&day].as_deref(), Some("103.49"));
    assert!(parse(&value, "600941.SZ", day, false).is_err());
    assert!(parse(&value, "600519.SS", day, false).is_err());
    let mut duplicate = value.clone();
    duplicate["data"]["klines"] = json!(["2026-11-06,1,2", "2026-11-06,1,3"]);
    assert!(parse(&duplicate, "600941.SS", day, false).is_err());
    let mut invalid = value;
    invalid["data"]["klines"] = json!(["2026-11-06,1,NaN"]);
    assert!(parse(&invalid, "600941.SS", day, false).is_err());
}
#[tokio::test]
#[ignore = "requires live public history APIs"]
async fn live_five_year_history() {
    let source = PublicMarketSource::new().unwrap();
    let today = crate::models::market::today().parse().unwrap();
    for symbol in [
        "600941.SS",
        "000001.SS",
        "518880.SS",
        "588000.SS",
        "508077.SS",
        "0700.HK",
        "QQQ",
        "PDD",
        "AAPL",
        "NVDA",
        "VOO",
        "QQQI",
    ] {
        let end = crate::models::market::close::completed_day(symbol, today, Utc::now());
        let h = source.history(symbol, end).await.unwrap();
        assert_eq!(h.window_start, history_start(end));
        assert!(h.bars.len() > 500);
        println!(
            "{symbol}: {} daily bars, {} .. {}, anchor {}, provider {}, splits {:?}, action sources {:?}",
            h.bars.len(),
            h.bars.first().unwrap().day,
            h.bars.last().unwrap().day,
            h.adjustment_anchor,
            h.provider_code,
            h.splits,
            h.action_source_urls
        );
    }
}

#[tokio::test]
async fn overseas_prices_try_eastmoney_first_and_use_yahoo_only_when_needed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    async fn reply(calls: &AtomicUsize, value: AppResult<PriceHistory>) -> AppResult<PriceHistory> {
        calls.fetch_add(1, Ordering::SeqCst);
        value
    }
    let day = "2026-11-06".parse().unwrap();
    for symbol in ["0700.HK", "QQQ", "PDD"] {
        for primary_ok in [true, false] {
            let primary_calls = AtomicUsize::new(0);
            let fallback_calls = AtomicUsize::new(0);
            let mut primary = crate::models::market::history::tests::fixture(symbol, day);
            primary.exchange = "primary".into();
            let mut fallback = primary.clone();
            fallback.exchange = "fallback".into();
            let history = coherent_history(
                symbol,
                day,
                reply(
                    &primary_calls,
                    if primary_ok { Ok(primary) } else { Err(bad()) },
                ),
                [
                    reply(&fallback_calls, Ok(fallback)),
                    reply(&fallback_calls, Err(bad())),
                ],
            )
            .await
            .unwrap();
            assert_eq!(primary_calls.load(Ordering::SeqCst), 1);
            assert_eq!(
                fallback_calls.load(Ordering::SeqCst),
                usize::from(!primary_ok)
            );
            assert_eq!(
                history.exchange,
                if primary_ok { "primary" } else { "fallback" }
            );
        }
    }
}

#[test]
fn listed_funds_keep_stock_distribution_rules_separate_and_reject_wrong_exchanges() {
    for symbol in [
        "518880.SS",
        "588000.SS",
        "508077.SS",
        "159920.SZ",
        "180101.SZ",
    ] {
        assert!(is_mainland_fund(symbol));
        assert!(!is_a_share_stock(symbol));
        assert_eq!(canonical_symbol(symbol).unwrap(), symbol);
    }
    for symbol in ["600941.SS", "000001.SS", "0700.HK", "QQQ", "PDD"] {
        assert!(!is_mainland_fund(symbol));
    }
    for (wrong, correct) in [("508077.SZ", "508077.SS"), ("180101.SS", "180101.SZ")] {
        assert!(canonical_symbol(wrong)
            .unwrap_err()
            .to_string()
            .contains(correct));
    }
}

#[tokio::test]
async fn mainland_fund_history_survives_both_yahoo_hosts_failing() {
    let day = "2026-11-06".parse().unwrap();
    for symbol in ["518880.SS", "588000.SS", "508077.SS"] {
        let history = crate::models::market::history::tests::fixture(symbol, day);
        let expected = history.revision().unwrap();
        let got = coherent_history(
            symbol,
            day,
            async { Ok(history) },
            [
                std::future::ready(Err(bad())),
                std::future::ready(Err(bad())),
            ],
        )
        .await
        .unwrap();
        assert_eq!(got.revision().unwrap(), expected);
    }
    let error = coherent_history(
        "518880.SS",
        day,
        async { Err(bad()) },
        [
            std::future::ready(Err(bad())),
            std::future::ready(Err(bad())),
        ],
    )
    .await
    .unwrap_err()
    .to_string();
    for source in ["东方财富", "Yahoo query1", "Yahoo query2", "保留原参考价"] {
        assert!(error.contains(source));
    }
}

#[tokio::test]
async fn public_get_retries_transient_status_once_but_not_missing_symbols() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let state = calls.clone();
    let missing_calls = Arc::new(AtomicUsize::new(0));
    let missing = missing_calls.clone();
    let offline_calls = Arc::new(AtomicUsize::new(0));
    let offline = offline_calls.clone();
    let invalid_calls = Arc::new(AtomicUsize::new(0));
    let invalid = invalid_calls.clone();
    let router = axum::Router::new()
        .route(
            "/",
            axum::routing::get(move || {
                let calls = state.clone();
                async move {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "busy")
                    } else {
                        (axum::http::StatusCode::OK, "{\"ok\":true}")
                    }
                }
            }),
        )
        .route(
            "/missing",
            axum::routing::get(move || {
                missing.fetch_add(1, Ordering::SeqCst);
                async { (axum::http::StatusCode::NOT_FOUND, "missing") }
            }),
        )
        .route(
            "/offline",
            axum::routing::get(move || {
                offline.fetch_add(1, Ordering::SeqCst);
                async { (axum::http::StatusCode::SERVICE_UNAVAILABLE, "busy") }
            }),
        )
        .route(
            "/invalid",
            axum::routing::get(move || {
                invalid.fetch_add(1, Ordering::SeqCst);
                async { "not JSON" }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    let source = PublicMarketSource::new().unwrap();
    assert_eq!(
        source
            .json(format!("http://{address}/").parse().unwrap())
            .await
            .unwrap()
            .0,
        json!({"ok":true})
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let error = source
        .json(format!("http://{address}/missing").parse().unwrap())
        .await
        .unwrap_err();
    assert_eq!(source_error(&error), "HTTP 404");
    assert_eq!(missing_calls.load(Ordering::SeqCst), 1);
    assert!(source
        .json(format!("http://{address}/offline").parse().unwrap())
        .await
        .is_err());
    assert_eq!(offline_calls.load(Ordering::SeqCst), 2);
    assert!(source
        .json(format!("http://{address}/invalid").parse().unwrap())
        .await
        .is_err());
    assert_eq!(invalid_calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[test]
fn yahoo_history_preserves_exact_numbers_and_rejects_misaligned_or_wrong_market_rows() {
    let end = "2026-11-06".parse::<NaiveDate>().unwrap();
    let previous = end - chrono::Duration::days(1);
    let mut payload = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"AAPL","currency":"USD","exchangeTimezoneName":"America/New_York","exchangeName":"NMS","instrumentType":"EQUITY"},"timestamp":[previous.and_hms_opt(14,0,0).unwrap().and_utc().timestamp(),end.and_hms_opt(14,0,0).unwrap().and_utc().timestamp()],"indicators":{"quote":[{"close":[80,100]}],"adjclose":[{"adjclose":[40,50]}]},"events":{}}]}});
    payload["chart"]["result"][0]["indicators"]["quote"][0]["close"][0] =
        serde_json::from_str("12345678901234.01").unwrap();
    let url = "https://query1.finance.yahoo.com/v8/finance/chart/AAPL?interval=1d";
    let h = parse_yahoo(&payload, &payload, "AAPL", end, url.into(), url.into()).unwrap();
    assert_eq!(h.bars[0].raw_close.as_deref(), Some("12345678901234.01"));
    assert_eq!(h.bars[0].adjusted_close.as_deref(), Some("80"));
    for path in ["length", "duplicate", "zone"] {
        let mut wrong = payload.clone();
        match path {
            "length" => {
                wrong["chart"]["result"][0]["indicators"]["adjclose"][0]["adjclose"] = json!([50])
            }
            "duplicate" => {
                wrong["chart"]["result"][0]["timestamp"][0] =
                    wrong["chart"]["result"][0]["timestamp"][1].clone()
            }
            _ => {
                wrong["chart"]["result"][0]["meta"]["exchangeTimezoneName"] = "Asia/Shanghai".into()
            }
        }
        assert!(parse_yahoo(&wrong, &wrong, "AAPL", end, url.into(), url.into()).is_err());
    }
    assert!(parse_yahoo(&payload, &payload, "MSFT", end, url.into(), url.into()).is_err());
}

#[tokio::test]
async fn fallback_keeps_complete_source_revisions_and_tries_a_second_lagging_yahoo_host() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let day = "2026-11-06".parse().unwrap();
    async fn reply(
        calls: &AtomicUsize,
        result: AppResult<PriceHistory>,
    ) -> AppResult<PriceHistory> {
        calls.fetch_add(1, Ordering::SeqCst);
        result
    }
    for mode in [
        "domestic",
        "primary_lagging",
        "failure",
        "lagging",
        "all_failed",
    ] {
        let domestic_calls = AtomicUsize::new(0);
        let yahoo_calls = AtomicUsize::new(0);
        let mut primary = crate::models::market::history::tests::fixture("600941.SS", day);
        primary.exchange = "domestic".into();
        let mut old = primary.clone();
        old.exchange = "old-yahoo".into();
        old.bars.pop();
        old = old.normalize().unwrap();
        let mut fresh = primary.clone();
        fresh.exchange = "fresh-yahoo".into();
        let primary_result = if mode == "domestic" {
            Ok(primary)
        } else if mode == "primary_lagging" {
            Ok(old.clone())
        } else {
            Err(bad())
        };
        let first = if mode == "lagging" {
            Ok(old)
        } else {
            Err(bad())
        };
        let second = if mode == "all_failed" {
            Err(bad())
        } else {
            Ok(fresh)
        };
        let result = coherent_history(
            "600941.SS",
            day,
            reply(&domestic_calls, primary_result),
            [reply(&yahoo_calls, first), reply(&yahoo_calls, second)],
        )
        .await;
        assert_eq!(domestic_calls.load(Ordering::SeqCst), 1);
        if mode == "all_failed" {
            assert!(result.is_err());
        } else {
            let h = result.unwrap();
            assert_eq!(
                h.exchange,
                if mode == "domestic" {
                    "domestic"
                } else {
                    "fresh-yahoo"
                }
            );
            assert_eq!(h.adjustment_anchor, day);
            h.validate().unwrap();
        }
        assert_eq!(
            yahoo_calls.load(Ordering::SeqCst),
            if mode == "domestic" { 0 } else { 2 }
        );
    }
}
