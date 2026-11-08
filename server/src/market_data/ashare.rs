//! Unadjusted mainland stock closes and fully collected implemented distributions.
//! The free quote cache and cross-account concurrency budget stay in the injected caller.
use crate::{
    models::{
        market::{canonical_symbol, close, is_a_share_stock, ASHARE_METHODOLOGY_URL},
        SecurityPriceQuery, SecurityPriceQuote, SplitEvent,
    },
    ports::market::SecurityPriceProvider,
    AppError, AppResult,
};
use async_trait::async_trait;
use chrono::{Duration as Days, NaiveDate, Utc};
use reqwest::Client;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

pub(crate) struct AshareSecurityPriceProvider {
    client: Client,
    price_url: String,
    actions_url: String,
}
impl AshareSecurityPriceProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(8))
                .user_agent("Mozilla/5.0 (compatible; mario/market-data)")
                .build()?,
            price_url: "https://push2his.eastmoney.com/api/qt/stock/kline/get".into(),
            actions_url: "https://datacenter-web.eastmoney.com/api/data/v1/get".into(),
        })
    }
}
fn bad() -> AppError {
    AppError::MarketData("东方财富证券、收盘价或送转资料不完整".into())
}

fn parse_close(
    payload: &Value,
    symbol: &str,
    day: NaiveDate,
    end: NaiveDate,
) -> AppResult<(NaiveDate, Decimal)> {
    let (code, exchange) = symbol.split_once('.').ok_or_else(bad)?;
    let market = if exchange == "SS" { 1 } else { 0 };
    let data = &payload["data"];
    if !is_a_share_stock(symbol)
        || payload["rc"] != 0
        || data["code"] != code
        || data["market"] != market
    {
        return Err(bad());
    }
    let mut seen = BTreeSet::new();
    let mut selected = None;
    for row in data["klines"].as_array().ok_or_else(bad)? {
        let fields = row.as_str().ok_or_else(bad)?.split(',').collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(bad());
        }
        let date = NaiveDate::parse_from_str(fields[0], "%Y-%m-%d").map_err(|_| bad())?;
        if !seen.insert(date) {
            return Err(bad());
        }
        if date > end || (day - date).num_days() > 10 {
            continue;
        }
        if matches!(fields[2], "" | "-" | "null") {
            continue;
        }
        let price = Decimal::from_str_exact(fields[2]).map_err(|_| bad())?;
        if price <= Decimal::ZERO || price > Decimal::from(1_000_000_000_000_000_i64) {
            return Err(bad());
        }
        if selected.is_none_or(|(previous, _)| date > previous) {
            selected = Some((date, price));
        }
    }
    selected.ok_or_else(bad)
}

#[async_trait]
impl SecurityPriceProvider for AshareSecurityPriceProvider {
    async fn quote(&self, query: &SecurityPriceQuery, _: &str) -> AppResult<SecurityPriceQuote> {
        let symbol = canonical_symbol(&query.symbol)?;
        if !is_a_share_stock(&symbol) {
            return Err(bad());
        }
        let day = query.on_date.parse::<NaiveDate>().map_err(|_| bad())?;
        let end = close::completed_day(&symbol, day, Utc::now());
        let (code, exchange) = symbol.split_once('.').unwrap();
        let market = if exchange == "SS" { "1" } else { "0" };
        let response = self
            .client
            .get(&self.price_url)
            .header("Referer", "https://quote.eastmoney.com/")
            .query(&[
                ("secid", format!("{market}.{code}")),
                ("klt", "101".into()),
                ("fqt", "0".into()),
                ("beg", (day - Days::days(10)).format("%Y%m%d").to_string()),
                ("end", end.format("%Y%m%d").to_string()),
                ("fields1", "f1,f2,f3,f4,f5,f6".into()),
                ("fields2", "f51,f52,f53".into()),
            ])
            .send()
            .await?
            .error_for_status()?;
        let source_url = response.url().to_string();
        let payload = super::read_public_json(response).await?;
        let (observed, price) = parse_close(&payload, &symbol, day, end)?;
        // A quote without a fully verified distribution history must fall back as a whole.
        // Check through today even for historical prices, to protect current share quantities.
        let (payload, _) =
            super::watch::distribution_payload(&self.client, &self.actions_url, &symbol).await?;
        let today = Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let events = super::watch::parse_dividends(&payload, &symbol, today)?;
        let mut bonus_by_day = BTreeMap::<NaiveDate, Decimal>::new();
        for event in events {
            let extra = Decimal::from_str_exact(&event.bonus_per_ten)
                .map_err(|_| bad())?
                .checked_add(Decimal::from_str_exact(&event.transfer_per_ten).map_err(|_| bad())?)
                .ok_or_else(bad)?;
            let previous = bonus_by_day.entry(event.ex_date).or_default();
            *previous = previous.checked_add(extra).ok_or_else(bad)?;
        }
        let splits = bonus_by_day
            .into_iter()
            .filter(|(_, extra)| *extra > Decimal::ZERO)
            .map(|(date, extra)| {
                Ok(SplitEvent {
                    effective_on: date.to_string(),
                    numerator: Decimal::from(10)
                        .checked_add(extra)
                        .ok_or_else(bad)?
                        .normalize()
                        .to_string(),
                    denominator: "10".into(),
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(SecurityPriceQuote {
            symbol: query.symbol.trim().into(), currency: "CNY".into(), close: price.to_f64().ok_or_else(bad)?, exact_close: Some(price.normalize().to_string()),
            requested_on: day.to_string(), observed_on: observed.to_string(), staleness_days: (day-observed).num_days(),
            provider_code: "eastmoney_daily_close".into(), provider_name: "东方财富".into(), exchange: if exchange == "SS" { "SHH" } else { "SHZ" }.into(),
            mic_code: if exchange == "SS" { "XSHG" } else { "XSHE" }.into(), instrument_type: "EQUITY".into(), price_basis: "unadjusted_daily_close".into(),
            source_url, methodology_url: ASHARE_METHODOLOGY_URL.into(), splits,
            disclaimer: "公开未复权日收盘价；送股、转增按完整已实施分配资料核对，不自动调整数量或执行交易。仅供持仓参考估值，其他公司行动仍须结合券商记录核对。".into(),
        })
    }
}

#[cfg(test)]
mod tests;
