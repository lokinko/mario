//! Public five-year history sources; no account data or caches.
use crate::{
    models::{
        market::{
            canonical_symbol,
            history::{history_start, DailyPrice, PriceHistory},
            is_a_share_stock, is_mainland_fund, mainland_methodology_url, METHODOLOGY_URL,
        },
        SplitEvent,
    },
    ports::market::SecurityHistoryProvider,
    AppError, AppResult,
};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};

mod fund_actions;

pub(crate) struct PublicMarketSource {
    client: Client,
}
impl PublicMarketSource {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(8))
                .user_agent("Mozilla/5.0 (compatible; mario/public-prices)")
                .build()?,
        })
    }
    async fn json(&self, url: reqwest::Url) -> AppResult<(Value, String)> {
        for attempt in 0..2 {
            let result = async {
                let response = self
                    .client
                    .get(url.clone())
                    .send()
                    .await?
                    .error_for_status()?;
                let source = response.url().to_string();
                Ok((super::read_public_json(response).await?, source))
            }
            .await;
            let retry = matches!(&result, Err(AppError::Network(error))
                if error.is_connect() || error.is_timeout()
                    || error.status().is_some_and(|s| s.is_server_error() || s.as_u16() == 429));
            if !retry || attempt == 1 {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        unreachable!()
    }
    async fn domestic_rows(
        &self,
        symbol: &str,
        through: NaiveDate,
        adjusted: bool,
    ) -> AppResult<(BTreeMap<NaiveDate, Option<String>>, String)> {
        let (code, exchange) = symbol.split_once('.').ok_or_else(bad)?;
        let response = self
            .client
            .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
            .header("Referer", "https://quote.eastmoney.com/")
            .query(&[
                (
                    "secid",
                    format!("{}.{code}", if exchange == "SS" { 1 } else { 0 }),
                ),
                ("klt", "101".into()),
                ("fqt", if adjusted { "1" } else { "0" }.into()),
                ("beg", history_start(through).format("%Y%m%d").to_string()),
                ("end", through.format("%Y%m%d").to_string()),
                ("fields1", "f1,f2,f3,f4,f5,f6".into()),
                ("fields2", "f51,f52,f53".into()),
            ])
            .send()
            .await?
            .error_for_status()?;
        let url = response.url().to_string();
        let payload = super::read_public_json(response).await?;
        Ok((parse_domestic(&payload, symbol, through, adjusted)?, url))
    }
    async fn domestic(&self, symbol: &str, through: NaiveDate) -> AppResult<PriceHistory> {
        let (raw, raw_url) = self.domestic_rows(symbol, through, false).await?;
        let (adjusted, adjusted_url) = self.domestic_rows(symbol, through, true).await?;
        let (splits, action_source_urls) = if is_a_share_stock(symbol) {
            let (payload, urls) = super::watch::distribution_payload(
                &self.client,
                "https://datacenter-web.eastmoney.com/api/data/v1/get",
                symbol,
            )
            .await?;
            let today = Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive();
            let events = super::watch::parse_dividends(&payload, symbol, today)?;
            let mut extra = BTreeMap::<NaiveDate, Decimal>::new();
            for event in events {
                let value = Decimal::from_str_exact(&event.bonus_per_ten)
                    .map_err(|_| bad())?
                    .checked_add(
                        Decimal::from_str_exact(&event.transfer_per_ten).map_err(|_| bad())?,
                    )
                    .ok_or_else(bad)?;
                let sum = extra.entry(event.ex_date).or_default();
                *sum = sum.checked_add(value).ok_or_else(bad)?;
            }
            (
                extra
                    .into_iter()
                    .filter(|(_, v)| *v > Decimal::ZERO)
                    .map(|(day, value)| {
                        Ok(SplitEvent {
                            effective_on: day.to_string(),
                            numerator: Decimal::from(10)
                                .checked_add(value)
                                .ok_or_else(bad)?
                                .normalize()
                                .to_string(),
                            denominator: "10".into(),
                        })
                    })
                    .collect::<AppResult<_>>()?,
                urls,
            )
        } else if is_mainland_fund(symbol) {
            let url = mainland_methodology_url(symbol);
            let mut response = self.client.get(&url).send().await?.error_for_status()?;
            let body = super::read_public_bytes(&mut response).await?;
            let html = std::str::from_utf8(&body).map_err(|_| bad())?;
            let today = Utc::now()
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive();
            (fund_actions::parse(html, &symbol[..6], today)?, vec![url])
        } else {
            (vec![], vec![])
        };
        PriceHistory {
            symbol: symbol.into(),
            currency: "CNY".into(),
            window_start: history_start(through),
            completed_through: through,
            adjustment_anchor: through,
            fetched_at: Utc::now().to_rfc3339(),
            provider_code: "eastmoney".into(),
            provider_name: "东方财富".into(),
            raw_price_basis: "unadjusted_daily_close".into(),
            raw_source_url: raw_url,
            adjusted_source_url: adjusted_url,
            action_source_urls,
            methodology_url: mainland_methodology_url(symbol),
            exchange: if symbol.ends_with(".SS") {
                "SHH"
            } else {
                "SHZ"
            }
            .into(),
            mic_code: if symbol.ends_with(".SS") {
                "XSHG"
            } else {
                "XSHE"
            }
            .into(),
            instrument_type: if is_mainland_fund(symbol) {
                "FUND"
            } else if is_a_share_stock(symbol) {
                "EQUITY"
            } else {
                "INDEX"
            }
            .into(),
            bars: raw
                .into_iter()
                .map(|(day, raw_close)| DailyPrice {
                    day,
                    raw_close,
                    adjusted_close: adjusted.get(&day).cloned().flatten(),
                })
                .collect(),
            splits,
        }
        .normalize()
    }
    async fn yahoo(&self, host: &str, symbol: &str, through: NaiveDate) -> AppResult<PriceHistory> {
        let url = format!("{host}/v8/finance/chart/{symbol}");
        let daily_url = self
            .client
            .get(&url)
            .query(&[
                (
                    "period1",
                    history_start(through)
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp()
                        .to_string(),
                ),
                (
                    "period2",
                    (through + chrono::Duration::days(2))
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp()
                        .to_string(),
                ),
                ("interval", "1d".into()),
                ("events", "splits".into()),
                ("includeAdjustedClose", "true".into()),
            ])
            .build()?
            .url()
            .clone();
        let (daily, source) = self.json(daily_url).await?;
        // Monthly sampling keeps the full split history small; these bars never become daily prices.
        let action_url = self
            .client
            .get(&url)
            .query(&[
                ("period1", "0".into()),
                (
                    "period2",
                    (Utc::now() + chrono::Duration::days(2))
                        .timestamp()
                        .to_string(),
                ),
                ("interval", "1mo".into()),
                ("events", "splits".into()),
            ])
            .build()?
            .url()
            .clone();
        let (actions, action_source) = self.json(action_url).await?;
        parse_yahoo(&daily, &actions, symbol, through, source, action_source)
    }
}
fn bad() -> AppError {
    AppError::MarketData("五年行情的证券、日期、复权序列或公司行动资料不完整".into())
}
fn decimal(value: &str) -> AppResult<Decimal> {
    Decimal::from_str_exact(value)
        .or_else(|_| Decimal::from_scientific(value))
        .map_err(|_| bad())
}
fn parse_domestic(
    payload: &Value,
    symbol: &str,
    through: NaiveDate,
    adjusted: bool,
) -> AppResult<BTreeMap<NaiveDate, Option<String>>> {
    let (code, exchange) = symbol.split_once('.').ok_or_else(bad)?;
    let data = &payload["data"];
    if payload["rc"] != 0
        || data["code"] != code
        || data["market"] != if exchange == "SS" { 1 } else { 0 }
    {
        return Err(bad());
    }
    let mut rows = BTreeMap::new();
    for value in data["klines"].as_array().ok_or_else(bad)? {
        let fields = value
            .as_str()
            .ok_or_else(bad)?
            .split(',')
            .collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(bad());
        }
        let day = fields[0].parse::<NaiveDate>().map_err(|_| bad())?;
        if day < history_start(through) || day > through {
            continue;
        }
        let price = if matches!(fields[2], "" | "-" | "null") {
            None
        } else {
            let value = decimal(fields[2])?;
            if value > Decimal::from(1_000_000_000_000_000_i64)
                || (!adjusted && value <= Decimal::ZERO)
            {
                return Err(bad());
            }
            (value > Decimal::ZERO).then(|| value.normalize().to_string())
        };
        if rows.insert(day, price).is_some() {
            return Err(bad());
        }
    }
    if rows.is_empty() {
        return Err(bad());
    }
    Ok(rows)
}
fn chart<'a>(payload: &'a Value, symbol: &str) -> AppResult<&'a Value> {
    if !payload["chart"]["error"].is_null() {
        return Err(bad());
    }
    let chart = &payload["chart"]["result"][0];
    if chart["meta"]["symbol"]
        .as_str()
        .is_none_or(|s| s.to_uppercase() != symbol)
    {
        return Err(bad());
    }
    Ok(chart)
}
fn parse_yahoo(
    daily: &Value,
    actions: &Value,
    symbol: &str,
    through: NaiveDate,
    source: String,
    action_source: String,
) -> AppResult<PriceHistory> {
    let data = chart(daily, symbol)?;
    let action = chart(actions, symbol)?;
    let zone = data["meta"]["exchangeTimezoneName"]
        .as_str()
        .ok_or_else(bad)?
        .parse::<chrono_tz::Tz>()
        .map_err(|_| bad())?;
    if action["meta"]["currency"] != data["meta"]["currency"]
        || !crate::models::market::close::matches_timezone(symbol, zone)
        || action["meta"]["exchangeTimezoneName"] != data["meta"]["exchangeTimezoneName"]
    {
        return Err(bad());
    }
    let currency = data["meta"]["currency"].as_str().ok_or_else(bad)?;
    let pence = matches!(currency, "GBp" | "GBX");
    let stamps = data["timestamp"].as_array().ok_or_else(bad)?;
    let closes = data["indicators"]["quote"][0]["close"]
        .as_array()
        .ok_or_else(bad)?;
    let adjusted = data["indicators"]["adjclose"][0]["adjclose"]
        .as_array()
        .ok_or_else(bad)?;
    if stamps.len() != closes.len() || stamps.len() != adjusted.len() {
        return Err(bad());
    }
    let price = |v: &Value| -> AppResult<Option<String>> {
        if v.is_null() {
            return Ok(None);
        }
        let value = decimal(&v.to_string())?;
        if value <= Decimal::ZERO {
            return Ok(None);
        }
        Ok(Some(
            if pence {
                value / Decimal::from(100)
            } else {
                value
            }
            .normalize()
            .to_string(),
        ))
    };
    let mut bars = Vec::new();
    for (i, stamp) in stamps.iter().enumerate() {
        let day = DateTime::from_timestamp(stamp.as_i64().ok_or_else(bad)?, 0)
            .ok_or_else(bad)?
            .with_timezone(&zone)
            .date_naive();
        if day < history_start(through) || day > through {
            continue;
        }
        bars.push(DailyPrice {
            day,
            raw_close: price(&closes[i])?,
            adjusted_close: price(&adjusted[i])?,
        });
    }
    let mut splits = Vec::new();
    if let Some(events) = action["events"]["splits"].as_object() {
        for event in events.values() {
            let day = DateTime::from_timestamp(event["date"].as_i64().ok_or_else(bad)?, 0)
                .ok_or_else(bad)?
                .with_timezone(&zone)
                .date_naive();
            if day > Utc::now().with_timezone(&zone).date_naive() {
                continue;
            }
            let number = |key: &str| -> AppResult<String> {
                let v = &event[key];
                let d = decimal(
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                        .as_str(),
                )?;
                if d <= Decimal::ZERO {
                    return Err(bad());
                }
                Ok(d.normalize().to_string())
            };
            splits.push(SplitEvent {
                effective_on: day.to_string(),
                numerator: number("numerator")?,
                denominator: number("denominator")?,
            });
        }
    }
    PriceHistory {
        symbol: symbol.into(),
        currency: if pence {
            "GBP".into()
        } else {
            currency.to_uppercase()
        },
        window_start: history_start(through),
        completed_through: through,
        adjustment_anchor: through,
        fetched_at: Utc::now().to_rfc3339(),
        provider_code: "yahoo".into(),
        provider_name: "Yahoo Finance".into(),
        raw_price_basis: "split_adjusted_daily_close".into(),
        raw_source_url: source.clone(),
        adjusted_source_url: source,
        action_source_urls: vec![action_source],
        methodology_url: METHODOLOGY_URL.into(),
        exchange: data["meta"]["exchangeName"]
            .as_str()
            .ok_or_else(bad)?
            .into(),
        mic_code: String::new(),
        instrument_type: data["meta"]["instrumentType"]
            .as_str()
            .ok_or_else(bad)?
            .into(),
        bars,
        splits,
    }
    .normalize()
}

