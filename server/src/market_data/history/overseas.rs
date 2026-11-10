//! Exact overseas security resolution and independent split evidence; no account state.
use super::{
    bad, chart, decimal, eastmoney_methodology_url, history_start, is_a_share_stock,
    is_mainland_fund, source_error, yahoo_splits, PublicMarketSource,
};
use crate::{
    models::{
        market::{
            eastmoney::{security, Security},
            history::{DailyPrice, PriceHistory},
        },
        SplitEvent,
    },
    AppError, AppResult,
};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
struct Resolved {
    identity: Security,
    instrument_type: &'static str,
    source: String,
}
impl Resolved {
    fn action_code(&self) -> String {
        format!(
            "{}.{}",
            self.identity.code,
            match self.identity.market {
                116 => "HK",
                105 => "O",
                106 => "N",
                _ => "A",
            }
        )
    }
}

fn resolve(payload: &Value, symbol: &str, source: String) -> AppResult<Resolved> {
    let hk = symbol.ends_with(".HK");
    let expected = if hk {
        security(symbol, "HKG").ok_or_else(bad)?.code
    } else {
        symbol.to_owned()
    };
    let rows = payload["QuotationCodeTable"]["Data"]
        .as_array()
        .ok_or_else(bad)?;
    let mut candidates = Vec::new();
    for row in rows {
        if row["Code"] != expected {
            continue;
        }
        let (exchange, instrument_type) = if hk {
            if row["Classify"] != "HK"
                || row["JYS"] != "HK"
                || row["TypeUS"] != "3"
                || !matches!(row["SecurityType"].as_str(), Some("6" | "19"))
            {
                continue;
            }
            ("HKG", "EQUITY")
        } else {
            if row["Classify"] != "UsStock" {
                continue;
            }
            let instrument = match row["TypeUS"].as_str() {
                Some("1" | "3") => "EQUITY",
                Some("5") => "ETF",
                _ => continue,
            };
            (row["JYS"].as_str().ok_or_else(bad)?, instrument)
        };
        let Some(identity) = security(symbol, exchange) else {
            continue;
        };
        let market = row["MktNum"]
            .as_str()
            .ok_or_else(bad)?
            .parse::<u16>()
            .map_err(|_| bad())?;
        if market != identity.market || row["QuoteID"] != identity.secid() {
            return Err(bad());
        }
        candidates.push(Resolved {
            identity,
            instrument_type,
            source: source.clone(),
        });
    }
    if candidates.len() != 1 {
        return Err(bad());
    }
    Ok(candidates.remove(0))
}

async fn get(request: reqwest::RequestBuilder) -> AppResult<(Value, String)> {
    let response = request.send().await?.error_for_status()?;
    let source = response.url().to_string();
    Ok((super::super::read_public_json(response).await?, source))
}

// A zero-row corporate-action report is not proof that this security never split.
fn page(payload: &Value) -> AppResult<Option<(u64, u64, Vec<Value>)>> {
    if payload["code"] == 9201 && payload["success"] == false && payload["result"].is_null() {
        return Ok(None);
    }
    if payload["code"] != 0 || payload["success"] != true {
        return Err(bad());
    }
    let result = &payload["result"];
    let pages = result["pages"].as_u64().ok_or_else(bad)?;
    let count = result["count"].as_u64().ok_or_else(bad)?;
    let rows = result["data"].as_array().ok_or_else(bad)?;
    if count == 0 && rows.is_empty() && pages <= 1 {
        return Ok(None);
    }
    if pages == 0 || pages > 20 || count.div_ceil(100) != pages || rows.len() > 100 {
        return Err(bad());
    }
    Ok(Some((pages, count, rows.clone())))
}

