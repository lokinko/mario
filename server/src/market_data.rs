use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use chrono::{Local, NaiveDate, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    models::{FxRateQuery, FxRateQuote, SecurityPriceQuery, SecurityPriceQuote},
};

const ECB_API_BASE: &str = "https://data-api.ecb.europa.eu/service/data";
const ECB_METHODOLOGY_URL: &str =
    "https://data.ecb.europa.eu/key-figures/ecb-interest-rates-and-exchange-rates/exchange-rates";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const LOOKBACK_DAYS: i64 = 7;
const SUPPORTED_CURRENCIES: &[&str] = &["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
const TWELVE_DATA_API_BASE: &str = "https://api.twelvedata.com";
const TWELVE_DATA_METHODOLOGY_URL: &str = "https://twelvedata.com/docs/market-data/time-series";
const PRICE_LOOKBACK_DAYS: i64 = 10;

#[async_trait]
pub trait FxRateProvider: Send + Sync {
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote>;
}

#[async_trait]
pub trait SecurityPriceProvider: Send + Sync {
    async fn quote(
        &self,
        query: &SecurityPriceQuery,
        api_key: &str,
    ) -> AppResult<SecurityPriceQuote>;
}

pub struct TwelveDataSecurityPriceProvider {
    client: Client,
    base_url: String,
}

impl TwelveDataSecurityPriceProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("mario/market-data")
                .build()?,
            base_url: TWELVE_DATA_API_BASE.into(),
        })
    }

    fn request_url(&self, symbol: &str, start: NaiveDate, end: NaiveDate) -> AppResult<Url> {
        let mut url = Url::parse(&format!(
            "{}/time_series",
            self.base_url.trim_end_matches('/')
        ))
        .map_err(|error| AppError::MarketData(format!("Twelve Data 接口地址无效：{error}")))?;
        url.query_pairs_mut()
            .append_pair("symbol", symbol)
            .append_pair("interval", "1day")
            .append_pair("start_date", &start.format("%Y-%m-%d").to_string())
            .append_pair("end_date", &end.format("%Y-%m-%d").to_string())
            .append_pair("outputsize", "10")
            .append_pair("order", "DESC")
            .append_pair("timezone", "Exchange")
            .append_pair("adjust", "none");
        Ok(url)
    }
}

#[async_trait]
impl SecurityPriceProvider for TwelveDataSecurityPriceProvider {
    async fn quote(
        &self,
        query: &SecurityPriceQuery,
        api_key: &str,
    ) -> AppResult<SecurityPriceQuote> {
        let symbol = normalized_symbol(&query.symbol)?;
        if api_key.trim().is_empty() {
            return Err(AppError::Validation("Twelve Data API Key 不能为空".into()));
        }
        let requested_on = NaiveDate::parse_from_str(query.on_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("证券价格日期必须使用 YYYY-MM-DD".into()))?;
        if requested_on > Local::now().date_naive() {
            return Err(AppError::Validation("证券价格日期不能晚于今天".into()));
        }
        let start = requested_on - chrono::Duration::days(PRICE_LOOKBACK_DAYS);
        let completed_through = completed_price_end_date(requested_on, Utc::now().date_naive());
        let url = self.request_url(&symbol, start, completed_through)?;
        let response = self
            .client
            .get(url.clone())
            .header("Authorization", format!("apikey {}", api_key.trim()))
            .send()
            .await?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AppError::MarketData(
                "Twelve Data 返回内容超过 1 MB 安全上限".into(),
            ));
        }
        let body = response.bytes().await?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::MarketData(
                "Twelve Data 返回内容超过 1 MB 安全上限".into(),
            ));
        }
        if !status.is_success() {
            return Err(AppError::MarketData(format!(
                "Twelve Data 返回 HTTP {}：{}",
                status,
                safe_provider_error(&body)
            )));
        }
        parse_twelve_data_quote(&body, &symbol, requested_on, url.to_string())
    }
}

fn completed_price_end_date(requested_on: NaiveDate, utc_today: NaiveDate) -> NaiveDate {
    requested_on.min(utc_today - chrono::Duration::days(1))
}

