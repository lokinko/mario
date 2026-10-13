//! Free, process-wide quotes. Only public market data is shared across accounts.
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    db::Database,
    market_data::SecurityPriceProvider,
    models::{SecurityPriceQuery, SecurityPriceQuote, Snapshot},
    AppError, AppResult,
};

use crate::models::market::{canonical_symbol, METHODOLOGY_URL};
const SUCCESS_TTL: Duration = Duration::from_secs(3600);
const FAILURE_TTL: Duration = Duration::from_secs(60);
const MAX_ENTRIES: usize = 512;
type QuoteResult = Result<SecurityPriceQuote, String>;
type QuoteEntries = BTreeMap<(String, String), Arc<Mutex<Entry>>>;

#[derive(Default)]
struct Entry {
    fetched: Option<(Instant, QuoteResult)>,
}
#[derive(Default)]
struct Cache {
    entries: Mutex<QuoteEntries>,
}
static CACHE: OnceLock<Arc<Cache>> = OnceLock::new();
static CLIENT: OnceLock<Client> = OnceLock::new();
static SLOTS: OnceLock<Semaphore> = OnceLock::new();

pub struct FreeSecurityPriceProvider {
    client: Client,
    hosts: Vec<String>,
    cache: Arc<Cache>,
}
impl FreeSecurityPriceProvider {
    pub fn new() -> AppResult<Self> {
        let client = match CLIENT.get() {
            Some(client) => client.clone(),
            None => {
                let client = Client::builder()
                    .timeout(Duration::from_secs(8))
                    .user_agent("Mozilla/5.0 (compatible; mario/market-data)")
                    .build()?;
                CLIENT.get_or_init(|| client).clone()
            }
        };
        Ok(Self {
            client,
            hosts: vec![
                "https://query1.finance.yahoo.com".into(),
                "https://query2.finance.yahoo.com".into(),
            ],
            cache: CACHE.get_or_init(|| Arc::new(Cache::default())).clone(),
        })
    }

