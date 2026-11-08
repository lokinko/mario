//! Public observation data. Never reads accounts, holdings or transactions.
mod plans;
use crate::{
    models::watch::{DividendBasis, DividendEvent, DividendHistory},
    ports::market::WatchMarketProvider,
    AppError, AppResult,
};
use async_trait::async_trait;
use chrono::NaiveDate;
use reqwest::Client;
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};
use tokio::sync::Semaphore;

pub(crate) struct PublicWatchProvider {
    client: Client,
    slots: Semaphore,
}
impl PublicWatchProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("Mozilla/5.0 (compatible; mario/observation)")
                .build()?,
            slots: Semaphore::new(2),
        })
    }
    async fn dividend_payload(&self, symbol: &str) -> AppResult<(Value, Vec<String>)> {
        let _permit = self.slots.acquire().await.map_err(|_| bad())?;
        distribution_payload(
            &self.client,
            "https://datacenter-web.eastmoney.com/api/data/v1/get",
            symbol,
        )
        .await
    }
}

async fn dividend_page(
    client: &Client,
    base_url: &str,
    symbol: &str,
    page: u64,
) -> AppResult<(Value, String)> {
    let filter = format!("(SECURITY_CODE=\"{}\")", &symbol[..6]);
    let response = client
        .get(base_url)
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
    Ok((super::read_public_json(response).await?, url))
}

pub(super) async fn distribution_payload(
    client: &Client,
    base_url: &str,
    symbol: &str,
) -> AppResult<(Value, Vec<String>)> {
    let (mut payload, source) = dividend_page(client, base_url, symbol, 1).await?;
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
            let (next, source) = dividend_page(client, base_url, symbol, page).await?;
            append_dividend_page(&mut payload, &next)?;
            sources.push(source);
        }
        // Only a fully collected result can certify an empty TTM window.
        payload["result"]["pages"] = Value::from(1);
    }
    Ok((payload, sources))
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

pub(super) fn parse_dividends(
    payload: &Value,
    symbol: &str,
    day: NaiveDate,
) -> AppResult<Vec<DividendEvent>> {
    // A missing source history does not establish zero dividends. A complete
    // nonempty history can still have an empty implemented/TTM window.
    if payload["code"] == 9201 && payload["result"].is_null() {
        return Err(bad());
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
        if !found.insert((date, report_date)) {
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
#[async_trait]
impl WatchMarketProvider for PublicWatchProvider {
    async fn dividend_plans(
        &self,
        symbol: &str,
        day: NaiveDate,
    ) -> AppResult<Vec<crate::models::watch::calendar::DividendPlan>> {
        let (payload, sources) = self.dividend_payload(symbol).await?;
        plans::parse(&payload, symbol, day, &sources)
    }

    async fn dividends(&self, symbol: &str, day: NaiveDate) -> AppResult<DividendHistory> {
        let (payload, sources) = self.dividend_payload(symbol).await?;
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
        assert!(parse_dividends(&json!({"code":9201,"result":null}), "600036.SS", day).is_err());
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

#[cfg(test)]
mod live_calendar_check {
    use crate::ports::market::WatchMarketProvider;
    #[tokio::test]
    #[ignore = "manual public-source connectivity check; no account data"]
    async fn live_dividend_calendar_structured_plans() {
        let provider = super::PublicWatchProvider::new().unwrap();
        let day = chrono::Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let (a, b) = tokio::time::timeout(std::time::Duration::from_secs(90), async {
            tokio::join!(
                provider.dividend_plans("600036.SS", day),
                provider.dividend_plans("601009.SS", day)
            )
        })
        .await
        .expect("public-source requests timed out");
        for (symbol, result) in [("600036.SS", a), ("601009.SS", b)] {
            let plans = result.unwrap();
            assert!(!plans.is_empty());
            assert!(plans.iter().all(|p| p.symbol == symbol
                && p.announced_on <= day
                && p.payment_date.is_none()
                && !p.sources.is_empty()));
            println!(
                "{symbol}: {} structured disclosed plans; no invented payment date",
                plans.len()
            );
        }
    }
}
