//! Completed daily bars only; exchange holidays are determined by actual source dates.
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;

/// Canonical Yahoo symbols used by the portfolio. The delay lets the daily bar settle.
fn session(symbol: &str) -> Option<(Tz, NaiveTime)> {
    let (zone, hour, minute) = if symbol.ends_with(".SS") || symbol.ends_with(".SZ") {
        (chrono_tz::Asia::Shanghai, 15, 5)
    } else if symbol.ends_with(".HK") {
        (chrono_tz::Asia::Hong_Kong, 16, 15)
    } else if !symbol.is_empty() && symbol.bytes().all(|b| b.is_ascii_uppercase() || b == b'-') {
        (chrono_tz::America::New_York, 16, 15)
    } else {
        return None;
    };
    Some((zone, NaiveTime::from_hms_opt(hour, minute, 0).unwrap()))
}

pub(crate) fn completed_day(
    canonical_symbol: &str,
    requested: NaiveDate,
    now: DateTime<Utc>,
) -> NaiveDate {
    let Some((zone, close)) = session(canonical_symbol) else {
        // Unknown exchanges keep the conservative policy instead of guessing their session.
        return requested.min(now.date_naive() - chrono::Duration::days(1));
    };
    let local = now.with_timezone(&zone);
    let latest = if local.time() >= close {
        local.date_naive()
    } else {
        local.date_naive() - chrono::Duration::days(1)
    };
    let mut day = requested.min(latest);
    while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
        day -= chrono::Duration::days(1);
    }
    day
}

/// A known exchange must not interpret its timestamps using another market's time zone.
pub(crate) fn matches_timezone(canonical_symbol: &str, zone: Tz) -> bool {
    session(canonical_symbol).is_none_or(|(expected, _)| expected == zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn end(symbol: &str, requested: &str, now: &str) -> String {
        completed_day(symbol, requested.parse().unwrap(), now.parse().unwrap()).to_string()
    }
    #[test]
    fn china_allows_today_only_after_the_close_and_publication_delay() {
        for symbol in ["600519.SS", "000001.SZ", "513380.SS", "159330.SZ"] {
            assert_eq!(
                end(symbol, "2026-11-06", "2026-11-06T07:04:59Z"),
                "2026-11-05"
            );
            assert_eq!(
                end(symbol, "2026-11-06", "2026-11-06T07:05:00Z"),
                "2026-11-06"
            );
            assert_eq!(
                end(symbol, "2026-11-06", "2026-11-07T00:00:00Z"),
                "2026-11-06"
            );
        }
    }
    #[test]
    fn hong_kong_waits_for_the_closing_auction_and_us_uses_dst() {
        assert_eq!(
            end("0700.HK", "2026-11-06", "2026-11-06T08:14:59Z"),
            "2026-11-05"
        );
        assert_eq!(
            end("0700.HK", "2026-11-06", "2026-11-06T08:15:00Z"),
            "2026-11-06"
        );
        for (date, before, after) in [
            ("2026-07-06", "2026-07-06T20:14:59Z", "2026-07-06T20:15:00Z"),
            ("2026-11-06", "2026-11-06T21:14:59Z", "2026-11-06T21:15:00Z"),
        ] {
            assert!(end("AAPL", date, before).as_str() < date);
            assert_eq!(end("AAPL", date, after), date);
        }
    }
    #[test]
    fn weekends_historical_dates_and_unknown_markets_never_claim_future_closes() {
        assert_eq!(
            end("600519.SS", "2026-11-08", "2026-11-08T08:00:00Z"),
            "2026-11-06"
        );
        assert_eq!(
            end("600519.SS", "2026-11-09", "2026-11-09T07:04:59Z"),
            "2026-11-06"
        );
        assert_eq!(
            end("600519.SS", "2026-11-04", "2026-11-06T08:00:00Z"),
            "2026-11-04"
        );
        assert_eq!(
            end("VOD.L", "2026-11-06", "2026-11-06T22:00:00Z"),
            "2026-11-05"
        );
        assert!(matches_timezone("600519.SS", chrono_tz::Asia::Shanghai));
        assert!(!matches_timezone("600519.SS", chrono_tz::America::New_York));
    }
}