fn action_rows(
    rows: &[Value],
    resolved: &Resolved,
    today: NaiveDate,
) -> AppResult<Vec<SplitEvent>> {
    let mut splits = BTreeMap::new();
    for row in rows {
        if row["SECUCODE"] != resolved.action_code()
            || row["SECURITY_CODE"] != resolved.identity.code
        {
            return Err(bad());
        }
        let hk = resolved.identity.market == 116;
        match row[if hk { "SPLIT_PROCESS" } else { "PLAN_PROCESS" }].as_str() {
            Some("实施") => {}
            Some("预案" | "董事会预案" | "股东大会通过" | "取消" | "已取消" | "不实施") => {
                continue
            }
            _ => return Err(bad()),
        }
        let date = row[if hk {
            "EFFECTIVE_DATE"
        } else {
            "EX_DIVIDEND_DATE"
        }]
        .as_str()
        .ok_or_else(bad)?;
        if !date.is_ascii()
            || !(date.len() == 10 || date.len() == 19 && date.ends_with(" 00:00:00"))
        {
            return Err(bad());
        }
        let day = date[..10].parse::<NaiveDate>().map_err(|_| bad())?;
        if day > today {
            continue;
        }
        let ratio = row["SPLIT_RATIO"].as_str().ok_or_else(bad)?;
        let (before, after) = ratio.split_once(':').ok_or_else(bad)?;
        let (before, after) = (decimal(before)?, decimal(after)?);
        if before <= Decimal::ZERO || after <= Decimal::ZERO {
            return Err(bad());
        }
        let split = SplitEvent {
            effective_on: day.to_string(),
            numerator: after.normalize().to_string(),
            denominator: before.normalize().to_string(),
        };
        if let Some(previous) = splits.insert(day, split.clone()) {
            if previous != split {
                return Err(bad());
            }
        }
    }
    Ok(splits.into_values().collect())
}

fn supplemental(payload: &Value, symbol: &str, resolved: &Resolved) -> AppResult<Vec<SplitEvent>> {
    let data = chart(payload, symbol)?;
    let zone = data["meta"]["exchangeTimezoneName"]
        .as_str()
        .ok_or_else(bad)?
        .parse::<chrono_tz::Tz>()
        .map_err(|_| bad())?;
    if data["meta"]["currency"] != resolved.identity.currency
        || data["meta"]["instrumentType"] != resolved.instrument_type
        || !crate::models::market::close::matches_timezone(symbol, zone)
    {
        return Err(bad());
    }
    yahoo_splits(data, zone)
}

