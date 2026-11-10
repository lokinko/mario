//! A single public, dated price revision. No account state or I/O.
use crate::{
    models::{SecurityPriceQuote, SplitEvent},
    AppError, AppResult,
};
use chrono::{DateTime, Months, NaiveDate};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const HISTORY_YEARS: u32 = 5;
pub fn history_start(through: NaiveDate) -> NaiveDate {
    through
        .checked_sub_months(Months::new(HISTORY_YEARS * 12))
        .unwrap()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyPrice {
    pub day: NaiveDate,
    pub raw_close: Option<String>,
    pub adjusted_close: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PriceHistory {
    pub symbol: String,
    pub currency: String,
    pub window_start: NaiveDate,
    pub completed_through: NaiveDate,
    pub adjustment_anchor: NaiveDate,
    pub fetched_at: String,
    pub provider_code: String,
    pub provider_name: String,
    pub raw_price_basis: String,
    pub raw_source_url: String,
    pub adjusted_source_url: String,
    pub action_source_urls: Vec<String>,
    pub methodology_url: String,
    pub exchange: String,
    pub mic_code: String,
    pub instrument_type: String,
    pub bars: Vec<DailyPrice>,
    pub splits: Vec<SplitEvent>,
}
fn invalid() -> AppError {
    AppError::MarketData("公共行情的证券、价格日期、复权基准或来源不一致".into())
}
fn price(text: &str) -> AppResult<Decimal> {
    let value = Decimal::from_str_exact(text)
        .or_else(|_| Decimal::from_scientific(text))
        .map_err(|_| invalid())?;
    if value <= Decimal::ZERO || value > Decimal::from(1_000_000_000_000_000_i64) {
        return Err(invalid());
    }
    Ok(value)
}
impl PriceHistory {
    pub fn revision(&self) -> AppResult<String> {
        use sha2::{Digest, Sha256};
        let mut content = self.clone();
        content.fetched_at.clear();
        content.action_source_urls.clear();
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&content)?)
        ))
    }
    /// Normalize a vendor's adjusted sequence to its latest completed raw close.
    pub fn normalize(mut self) -> AppResult<Self> {
        self.bars.sort_by_key(|b| b.day);
        let anchor = self
            .bars
            .iter()
            .rev()
            .find(|b| b.raw_close.is_some() && b.adjusted_close.is_some())
            .ok_or_else(invalid)?;
        self.adjustment_anchor = anchor.day;
        let raw = price(anchor.raw_close.as_deref().unwrap())?;
        let adjusted = price(anchor.adjusted_close.as_deref().unwrap())?;
        for bar in &mut self.bars {
            if let Some(value) = &bar.adjusted_close {
                bar.adjusted_close = Some(if bar.day == self.adjustment_anchor {
                    raw.normalize().to_string()
                } else {
                    price(value)?
                        .checked_mul(raw)
                        .and_then(|v| v.checked_div(adjusted))
                        .ok_or_else(invalid)?
                        .normalize()
                        .to_string()
                });
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> AppResult<()> {
        if super::canonical_symbol(&self.symbol)? != self.symbol
            || self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
            || self.window_start != history_start(self.completed_through)
            || self.bars.is_empty()
            || self.bars.len() > 2000
            || !["eastmoney", "yahoo"].contains(&self.provider_code.as_str())
            || self.provider_name.is_empty()
            || self.raw_source_url.is_empty()
            || self.adjusted_source_url.is_empty()
            || self.methodology_url.is_empty()
            || DateTime::parse_from_rfc3339(&self.fetched_at).is_err()
        {
            return Err(invalid());
        }
        let param = |url: &url::Url, key: &str, value: &str| {
            url.query_pairs()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                .collect::<Vec<_>>()
                == [value]
        };
        for (source, adjusted) in [
            (&self.raw_source_url, false),
            (&self.adjusted_source_url, true),
        ] {
            let url = url::Url::parse(source).map_err(|_| invalid())?;
            if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
                return Err(invalid());
            }
            let valid = match self.provider_code.as_str() {
                "eastmoney" => {
                    super::eastmoney::security(&self.symbol, &self.exchange).is_some_and(|s| {
                        s.currency == self.currency && s.matches_url(&url, adjusted)
                    }) && self.raw_price_basis == "unadjusted_daily_close"
                }
                "yahoo" => {
                    matches!(
                        url.host_str(),
                        Some("query1.finance.yahoo.com" | "query2.finance.yahoo.com")
                    ) && url
                        .path()
                        .strip_prefix("/v8/finance/chart/")
                        .is_some_and(|code| code == self.symbol)
                        && param(&url, "interval", "1d")
                        && self.raw_price_basis == "split_adjusted_daily_close"
                }
                _ => false,
            };
            if !valid {
                return Err(invalid());
            }
        }
        let mut seen = BTreeSet::new();
        let mut previous = None;
        for bar in &self.bars {
            if !seen.insert(bar.day)
                || previous.is_some_and(|p| p >= bar.day)
                || bar.day < self.window_start
                || bar.day > self.completed_through
            {
                return Err(invalid());
            }
            previous = Some(bar.day);
            for p in [&bar.raw_close, &bar.adjusted_close].into_iter().flatten() {
                price(p)?;
            }
        }
        let anchor = self
            .bars
            .iter()
            .rev()
            .find(|b| b.raw_close.is_some() && b.adjusted_close.is_some())
            .ok_or_else(invalid)?;
        if anchor.day != self.adjustment_anchor
            || price(anchor.raw_close.as_deref().unwrap())?
                != price(anchor.adjusted_close.as_deref().unwrap())?
        {
            return Err(invalid());
        }
        for split in &self.splits {
            let day = split
                .effective_on
                .parse::<NaiveDate>()
                .map_err(|_| invalid())?;
            if day
                > DateTime::parse_from_rfc3339(&self.fetched_at)
                    .map_err(|_| invalid())?
                    .with_timezone(&chrono_tz::Asia::Shanghai)
                    .date_naive()
            {
                return Err(invalid());
            }
            price(&split.numerator)?;
            price(&split.denominator)?;
        }
        Ok(())
    }
    pub fn quote(
        &self,
        requested: NaiveDate,
        cutoff: NaiveDate,
        original_symbol: &str,
    ) -> AppResult<SecurityPriceQuote> {
        self.validate()?;
        let bar = self
            .bars
            .iter()
            .rev()
            .find(|b| b.day <= cutoff && b.raw_close.is_some() && b.adjusted_close.is_some())
            .ok_or_else(|| AppError::MarketData("五年公共行情中没有该日期的有效收盘价".into()))?;
        let exact = bar.adjusted_close.clone().unwrap();
        Ok(SecurityPriceQuote {
            symbol:original_symbol.trim().into(),currency:self.currency.clone(),close:price(&exact)?.to_f64().ok_or_else(invalid)?,exact_close:Some(exact),
            requested_on:requested.to_string(),observed_on:bar.day.to_string(),staleness_days:(requested-bar.day).num_days(),
            provider_code:format!("{}_forward_close",self.provider_code),provider_name:self.provider_name.clone(),exchange:self.exchange.clone(),mic_code:self.mic_code.clone(),instrument_type:self.instrument_type.clone(),
            price_basis:"forward_adjusted_daily_close".into(),source_url:self.adjusted_source_url.clone(),methodology_url:self.methodology_url.clone(),splits:self.splits.clone(),
            disclaimer:format!("公共数据库五年前复权收盘序列；复权基准 {}，行情采集 {}。历史复权价格不等于当时实际成交价；成交和成本保留原值，公司行动须人工核对。",self.adjustment_anchor,self.fetched_at),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;
