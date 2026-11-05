//! Public observation data. Never reads accounts, holdings or transactions.
use crate::{
    models::watch::{DividendBasis, DividendEvent, DividendHistory, PriceBar, WatchMarketData},
    ports::market::WatchMarketProvider,
    AppError, AppResult,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration as Days, NaiveDate};
use reqwest::Client;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};

type Cached = (Instant, WatchMarketData);
pub(crate) struct PublicWatchProvider {
    client: Client,
    slots: Semaphore,
    cache: Mutex<BTreeMap<(String, NaiveDate), Cached>>,
}
impl PublicWatchProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("Mozilla/5.0 (compatible; mario/observation)")
                .build()?,
            slots: Semaphore::new(2),
            cache: Mutex::new(BTreeMap::new()),
        })
    }
    async fn chart(&self, symbol: &str, day: NaiveDate) -> AppResult<(Value, String)> {
        for host in ["query1.finance.yahoo.com", "query2.finance.yahoo.com"] {
            let url = format!("https://{host}/v8/finance/chart/{symbol}");
            let response = self
                .client
                .get(&url)
                .query(&[
                    (
                        "period1",
                        (day - Days::days(730))
                            .and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc()
                            .timestamp()
                            .to_string(),
                    ),
                    (
                        "period2",
                        (day + Days::days(1))
                            .and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc()
                            .timestamp()
                            .to_string(),
                    ),
                    ("interval", "1d".into()),
                    ("events", "splits".into()),
                ])
                .send()
                .await;
            if let Ok(response) = response {
                let source = response.url().to_string();
                if let Ok(response) = response.error_for_status() {
                    if let Ok(value) = response.json::<Value>().await {
                        if value["chart"]["error"].is_null()
                            && value["chart"]["result"][0].is_object()
                        {
                            return Ok((value["chart"]["result"][0].clone(), source));
                        }
                    }
                }
            }
        }
        Err(AppError::MarketData("公开日线暂时不可用".into()))
    }
    async fn dividend_page(&self, symbol: &str, page: u64) -> AppResult<(Value, String)> {
        let filter = format!("(SECURITY_CODE=\"{}\")", &symbol[..6]);
        let response = self
            .client
            .get("https://datacenter-web.eastmoney.com/api/data/v1/get")
            .header("Referer", "https://data.eastmoney.com/yjfp/")
            .query(&[
                ("reportName", "RPT_SHAREBONUS_DET"),
                ("columns", "ALL"),
                ("filter", filter.as_str()),
                ("pageSize", "100"),
                ("pageNumber", &page.to_string()),
                ("sortColumns", "REPORT_DATE"),
                ("sortTypes", "-1"),
                ("source", "WEB"),
                ("client", "WEB"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let url = response.url().to_string();
        Ok((response.json::<Value>().await?, url))
    }
}

fn bad() -> AppError {
    AppError::MarketData("行情或分红来源不完整，请稍后重新检查".into())
}
fn decimal(value: &Value, absent_zero: bool) -> AppResult<String> {
    if value.is_null() && absent_zero {
        return Ok("0".into());
    }
    let value = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let amount = rust_decimal::Decimal::from_str_exact(&value).map_err(|_| bad())?;
    if amount < rust_decimal::Decimal::ZERO {
        return Err(bad());
    }
    Ok(amount.normalize().to_string())
}
fn append_dividend_page(payload: &mut Value, next: &Value) -> AppResult<()> {
    if next["success"] != true
        || next["result"]["pages"] != payload["result"]["pages"]
        || next["result"]["count"] != payload["result"]["count"]
    {
        return Err(bad());
    }
    let rows = next["result"]["data"].as_array().ok_or_else(bad)?;
    payload["result"]["data"]
        .as_array_mut()
        .ok_or_else(bad)?
        .extend(rows.iter().cloned());
    Ok(())
}

fn parse_dividends(payload: &Value, symbol: &str, day: NaiveDate) -> AppResult<Vec<DividendEvent>> {
    // An explicit no-record result is usable; transport/schema failure is unknown.
    if payload["code"] == 9201 && payload["result"].is_null() {
        return Ok(Vec::new());
    }
    let rows = payload["result"]["data"].as_array().ok_or_else(bad)?;
    if payload["success"] != true
        || payload["result"]["pages"] != 1
        || payload["result"]["count"].as_u64() != Some(rows.len() as u64)
    {
        return Err(bad());
    }
    let expected = symbol.replace(".SS", ".SH");
    let mut found = BTreeSet::new();
    let mut events = Vec::new();
    for row in rows {
        if row["SECUCODE"] != expected || row["SECURITY_CODE"] != symbol[..6] {
            return Err(bad());
        }
        if row["ASSIGN_PROGRESS"] != "实施分配" {
            continue;
        }
        let date = row["EX_DIVIDEND_DATE"]
            .as_str()
            .and_then(|s| s.get(..10))
            .ok_or_else(bad)?;
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| bad())?;
        if date > day {
            continue;
        }
        let report_date = row["REPORT_DATE"]
            .as_str()
            .and_then(|s| s.get(..10))
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .ok_or_else(bad)?;
        if report_date > date || !found.insert((date, report_date)) {
            return Err(bad());
        }
        events.push(DividendEvent {
            report_date: Some(report_date),
            ex_date: date,
            cash_per_ten: decimal(&row["PRETAX_BONUS_RMB"], true)?,
            // In this vendor's implemented distribution rows, null means the
            // corresponding cash/bonus/capitalization component is absent.
            bonus_per_ten: decimal(&row["BONUS_RATIO"], true)?,
            transfer_per_ten: decimal(&row["IT_RATIO"], true)?,
        });
    }
    events.sort_by_key(|e| e.ex_date);
    Ok(events)
}
fn parse_bars(chart: &Value, symbol: &str, day: NaiveDate) -> AppResult<Vec<PriceBar>> {
    if chart["meta"]["symbol"] != symbol
        || chart["meta"]["exchangeTimezoneName"] != "Asia/Shanghai"
        || chart["meta"]["currency"] != "CNY"
    {
        return Err(bad());
    }
    let stamps = chart["timestamp"].as_array().ok_or_else(bad)?;
    let closes = chart["indicators"]["quote"][0]["close"]
        .as_array()
        .ok_or_else(bad)?;
    let adjusted = chart["indicators"]["adjclose"][0]["adjclose"]
        .as_array()
        .ok_or_else(bad)?;
    if stamps.len() != closes.len() || stamps.len() != adjusted.len() {
        return Err(bad());
    }
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, stamp) in stamps.iter().enumerate() {
        let date = DateTime::from_timestamp(stamp.as_i64().ok_or_else(bad)?, 0)
            .ok_or_else(bad)?
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        if date > day {
            continue;
        }
        if !seen.insert(date) {
            return Err(bad());
        }
        result.push(PriceBar {
            day: date,
            close: closes[i].as_f64(),
            adjusted_close: adjusted[i].as_f64(),
        });
    }
    result.sort_by_key(|b| b.day);
    Ok(result)
}

