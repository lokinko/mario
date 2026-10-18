//! Free, process-wide quotes. Only public market data is shared across accounts.
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};

use crate::{
    models::{SecurityPriceQuery, SecurityPriceQuote},
    ports::market::SecurityPriceProvider,
    AppError, AppResult,
};

use crate::models::market::{canonical_symbol, METHODOLOGY_URL};
const SUCCESS_TTL: Duration = Duration::from_secs(3600);
const FAILURE_TTL: Duration = Duration::from_secs(60);
const MAX_ENTRIES: usize = 512;
type QuoteResult = Result<SecurityPriceQuote, String>;
type QuoteEntries = BTreeMap<(String, String, Option<String>), Arc<Mutex<Entry>>>;

#[derive(Default)]
struct Entry {
    fetched: Option<(Instant, QuoteResult)>,
}
#[derive(Default)]
struct Cache {
    entries: Mutex<QuoteEntries>,
}
pub struct FreeSecurityPriceProvider {
    client: Client,
    hosts: Vec<String>,
    cache: Arc<Cache>,
    slots: Semaphore,
}
impl FreeSecurityPriceProvider {
    pub fn new() -> AppResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent("Mozilla/5.0 (compatible; mario/market-data)")
            .build()?;
        Ok(Self {
            client,
            hosts: vec![
                "https://query1.finance.yahoo.com".into(),
                "https://query2.finance.yahoo.com".into(),
            ],
            cache: Arc::new(Cache::default()),
            slots: Semaphore::new(4),
        })
    }

    async fn fetch(
        &self,
        symbol: &str,
        day: NaiveDate,
        action_from: Option<&str>,
    ) -> AppResult<SecurityPriceQuote> {
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| AppError::MarketData("行情服务已停止".into()))?;
        let end = day.min(Utc::now().date_naive() - chrono::Duration::days(1));
        // Yahoo normalizes older closes to current share units, so a historical
        // valuation of a managed position still needs actions through today.
        let event_end = if action_from.is_some() {
            end.max(Utc::now().date_naive())
        } else {
            end
        };
        let start = if let Some(from) = action_from {
            let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")
                .map_err(|_| AppError::Validation("公司行动检查起始日无效".into()))?;
            if from > day {
                return Err(AppError::Validation(
                    "公司行动检查起始日不能晚于请求日".into(),
                ));
            }
            from.min(day - chrono::Duration::days(10))
        } else {
            day - chrono::Duration::days(10)
        };
        let mut last_error = None;
        for host in &self.hosts {
            let mut url =
                Url::parse(host).map_err(|_| AppError::MarketData("行情地址无效".into()))?;
            url.path_segments_mut()
                .map_err(|_| AppError::MarketData("行情地址无效".into()))?
                .extend(["v8", "finance", "chart", symbol]);
            url.query_pairs_mut()
                .append_pair(
                    "period1",
                    &start
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp()
                        .to_string(),
                )
                // Buffer both sides for exchange-local dates; the parser enforces the cutoff.
                .append_pair(
                    "period2",
                    &(event_end + chrono::Duration::days(2))
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp()
                        .to_string(),
                )
                .append_pair("interval", "1d")
                .append_pair("events", "splits")
                .append_pair("includeAdjustedClose", "false");
            let result = async {
                let mut response = self.client.get(url.clone()).send().await?;
                if !response.status().is_success() {
                    return Err(AppError::MarketData(format!(
                        "Yahoo Finance 返回 HTTP {}",
                        response.status()
                    )));
                }
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    if body.len() + chunk.len() > 1024 * 1024 {
                        return Err(AppError::MarketData("行情响应超过 1 MB".into()));
                    }
                    body.extend_from_slice(&chunk);
                }
                parse_quote(&body, symbol, day, end, url.to_string())
            }
            .await;
            match result {
                Ok(quote) => return Ok(quote),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::MarketData("行情来源未配置".into())))
    }
}