impl PublicMarketSource {
    pub(super) async fn eastmoney_primary(
        &self,
        symbol: &str,
        through: NaiveDate,
    ) -> AppResult<PriceHistory> {
        if is_a_share_stock(symbol) || is_mainland_fund(symbol) || symbol == "000001.SS" {
            return self.domestic(symbol, through).await;
        }
        let input = if symbol.ends_with(".HK") {
            security(symbol, "HKG").ok_or_else(bad)?.code
        } else {
            // Unknown exchanges and share-class aliases are left to Yahoo, never guessed.
            security(symbol, "NASDAQ").ok_or_else(bad)?.code
        };
        let (payload, search_source) = get(self
            .client
            .get("https://searchapi.eastmoney.com/api/suggest/get")
            .query(&[
                ("input", input.as_str()),
                ("type", "14"),
                ("count", "10"),
                ("token", "D43BF722C8E33CDC906FB84D85E326E8"),
            ]))
        .await?;
        let resolved = resolve(&payload, symbol, search_source)?;
        let (raw, raw_source_url) = self
            .eastmoney_rows(&resolved.identity, through, false)
            .await?;
        let (adjusted, adjusted_source_url) = self
            .eastmoney_rows(&resolved.identity, through, true)
            .await?;
        let mut action_source_urls = vec![resolved.source.clone()];
        let splits = match self
            .overseas_actions(
                &resolved,
                "https://datacenter.eastmoney.com/securities/api/data/v1/get",
            )
            .await
        {
            Ok((Some(splits), urls)) => {
                action_source_urls.extend(urls);
                splits
            }
            result => {
                match result {
                    Ok((_, urls)) => action_source_urls.extend(urls),
                    Err(error) => {
                        tracing::warn!(symbol,error=?error,"overseas split archive incomplete; checking Yahoo actions")
                    }
                }
                let (splits, url) = self.supplemental_actions(symbol, &resolved).await?;
                action_source_urls.push(url);
                splits
            }
        };
        PriceHistory {
            symbol: symbol.into(),
            currency: resolved.identity.currency.into(),
            window_start: history_start(through),
            completed_through: through,
            adjustment_anchor: through,
            fetched_at: Utc::now().to_rfc3339(),
            provider_code: "eastmoney".into(),
            provider_name: "东方财富".into(),
            raw_price_basis: "unadjusted_daily_close".into(),
            raw_source_url,
            adjusted_source_url,
            action_source_urls,
            methodology_url: eastmoney_methodology_url(symbol),
            exchange: resolved.identity.exchange.into(),
            mic_code: resolved.identity.mic.into(),
            instrument_type: resolved.instrument_type.into(),
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

    async fn overseas_actions(
        &self,
        resolved: &Resolved,
        endpoint: &str,
    ) -> AppResult<(Option<Vec<SplitEvent>>, Vec<String>)> {
        let hk = resolved.identity.market == 116;
        let mut collected = Vec::new();
        let mut urls = Vec::new();
        let mut expected = None;
        for number in 1..=20 {
            let (payload, source) = get(self.client.get(endpoint).query(&[
                (
                    "reportName",
                    if hk {
                        "RPT_HKF10_SH_SPLIT".into()
                    } else {
                        "RPT_USF10_SH_SPLIT".into()
                    },
                ),
                ("columns", "ALL".into()),
                (
                    "filter",
                    format!("(SECUCODE=\"{}\")", resolved.action_code()),
                ),
                ("pageNumber", number.to_string()),
                ("pageSize", "100".into()),
                (
                    "sortColumns",
                    if hk {
                        "NOTICE_DATE".into()
                    } else {
                        "EX_DIVIDEND_DATE,NOTICE_DATE".into()
                    },
                ),
                ("sortTypes", if hk { "-1".into() } else { "-1,-1".into() }),
                ("source", "SECURITIES".into()),
                ("client", "PC".into()),
            ]))
            .await?;
            urls.push(source);
            let Some((pages, count, rows)) = page(&payload)? else {
                return if number == 1 {
                    Ok((None, urls))
                } else {
                    Err(bad())
                };
            };
            if expected.is_some_and(|e| e != (pages, count)) {
                return Err(bad());
            }
            expected = Some((pages, count));
            if rows.len()
                != if number < pages {
                    100
                } else {
                    (count - (pages - 1) * 100) as usize
                }
            {
                return Err(bad());
            }
            collected.extend(rows);
            if number == pages {
                if collected.len() as u64 != count {
                    return Err(bad());
                }
                let today = Utc::now()
                    .with_timezone(&chrono_tz::Asia::Shanghai)
                    .date_naive();
                return Ok((Some(action_rows(&collected, resolved, today)?), urls));
            }
        }
        Err(bad())
    }

    async fn supplemental_actions(
        &self,
        symbol: &str,
        resolved: &Resolved,
    ) -> AppResult<(Vec<SplitEvent>, String)> {
        let mut errors = Vec::new();
        for host in [
            "https://query1.finance.yahoo.com",
            "https://query2.finance.yahoo.com",
        ] {
            let result = async {
                let (payload, source) = self.yahoo_actions(host, symbol).await?;
                Ok((supplemental(&payload, symbol, resolved)?, source))
            }
            .await;
            match result {
                Ok(actions) => return Ok(actions),
                Err(error) => {
                    tracing::warn!(symbol,error=?error,"supplemental Yahoo split query failed");
                    errors.push(source_error(&error));
                }
            }
        }
        Err(AppError::MarketData(format!(
            "拆合股资料未能完成核验（Yahoo 补充）：{}",
            errors.join("；")
        )))
    }
}

#[cfg(test)]
mod tests;