async fn coherent_history<D, Y>(
    symbol: &str,
    through: NaiveDate,
    domestic: D,
    yahoo: [Y; 2],
) -> AppResult<PriceHistory>
where
    D: std::future::Future<Output = AppResult<PriceHistory>>,
    Y: std::future::Future<Output = AppResult<PriceHistory>>,
{
    let mut candidate = None;
    let mut errors = Vec::new();
    if is_a_share_stock(symbol) || is_mainland_fund(symbol) || symbol == "000001.SS" {
        match tokio::time::timeout(Duration::from_secs(24), domestic).await {
            Ok(Ok(h)) if h.adjustment_anchor == through => return Ok(h),
            Ok(Ok(h)) => candidate = Some(h),
            Ok(Err(error)) => {
                tracing::warn!(symbol,error=?error,"public domestic history failed; trying Yahoo");
                errors.push(format!("东方财富：{}", source_error(&error)));
            }
            Err(_) => {
                errors.push("东方财富：查询超过 24 秒时限".into());
            }
        }
    }
    for (index, request) in yahoo.into_iter().enumerate() {
        match request.await {
            Ok(h) if h.adjustment_anchor == through => {
                return Ok(candidate
                    .filter(|c: &PriceHistory| c.adjustment_anchor >= h.adjustment_anchor)
                    .unwrap_or(h))
            }
            Ok(h) => {
                if candidate
                    .as_ref()
                    .is_none_or(|c: &PriceHistory| c.adjustment_anchor < h.adjustment_anchor)
                {
                    candidate = Some(h)
                }
            }
            Err(error) => {
                tracing::warn!(symbol,error=?error,"public Yahoo history failed");
                errors.push(format!(
                    "Yahoo query{}：{}",
                    index + 1,
                    source_error(&error)
                ));
            }
        }
    }
    candidate.ok_or_else(|| {
        AppError::MarketData(format!(
            "本次价格未更新，保留原参考价。{}",
            errors.join("；")
        ))
    })
}
fn source_error(error: &AppError) -> String {
    match error {
        AppError::Network(error) if error.is_timeout() => "请求超时".into(),
        AppError::Network(error) if error.is_connect() => {
            "服务器连接失败，请检查 DNS、代理及出站网络".into()
        }
        AppError::Network(error) => error.status().map_or_else(
            || "网络传输失败".into(),
            |status| format!("HTTP {}", status.as_u16()),
        ),
        AppError::Json(_) => "来源响应格式无效".into(),
        _ => error.to_string(),
    }
}
#[async_trait::async_trait]
impl SecurityHistoryProvider for PublicMarketSource {
    async fn history(&self, symbol: &str, through: NaiveDate) -> AppResult<PriceHistory> {
        let symbol = canonical_symbol(symbol)?;
        coherent_history(
            &symbol,
            through,
            self.domestic(&symbol, through),
            [
                self.yahoo("https://query1.finance.yahoo.com", &symbol, through),
                self.yahoo("https://query2.finance.yahoo.com", &symbol, through),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests;