#[async_trait]
impl SecurityPriceProvider for FreeSecurityPriceProvider {
    async fn quote(
        &self,
        query: &SecurityPriceQuery,
        _api_key: &str,
    ) -> AppResult<SecurityPriceQuote> {
        let symbol = canonical_symbol(&query.symbol)?;
        let day = NaiveDate::parse_from_str(query.on_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("证券价格日期必须使用 YYYY-MM-DD".into()))?;
        let today = NaiveDate::parse_from_str(&crate::models::market::today(), "%Y-%m-%d").unwrap();
        if day > today || day < NaiveDate::from_ymd_opt(1970, 1, 12).unwrap() {
            return Err(AppError::Validation("证券价格日期超出支持范围".into()));
        }
        let key = (symbol.clone(), day.to_string(), query.action_from.clone());
        let entry = {
            let mut cache = self.cache.entries.lock().await;
            if !cache.contains_key(&key) && cache.len() >= MAX_ENTRIES {
                // Never evict an in-flight entry: all waiters must share the same request.
                let removable = cache
                    .iter()
                    .find(|(_, cell)| Arc::strong_count(cell) == 1)
                    .map(|(key, _)| key.clone());
                if let Some(key) = removable {
                    cache.remove(&key);
                } else {
                    return Err(AppError::RateLimit("行情缓存繁忙，请稍后重试".into()));
                }
            }
            cache.entry(key).or_default().clone()
        };
        let mut entry = entry.lock().await;
        let result = if let Some((_, result)) = entry.fetched.as_ref().filter(|(at, result)| {
            at.elapsed()
                < if result.is_ok() {
                    SUCCESS_TTL
                } else {
                    FAILURE_TTL
                }
        }) {
            result.clone()
        } else {
            let result = self
                .fetch(&symbol, day, query.action_from.as_deref())
                .await
                .map_err(|error| match error {
                    AppError::MarketData(message) => message,
                    other => other.to_string(),
                });
            entry.fetched = Some((Instant::now(), result.clone()));
            result
        };
        let mut quote = result.map_err(AppError::MarketData)?;
        // Preserve the user's code: price updates must never change a trade's identity.
        quote.symbol = query.symbol.trim().into();
        Ok(quote)
    }
}

#[derive(Deserialize)]
struct Payload {
    chart: Chart,
}
#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
    error: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct ChartResult {
    meta: Meta,
    #[serde(default)]
    timestamp: Vec<i64>,
    indicators: Indicators,
    #[serde(default)]
    events: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    symbol: String,
    currency: String,
    exchange_name: String,
    exchange_timezone_name: String,
    instrument_type: String,
}
#[derive(Deserialize)]
struct Indicators {
    quote: Vec<Closes>,
}
#[derive(Deserialize)]
struct Closes {
    close: Vec<Option<serde_json::Number>>,
}