    async fn fetch(&self, symbol: &str, day: NaiveDate) -> AppResult<SecurityPriceQuote> {
        let _permit = SLOTS
            .get_or_init(|| Semaphore::new(4))
            .acquire()
            .await
            .map_err(|_| AppError::MarketData("行情服务已停止".into()))?;
        let end = day.min(Utc::now().date_naive() - chrono::Duration::days(1));
        let start = day - chrono::Duration::days(10);
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
                    &(end + chrono::Duration::days(2))
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
        let today = NaiveDate::parse_from_str(&crate::fx::today(), "%Y-%m-%d").unwrap();
        if day > today || day < NaiveDate::from_ymd_opt(1970, 1, 12).unwrap() {
            return Err(AppError::Validation("证券价格日期超出支持范围".into()));
        }
        let key = (symbol.clone(), day.to_string());
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
            let result = self.fetch(&symbol, day).await.map_err(|error| match error {
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
    close: Vec<Option<f64>>,
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
            let price = (*close)?;
            (date <= end
                && (day - date).num_days() <= 10
                && price.is_finite()
                && price > 0.0
                && price <= 1e15)
                .then_some((date, price))
        })
        .max_by_key(|(day, _)| *day)
        .ok_or_else(|| AppError::MarketData("最近 10 天内没有已完成的有效日收盘价".into()))?;
    Ok(SecurityPriceQuote {
        symbol: symbol.into(), currency, close: if pence { selected.1 / 100.0 } else { selected.1 },
        requested_on: day.to_string(), observed_on: selected.0.to_string(),
        staleness_days: (day - selected.0).num_days(),
        provider_code: "yahoo_daily_close".into(), provider_name: "Yahoo Finance".into(),
        exchange: result.meta.exchange_name, mic_code: String::new(), instrument_type: result.meta.instrument_type,
        price_basis: "split_adjusted_daily_close".into(), source_url, methodology_url: METHODOLOGY_URL.into(),
        disclaimer: format!("免费公开日线接口，无可用性保证；收盘价可能经过拆股调整，不采用分红复权价。仅供参考，不等于实时成交价；公司行动及历史数量需自行核对。{}", if pence { "源价格以便士 GBp/GBX 计价，已除以 100 换算为 GBP。" } else { "" }),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub snapshot: Snapshot,
    pub updated_count: usize,
    pub errors: Vec<String>,
}

/// A new position can supply a code and quantity without entering a placeholder price.
pub async fn complete_input(
    mut input: crate::models::HoldingInput,
    provider: &dyn SecurityPriceProvider,
) -> AppResult<crate::models::HoldingInput> {
    if input.asset_class != "现金"
        && !input.symbol.trim().is_empty()
        && input
            .quantity
            .is_some_and(|q| q.is_finite() && q > 0.0 && q <= 1e15)
        && input.current_price.is_none()
    {
        let quote = provider
            .quote(
                &SecurityPriceQuery {
                    symbol: input.symbol.clone(),
                    on_date: crate::fx::today(),
                },
                "",
            )
            .await?;
        if !input.currency.trim().eq_ignore_ascii_case(&quote.currency) {
            return Err(AppError::Validation(format!(
                "行情币种 {} 与持仓币种 {} 不一致",
                quote.currency, input.currency
            )));
        }
        input.current_price = Some(quote.close);
        input.valuation_date = quote.requested_on;
    }
    Ok(input)
}

pub async fn refresh(
    db: &Database,
    provider: &dyn SecurityPriceProvider,
) -> AppResult<RefreshResult> {
    let snapshot = db.snapshot()?;
    let day = crate::fx::today();
    let mut errors = Vec::new();
    let mut updated_count = 0;
    let holdings = snapshot
        .holdings
        .iter()
        .filter(|h| {
            h.quantity.is_some_and(|q| q > 0.0)
                && h.asset_class != "现金"
                && !h.symbol.trim().is_empty()
        })
        .collect::<Vec<_>>();
    for chunk in holdings.chunks(4) {
        let fetch = |index: usize| {
            let holding = chunk.get(index).copied();
            let day = day.clone();
            async move {
                match holding {
                    Some(holding) => {
                        let query = SecurityPriceQuery {
                            symbol: holding.symbol.clone(),
                            on_date: day,
                        };
                        Some((holding, provider.quote(&query, "").await))
                    }
                    None => None,
                }
            }
        };
        let (a, b, c, d) = tokio::join!(fetch(0), fetch(1), fetch(2), fetch(3));
        for (holding, quote) in [a, b, c, d].into_iter().flatten() {
            let quantity = holding.quantity.unwrap();
            let result = (|| {
                let quote = quote?;
                if !holding.currency.eq_ignore_ascii_case(&quote.currency) {
                    return Err(AppError::MarketData(format!(
                        "行情币种 {} 与持仓币种 {} 不一致",
                        quote.currency, holding.currency
                    )));
                }
                if snapshot.holding_valuations.iter().any(|v| {
                    v.holding_id == holding.id
                        && v.quantity == quantity
                        && v.unit_price == quote.close
                        && v.requested_on == quote.requested_on
                        && v.observed_on == quote.observed_on
                        && v.provider_code == quote.provider_code
                }) {
                    return Ok(false);
                }
                db.apply_verified_holding_valuation(&holding.id, quantity, &quote)?;
                Ok(true)
            })();
            match result {
                Ok(true) => updated_count += 1,
                Ok(false) => {}
                Err(error) => {
                    errors.push(format!("{}（{}）：{}", holding.name, holding.symbol, error))
                }
            }
        }
    }
    let mut snapshot = db.snapshot()?;
    snapshot.price_refresh_errors = errors.clone();
    Ok(RefreshResult {
        snapshot,
        updated_count,
        errors,
    })
}

/// Refresh recently requested current quotes without touching account data.
pub fn start_refresh() {
    tokio::spawn(async {
        let Ok(provider) = FreeSecurityPriceProvider::new() else {
            return;
        };
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let today = crate::fx::today();
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
                .filter(|(_, day)| day >= &yesterday)
                .map(|(symbol, _)| symbol.clone())
                .collect();
            for symbol in symbols {
                let _ = provider
                    .quote(
                        &SecurityPriceQuery {
                            symbol,
                            on_date: today.clone(),
                        },
                        "",
                    )
                    .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
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
            ("sz000001", "000001.SZ"),
            ("601988.SS", "601988.SS"),
            ("sh601988", "601988.SS"),
            ("000001.SS", "000001.SS"), // Shanghai Composite index, not a stock.
            ("510300.SS", "510300.SS"),
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
            }),
            calls,
            failed,
            task,
        )
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
        };
        let mut query = SecurityPriceQuery {
            symbol: "601988.SZ".into(),
            on_date: "2026-09-08".into(),
        };
        let error = provider.quote(&query, "").await.unwrap_err();
        assert!(error.to_string().contains("请改为 601988.SS"));
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
    #[tokio::test]
    async fn concurrent_accounts_share_one_fetch_and_failures_back_off_then_recover() {
        let (provider, calls, failed, task) = mock_provider().await;
        let mut jobs = tokio::task::JoinSet::new();
        for index in 0..16 {
            // Hosted requests construct new providers but share public cache and client.
            let provider = FreeSecurityPriceProvider {
                client: provider.client.clone(),
                hosts: provider.hosts.clone(),
                cache: provider.cache.clone(),
            };
            jobs.spawn(async move {
                provider
                    .quote(
                        &SecurityPriceQuery {
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
            symbol: "AAPL".into(),
            on_date: "2026-09-08".into(),
        };
        let entry = provider
            .cache
            .entries
            .lock()
            .await
            .get(&("AAPL".into(), query.on_date.clone()))
            .unwrap()
            .clone();
        entry.lock().await.fetched.as_mut().unwrap().0 = Instant::now() - SUCCESS_TTL;
        failed.store(true, Ordering::SeqCst);
        assert!(provider.quote(&query, "").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3); // primary and fallback
        failed.store(false, Ordering::SeqCst);
        assert!(provider.quote(&query, "").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        entry.lock().await.fetched.as_mut().unwrap().0 = Instant::now() - FAILURE_TTL;
        assert_eq!(provider.quote(&query, "").await.unwrap().close, 230.0);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        // All entries busy: enforce the capacity without splitting in-flight keys.
        let mut pinned = Vec::new();
        {
            let mut entries = provider.cache.entries.lock().await;
            for index in 0..MAX_ENTRIES - 1 {
                let cell = Arc::new(Mutex::new(Entry::default()));
                pinned.push(cell.clone());
                entries.insert((format!("X{index}"), query.on_date.clone()), cell);
            }
        }
        assert!(provider
            .quote(
                &SecurityPriceQuery {
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
    struct FixedProvider {
        price: f64,
        currency: &'static str,
        failed: bool,
    }
    #[async_trait]
    impl SecurityPriceProvider for FixedProvider {
        async fn quote(
            &self,
            query: &SecurityPriceQuery,
            _: &str,
        ) -> AppResult<SecurityPriceQuote> {
            if self.failed {
                return Err(AppError::MarketData("offline".into()));
            }
            let date = day(&query.on_date) - chrono::Duration::days(1);
            parse_quote(
                &payload(
                    &query.symbol,
                    self.currency,
                    &[(&date.to_string(), Some(self.price))],
                ),
                &query.symbol,
                day(&query.on_date),
                date,
                format!(
                    "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d",
                    query.symbol
                ),
            )
        }
    }
    fn db() -> Database {
        Database::open(std::path::Path::new(":memory:")).unwrap()
    }
    fn holding(
        symbol: &str,
        asset_class: &str,
        quantity: Option<f64>,
    ) -> crate::models::HoldingInput {
        serde_json::from_value(serde_json::json!({"symbol":symbol,"name":"测试资产","assetClass":asset_class,"marketValue":100,"costBasis":50,"targetPct":0,"currency":"USD","valuationDate":crate::fx::today(),"quantity":quantity,"currentPrice":quantity.map(|_|10.0)})).unwrap()
    }
    #[tokio::test]
    async fn refresh_is_idempotent_preserves_manual_assets_and_recovers_without_rewriting_history()
    {
        let db = db();
        let source = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let id = db
            .add_holding(&holding("AAPL", "股票", Some(10.0)))
            .unwrap()
            .holdings[0]
            .id
            .clone();
        db.add_holding(&holding("", "股票", None)).unwrap();
        db.add_holding(&holding("CASH", "现金", Some(10.0)))
            .unwrap();
        let yesterday = (day(&crate::fx::today()) - chrono::Duration::days(1)).to_string();
        {
            let conn = db.conn().unwrap();
            conn.execute(
                "UPDATE daily_entries SET day=?1,id=?1 || ':' || entity_id",
                [&yesterday],
            )
            .unwrap();
            conn.execute("DELETE FROM daily_totals", []).unwrap();
        }
        let result = refresh(&db, &source).await.unwrap();
        assert_eq!(result.updated_count, 1);
        assert!(result.errors.is_empty());
        let updated = result
            .snapshot
            .holdings
            .iter()
            .find(|h| h.id == id)
            .unwrap();
        assert_eq!(
            (
                updated.quantity,
                updated.current_price,
                updated.market_value,
                updated.cost_basis
            ),
            (Some(10.0), Some(23.0), 230.0, 50.0)
        );
        let hash = db.export_sync_data().unwrap().content_hash().unwrap();
        assert_eq!(refresh(&db, &source).await.unwrap().updated_count, 0);
        assert_eq!(db.export_sync_data().unwrap().content_hash().unwrap(), hash);
        for provider in [
            FixedProvider {
                failed: true,
                ..source
            },
            FixedProvider {
                currency: "HKD",
                ..source
            },
        ] {
            let failed = refresh(&db, &provider).await.unwrap();
            assert_eq!(failed.updated_count, 0);
            assert_eq!(failed.errors.len(), 1);
            assert_eq!(failed.snapshot.price_refresh_errors, failed.errors);
            assert_eq!(db.export_sync_data().unwrap().content_hash().unwrap(), hash);
        }
        let restored = self::db();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(
            restored.snapshot().unwrap().holding_valuations[0].provider_code,
            "yahoo_daily_close"
        );
        let next = FixedProvider {
            price: 25.0,
            ..source
        };
        assert_eq!(refresh(&db, &next).await.unwrap().updated_count, 1);
        let history = db
            .daily_history(&crate::models::daily::HistoryQuery::default())
            .unwrap();
        let before = history.records.iter().find(|r| r.day == yesterday).unwrap();
        assert_eq!(
            before
                .assets
                .iter()
                .find(|a| a.holding.id == id)
                .unwrap()
                .holding
                .market_value,
            100.0
        );
        let evidence = &db.snapshot().unwrap().holding_valuations[0];
        let mut forged: crate::models::HoldingValuationEvidence = evidence.clone();
        forged.source_url = forged
            .source_url
            .replace("query1.finance.yahoo.com", "attacker.test");
        let mut data = db.export_sync_data().unwrap();
        let valuations = data
            .tables
            .iter_mut()
            .find(|t| t.name == "holding_valuations")
            .unwrap();
        let source_index = valuations
            .columns
            .iter()
            .position(|c| c == "source_url")
            .unwrap();
        valuations.rows[0][source_index] = crate::db::SyncValue::Text(forged.source_url);
        assert!(restored.import_sync_data(&data).is_err());
    }
    #[tokio::test]
    #[ignore = "requires live free public API"]
    async fn live_free_quotes() {
        let provider = FreeSecurityPriceProvider::new().unwrap();
        for (symbol, currency) in [("AAPL", "USD"), ("0700.HK", "HKD"), ("600519.SS", "CNY")] {
            let q = provider
                .quote(
                    &SecurityPriceQuery {
                        symbol: symbol.into(),
                        on_date: crate::fx::today(),
                    },
                    "",
                )
                .await
                .unwrap();
            assert_eq!(q.currency, currency);
            assert!(q.close > 0.0 && q.staleness_days > 0);
            let db = db();
            let mut input = holding(symbol, "股票", Some(10.0));
            input.currency = currency.into();
            db.add_holding(&input).unwrap();
            let applied = refresh(&db, &provider).await.unwrap();
            assert!(applied.errors.is_empty(), "{:?}", applied.errors);
            assert_eq!(applied.updated_count, 1);
            assert_eq!(applied.snapshot.holdings[0].market_value, q.close * 10.0);
            println!(
                "{symbol}: {} {} observed {}",
                q.close, q.currency, q.observed_on
            );
        }
    }

    #[tokio::test]
    async fn new_positions_can_omit_price_but_currency_errors_do_not_save() {
        let db = db();
        let provider = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let mut input = holding("AAPL", "股票", Some(10.0));
        input.current_price = None;
        let filled = complete_input(input.clone(), &provider).await.unwrap();
        let saved = db.add_holding(&filled).unwrap();
        assert_eq!(saved.holdings[0].market_value, 230.0);
        assert!(complete_input(
            input,
            &FixedProvider {
                currency: "HKD",
                ..provider
            }
        )
        .await
        .is_err());
        assert_eq!(db.snapshot().unwrap().holdings.len(), 1);
    }

    #[tokio::test]
    async fn price_refresh_keeps_trade_quantities_costs_and_execution_records() {
        let db = db();
        let trade = crate::models::HoldingTradeInput {
            request_id: uuid::Uuid::new_v4().to_string(),
            holding_id: String::new(),
            expected_revision: None,
            new_holding: Some(crate::models::TradeSecurityInput {
                symbol: "AAPL".into(),
                name: "Apple".into(),
                asset_class: "股票".into(),
                currency: "USD".into(),
            }),
            kind: "buy".into(),
            occurred_on: crate::fx::today(),
            sequence: 1,
            quantity: "10".into(),
            price: "10".into(),
            gross_amount: "100".into(),
            fees: "2".into(),
            taxes: "0".into(),
            account: "broker".into(),
            external_id: String::new(),
            note: String::new(),
        };
        let before = db.save_holding_trade(&trade).unwrap();
        let provider = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let updated = refresh(&db, &provider).await.unwrap();
        assert_eq!(updated.updated_count, 1);
        assert_eq!(updated.snapshot.holding_trades, before.holding_trades);
        let holding = &updated.snapshot.holdings[0];
        assert_eq!(holding.quantity, before.holdings[0].quantity);
        assert_eq!(holding.cost_basis, before.holdings[0].cost_basis);
        assert_eq!(holding.cost_price, before.holdings[0].cost_price);
        assert_eq!(holding.market_value, 230.0);
        let restored = self::db();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(restored.snapshot().unwrap().holdings[0].market_value, 230.0);
    }
}