#[async_trait]
impl WatchMarketProvider for PublicWatchProvider {
    async fn dividends(&self, symbol: &str, day: NaiveDate) -> AppResult<DividendHistory> {
        let _permit = self.slots.acquire().await.map_err(|_| bad())?;
        let (mut payload, source) = self.dividend_page(symbol, 1).await?;
        let mut sources = vec![source];
        if !(payload["code"] == 9201 && payload["result"].is_null()) {
            let pages = payload["result"]["pages"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 50)
                .ok_or_else(bad)?;
            if payload["success"] != true {
                return Err(bad());
            }
            for page in 2..=pages {
                let (next, source) = self.dividend_page(symbol, page).await?;
                append_dividend_page(&mut payload, &next)?;
                sources.push(source);
            }
            // Only a fully collected result can certify an empty TTM window.
            payload["result"]["pages"] = Value::from(1);
        }
        let history = DividendHistory {
            basis: DividendBasis {
                as_of: day,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            },
            events: parse_dividends(&payload, symbol, day)?,
            sources,
        };
        history.validate()?;
        Ok(history)
    }
    async fn invalidate(&self) {
        self.cache.lock().await.clear();
    }
    async fn observation(&self, symbol: &str, day: NaiveDate) -> AppResult<WatchMarketData> {
        let key = (symbol.to_owned(), day);
        if let Some((when, data)) = self.cache.lock().await.get(&key) {
            if when.elapsed() < Duration::from_secs(1800) {
                return Ok(data.clone());
            }
        }
        let _permit = self.slots.acquire().await.map_err(|_| bad())?;
        let (chart, price_source) = self.chart(symbol, day).await?;
        let bars = parse_bars(&chart, symbol, day)?;
        let (index, calendar_source) = self.chart("000001.SS", day).await?;
        let calendar = parse_bars(&index, "000001.SS", day)?
            .into_iter()
            .filter(|b| b.close.is_some_and(|p| p > 0.0))
            .map(|b| b.day)
            .collect();
        let data = WatchMarketData {
            bars,
            calendar,
            dividends: None,
            dividend_basis: None,
            sources: vec![price_source, calendar_source],
        };
        let mut cache = self.cache.lock().await;
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), data.clone()));
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn full_history_pagination_preserves_old_events_and_rejects_partial_or_duplicate_results() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let old = json!({"SECUCODE":"600036.SH","SECURITY_CODE":"600036","ASSIGN_PROGRESS":"实施分配","REPORT_DATE":"2020-12-31","EX_DIVIDEND_DATE":"2021-07-01 00:00:00","PRETAX_BONUS_RMB":"12.3456789","BONUS_RATIO":null,"IT_RATIO":null});
        let mut recent = old.clone();
        recent["REPORT_DATE"] = json!("2025-12-31");
        recent["EX_DIVIDEND_DATE"] = json!("2026-07-01 00:00:00");
        let first = json!({"success":true,"result":{"pages":2,"count":2,"data":[old]}});
        let next = json!({"success":true,"result":{"pages":2,"count":2,"data":[recent]}});
        let mut merged = first.clone();
        append_dividend_page(&mut merged, &next).unwrap();
        merged["result"]["pages"] = json!(1);
        let events = parse_dividends(&merged, "600036.SS", day).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].cash_per_ten, "12.3456789");
        assert_eq!(
            events[0].report_date,
            Some(NaiveDate::from_ymd_opt(2020, 12, 31).unwrap())
        );
        let mut partial = first.clone();
        partial["result"]["pages"] = json!(1);
        assert!(parse_dividends(&partial, "600036.SS", day).is_err());
        let mut changed = next;
        changed["result"]["count"] = json!(3);
        assert!(append_dividend_page(&mut first.clone(), &changed).is_err());
        let mut repeated = first.clone();
        append_dividend_page(&mut repeated, &first).unwrap();
        repeated["result"]["pages"] = json!(1);
        assert!(parse_dividends(&repeated, "600036.SS", day).is_err());
        assert!(
            parse_dividends(&json!({"code":9201,"result":null}), "600036.SS", day)
                .unwrap()
                .is_empty()
        );
        assert!(
            parse_dividends(&json!({"success":false,"result":null}), "600036.SS", day).is_err()
        );
    }

    #[test]
    fn proposals_future_events_and_incomplete_pages_do_not_inflate_yield() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let row = json!({"SECUCODE":"600036.SH","SECURITY_CODE":"600036","ASSIGN_PROGRESS":"实施分配","REPORT_DATE":"2025-12-31","EX_DIVIDEND_DATE":"2026-07-01 00:00:00","PRETAX_BONUS_RMB":10,"BONUS_RATIO":null,"IT_RATIO":null});
        let mut proposal = row.clone();
        proposal["ASSIGN_PROGRESS"] = json!("董事会预案");
        proposal["PRETAX_BONUS_RMB"] = json!(999);
        let mut future = row.clone();
        future["EX_DIVIDEND_DATE"] = json!("2027-01-01 00:00:00");
        let mut payload =
            json!({"success":true,"result":{"pages":1,"count":3,"data":[row,proposal,future]}});
        let events = parse_dividends(&payload, "600036.SS", day).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].cash_per_ten, "10");
        payload["result"]["pages"] = json!(2);
        assert!(parse_dividends(&payload, "600036.SS", day).is_err());
    }
}
