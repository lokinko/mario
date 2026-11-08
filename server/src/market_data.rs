mod history;
pub(crate) mod watch;
pub(crate) mod watch_archive;
pub(crate) mod watch_catalog;
pub(crate) use history::PublicMarketSource;
use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    models::{FxRateQuery, FxRateQuote},
    ports::market::FxRateProvider,
};

const ECB_API_BASE: &str = "https://data-api.ecb.europa.eu/service/data";
const ECB_METHODOLOGY_URL: &str =
    "https://data.ecb.europa.eu/key-figures/ecb-interest-rates-and-exchange-rates/exchange-rates";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
async fn read_public_json(mut response: reqwest::Response) -> AppResult<serde_json::Value> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::MarketData("行情响应超过 1 MB".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}
const LOOKBACK_DAYS: i64 = 7;
use crate::models::market::SUPPORTED_CURRENCIES;
type FxCache = BTreeMap<String, (std::time::Instant, Result<Vec<u8>, String>)>;
pub struct EcbFxRateProvider {
    cache: tokio::sync::Mutex<FxCache>,
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
            cache: Default::default(),
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
    async fn invalidate(&self) {
        self.cache.lock().await.clear();
    }
    async fn quote(&self, query: &FxRateQuery) -> AppResult<FxRateQuote> {
        let from = normalized_currency(&query.from_currency)?;
        let to = normalized_currency(&query.to_currency)?;
        let requested_on = NaiveDate::parse_from_str(query.on_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("汇率日期必须使用 YYYY-MM-DD".into()))?;
        if requested_on
            > Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive()
        {
            return Err(AppError::Validation("汇率日期不能晚于今天".into()));
        }
        if requested_on < NaiveDate::from_ymd_opt(1999, 1, 4).expect("valid ECB start date") {
            return Err(AppError::Validation(
                "ECB 参考汇率不支持 1999-01-04 之前的日期".into(),
            ));
        }
        if from == to {
            return Ok(FxRateQuote {
                exact_rate: None,
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

        // One bounded request supplies all supported pairs, including historical dates.
        let mut cache = self.cache.lock().await;
        let cache_key = format!("{}:{}", self.base_url, requested_on);
        let start = requested_on - chrono::Duration::days(LOOKBACK_DAYS);
        let currencies = SUPPORTED_CURRENCIES
            .iter()
            .filter(|c| **c != "EUR")
            .map(|c| c.to_string())
            .collect::<Vec<_>>();
        let url = self.request_url(&currencies, start, requested_on)?;
        let cached = cache.get(&cache_key).filter(|(at, result)| {
            at.elapsed() < Duration::from_secs(if result.is_ok() { 3600 } else { 60 })
        });
        let body = if let Some((_, result)) = cached {
            result.clone().map_err(AppError::MarketData)?
        } else {
            let fetched = async {
                let response = self.client.get(url.clone()).send().await?;
                if !response.status().is_success() {
                    return Err(AppError::MarketData(format!(
                        "ECB 返回 HTTP {}",
                        response.status()
                    )));
                }
                if response
                    .content_length()
                    .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
                {
                    return Err(AppError::MarketData(
                        "ECB 返回内容超过 1 MB 安全上限".into(),
                    ));
                }
                let body = response.bytes().await?;
                if body.len() > MAX_RESPONSE_BYTES {
                    return Err(AppError::MarketData(
                        "ECB 返回内容超过 1 MB 安全上限".into(),
                    ));
                }
                Ok::<_, AppError>(body.to_vec())
            }
            .await
            .map_err(|error| error.to_string());
            // Bound memory even when many different historical dates are requested.
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(cache_key, (std::time::Instant::now(), fetched.clone()));
            fetched.map_err(AppError::MarketData)?
        };
        drop(cache);
        let (exact_rate, observed_on) = parse_ecb_quote_exact(&body, &from, &to, requested_on)?;
        use rust_decimal::prelude::ToPrimitive;
        let rate = exact_rate
            .to_f64()
            .ok_or_else(|| AppError::MarketData("汇率超出范围".into()))?;
        Ok(FxRateQuote {
            exact_rate: Some(exact_rate.normalize().to_string()),
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
    observation: String,
}

fn parse_ecb_quote_exact(
    bytes: &[u8],
    from: &str,
    to: &str,
    requested_on: NaiveDate,
) -> AppResult<(rust_decimal::Decimal, NaiveDate)> {
    let mut observations = BTreeMap::<NaiveDate, BTreeMap<String, rust_decimal::Decimal>>::new();
    let mut reader = csv::Reader::from_reader(bytes);
    for result in reader.deserialize::<EcbCsvRow>() {
        let row = result
            .map_err(|error| AppError::MarketData(format!("无法解析 ECB 汇率响应：{error}")))?;
        let Ok(observation) = rust_decimal::Decimal::from_str_exact(&row.observation) else {
            continue;
        };
        if row.denominator != "EUR" || observation <= rust_decimal::Decimal::ZERO {
            continue;
        }
        let date = NaiveDate::parse_from_str(&row.time_period, "%Y-%m-%d")
            .map_err(|_| AppError::MarketData("ECB 返回了无效日期".into()))?;
        if date <= requested_on {
            observations
                .entry(date)
                .or_default()
                .insert(row.currency, observation);
        }
    }

    for (date, rates) in observations.iter().rev() {
        let from_per_euro = if from == "EUR" {
            Some(rust_decimal::Decimal::ONE)
        } else {
            rates.get(from).copied()
        };
        let to_per_euro = if to == "EUR" {
            Some(rust_decimal::Decimal::ONE)
        } else {
            rates.get(to).copied()
        };
        if let (Some(from_rate), Some(to_rate)) = (from_per_euro, to_per_euro) {
            let Some(cross_rate) = to_rate.checked_div(from_rate) else {
                continue;
            };
            if cross_rate > rust_decimal::Decimal::ZERO {
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
fn parse_ecb_quote(
    bytes: &[u8],
    from: &str,
    to: &str,
    requested_on: NaiveDate,
) -> AppResult<(f64, NaiveDate)> {
    use rust_decimal::prelude::ToPrimitive;
    let (rate, date) = parse_ecb_quote_exact(bytes, from, to, requested_on)?;
    Ok((rate.to_f64().unwrap(), date))
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
}
