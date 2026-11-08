use super::*;
use serde_json::json;
#[test]
fn domestic_daily_rows_keep_exact_prices_and_match_by_date() {
    let day = "2026-11-06".parse().unwrap();
    let value = json!({"rc":0,"data":{"code":"600941","market":1,"klines":["2026-11-05,1,100.24","2026-11-06,1,103.49"]}});
    let rows = parse_domestic(&value, "600941.SS", day, false).unwrap();
    assert_eq!(rows[&day].as_deref(), Some("103.49"));
    assert!(parse_domestic(&value, "600941.SZ", day, false).is_err());
    assert!(parse_domestic(&value, "600519.SS", day, false).is_err());
    let mut duplicate = value.clone();
    duplicate["data"]["klines"] = json!(["2026-11-06,1,2", "2026-11-06,1,3"]);
    assert!(parse_domestic(&duplicate, "600941.SS", day, false).is_err());
    let mut invalid = value;
    invalid["data"]["klines"] = json!(["2026-11-06,1,NaN"]);
    assert!(parse_domestic(&invalid, "600941.SS", day, false).is_err());
}
#[tokio::test]
#[ignore = "requires live public history APIs"]
async fn live_five_year_history() {
    let source = PublicMarketSource::new().unwrap();
    let today = crate::models::market::today().parse().unwrap();
    for symbol in ["600941.SS", "000001.SS", "AAPL"] {
        let end = crate::models::market::close::completed_day(symbol, today, Utc::now());
        let h = source.history(symbol, end).await.unwrap();
        assert_eq!(h.window_start, history_start(end));
        assert!(h.bars.len() > 500);
        println!(
            "{symbol}: {} daily bars, {} .. {}, anchor {}, provider {}",
            h.bars.len(),
            h.bars.first().unwrap().day,
            h.bars.last().unwrap().day,
            h.adjustment_anchor,
            h.provider_code
        );
    }
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
