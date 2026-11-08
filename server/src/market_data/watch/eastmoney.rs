//! Domestic daily source: unadjusted closes, backward-adjusted closes and index
//! sessions are fetched together. No Yahoo values enter this observation.
use crate::{
    models::watch::{PriceBar, WatchMarketData},
    AppResult,
};
use chrono::{Duration, NaiveDate};
use reqwest::Client;
use serde_json::Value;
use std::collections::BTreeMap;

fn identity(symbol: &str) -> AppResult<(&str, u64)> {
    let (code, exchange) = symbol.split_once('.').ok_or_else(super::bad)?;
    let market = match exchange {
        "SS" => 1,
        "SZ" => 0,
        _ => return Err(super::bad()),
    };
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(super::bad());
    }
    Ok((code, market))
}

fn parse(
    payload: &Value,
    symbol: &str,
    day: NaiveDate,
) -> AppResult<BTreeMap<NaiveDate, Option<f64>>> {
    let (code, market) = identity(symbol)?;
    let data = &payload["data"];
    if payload["rc"] != 0 || data["code"] != code || data["market"] != market {
        return Err(super::bad());
    }
    let rows = data["klines"].as_array().ok_or_else(super::bad)?;
    let mut closes = BTreeMap::new();
    for row in rows {
        // fields2=f51,f52,f53 means date, open, close. A missing close
        // remains unknown; it is never substituted with open or previous close.
        let fields: Vec<_> = row.as_str().ok_or_else(super::bad)?.split(',').collect();
        if fields.len() != 3 {
            return Err(super::bad());
        }
        let date = NaiveDate::parse_from_str(fields[0], "%Y-%m-%d").map_err(|_| super::bad())?;
        if date > day {
            continue;
        }
        let close = if matches!(fields[2], "" | "-" | "null") {
            None
        } else {
            let value = fields[2].parse::<f64>().map_err(|_| super::bad())?;
            if !value.is_finite() {
                return Err(super::bad());
            }
            (value > 0.0).then_some(value)
        };
        if closes.insert(date, close).is_some() {
            return Err(super::bad());
        }
    }
    if closes.is_empty() {
        return Err(super::bad());
    }
    Ok(closes)
}

async fn history(
    client: &Client,
    url: &str,
    symbol: &str,
    day: NaiveDate,
    adjustment: &str,
) -> AppResult<(BTreeMap<NaiveDate, Option<f64>>, String)> {
    let (code, market) = identity(symbol)?;
    let response = client
        .get(url)
        .header("Referer", "https://quote.eastmoney.com/")
        .query(&[
            ("secid", format!("{market}.{code}")),
            ("klt", "101".into()),
            ("fqt", adjustment.into()),
            (
                "beg",
                (day - Duration::days(super::HISTORY_DAYS))
                    .format("%Y%m%d")
                    .to_string(),
            ),
            ("end", day.format("%Y%m%d").to_string()),
            ("fields1", "f1,f2,f3,f4,f5,f6".into()),
            ("fields2", "f51,f52,f53".into()),
        ])
        .send()
        .await?
        .error_for_status()?;
    let source = response.url().to_string();
    Ok((
        parse(&response.json::<Value>().await?, symbol, day)?,
        source,
    ))
}

pub(super) async fn observation(
    client: &Client,
    url: &str,
    symbol: &str,
    day: NaiveDate,
) -> AppResult<WatchMarketData> {
    // Requests remain sequential under the caller's two-permit semaphore.
    let (raw, raw_source) = history(client, url, symbol, day, "0").await?;
    let (adjusted, adjusted_source) = history(client, url, symbol, day, "2").await?;
    let (index, calendar_source) = history(client, url, "000001.SS", day, "0").await?;
    let bars = raw
        .into_iter()
        .map(|(day, close)| PriceBar {
            day,
            close,
            adjusted_close: adjusted.get(&day).copied().flatten(),
        })
        .collect();
    let calendar: Vec<_> = index
        .into_iter()
        .filter_map(|(day, close)| close.map(|_| day))
        .collect();
    if calendar.is_empty() {
        return Err(super::bad());
    }
    Ok(WatchMarketData {
        bars,
        calendar,
        dividends: None,
        dividend_basis: None,
        sources: vec![raw_source, adjusted_source, calendar_source],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_wrong_security_exchange_duplicates_and_protocol_errors() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let payload =
            json!({"rc":0,"data":{"code":"000807","market":0,"klines":["2026-11-04,25.05,25.30"]}});
        let bars = parse(&payload, "000807.SZ", day).unwrap();
        assert_eq!(bars[&day], Some(25.30));
        assert!(parse(&payload, "000807.SS", day).is_err());
        assert!(parse(&payload, "600036.SS", day).is_err());
        for value in [
            Value::Null,
            json!("NaN"),
            json!("2026-11-04,25.05,NaN"),
            json!("2026-11-04,25.05,25.30,999"),
        ] {
            let mut invalid = payload.clone();
            invalid["data"]["klines"][0] = value;
            assert!(parse(&invalid, "000807.SZ", day).is_err());
        }
        let mut duplicate = payload.clone();
        duplicate["data"]["klines"]
            .as_array_mut()
            .unwrap()
            .push(payload["data"]["klines"][0].clone());
        assert!(parse(&duplicate, "000807.SZ", day).is_err());
        assert!(parse(&json!({"rc":1,"data":null}), "000807.SZ", day).is_err());
    }

    #[test]
    fn missing_closes_stay_unknown_and_future_rows_are_excluded() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let payload = json!({"rc":0,"data":{"code":"000807","market":0,"klines":[
            "2026-11-02,24.00,-", "2026-11-03,25.08,0", "2026-11-04,25.05,25.30", "2026-11-05,99,99"
        ]}});
        let bars = parse(&payload, "000807.SZ", day).unwrap();
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[&(day - Duration::days(1))], None);
        assert_eq!(bars[&(day - Duration::days(2))], None);
        assert_eq!(bars[&day], Some(25.30));
    }
}
