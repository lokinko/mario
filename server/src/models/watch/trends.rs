//! Chart facts only. No observation thresholds, account state or trading actions.
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub day: NaiveDate,
    pub close: Option<f64>,
    pub weekly_lower: Option<f64>,
    pub weekly_middle: Option<f64>,
    pub weekly_upper: Option<f64>,
    pub monthly_middle: Option<f64>,
    pub ma20: Option<f64>,
    pub ma60: Option<f64>,
    pub golden_cross: bool,
    pub provisional: bool,
}

#[derive(Default)]
struct Period {
    last: Option<f64>,
    missing: bool,
}
impl Period {
    fn push(&mut self, value: Option<f64>) {
        self.last = value.filter(|v| v.is_finite() && *v > 0.0);
        self.missing |= self.last.is_none();
    }
    fn close(&self) -> Option<f64> {
        self.last.filter(|_| !self.missing)
    }
}

fn average(values: &[Option<f64>], length: usize) -> Option<f64> {
    let window = values.get(values.len().checked_sub(length)?..)?;
    let mean = window.iter().copied().sum::<Option<f64>>()? / length as f64;
    mean.is_finite().then_some(mean)
}

/// Input prices already share the observation's adjusted, current-price unit.
/// Monthly values are frozen at each weekly endpoint, including that partial
/// month; a later monthly close must never be copied into earlier weeks.
pub fn calculate(sessions: &[(NaiveDate, Option<f64>)]) -> Vec<TrendPoint> {
    let mut sessions: BTreeMap<_, _> = sessions.iter().copied().collect();
    if let Some((&end, _)) = sessions.last_key_value() {
        let start = crate::models::market::history::history_start(end);
        sessions.retain(|day, _| *day >= start);
    }
    let mut weeks = BTreeMap::<_, Period>::new();
    let mut months = BTreeMap::<_, Period>::new();
    let mut points = Vec::new();
    let mut previous = None;
    let mut dates = sessions.iter().peekable();
    while let Some((&day, &value)) = dates.next() {
        let week = day.iso_week();
        let week_key = (week.year(), week.week());
        weeks.entry(week_key).or_default().push(value);
        months
            .entry((day.year(), day.month()))
            .or_default()
            .push(value);
        if dates
            .peek()
            .is_some_and(|(next, _)| next.iso_week() == week)
        {
            continue;
        }
        // The source starts on an arbitrary day. Discard its first week/month
        // from indicator windows rather than treating truncated periods as full.
        let weekly: Vec<_> = weeks
            .values()
            .enumerate()
            .map(|(i, p)| (i > 0).then(|| p.close()).flatten())
            .collect();
        let monthly: Vec<_> = months
            .values()
            .enumerate()
            .map(|(i, p)| (i > 0).then(|| p.close()).flatten())
            .collect();
        // An earlier missing session invalidates indicator windows, but does
        // not erase the known closing price at this endpoint.
        let close = weeks[&week_key].last;
        let bands = super::band(&weekly, close.unwrap_or(1.0));
        let ma20 = average(&weekly, 20);
        let ma60 = average(&weekly, 60);
        let pair = ma20.zip(ma60);
        let golden_cross = previous
            .zip(pair)
            .is_some_and(|((a, b), (c, d))| a <= b && c > d);
        previous = pair;
        points.push(TrendPoint {
            day,
            close,
            weekly_lower: bands.as_ref().map(|b| b.lower),
            weekly_middle: bands.as_ref().map(|b| b.middle),
            weekly_upper: bands.as_ref().map(|b| b.upper),
            monthly_middle: average(&monthly, 20),
            ma20,
            ma60,
            golden_cross,
            // Without future exchange sessions, a current week ending before
            // Friday stays conservative, including holiday-shortened weeks.
            provisional: dates.peek().is_none() && day.weekday().num_days_from_monday() < 4,
        });
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sessions() -> Vec<(NaiveDate, Option<f64>)> {
        let start = NaiveDate::from_ymd_opt(2022, 1, 3).unwrap();
        (0..1460)
            .map(|i| start + Duration::days(i))
            .filter(|d| d.weekday().num_days_from_monday() < 5)
            .enumerate()
            .map(|(i, day)| (day, Some(50.0 + i as f64 / 100.0)))
            .collect()
    }

    #[test]
    fn bounded_chart_uses_existing_boll_and_independent_monthly_windows() {
        let input = sessions();
        let points = calculate(&input);
        assert!(points.len() > 104 && points.len() <= 263);
        assert!(points.windows(2).all(|p| p[0].day < p[1].day));
        let last = points.last().unwrap();
        let mut weeks = BTreeMap::new();
        let mut months = BTreeMap::new();
        for (day, value) in input {
            weeks.insert(day.iso_week(), value);
            months.insert((day.year(), day.month()), value);
        }
        let weekly: Vec<_> = weeks.values().copied().collect();
        let monthly: Vec<_> = months.values().copied().collect();
        let band = super::super::band(&weekly, last.close.unwrap()).unwrap();
        assert_eq!(last.weekly_lower, Some(band.lower));
        assert_eq!(last.weekly_middle, Some(band.middle));
        assert_eq!(last.weekly_upper, Some(band.upper));
        assert_eq!(last.ma20, last.weekly_middle);
        assert_eq!(last.ma60, average(&weekly, 60));
        assert_eq!(last.monthly_middle, average(&monthly, 20));
    }

    #[test]
    fn every_historical_point_matches_what_was_known_at_its_date() {
        let input = sessions();
        for point in calculate(&input).iter().step_by(7) {
            let prefix: Vec<_> = input
                .iter()
                .copied()
                .filter(|(d, _)| *d <= point.day)
                .collect();
            let then = calculate(&prefix).pop().unwrap();
            assert_eq!(point.monthly_middle, then.monthly_middle);
            assert_eq!(point.weekly_middle, then.weekly_middle);
            assert_eq!(point.ma60, then.ma60);
            assert_eq!(point.golden_cross, then.golden_cross);
        }
    }

    #[test]
    fn gaps_invalidate_only_affected_windows_and_never_bridge_a_cross() {
        let mut input = sessions();
        let missing = input.len() - 120;
        input[missing].1 = None;
        let last = calculate(&input).pop().unwrap();
        assert!(last.close.is_some());
        assert!(last.weekly_middle.is_some());
        assert!(last.ma60.is_none());
        assert!(last.monthly_middle.is_none());
        assert!(!last.golden_cross);
        let short = calculate(&input[..20]);
        assert!(short
            .iter()
            .all(|p| p.ma20.is_none() && p.ma60.is_none() && p.monthly_middle.is_none()));
        let recent_gap = input.len() - 3;
        input[recent_gap].1 = None;
        let last = calculate(&input).pop().unwrap();
        assert!(last.close.is_some());
        assert!(last.weekly_middle.is_none());
    }

    #[test]
    fn cross_requires_an_actual_upward_transition_and_marks_partial_week() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let mut input: Vec<_> = (0..70)
            .map(|i| (start + Duration::weeks(i), Some(100.0)))
            .collect();
        let day = input.last().unwrap().0 + Duration::days(3);
        input.push((day, Some(120.0)));
        let points = calculate(&input);
        assert!(points.last().unwrap().golden_cross);
        assert!(points.last().unwrap().provisional);
        assert!(points[..70]
            .iter()
            .all(|p| !p.golden_cross && !p.provisional));
        input.push((day + Duration::days(4), Some(120.0)));
        assert!(!calculate(&input).last().unwrap().provisional);
        input.push((day + Duration::days(11), Some(120.0)));
        assert!(!calculate(&input).last().unwrap().golden_cross);
    }

    #[test]
    fn old_observations_without_trends_keep_their_original_facts() {
        let (data, day) = super::super::tests::market_fixture();
        let observation =
            super::super::evaluate(&Default::default(), "600036.SS", &data, day, String::new());
        let mut payload = serde_json::to_value(&observation).unwrap();
        payload.as_object_mut().unwrap().remove("trends");
        payload.as_object_mut().unwrap().remove("trendWindowYears");
        let old: super::super::Observation = serde_json::from_value(payload).unwrap();
        assert!(old.trends.is_none());
        assert!(old.trend_window_years.is_none());
        assert_eq!(old.daily, observation.daily);
        assert_eq!(old.weekly, observation.weekly);
        assert_eq!(old.status, observation.status);
        assert_eq!(old.price, observation.price);
    }
}
