use super::{eastmoney, PublicWatchProvider};
use crate::models::watch::{evaluate, WatchIssue};
use crate::ports::market::WatchMarketProvider;
use chrono::{Duration as Days, NaiveDate};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

struct Server {
    url: String,
    paths: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(reply: impl Fn(&reqwest::Url) -> Value + Send + 'static) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let paths = Arc::new(Mutex::new(Vec::new()));
    let recorded = paths.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(request).unwrap();
            let path = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            recorded.lock().await.push(path.into());
            let value = reply(&reqwest::Url::parse(&format!("http://localhost{path}")).unwrap())
                .to_string();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{value}", value.len());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    Server { url, paths, task }
}

fn domestic(url: &reqwest::Url, missing_adjusted: bool) -> Value {
    let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["klt"], "101");
    assert_eq!(query["fields2"], "f51,f52,f53");
    assert_eq!(query["end"], "20261104");
    let (market, code) = query["secid"].split_once('.').unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    let rows: Vec<_> = (0..160)
        .rev()
        .filter_map(|n| {
            let date = day - Days::days(n);
            if chrono::Datelike::weekday(&date).num_days_from_monday() >= 5
                || (missing_adjusted && query["fqt"] == "2" && n == 1)
            {
                return None;
            }
            // Raw history contains a price-unit change; adjusted history is flat.
            let price = if query["fqt"] == "2" {
                200
            } else if n > 10 {
                50
            } else {
                100
            };
            Some(format!("{date},999,{price}"))
        })
        .collect();
    json!({"rc":0,"data":{"code":code,"market":market.parse::<u64>().unwrap(),"klines":rows}})
}

#[tokio::test]
async fn domestic_prices_work_without_yahoo_and_align_adjustments_by_date() {
    for missing in [false, true] {
        let server = serve(move |url| domestic(url, missing)).await;
        let provider = PublicWatchProvider::new().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        // An invalid Yahoo URL makes accidental dependence fail this test.
        let mut data = provider
            .prices("000807.SZ", day, &server.url, &["invalid://yahoo"])
            .await
            .unwrap();
        let (fixture, _) = crate::models::watch::tests::market_fixture();
        data.dividends = fixture.dividends;
        let result = evaluate("000807.SZ", &data, day, "test".into());
        assert_eq!(result.price, Some(100.0));
        assert_eq!(result.yield_pct, Some(5.0));
        assert_eq!(result.daily.is_none(), missing);
        assert_eq!(result.weekly.is_none(), missing);
        if missing {
            assert!(result.issues.contains(&WatchIssue::DailyHistoryIncomplete));
            assert!(data
                .bars
                .iter()
                .any(|b| b.day == day - Days::days(1) && b.adjusted_close.is_none()));
        } else {
            assert_eq!(result.daily.unwrap().middle, 100.0);
            assert_eq!(result.weekly.unwrap().middle, 100.0);
        }
        assert_eq!(server.paths.lock().await.len(), 3);
        assert!(data.sources.iter().all(|s| s.starts_with(&server.url)));
        assert!(data.sources.iter().any(|s| s.contains("secid=1.000001")));
    }
}

#[tokio::test]
async fn invalid_domestic_data_falls_back_as_a_whole_and_all_failures_stay_errors() {
    let server = serve(|url| {
        if url.path() == "/eastmoney" {
            // Fail only after both stock series succeeded: the fallback
            // observation must still contain no domestic price values.
            if !url
                .query_pairs()
                .any(|(k, v)| k == "secid" && v == "1.000001")
            {
                return domestic(url, false);
            }
            return json!({"rc":0,"data":{"code":"WRONG","market":0,"klines":["2026-11-04,1,1"]}});
        }
        let symbol = url.path().rsplit('/').next().unwrap();
        json!({"chart":{"error":null,"result":[{
            "meta":{"symbol":symbol,"currency":"CNY","exchangeTimezoneName":"Asia/Shanghai"},
            "timestamp":[1793750400],
            "indicators":{"quote":[{"close":[30.0]}],"adjclose":[{"adjclose":[20.0]}]}
        }]}})
    })
    .await;
    let provider = PublicWatchProvider::new().unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    let data = provider
        .prices(
            "000807.SZ",
            day,
            &format!("{}/eastmoney", server.url),
            &[&server.url],
        )
        .await
        .unwrap();
    assert_eq!(data.bars[0].close, Some(30.0));
    assert_eq!(data.bars[0].adjusted_close, Some(20.0));
    assert_eq!(data.bars[0].day, day);
    assert_eq!(data.sources.len(), 2);
    assert!(data
        .sources
        .iter()
        .all(|s| s.contains("/v8/finance/chart/")));
    assert_eq!(server.paths.lock().await.len(), 5);
    assert!(provider
        .prices(
            "000807.SZ",
            day,
            &format!("{}/eastmoney", server.url),
            &["invalid://yahoo"]
        )
        .await
        .is_err());
}

#[tokio::test]
#[ignore = "manual public-source connectivity check; requires internet"]
async fn live_yunlu_prices_and_dividends() {
    let provider = PublicWatchProvider::new().unwrap();
    let day = chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive()
        - Days::days(1);
    let mut data = eastmoney::observation(
        &provider.client,
        "https://push2his.eastmoney.com/api/qt/stock/kline/get",
        "000807.SZ",
        day,
    )
    .await
    .unwrap();
    let history = provider.dividends("000807.SZ", day).await.unwrap();
    data.dividends = Some(history.events);
    data.dividend_basis = Some(history.basis);
    let result = evaluate("000807.SZ", &data, day, chrono::Utc::now().to_rfc3339());
    assert!(result.price.is_some());
    assert!(result.yield_pct.is_some());
    assert!(result.daily.is_some());
    assert!(result.weekly.is_some());
    assert!(result.observed_on <= day.to_string());
    println!(
        "observed_on={} close={:?} yield_pct={:?} sources={:?}",
        result.observed_on, result.price, result.yield_pct, data.sources
    );
}
