//! Buy-watch contracts and deterministic calculations. No account or network access.
pub(crate) mod calendar;
pub(crate) mod catalog;
pub(crate) mod grid;
pub(crate) mod rules;
pub(crate) mod trends;
use rules::WatchRules;

use crate::{AppError, AppResult};
use chrono::{Datelike, NaiveDate};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MIN_YIELD_PCT: f64 = 5.0;
pub const BAND_DISTANCE_PCT: f64 = 2.0;
pub const MAX_WATCHES: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WatchInput {
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub thesis: String,
}
impl WatchInput {
    pub fn normalize(mut self) -> AppResult<Self> {
        self.symbol = self.symbol.trim().to_uppercase().replace(".SH", ".SS");
        self.name = self.name.trim().into();
        self.thesis = self.thesis.trim().into();
        let valid_symbol = self.symbol.len() == 9
            && self.symbol.as_bytes()[..6]
                .iter()
                .all(|c| c.is_ascii_digit())
            && ((self.symbol.ends_with(".SS")
                && (self.symbol.starts_with("60") || self.symbol.starts_with("68")))
                || (self.symbol.ends_with(".SZ")
                    && (self.symbol.starts_with("00") || self.symbol.starts_with("30"))));
        if !valid_symbol
            || self.name.is_empty()
            || self.name.chars().count() > 80
            || self.thesis.chars().count() > 2000
        {
            return Err(AppError::Validation(
                "请填写沪深 A 股代码（如 600036.SS）、公司名称和不超过 2000 字的关注理由".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEntry {
    #[serde(flatten)]
    pub input: WatchInput,
    pub created_at: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_plan: Option<grid::GridPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceBar {
    pub day: NaiveDate,
    pub close: Option<f64>,
    pub adjusted_close: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividendEvent {
    // Absent only in observations saved before independent dividend storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_date: Option<NaiveDate>,
    pub ex_date: NaiveDate,
    pub cash_per_ten: String,
    pub bonus_per_ten: String,
    pub transfer_per_ten: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendBasis {
    pub as_of: NaiveDate,
    pub fetched_at: String,
}

/// One complete, replaceable source snapshot per security, including known-empty results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendHistory {
    #[serde(flatten)]
    pub basis: DividendBasis,
    pub sources: Vec<String>,
    pub events: Vec<DividendEvent>,
}

impl DividendHistory {
    /// Coverage wins over collection time; a delayed older cutoff cannot downgrade facts.
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.basis
            .as_of
            .cmp(&other.basis.as_of)
            .then_with(|| {
                chrono::DateTime::parse_from_rfc3339(&self.basis.fetched_at)
                    .ok()
                    .cmp(&chrono::DateTime::parse_from_rfc3339(&other.basis.fetched_at).ok())
            })
            .is_gt()
    }
    pub fn validate(&self) -> AppResult<()> {
        let fetched = chrono::DateTime::parse_from_rfc3339(&self.basis.fetched_at)
            .map_err(|_| AppError::Validation("分红采集时间无效".into()))?;
        let mut seen = std::collections::BTreeSet::new();
        if self.basis.as_of
            > fetched
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive()
            || self.sources.is_empty()
            || self.sources.iter().any(|s| !s.starts_with("https://"))
            || self.events.iter().any(|e| {
                e.ex_date > self.basis.as_of
                    || e.report_date.is_none()
                    || !seen.insert((e.ex_date, e.report_date))
                    || [&e.cash_per_ten, &e.bonus_per_ten, &e.transfer_per_ten]
                        .iter()
                        .any(|v| Decimal::from_str_exact(v).map_or(true, |v| v < Decimal::ZERO))
            })
        {
            return Err(AppError::Validation("分红事件或来源覆盖无效".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WatchMarketData {
    pub price_revision: Option<String>,
    pub bars: Vec<PriceBar>,
    pub calendar: Vec<NaiveDate>,
    pub dividends: Option<Vec<DividendEvent>>,
    pub dividend_basis: Option<DividendBasis>,
    pub sources: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandReference {
    #[default]
    Lower,
    Middle,
    Upper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub lower: f64,
    pub middle: f64,
    pub upper: f64,
    pub distance_pct: Option<f64>,
    #[serde(default)]
    pub reference: BandReference,
    #[serde(default)]
    pub matches_rule: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchStatus {
    Attention,
    Waiting,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchIssue {
    PriceUnavailable,
    DailyHistoryIncomplete,
    WeeklyHistoryIncomplete,
    DividendUnavailable,
    NonPositiveBand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    #[serde(default)]
    pub rules: WatchRules,
    #[serde(default)]
    pub yield_matches_rule: Option<bool>,
    pub symbol: String,
    pub observed_on: String,
    pub checked_at: String,
    pub price: Option<f64>,
    pub dividend_per_share: Option<String>,
    pub yield_pct: Option<f64>,
    pub daily: Option<Band>,
    pub weekly: Option<Band>,
    pub status: WatchStatus,
    pub issues: Vec<WatchIssue>,
    pub sources: Vec<String>,
    pub dividend_events: Vec<DividendEvent>,
    #[serde(default)]
    pub dividend_basis: Option<DividendBasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dividend_history: Option<DividendWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trends: Option<Vec<trends::TrendPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend_window_years: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_revision: Option<String>,
}

/// Display evidence only; TTM amounts and frozen calculation events stay independent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub events: Vec<DividendEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchRow {
    pub entry: WatchEntry,
    pub latest: Option<Observation>,
    pub dividend_prices: Vec<DividendPrice>,
    pub grid_projection: Option<grid::GridProjection>,
    pub alert: bool,
    pub checked_at: Option<String>,
    pub checked_for: Option<String>,
    pub refresh_failed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DividendPrice {
    pub yield_pct: f64,
    pub price: String,
    pub reached: Option<bool>,
    pub change_pct: Option<f64>,
}

fn positive_ttm_dividend(observation: &Observation) -> Option<Decimal> {
    if observation
        .dividend_basis
        .as_ref()
        .is_some_and(|basis| basis.as_of.to_string().as_str() < observation.observed_on.as_str())
    {
        return None;
    }
    observation
        .dividend_per_share
        .as_deref()
        .and_then(|v| Decimal::from_str_exact(v).ok())
        .filter(|v| *v > Decimal::ZERO)
}

/// Read projection only: historical observations and backup payloads stay unchanged.
pub fn dividend_prices(observation: Option<&Observation>) -> Vec<DividendPrice> {
    let Some((observation, dividend)) =
        observation.and_then(|o| positive_ttm_dividend(o).map(|d| (o, d)))
    else {
        return vec![];
    };
    let current = observation
        .price
        .filter(|p| p.is_finite() && *p > 0.0)
        .and_then(|p| Decimal::from_str_exact(&p.to_string()).ok());
    let mut levels = vec![400, 500, 600, 700, observation.rules.min_yield_bps];
    levels.sort_unstable();
    levels.dedup();
    levels
        .into_iter()
        .filter_map(|yield_bps| {
            let yield_pct = Decimal::new(yield_bps.into(), 2);
            // Round down to a tradable cent so the shown price never overstates the threshold.
            let price = dividend
                .checked_mul(Decimal::ONE_HUNDRED)?
                .checked_div(yield_pct)?
                .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::ToNegativeInfinity);
            let change_pct = current.and_then(|p| {
                price
                    .checked_div(p)?
                    .checked_sub(Decimal::ONE)?
                    .checked_mul(Decimal::ONE_HUNDRED)?
                    .to_f64()
            });
            Some(DividendPrice {
                yield_pct: yield_bps as f64 / 100.0,
                price: format!("{price:.2}"),
                reached: current.and_then(|p| {
                    Some(dividend.checked_mul(Decimal::ONE_HUNDRED)? >= p.checked_mul(yield_pct)?)
                }),
                change_pct,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchBoard {
    pub default_rules: WatchRules,
    pub rows: Vec<WatchRow>,
    pub week_start: String,
    pub week_end: String,
    pub weekly: Vec<Observation>,
    pub rules: WatchRules,
}

fn band(values: &[Option<f64>], price: f64) -> Option<Band> {
    if values.len() < 20 {
        return None;
    }
    let values: Vec<f64> = values[values.len() - 20..]
        .iter()
        .copied()
        .collect::<Option<_>>()?;
    if !values.iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let middle = values.iter().sum::<f64>() / 20.0;
    let deviation = (values.iter().map(|v| (v - middle).powi(2)).sum::<f64>() / 20.0).sqrt();
    let lower = middle - 2.0 * deviation;
    let mut result = Band {
        lower,
        middle,
        upper: middle + 2.0 * deviation,
        distance_pct: None,
        reference: BandReference::Lower,
        matches_rule: false,
    };
    result.apply_rule(price, &WatchRules::default().daily);
    Some(result)
}

impl Observation {
    /// Reuse the exact same rules for live inputs and migration of saved indicator facts.
    pub(crate) fn apply_rules(&mut self, rules: &WatchRules) {
        self.rules = *rules;
        for (band, rule) in [
            (&mut self.daily, &rules.daily),
            (&mut self.weekly, &rules.weekly),
        ] {
            if let (Some(band), Some(price)) = (band.as_mut(), self.price) {
                band.apply_rule(price, rule);
            }
        }
        self.issues
            .retain(|issue| *issue != WatchIssue::NonPositiveBand);
        if self
            .daily
            .iter()
            .chain(self.weekly.iter())
            .any(|b| b.distance_pct.is_none())
        {
            self.issues.push(WatchIssue::NonPositiveBand);
        }
        self.yield_matches_rule = self
            .dividend_per_share
            .as_deref()
            .filter(|_| {
                self.yield_pct.is_some()
                    && self
                        .dividend_basis
                        .as_ref()
                        .is_none_or(|b| b.as_of.to_string() >= self.observed_on)
            })
            .and_then(|v| Decimal::from_str_exact(v).ok())
            .zip(
                self.price
                    .and_then(|p| Decimal::from_str_exact(&p.to_string()).ok()),
            )
            .and_then(|(d, p)| {
                Some(
                    d.checked_mul(Decimal::ONE_HUNDRED)?
                        >= p.checked_mul(Decimal::new(rules.min_yield_bps.into(), 2))?,
                )
            });
        self.status = if !self.issues.is_empty() {
            WatchStatus::Incomplete
        } else if self.yield_matches_rule == Some(true)
            && self.daily.as_ref().is_some_and(|b| b.matches_rule)
            && self.weekly.as_ref().is_some_and(|b| b.matches_rule)
        {
            WatchStatus::Attention
        } else {
            WatchStatus::Waiting
        };
    }
}

pub fn trailing_dividends(events: &[DividendEvent], day: NaiveDate) -> Option<Decimal> {
    let start = day - chrono::Duration::days(365);
    let mut total = Decimal::ZERO;
    for event in events
        .iter()
        .filter(|e| e.ex_date > start && e.ex_date <= day)
    {
        let mut cash = Decimal::from_str_exact(&event.cash_per_ten).ok()? / Decimal::TEN;
        if cash < Decimal::ZERO {
            return None;
        }
        // Normalize every past cash distribution to the current share unit, including
        // simultaneous bonus shares. The provider supplies per-ten original shares.
        for action in events
            .iter()
            .filter(|a| a.ex_date >= event.ex_date && a.ex_date <= day)
        {
            let bonus = Decimal::from_str_exact(&action.bonus_per_ten).ok()?;
            let transfer = Decimal::from_str_exact(&action.transfer_per_ten).ok()?;
            if bonus < Decimal::ZERO || transfer < Decimal::ZERO {
                return None;
            }
            let factor = Decimal::ONE.checked_add(bonus.checked_add(transfer)? / Decimal::TEN)?;
            cash = cash.checked_div(factor)?;
        }
        total = total.checked_add(cash)?;
    }
    Some(total)
}

pub fn evaluate(
    rules: &WatchRules,
    symbol: &str,
    data: &WatchMarketData,
    cutoff: NaiveDate,
    checked_at: String,
) -> Observation {
    let day = data
        .calendar
        .iter()
        .copied()
        .filter(|d| *d <= cutoff)
        .max()
        .unwrap_or(cutoff);
    let bars: BTreeMap<_, _> = data
        .bars
        .iter()
        .filter(|b| b.day <= day)
        .map(|b| (b.day, b))
        .collect();
    let current = bars.get(&day);
    let price = current
        .and_then(|b| b.close)
        .filter(|v| v.is_finite() && *v > 0.0);
    let scale = price
        .zip(current.and_then(|b| b.adjusted_close))
        .filter(|(_, a)| a.is_finite() && *a > 0.0)
        .map(|(p, a)| p / a);
    let mut weeks: BTreeMap<(i32, u32), Vec<Option<f64>>> = BTreeMap::new();
    let mut daily = Vec::new();
    let mut sessions = Vec::new();
    for date in data.calendar.iter().filter(|d| **d <= day) {
        let value = bars
            .get(date)
            .and_then(|b| b.adjusted_close)
            .zip(scale)
            .filter(|(v, s)| v.is_finite() && *v > 0.0 && s.is_finite())
            .map(|(v, s)| v * s);
        daily.push(value);
        sessions.push((*date, value));
        let week = date.iso_week();
        weeks
            .entry((week.year(), week.week()))
            .or_default()
            .push(value);
    }
    let weekly: Vec<_> = weeks
        .values()
        .map(|v| {
            if v.iter().all(Option::is_some) {
                v.last().copied().flatten()
            } else {
                None
            }
        })
        .collect();
    let daily = price.and_then(|p| band(&daily, p));
    let weekly = price.and_then(|p| band(&weekly, p));
    let dividends = data
        .dividends
        .as_ref()
        .filter(|_| data.dividend_basis.as_ref().is_none_or(|b| b.as_of >= day))
        .and_then(|d| trailing_dividends(d, day));
    let yield_decimal = dividends
        .zip(price.and_then(|p| Decimal::from_str_exact(&p.to_string()).ok()))
        .and_then(|(d, p)| d.checked_mul(Decimal::ONE_HUNDRED)?.checked_div(p));
    let yield_pct = yield_decimal.and_then(|v| v.to_f64());
    let mut issues = Vec::new();
    if price.is_none() {
        issues.push(WatchIssue::PriceUnavailable);
    }
    if daily.is_none() {
        issues.push(WatchIssue::DailyHistoryIncomplete);
    }
    if weekly.is_none() {
        issues.push(WatchIssue::WeeklyHistoryIncomplete);
    }
    if yield_pct.is_none() {
        issues.push(WatchIssue::DividendUnavailable);
    }
    let mut observation = Observation {
        rules: *rules,
        yield_matches_rule: None,
        symbol: symbol.into(),
        observed_on: day.to_string(),
        checked_at,
        price,
        dividend_per_share: dividends.map(|v| v.normalize().to_string()),
        yield_pct,
        daily,
        weekly,
        status: WatchStatus::Incomplete,
        issues,
        sources: data.sources.clone(),
        dividend_events: data
            .dividends
            .as_ref()
            .map(|events| {
                events
                    .iter()
                    .filter(|e| e.ex_date > day - chrono::Duration::days(365) && e.ex_date <= day)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        dividend_basis: data.dividend_basis.clone(),
        trends: Some(trends::calculate(&sessions)),
        price_revision: data.price_revision.clone(),
        trend_window_years: Some(crate::models::market::history::HISTORY_YEARS),
        dividend_history: data.dividends.as_ref().map(|events| {
            let start = day.checked_sub_months(chrono::Months::new(36)).unwrap();
            DividendWindow {
                start,
                end: day,
                events: events
                    .iter()
                    .filter(|e| e.ex_date >= start && e.ex_date <= day)
                    .cloned()
                    .collect(),
            }
        }),
    };
    observation.apply_rules(rules);
    observation
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    pub fn market_fixture() -> (WatchMarketData, NaiveDate) {
        let first = date("2026-01-05");
        let calendar: Vec<_> = (0..180)
            .map(|i| first + chrono::Duration::days(i))
            .filter(|d| d.weekday().num_days_from_monday() < 5)
            .collect();
        let day = *calendar.last().unwrap();
        let bars = calendar
            .iter()
            .map(|day| PriceBar {
                day: *day,
                close: Some(100.0),
                adjusted_close: Some(100.0),
            })
            .collect();
        (
            WatchMarketData {
                price_revision: None,
                bars,
                calendar,
                dividend_basis: None,
                dividends: Some(vec![DividendEvent {
                    report_date: None,
                    ex_date: day - chrono::Duration::days(2),
                    cash_per_ten: "50".into(),
                    bonus_per_ten: "0".into(),
                    transfer_per_ten: "0".into(),
                }]),
                sources: vec!["https://example.test/prices".into()],
            },
            day,
        )
    }
    #[test]
    fn dividend_reference_prices_are_exact_conservative_and_do_not_change_signals() {
        let (data, day) = market_fixture();
        let mut observation = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        observation.dividend_per_share = Some("2.016".into());
        observation.price = Some(40.32);
        let before = observation.clone();
        let prices = dividend_prices(Some(&observation));
        assert_eq!(
            prices.iter().map(|p| p.price.as_str()).collect::<Vec<_>>(),
            vec!["50.40", "40.32", "33.60", "28.80"]
        );
        assert_eq!(prices[1].reached, Some(true));
        assert_eq!(prices[1].change_pct, Some(0.0));
        assert_eq!(prices[2].reached, Some(false));
        assert_eq!(observation, before);
        observation.dividend_per_share = Some("1".into());
        observation.price = Some(16.67);
        let prices = dividend_prices(Some(&observation));
        assert_eq!(prices[2].price, "16.66");
        assert_eq!(prices[2].reached, Some(false));
        observation.price = None;
        assert!(dividend_prices(Some(&observation))
            .iter()
            .all(|p| p.reached.is_none() && p.change_pct.is_none()));
        for value in [None, Some("0"), Some("-1"), Some("invalid")] {
            observation.dividend_per_share = value.map(String::from);
            assert!(dividend_prices(Some(&observation)).is_empty());
        }
        observation.dividend_per_share = Some("1".into());
        observation.dividend_basis = Some(DividendBasis {
            as_of: day - chrono::Duration::days(1),
            fetched_at: "2026-07-03T12:00:00Z".into(),
        });
        assert!(dividend_prices(Some(&observation)).is_empty());
        assert!(dividend_prices(None).is_empty());
    }
    #[test]
    fn five_percent_is_inclusive_and_both_bands_are_required() {
        let (mut data, day) = market_fixture();
        let observation = evaluate(
            &Default::default(),
            "600036.SS",
            &data,
            day,
            "2026-07-03T12:00:00Z".into(),
        );
        assert_eq!(observation.yield_pct, Some(5.0));
        assert_eq!(observation.status, WatchStatus::Attention);
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "49.99".into();
        assert_eq!(
            evaluate(&Default::default(), "600036.SS", &data, day, String::new()).status,
            WatchStatus::Waiting
        );
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "50".into();
        // Older daily prices affect the weekly band without changing the latest 20 daily closes.
        for bar in data.bars.iter_mut().take(80) {
            bar.adjusted_close = Some(200.0);
        }
        let result = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert!(result.daily.unwrap().matches_rule);
        assert!(result.weekly.as_ref().unwrap().matches_rule);
        assert!(result.weekly.as_ref().unwrap().lower < 100.0);
        assert_eq!(result.status, WatchStatus::Attention);
        // A close above the weekly middle fails even when the daily lower still matches.
        for bar in data.bars.iter_mut().take(80) {
            bar.adjusted_close = Some(50.0);
        }
        let result = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert!(result.daily.unwrap().matches_rule);
        assert!(!result.weekly.unwrap().matches_rule);
        assert_eq!(result.status, WatchStatus::Waiting);
    }
    #[test]
    fn future_rows_are_ignored_and_missing_history_is_unknown() {
        let (mut data, day) = market_fixture();
        let before = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        data.bars.push(PriceBar {
            day: day + chrono::Duration::days(3),
            close: Some(1.0),
            adjusted_close: Some(1.0),
        });
        data.calendar.push(day + chrono::Duration::days(3));
        assert_eq!(
            before,
            evaluate(&Default::default(), "600036.SS", &data, day, String::new())
        );
        data.bars.remove(data.bars.len() - 3);
        let missing = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert_eq!(missing.status, WatchStatus::Incomplete);
        assert_eq!(missing.yield_pct, Some(5.0));
        assert!(missing.daily.is_none());
        assert!(missing.weekly.is_none());
    }
    #[test]
    fn proximity_is_symmetric_two_percent_of_lower_price() {
        let values = vec![Some(100.0); 20];
        assert!(band(&values, 98.0).unwrap().matches_rule);
        assert!(band(&values, 102.0).unwrap().matches_rule);
        assert!(!band(&values, 97.9).unwrap().matches_rule);
        assert!(!band(&values, 102.1).unwrap().matches_rule);
        let mut missing = values;
        missing[10] = None;
        assert!(band(&missing, 100.0).is_none());
    }
    #[test]
    fn weekly_middle_is_inclusive_and_does_not_require_a_positive_lower() {
        let mut weekly = band(&[Some(100.0); 20], 100.0).unwrap();
        weekly.lower = -10.0;
        weekly.apply_rule(100.0, &WatchRules::default().weekly);
        assert!(weekly.matches_rule);
        assert_eq!(weekly.distance_pct, Some(0.0));
        weekly.apply_rule(100.000001, &WatchRules::default().weekly);
        assert!(!weekly.matches_rule);
        weekly.apply_rule(40.0, &WatchRules::default().weekly);
        assert!(weekly.matches_rule);
        assert_eq!(weekly.distance_pct, Some(-60.0));
        let (data, day) = market_fixture();
        let mut observation = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        observation.weekly.as_mut().unwrap().lower = -10.0;
        observation.apply_rules(&Default::default());
        assert_eq!(observation.status, WatchStatus::Attention);
        observation.daily.as_mut().unwrap().lower = -10.0;
        observation.apply_rules(&Default::default());
        assert_eq!(observation.status, WatchStatus::Incomplete);
        assert!(observation.issues.contains(&WatchIssue::NonPositiveBand));
    }
    #[test]
    fn dividends_use_implemented_window_and_current_share_units() {
        let events = vec![
            DividendEvent {
                report_date: None,
                ex_date: date("2026-01-01"),
                cash_per_ten: "10".into(),
                bonus_per_ten: "0".into(),
                transfer_per_ten: "0".into(),
            },
            DividendEvent {
                report_date: None,
                ex_date: date("2026-06-01"),
                cash_per_ten: "0".into(),
                bonus_per_ten: "10".into(),
                transfer_per_ten: "0".into(),
            },
            DividendEvent {
                report_date: None,
                ex_date: date("2027-01-01"),
                cash_per_ten: "999".into(),
                bonus_per_ten: "0".into(),
                transfer_per_ten: "0".into(),
            },
        ];
        assert_eq!(
            trailing_dividends(&events, date("2026-11-04"))
                .unwrap()
                .normalize()
                .to_string(),
            "0.5"
        );
    }
    #[test]
    fn stored_events_reprice_roll_out_and_require_current_coverage() {
        let (mut data, day) = market_fixture();
        data.dividend_basis = Some(DividendBasis {
            as_of: day,
            fetched_at: "2026-07-03T12:00:00Z".into(),
        });
        let first = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert_eq!(first.yield_pct, Some(5.0));
        data.bars.last_mut().unwrap().close = Some(50.0);
        assert_eq!(
            evaluate(&Default::default(), "600036.SS", &data, day, String::new()).yield_pct,
            Some(10.0)
        );
        data.dividends.as_mut().unwrap()[0].ex_date = day - chrono::Duration::days(365);
        let expired = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert_eq!(expired.dividend_per_share.as_deref(), Some("0"));
        assert_eq!(expired.yield_pct, Some(0.0));
        assert!(expired.dividend_events.is_empty());
        data.dividend_basis.as_mut().unwrap().as_of = day - chrono::Duration::days(1);
        let stale = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert_eq!(stale.yield_pct, None);
        assert!(stale.issues.contains(&WatchIssue::DividendUnavailable));
    }

    #[test]
    fn three_year_history_includes_boundaries_and_never_changes_ttm() {
        let (mut data, day) = market_fixture();
        let original = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        let start = day.checked_sub_months(chrono::Months::new(36)).unwrap();
        let template = data.dividends.as_ref().unwrap()[0].clone();
        for ex_date in [
            start - chrono::Duration::days(1),
            start,
            day,
            day + chrono::Duration::days(1),
        ] {
            data.dividends.as_mut().unwrap().push(DividendEvent {
                ex_date,
                cash_per_ten: if ex_date == day { "0" } else { "999" }.into(),
                ..template.clone()
            });
        }
        let result = evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        assert_eq!(result.dividend_per_share, original.dividend_per_share);
        assert_eq!(result.yield_pct, original.yield_pct);
        let history = result.dividend_history.as_ref().unwrap();
        assert_eq!(history.start, start);
        assert_eq!(history.end, day);
        assert_eq!(history.events.len(), 3);
        assert!(history.events.iter().any(|e| e.ex_date == start));
        assert!(!result.dividend_events.iter().any(|e| e.ex_date == start));
        data.dividends = None;
        assert!(
            evaluate(&Default::default(), "600036.SS", &data, day, String::new())
                .dividend_history
                .is_none()
        );
        // Calendar months preserve a true three-year window across leap years.
        assert_eq!(
            date("2024-02-29")
                .checked_sub_months(chrono::Months::new(36))
                .unwrap(),
            date("2021-02-28")
        );
        // Old saved observations lack the field; reading them never invents history.
        let mut old = serde_json::to_value(&original).unwrap();
        old.as_object_mut().unwrap().remove("dividendHistory");
        assert!(serde_json::from_value::<Observation>(old)
            .unwrap()
            .dividend_history
            .is_none());
    }

    #[test]
    fn foreign_symbols_and_non_stock_codes_are_rejected() {
        for symbol in ["0941.HK", "510300.SS", "ABC", "中中中.SS"] {
            assert!(WatchInput {
                symbol: symbol.into(),
                name: "公司".into(),
                thesis: String::new()
            }
            .normalize()
            .is_err());
        }
        assert_eq!(
            WatchInput {
                symbol: "600036.sh".into(),
                name: "招行".into(),
                thesis: String::new()
            }
            .normalize()
            .unwrap()
            .symbol,
            "600036.SS"
        );
    }
}
