//! Buy-watch contracts and deterministic calculations. No account or network access.
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
                    || e.report_date.is_none_or(|d| d > e.ex_date)
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
    pub bars: Vec<PriceBar>,
    pub calendar: Vec<NaiveDate>,
    pub dividends: Option<Vec<DividendEvent>>,
    pub dividend_basis: Option<DividendBasis>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub lower: f64,
    pub middle: f64,
    pub upper: f64,
    pub distance_pct: Option<f64>,
    pub near_lower: bool,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchRow {
    pub entry: WatchEntry,
    pub latest: Option<Observation>,
    pub alert: bool,
    pub checked_at: Option<String>,
    pub checked_for: Option<String>,
    pub refresh_failed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchBoard {
    pub rows: Vec<WatchRow>,
    pub week_start: String,
    pub week_end: String,
    pub weekly: Vec<Observation>,
    pub min_yield_pct: f64,
    pub band_distance_pct: f64,
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
    let distance_pct = (lower > 0.0).then(|| (price / lower - 1.0) * 100.0);
    Some(Band {
        lower,
        middle,
        upper: middle + 2.0 * deviation,
        distance_pct,
        near_lower: distance_pct.is_some_and(|d| d.abs() <= BAND_DISTANCE_PCT + 1e-9),
    })
}

fn trailing_dividends(events: &[DividendEvent], day: NaiveDate) -> Option<Decimal> {
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
    for date in data.calendar.iter().filter(|d| **d <= day) {
        let value = bars
            .get(date)
            .and_then(|b| b.adjusted_close)
            .zip(scale)
            .filter(|(v, s)| v.is_finite() && *v > 0.0 && s.is_finite())
            .map(|(v, s)| v * s);
        daily.push(value);
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
    if daily
        .iter()
        .chain(weekly.iter())
        .any(|b| b.distance_pct.is_none())
    {
        issues.push(WatchIssue::NonPositiveBand);
    }
    let status = if !issues.is_empty() {
        WatchStatus::Incomplete
    } else if yield_decimal.is_some_and(|y| y >= Decimal::new(5, 0))
        && daily.as_ref().is_some_and(|b| b.near_lower)
        && weekly.as_ref().is_some_and(|b| b.near_lower)
    {
        WatchStatus::Attention
    } else {
        WatchStatus::Waiting
    };
    Observation {
        symbol: symbol.into(),
        observed_on: day.to_string(),
        checked_at,
        price,
        dividend_per_share: dividends.map(|v| v.normalize().to_string()),
        yield_pct,
        daily,
        weekly,
        status,
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
    }
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
    fn five_percent_is_inclusive_and_both_bands_are_required() {
        let (mut data, day) = market_fixture();
        let observation = evaluate("600036.SS", &data, day, "2026-07-03T12:00:00Z".into());
        assert_eq!(observation.yield_pct, Some(5.0));
        assert_eq!(observation.status, WatchStatus::Attention);
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "49.99".into();
        assert_eq!(
            evaluate("600036.SS", &data, day, String::new()).status,
            WatchStatus::Waiting
        );
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "50".into();
        // Older daily prices affect the weekly band without changing the latest 20 daily closes.
        for bar in data.bars.iter_mut().take(80) {
            bar.adjusted_close = Some(200.0);
        }
        let result = evaluate("600036.SS", &data, day, String::new());
        assert!(result.daily.unwrap().near_lower);
        assert!(!result.weekly.unwrap().near_lower);
        assert_ne!(result.status, WatchStatus::Attention);
    }
    #[test]
    fn future_rows_are_ignored_and_missing_history_is_unknown() {
        let (mut data, day) = market_fixture();
        let before = evaluate("600036.SS", &data, day, String::new());
        data.bars.push(PriceBar {
            day: day + chrono::Duration::days(3),
            close: Some(1.0),
            adjusted_close: Some(1.0),
        });
        data.calendar.push(day + chrono::Duration::days(3));
        assert_eq!(before, evaluate("600036.SS", &data, day, String::new()));
        data.bars.remove(data.bars.len() - 3);
        let missing = evaluate("600036.SS", &data, day, String::new());
        assert_eq!(missing.status, WatchStatus::Incomplete);
        assert_eq!(missing.yield_pct, Some(5.0));
        assert!(missing.daily.is_none());
        assert!(missing.weekly.is_none());
    }
    #[test]
    fn proximity_is_symmetric_two_percent_of_lower_price() {
        let values = vec![Some(100.0); 20];
        assert!(band(&values, 98.0).unwrap().near_lower);
        assert!(band(&values, 102.0).unwrap().near_lower);
        assert!(!band(&values, 97.9).unwrap().near_lower);
        assert!(!band(&values, 102.1).unwrap().near_lower);
        let mut missing = values;
        missing[10] = None;
        assert!(band(&missing, 100.0).is_none());
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
        let first = evaluate("600036.SS", &data, day, String::new());
        assert_eq!(first.yield_pct, Some(5.0));
        data.bars.last_mut().unwrap().close = Some(50.0);
        assert_eq!(
            evaluate("600036.SS", &data, day, String::new()).yield_pct,
            Some(10.0)
        );
        data.dividends.as_mut().unwrap()[0].ex_date = day - chrono::Duration::days(365);
        let expired = evaluate("600036.SS", &data, day, String::new());
        assert_eq!(expired.dividend_per_share.as_deref(), Some("0"));
        assert_eq!(expired.yield_pct, Some(0.0));
        assert!(expired.dividend_events.is_empty());
        data.dividend_basis.as_mut().unwrap().as_of = day - chrono::Duration::days(1);
        let stale = evaluate("600036.SS", &data, day, String::new());
        assert_eq!(stale.yield_pct, None);
        assert!(stale.issues.contains(&WatchIssue::DividendUnavailable));
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