#[derive(Deserialize)]
struct TwelveDataResponse {
    meta: Option<TwelveDataMeta>,
    values: Option<Vec<TwelveDataValue>>,
    status: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct TwelveDataMeta {
    #[serde(rename = "symbol")]
    _symbol: String,
    currency: String,
    #[serde(default)]
    exchange: String,
    #[serde(default)]
    mic_code: String,
    #[serde(rename = "type", default)]
    instrument_type: String,
}

#[derive(Deserialize)]
struct TwelveDataValue {
    datetime: String,
    close: String,
}

fn parse_twelve_data_quote(
    bytes: &[u8],
    requested_symbol: &str,
    requested_on: NaiveDate,
    source_url: String,
) -> AppResult<SecurityPriceQuote> {
    let payload: TwelveDataResponse = serde_json::from_slice(bytes)
        .map_err(|error| AppError::MarketData(format!("无法解析 Twelve Data 响应：{error}")))?;
    if payload.status.as_deref() == Some("error") {
        return Err(AppError::MarketData(
            payload
                .message
                .unwrap_or_else(|| "Twelve Data 拒绝了价格查询".into())
                .chars()
                .take(240)
                .collect(),
        ));
    }
    let meta = payload
        .meta
        .ok_or_else(|| AppError::MarketData("Twelve Data 响应缺少证券元数据".into()))?;
    let currency = meta.currency.trim().to_ascii_uppercase();
    if currency.len() != 3 || !currency.chars().all(|value| value.is_ascii_alphabetic()) {
        return Err(AppError::MarketData(
            "Twelve Data 返回了无效的计价币种".into(),
        ));
    }
    let values = payload
        .values
        .ok_or_else(|| AppError::MarketData("Twelve Data 响应没有日线价格".into()))?;
    let mut selected: Option<(NaiveDate, f64)> = None;
    for value in values {
        let Some(date_text) = value.datetime.get(..10) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(date_text, "%Y-%m-%d") else {
            continue;
        };
        let Ok(close) = value.close.parse::<f64>() else {
            continue;
        };
        if date <= requested_on
            && (requested_on - date).num_days() <= PRICE_LOOKBACK_DAYS
            && close.is_finite()
            && close > 0.0
            && selected.is_none_or(|(current, _)| date > current)
        {
            selected = Some((date, close));
        }
    }
    let (observed_on, close) = selected.ok_or_else(|| {
        AppError::MarketData(format!(
            "Twelve Data 在 {} 及之前 {} 天内没有 {} 的有效日收盘价",
            requested_on.format("%Y-%m-%d"),
            PRICE_LOOKBACK_DAYS,
            requested_symbol
        ))
    })?;
    Ok(SecurityPriceQuote {
        symbol: requested_symbol.into(),
        currency,
        close,
        requested_on: requested_on.format("%Y-%m-%d").to_string(),
        observed_on: observed_on.format("%Y-%m-%d").to_string(),
        staleness_days: (requested_on - observed_on).num_days(),
        provider_code: "twelve_data_raw_close".into(),
        provider_name: "Twelve Data".into(),
        exchange: meta.exchange.trim().into(),
        mic_code: meta.mic_code.trim().into(),
        instrument_type: meta.instrument_type.trim().into(),
        price_basis: "unadjusted_daily_close".into(),
        source_url,
        methodology_url: TWELVE_DATA_METHODOLOGY_URL.into(),
        disclaimer: "日收盘价按交易所本地日期返回，未做拆股或分红复权，不等于实时成交价、券商结算价或专业估值。".into(),
    })
}

fn safe_provider_error(bytes: &[u8]) -> String {
    serde_json::from_slice::<TwelveDataResponse>(bytes)
        .ok()
        .and_then(|payload| payload.message)
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
        .chars()
        .take(240)
        .collect()
}

fn normalized_symbol(value: &str) -> AppResult<String> {
    let symbol = value.trim().to_ascii_uppercase();
    if symbol.is_empty() || symbol.chars().count() > 80 {
        return Err(AppError::Validation("证券代码应为 1–80 个字符".into()));
    }
    if symbol.chars().any(|character| character.is_control()) {
        return Err(AppError::Validation("证券代码不能包含控制字符".into()));
    }
    Ok(symbol)
}

pub struct EcbFxRateProvider {
    client: Client,
    base_url: String,
}

impl EcbFxRateProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("mario/market-data")
                .build()?,
            base_url: ECB_API_BASE.into(),
        })
    }

    fn request_url(
        &self,
        currencies: &[String],
        start: NaiveDate,
        end: NaiveDate,
    ) -> AppResult<Url> {
        let series = currencies.join("+");
        let mut url = Url::parse(&format!(
            "{}/EXR/D.{series}.EUR.SP00.A",
            self.base_url.trim_end_matches('/')
        ))
        .map_err(|error| AppError::MarketData(format!("ECB 接口地址无效：{error}")))?;
        url.query_pairs_mut()
            .append_pair("startPeriod", &start.format("%Y-%m-%d").to_string())
            .append_pair("endPeriod", &end.format("%Y-%m-%d").to_string())
            .append_pair("format", "csvdata")
            .append_pair("detail", "dataonly");
        Ok(url)
    }
}