fn parse_quote(
    bytes: &[u8],
    symbol: &str,
    day: NaiveDate,
    end: NaiveDate,
    source_url: String,
) -> AppResult<SecurityPriceQuote> {
    let payload: Payload = serde_json::from_slice(bytes)
        .map_err(|_| AppError::MarketData("无法解析 Yahoo Finance 行情".into()))?;
    if payload.chart.error.is_some() {
        return Err(AppError::MarketData(
            "Yahoo Finance 无此标的或暂时不可用".into(),
        ));
    }
    let result = payload
        .chart
        .result
        .and_then(|r| r.into_iter().next())
        .ok_or_else(|| AppError::MarketData("行情没有有效数据".into()))?;
    // Yahoo quotes London instruments in pence (GBp/GBX), not pounds.
    let pence = result.meta.currency == "GBp" || result.meta.currency == "GBX";
    let currency = if pence {
        "GBP".into()
    } else {
        result.meta.currency.to_ascii_uppercase()
    };
    if result.meta.symbol.to_ascii_uppercase() != symbol
        || currency.len() != 3
        || !currency.bytes().all(|b| b.is_ascii_alphabetic())
    {
        return Err(AppError::MarketData("行情代码或币种不匹配".into()));
    }
    let zone: chrono_tz::Tz = result
        .meta
        .exchange_timezone_name
        .parse()
        .map_err(|_| AppError::MarketData("行情交易所时区无效".into()))?;
    let prices = result
        .indicators
        .quote
        .first()
        .ok_or_else(|| AppError::MarketData("行情缺少收盘价".into()))?;
    let selected = result
        .timestamp
        .iter()
        .zip(&prices.close)
        .filter_map(|(ts, close)| {
            let date = DateTime::from_timestamp(*ts, 0)?
                .with_timezone(&zone)
                .date_naive();
            let raw = close.as_ref()?.to_string();
            let price = rust_decimal::Decimal::from_str_exact(&raw)
                .or_else(|_| rust_decimal::Decimal::from_scientific(&raw))
                .ok()?;
            (date <= end
                && (day - date).num_days() <= 10
                && price > rust_decimal::Decimal::ZERO
                && price <= rust_decimal::Decimal::from(1_000_000_000_000_000_i64))
            .then_some((date, price))
        })
        .max_by_key(|(day, _)| *day)
        .ok_or_else(|| AppError::MarketData("最近 10 天内没有已完成的有效日收盘价".into()))?;
    let mut splits = Vec::new();
    if let Some(events) = result.events.get("splits").and_then(|v| v.as_object()) {
        for event in events.values() {
            let timestamp = event
                .get("date")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::MarketData("拆股日期无效".into()))?;
            let effective = DateTime::from_timestamp(timestamp, 0)
                .ok_or_else(|| AppError::MarketData("拆股时间无效".into()))?
                .with_timezone(&zone)
                .date_naive();
            // A split effective today may already adjust yesterday's close.
            if effective.to_string() > crate::models::market::today() {
                continue;
            }
            let ratio = |key: &str| -> AppResult<String> {
                let value = event
                    .get(key)
                    .ok_or_else(|| AppError::MarketData("拆股比例缺失".into()))?;
                let text = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                let number = rust_decimal::Decimal::from_str_exact(&text)
                    .map_err(|_| AppError::MarketData("拆股比例无效".into()))?;
                if number <= rust_decimal::Decimal::ZERO {
                    return Err(AppError::MarketData("拆股比例须为正数".into()));
                }
                Ok(number.normalize().to_string())
            };
            splits.push(crate::models::SplitEvent {
                effective_on: effective.to_string(),
                numerator: ratio("numerator")?,
                denominator: ratio("denominator")?,
            });
        }
    }
    use rust_decimal::prelude::ToPrimitive;
    let exact_close = if pence {
        selected.1 / rust_decimal::Decimal::from(100)
    } else {
        selected.1
    };
    Ok(SecurityPriceQuote {
        exact_close: Some(exact_close.normalize().to_string()),
        splits,
        symbol: symbol.into(), currency, close: exact_close.to_f64().ok_or_else(||AppError::MarketData("价格超出范围".into()))?,
        requested_on: day.to_string(), observed_on: selected.0.to_string(),
        staleness_days: (day - selected.0).num_days(),
        provider_code: "yahoo_daily_close".into(), provider_name: "Yahoo Finance".into(),
        exchange: result.meta.exchange_name, mic_code: String::new(), instrument_type: result.meta.instrument_type,
        price_basis: "split_adjusted_daily_close".into(), source_url, methodology_url: METHODOLOGY_URL.into(),
        disclaimer: format!("免费公开日线接口，无可用性保证；收盘价可能经过拆股调整，不采用分红复权价。仅供参考，不等于实时成交价；公司行动及历史数量需自行核对。{}", if pence { "源价格以便士 GBp/GBX 计价，已除以 100 换算为 GBP。" } else { "" }),
    })
}