#[async_trait]
impl FxRateProvider for EcbFxRateProvider {
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote> {
        let from = normalized_currency(&query.from_currency)?;
        let to = normalized_currency(&query.to_currency)?;
        let requested_on = NaiveDate::parse_from_str(query.on_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("汇率日期必须使用 YYYY-MM-DD".into()))?;
        if requested_on > Local::now().date_naive() {
            return Err(AppError::Validation("汇率日期不能晚于今天".into()));
        }
        if requested_on < NaiveDate::from_ymd_opt(1999, 1, 4).expect("valid ECB start date") {
            return Err(AppError::Validation(
                "ECB 参考汇率不支持 1999-01-04 之前的日期".into(),
            ));
        }
        if from == to {
            return Ok(FxRateQuote {
                from_currency: from,
                to_currency: to,
                rate: 1.0,
                requested_on: query.on_date.trim().into(),
                observed_on: query.on_date.trim().into(),
                staleness_days: 0,
                provider_code: "identity".into(),
                provider_name: "同币种换算".into(),
                source_url: ECB_METHODOLOGY_URL.into(),
                methodology_url: ECB_METHODOLOGY_URL.into(),
                disclaimer: "同一币种之间的换算率恒为 1。".into(),
            });
        }

        let mut currencies = [&from, &to]
            .into_iter()
            .filter(|currency| currency.as_str() != "EUR")
            .cloned()
            .collect::<Vec<_>>();
        currencies.sort();
        currencies.dedup();
        let start = requested_on - chrono::Duration::days(LOOKBACK_DAYS);
        let url = self.request_url(&currencies, start, requested_on)?;
        let response = self.client.get(url.clone()).send().await?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AppError::MarketData(
                "ECB 返回内容超过 1 MB 安全上限".into(),
            ));
        }
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(AppError::MarketData(format!(
                "ECB 返回 HTTP {}：{}",
                status,
                String::from_utf8_lossy(&body)
                    .chars()
                    .take(240)
                    .collect::<String>()
            )));
        }
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::MarketData(
                "ECB 返回内容超过 1 MB 安全上限".into(),
            ));
        }
        let (rate, observed_on) = parse_ecb_quote(&body, &from, &to, requested_on)?;
        Ok(FxRateQuote {
            from_currency: from,
            to_currency: to,
            rate,
            requested_on: requested_on.format("%Y-%m-%d").to_string(),
            observed_on: observed_on.format("%Y-%m-%d").to_string(),
            staleness_days: (requested_on - observed_on).num_days(),
            provider_code: "ecb_reference".into(),
            provider_name: "European Central Bank (ECB)".into(),
            source_url: url.to_string(),
            methodology_url: ECB_METHODOLOGY_URL.into(),
            disclaimer:
                "ECB 参考汇率仅供信息使用，是买卖报价的平均参考值，不一定等于实际成交汇率。".into(),
        })
    }
}

#[derive(Deserialize)]
struct EcbCsvRow {
    #[serde(rename = "CURRENCY")]
    currency: String,
    #[serde(rename = "CURRENCY_DENOM")]
    denominator: String,
    #[serde(rename = "TIME_PERIOD")]
    time_period: String,
    #[serde(rename = "OBS_VALUE")]
    observation: f64,
}

fn parse_ecb_quote(
    bytes: &[u8],
    from: &str,
    to: &str,
    requested_on: NaiveDate,
) -> AppResult<(f64, NaiveDate)> {
    let mut observations = BTreeMap::<NaiveDate, BTreeMap<String, f64>>::new();
    let mut reader = csv::Reader::from_reader(bytes);
    for result in reader.deserialize::<EcbCsvRow>() {
        let row = result
            .map_err(|error| AppError::MarketData(format!("无法解析 ECB 汇率响应：{error}")))?;
        if row.denominator != "EUR" || !row.observation.is_finite() || row.observation <= 0.0 {
            continue;
        }
        let date = NaiveDate::parse_from_str(&row.time_period, "%Y-%m-%d")
            .map_err(|_| AppError::MarketData("ECB 返回了无效日期".into()))?;
        if date <= requested_on {
            observations
                .entry(date)
                .or_default()
                .insert(row.currency, row.observation);
        }
    }

    for (date, rates) in observations.iter().rev() {
        let from_per_euro = if from == "EUR" {
            Some(1.0)
        } else {
            rates.get(from).copied()
        };
        let to_per_euro = if to == "EUR" {
            Some(1.0)
        } else {
            rates.get(to).copied()
        };
        if let (Some(from_rate), Some(to_rate)) = (from_per_euro, to_per_euro) {
            let cross_rate = to_rate / from_rate;
            if cross_rate.is_finite() && cross_rate > 0.0 {
                return Ok((cross_rate, *date));
            }
        }
    }
    Err(AppError::MarketData(format!(
        "ECB 在 {} 及之前 {} 天内没有 {}→{} 的共同参考汇率",
        requested_on.format("%Y-%m-%d"),
        LOOKBACK_DAYS,
        from,
        to
    )))
}

fn normalized_currency(value: &str) -> AppResult<String> {
    let currency = value.trim().to_ascii_uppercase();
    if !SUPPORTED_CURRENCIES.contains(&currency.as_str()) {
        return Err(AppError::Validation(format!(
            "暂不支持币种 {currency}；当前支持 {}",
            SUPPORTED_CURRENCIES.join("、")
        )));
    }
    Ok(currency)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = b"KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE\nEXR.D.CNY.EUR.SP00.A,D,CNY,EUR,SP00,A,2026-09-03,7.8042\nEXR.D.CNY.EUR.SP00.A,D,CNY,EUR,SP00,A,2026-09-04,7.7994\nEXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-09-03,1.1600\nEXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-09-04,1.1589\n";

    #[test]
    fn derives_cross_rate_from_latest_common_business_day() {
        let requested = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
        let (rate, observed) = parse_ecb_quote(FIXTURE, "USD", "CNY", requested).unwrap();
        assert_eq!(observed, NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());
        assert!((rate - 7.7994 / 1.1589).abs() < 1e-12);
    }

    #[test]
    fn supports_euro_as_an_implicit_leg() {
        let requested = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (to_cny, _) = parse_ecb_quote(FIXTURE, "EUR", "CNY", requested).unwrap();
        let (to_eur, _) = parse_ecb_quote(FIXTURE, "USD", "EUR", requested).unwrap();
        assert!((to_cny - 7.7994).abs() < 1e-12);
        assert!((to_eur - 1.0 / 1.1589).abs() < 1e-12);
    }

    #[test]
    fn refuses_to_mix_observations_from_different_dates() {
        let fixture = b"KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE\nkey,D,CNY,EUR,SP00,A,2026-09-04,7.8\nkey,D,USD,EUR,SP00,A,2026-09-03,1.16\n";
        assert!(matches!(
            parse_ecb_quote(
                fixture,
                "USD",
                "CNY",
                NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()
            ),
            Err(AppError::MarketData(_))
        ));
    }

    #[test]
    fn parses_latest_valid_unadjusted_security_close() {
        let fixture = br#"{
          "meta":{"symbol":"AAPL","interval":"1day","currency":"USD","exchange":"NASDAQ","mic_code":"XNAS","type":"Common Stock"},
          "values":[
            {"datetime":"2026-09-04","open":"100","high":"102","low":"99","close":"101.25","volume":"10"},
            {"datetime":"2026-09-03","open":"98","high":"101","low":"97","close":"100","volume":"11"}
          ],
          "status":"ok"
        }"#;
        let quote = parse_twelve_data_quote(
            fixture,
            "AAPL",
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
            "https://api.twelvedata.com/time_series?symbol=AAPL".into(),
        )
        .unwrap();
        assert_eq!(quote.close, 101.25);
        assert_eq!(quote.currency, "USD");
        assert_eq!(quote.observed_on, "2026-09-04");
        assert_eq!(quote.staleness_days, 1);
        assert_eq!(quote.price_basis, "unadjusted_daily_close");
        assert!(!quote.source_url.contains("apikey"));
    }

    #[test]
    fn surfaces_provider_errors_without_guessing_a_price() {
        let fixture = br#"{"code":429,"message":"API credits exhausted","status":"error"}"#;
        let error = parse_twelve_data_quote(
            fixture,
            "AAPL",
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
            "https://api.twelvedata.com/time_series".into(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("API credits exhausted"));
    }

    #[test]
    fn never_requests_an_in_progress_utc_daily_bar() {
        let utc_today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        assert_eq!(
            completed_price_end_date(utc_today, utc_today),
            NaiveDate::from_ymd_opt(2026, 9, 7).unwrap()
        );
        assert_eq!(
            completed_price_end_date(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), utc_today),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
    }
}