/// Refresh recently requested current quotes without touching account data.
pub fn start_refresh(provider: Arc<FreeSecurityPriceProvider>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let today = crate::models::market::today();
            let yesterday = (Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive()
                - chrono::Duration::days(1))
            .to_string();
            let symbols: std::collections::BTreeSet<_> = provider
                .cache
                .entries
                .lock()
                .await
                .keys()
                .filter(|(_, day, _)| day >= &yesterday)
                .map(|(symbol, _, _)| symbol.clone())
                .collect();
            for symbol in symbols {
                let _ = provider
                    .quote(
                        &SecurityPriceQuery {
                            action_from: None,
                            symbol,
                            on_date: today.clone(),
                        },
                        "",
                    )
                    .await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }
    #[test]
    fn split_effective_today_is_retained_when_only_previous_close_is_completed() {
        let mut value: serde_json::Value =
            serde_json::from_slice(&payload("AAPL", "USD", &[("2026-09-07", Some(10.0))])).unwrap();
        value["chart"]["result"][0]["indicators"]["quote"][0]["close"][0] =
            serde_json::from_str("12345678901234.01").unwrap();
        value["chart"]["result"][0]["events"] = serde_json::json!({"splits":{"event":{"date":day("2026-09-08").and_hms_opt(14,0,0).unwrap().and_utc().timestamp(),"numerator":2,"denominator":1}}});
        let q = parse_quote(
            &serde_json::to_vec(&value).unwrap(),
            "AAPL",
            day("2026-09-08"),
            day("2026-09-07"),
            String::new(),
        )
        .unwrap();
        assert_eq!(q.exact_close.as_deref(), Some("12345678901234.01"));
        assert_eq!(q.observed_on, "2026-09-07");
        assert_eq!(q.splits[0].effective_on, "2026-09-08");
        let previous_request = parse_quote(
            &serde_json::to_vec(&value).unwrap(),
            "AAPL",
            day("2026-09-07"),
            day("2026-09-07"),
            String::new(),
        )
        .unwrap();
        assert_eq!(previous_request.splits[0].effective_on, "2026-09-08");
    }
    fn payload(symbol: &str, currency: &str, dates: &[(&str, Option<f64>)]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"chart":{"error":null,"result":[{
            "meta":{"symbol":symbol,"currency":currency,"exchangeName":"NMS","exchangeTimezoneName":"America/New_York","instrumentType":"EQUITY"},
            "timestamp":dates.iter().map(|(d,_)| day(d).and_hms_opt(14,0,0).unwrap().and_utc().timestamp()).collect::<Vec<_>>(),
            "indicators":{"quote":[{"close":dates.iter().map(|(_,price)| price).collect::<Vec<_>>() }],"adjclose":[{"adjclose":[1,1,1]}]}
        }]}})).unwrap()
    }
    #[test]
    fn parser_uses_completed_exchange_dates_and_close_not_adjclose() {
        let bytes = payload(
            "AAPL",
            "USD",
            &[
                ("2026-09-04", Some(220.0)),
                ("2026-09-07", Some(230.0)),
                ("2026-09-08", Some(999.0)),
            ],
        );
        let q = parse_quote(
            &bytes,
            "AAPL",
            day("2026-09-08"),
            day("2026-09-07"),
            "https://query1.finance.yahoo.com/v8/finance/chart/AAPL?interval=1d".into(),
        )
        .unwrap();
        assert_eq!(
            (q.close, q.observed_on.as_str(), q.staleness_days),
            (230.0, "2026-09-07", 1)
        );
        assert!(parse_quote(
            &bytes,
            "MSFT",
            day("2026-09-08"),
            day("2026-09-07"),
            String::new()
        )
        .is_err());
        for price in [None, Some(0.0), Some(-10.0), Some(1e16)] {
            assert!(parse_quote(
                &payload("AAPL", "USD", &[("2026-09-07", price)]),
                "AAPL",
                day("2026-09-08"),
                day("2026-09-07"),
                String::new()
            )
            .is_err());
        }
        assert!(parse_quote(
            &bytes,
            "AAPL",
            day("2026-10-01"),
            day("2026-09-30"),
            String::new()
        )
        .is_err());
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Tokyo midnight is the previous UTC calendar date.
        json["chart"]["result"][0]["meta"]["exchangeTimezoneName"] = "Asia/Tokyo".into();
        json["chart"]["result"][0]["timestamp"] = serde_json::json!([day("2026-09-06")
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()]);
        let q = parse_quote(
            &serde_json::to_vec(&json).unwrap(),
            "AAPL",
            day("2026-09-08"),
            day("2026-09-07"),
            String::new(),
        )
        .unwrap();
        assert_eq!(q.observed_on, "2026-09-07");
    }
    #[test]
    fn codes_are_unambiguous_and_aliases_share_keys() {
        for (input, expected) in [
            (" aapl ", "AAPL"),
            ("00700.HK", "0700.HK"),
            ("sh600519", "600519.SS"),
            ("600519.SH", "600519.SS"),
            ("601988.sh", "601988.SS"),
            ("000001.SH", "000001.SS"),
            ("002241.SZ", "002241.SZ"),
            ("sz002241", "002241.SZ"),
            ("sz000001", "000001.SZ"),
            ("601988.SS", "601988.SS"),
            ("sh601988", "601988.SS"),
            ("000001.SS", "000001.SS"), // Shanghai Composite index, not a stock.
            ("510300.SS", "510300.SS"),
            ("513380.SS", "513380.SS"),
            ("sh513380", "513380.SS"),
            ("159330.SZ", "159330.SZ"),
            ("sz159330", "159330.SZ"),
        ] {
            assert_eq!(canonical_symbol(input).unwrap(), expected);
        }
        for input in [
            "",
            "600519",
            "00700",
            "../secret",
            "https://bad.test",
            "AAPL?x=1",
            "中文",
        ] {
            assert!(canonical_symbol(input).is_err());
        }
    }
    #[test]
    fn shanghai_equities_with_shenzhen_suffix_give_a_correction() {
        for input in ["601988.SZ", "sz601988", "600519.SZ", "688981.SZ"] {
            let error = canonical_symbol(input).unwrap_err();
            assert!(matches!(error, AppError::Validation(_)));
            assert!(error.to_string().contains("交易所后缀不匹配"));
            assert!(error.to_string().contains(".SS"));
        }
    }
    #[test]
    fn shenzhen_equities_with_shanghai_suffix_give_a_correction() {
        for (input, expected) in [
            ("002241.SH", "002241.SZ"),
            ("002241.SS", "002241.SZ"),
            ("sh002241", "002241.SZ"),
            ("003816.SH", "003816.SZ"),
            ("300750.SS", "300750.SZ"),
            ("301308.SH", "301308.SZ"),
        ] {
            let error = canonical_symbol(input).unwrap_err();
            assert!(matches!(error, AppError::Validation(_)));
            assert!(error.to_string().contains("深交所"));
            assert!(error.to_string().contains(&format!("请改为 {expected}")));
        }
    }
    #[test]
    fn etf_exchange_errors_give_the_correct_listing_suffix() {
        for (input, expected) in [
            ("513380.SZ", "513380.SS"),
            ("sz513380", "513380.SS"),
            ("510300.SZ", "510300.SS"),
            ("560800.SZ", "560800.SS"),
            ("588000.SZ", "588000.SS"),
            ("159330.SS", "159330.SZ"),
            ("159330.SH", "159330.SZ"),
            ("sh159330", "159330.SZ"),
        ] {
            let error = canonical_symbol(input).unwrap_err();
            assert!(matches!(error, AppError::Validation(_)));
            assert!(error.to_string().contains(&format!("请改为 {expected}")));
        }
    }
    #[test]
    fn pence_prices_are_converted_to_pounds() {
        for currency in ["GBp", "GBX"] {
            let q = parse_quote(
                &payload("VOD.L", currency, &[("2026-09-07", Some(120.0))]),
                "VOD.L",
                day("2026-09-08"),
                day("2026-09-07"),
                String::new(),
            )
            .unwrap();
            assert_eq!(q.currency, "GBP");
            assert_eq!(q.close, 1.2);
        }
    }
    async fn mock_provider() -> (
        Arc<FreeSecurityPriceProvider>,
        Arc<AtomicUsize>,
        Arc<AtomicBool>,
        tokio::task::JoinHandle<()>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let counter = calls.clone();
        let fail = failed.clone();
        let router = axum::Router::new().route(
            "/v8/finance/chart/{symbol}",
            axum::routing::get(move || {
                let counter = counter.clone();
                let fail = fail.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(15)).await;
                    if fail.load(Ordering::SeqCst) {
                        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Vec::new());
                    }
                    (
                        axum::http::StatusCode::OK,
                        payload("AAPL", "USD", &[("2026-09-07", Some(230.0))]),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (
            Arc::new(FreeSecurityPriceProvider {
                client: Client::new(),
                hosts: vec![url.clone(), url],
                cache: Arc::new(Cache::default()),
                slots: Semaphore::new(4),
            }),
            calls,
            failed,
            task,
        )
    }
    #[tokio::test]
    async fn separate_runtime_instances_do_not_share_cached_prices() {
        let (first, calls, _, task) = mock_provider().await;
        let second = FreeSecurityPriceProvider {
            client: first.client.clone(),
            hosts: first.hosts.clone(),
            cache: Arc::new(Cache::default()),
            slots: Semaphore::new(4),
        };
        let query = SecurityPriceQuery {
            symbol: "AAPL".into(),
            on_date: "2026-09-08".into(),
            action_from: None,
        };
        first.quote(&query, "").await.unwrap();
        second.quote(&query, "").await.unwrap();
        first.quote(&query, "").await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        task.abort();
    }
    #[tokio::test]
    async fn wrong_exchange_is_rejected_before_fetch_and_cached_http_errors_are_not_rewrapped() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let router = axum::Router::new().route(
            "/v8/finance/chart/{symbol}",
            axum::routing::get(
                move |axum::extract::Path(symbol): axum::extract::Path<String>| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        if symbol == "601988.SS" {
                            (
                                axum::http::StatusCode::OK,
                                payload(&symbol, "CNY", &[("2026-09-07", Some(5.13))]),
                            )
                        } else {
                            (axum::http::StatusCode::NOT_FOUND, Vec::new())
                        }
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let provider = FreeSecurityPriceProvider {
            client: Client::new(),
            hosts: vec![url.clone(), url],
            cache: Arc::new(Cache::default()),
            slots: Semaphore::new(4),
        };
        let mut query = SecurityPriceQuery {
            action_from: None,
            symbol: "601988.SZ".into(),
            on_date: "2026-09-08".into(),
        };
        for (symbol, expected) in [
            ("601988.SZ", "601988.SS"),
            ("513380.SZ", "513380.SS"),
            ("159330.SS", "159330.SZ"),
            ("002241.SH", "002241.SZ"),
            ("002241.SS", "002241.SZ"),
            ("sh002241", "002241.SZ"),
        ] {
            query.symbol = symbol.into();
            let error = provider.quote(&query, "").await.unwrap_err();
            assert!(error.to_string().contains(&format!("请改为 {expected}")));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        query.symbol = "601988.SS".into();
        let quote = provider.quote(&query, "").await.unwrap();
        assert_eq!(quote.symbol, "601988.SS");
        assert_eq!(quote.currency, "CNY");
        assert_eq!(quote.close, 5.13);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        query.symbol = "UNKNOWN".into();
        for _ in 0..2 {
            let error = provider.quote(&query, "").await.unwrap_err();
            assert_eq!(
                error.to_string(),
                "市场数据服务错误：Yahoo Finance 返回 HTTP 404 Not Found"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3); // Two hosts, then cached failure.
        task.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn concurrent_accounts_share_one_fetch_and_failures_back_off_then_recover() {
        let (provider, calls, failed, task) = mock_provider().await;
        let mut jobs = tokio::task::JoinSet::new();
        for index in 0..16 {
            // All account requests share the process-owned provider.
            let provider = provider.clone();
            jobs.spawn(async move {
                provider
                    .quote(
                        &SecurityPriceQuery {
                            action_from: None,
                            symbol: if index % 2 == 0 { " aapl " } else { "AAPL" }.into(),
                            on_date: "2026-09-08".into(),
                        },
                        "",
                    )
                    .await
                    .unwrap()
            });
        }
        while let Some(result) = jobs.join_next().await {
            assert_eq!(result.unwrap().close, 230.0);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let query = SecurityPriceQuery {
            action_from: None,
            symbol: "AAPL".into(),
            on_date: "2026-09-08".into(),
        };
        let entry = provider
            .cache
            .entries
            .lock()
            .await
            .get(&("AAPL".into(), query.on_date.clone(), None))
            .unwrap()
            .clone();
        tokio::time::advance(SUCCESS_TTL).await;
        failed.store(true, Ordering::SeqCst);
        assert!(provider.quote(&query, "").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3); // primary and fallback
        failed.store(false, Ordering::SeqCst);
        assert!(provider.quote(&query, "").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        tokio::time::advance(FAILURE_TTL).await;
        assert_eq!(provider.quote(&query, "").await.unwrap().close, 230.0);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        // All entries busy: enforce the capacity without splitting in-flight keys.
        let mut pinned = Vec::new();
        {
            let mut entries = provider.cache.entries.lock().await;
            for index in 0..MAX_ENTRIES - 1 {
                let cell = Arc::new(Mutex::new(Entry::default()));
                pinned.push(cell.clone());
                entries.insert((format!("X{index}"), query.on_date.clone(), None), cell);
            }
        }
        assert!(provider
            .quote(
                &SecurityPriceQuery {
                    action_from: None,
                    symbol: "NEW".into(),
                    ..query.clone()
                },
                ""
            )
            .await
            .is_err());
        assert_eq!(provider.cache.entries.lock().await.len(), MAX_ENTRIES);
        drop(pinned);
        drop(entry);
        provider
            .quote(
                &SecurityPriceQuery {
                    action_from: None,
                    symbol: "NEW".into(),
                    ..query
                },
                "",
            )
            .await
            .unwrap_err();
        assert_eq!(provider.cache.entries.lock().await.len(), MAX_ENTRIES);
        task.abort();
    }
}
