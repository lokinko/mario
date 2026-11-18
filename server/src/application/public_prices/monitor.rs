//! Collection monitoring uses public state only. Reading the dashboard never fetches sources.
use super::PublicPriceService;
use crate::{
    models::market::{
        close,
        history::{history_start, HISTORY_YEARS},
        is_mainland_fund,
        monitor::{PriceCollectionDashboard, PriceCollectionDay, PriceCollectionRow},
    },
    ports::market::PriceUniverseProvider,
    AppError, AppResult,
};
use chrono::{DateTime, Utc};

/// The Shanghai composite supplies the mainland trading calendar for lag checks.
const REFERENCE: &str = "000001.SS";

fn market(symbol: &str) -> &'static str {
    if symbol.ends_with(".SS") || symbol.ends_with(".SZ") {
        "cn"
    } else if symbol.ends_with(".HK") {
        "hk"
    } else if !symbol.is_empty() && symbol.bytes().all(|b| b.is_ascii_uppercase() || b == b'-') {
        "us"
    } else {
        "other"
    }
}

impl PublicPriceService {
    pub async fn refresh_universe(&self, source: &dyn PriceUniverseProvider) -> AppResult<()> {
        let token = uuid::Uuid::new_v4().to_string();
        if !self
            .store
            .claim_universe(Utc::now().timestamp_millis(), &token)?
        {
            return Ok(());
        }
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(25), source.constituents())
                .await
                .map_err(|_| AppError::MarketData("沪深300成分表获取超过25秒".into()))
                .and_then(|r| r)
                .and_then(|scope| {
                    scope.validate()?;
                    Ok(scope)
                });
        self.store.finish_universe(&token, &result)?;
        result.map(|_| ())
    }
    pub fn save_collection_day(&self) -> AppResult<()> {
        let data = self.collection_dashboard(7)?;
        self.store.save_monitor_day(&data.summary)
    }
    pub fn collection_dashboard(&self, days: i64) -> AppResult<PriceCollectionDashboard> {
        self.collection_dashboard_at(days, Utc::now())
    }
    fn collection_dashboard_at(
        &self,
        days: i64,
        now: DateTime<Utc>,
    ) -> AppResult<PriceCollectionDashboard> {
        let date = now.with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
        let target = close::completed_day(REFERENCE, date, now);
        let snapshot = self.store.monitor_snapshot(days, &date.to_string())?;
        let mut prices = snapshot
            .prices
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        // An actual source date prevents weekends and exchange holidays becoming false alarms.
        let trading_day = prices
            .get(REFERENCE)
            .and_then(|s| s.history.as_ref())
            .filter(|h| h.completed_through == target)
            .map(|h| h.adjustment_anchor);
        let mut summary = PriceCollectionDay {
            date: date.to_string(),
            updated_at: now.timestamp(),
            ..Default::default()
        };
        let mut entries = Vec::new();
        for member in snapshot.scope.universe.iter().flat_map(|u| &u.members) {
            let state = prices.remove(&member.symbol);
            entries.push((
                member.symbol.clone(),
                member.name.clone(),
                "csi300",
                true,
                state,
            ));
        }
        for (symbol, state) in prices {
            let scope = if symbol == REFERENCE {
                "reference"
            } else {
                "other"
            };
            let tracked = state.tracked;
            entries.push((symbol, String::new(), scope, tracked, Some(state)));
        }
        let mut rows = Vec::new();
        for (symbol, name, scope, tracked, state) in entries {
            let market = market(&symbol);
            let target = close::completed_day(&symbol, date, now);
            // Only mainland symbols share the Shanghai index as their trading calendar.
            let trading_day = if market == "cn" { trading_day } else { None };
            let history = state.as_ref().and_then(|s| s.history.as_ref());
            let attempted = state.as_ref().map_or(0, |s| s.attempted_at);
            let lease = state.as_ref().map_or(0, |s| s.lease_until);
            let mut error = state
                .as_ref()
                .map_or(String::new(), |s| s.last_error.clone());
            let mut codes = Vec::new();
            let fetching = lease > now.timestamp_millis();
            if fetching {
                codes.push("fetch_in_progress".into());
            }
            let status = if !tracked {
                codes.push("tracking_inactive".into());
                "inactive"
            } else if lease > 0 && !fetching {
                codes.push("fetch_interrupted".into());
                error = "上次采集中断，租约已到期，等待自动重试".into();
                "failed"
            } else if !error.is_empty() {
                codes.push("source_failed".into());
                "failed"
            } else if history.is_none() {
                codes.push("history_pending".into());
                "pending"
            } else if history.is_some_and(|h| h.window_start > history_start(h.completed_through)) {
                codes.push("backfill_pending".into());
                "pending"
            } else if history.is_some_and(|h| h.completed_through < target) {
                codes.push("close_check_pending".into());
                "pending"
            } else if history
                .is_some_and(|h| trading_day.is_some_and(|day| h.adjustment_anchor < day))
            {
                codes.push("latest_close_lagging".into());
                "lagging"
            } else if !snapshot.successes.contains(&symbol) {
                codes.push("today_pending".into());
                "pending"
            } else {
                "success"
            };
            if let Some(h) = history {
                if h.missing_close_values {
                    codes.push("missing_close_values".into());
                }
                if h.first_on.is_some_and(|day| {
                    day > history_start(h.completed_through) + chrono::Duration::days(14)
                }) {
                    codes.push("short_history".into());
                }
                if tracked && market == "cn" && trading_day.is_none() {
                    codes.push("trading_calendar_pending".into());
                }
            }
            if tracked {
                summary.expected += 1;
                match status {
                    "success" => summary.successful += 1,
                    "failed" => summary.failed += 1,
                    "lagging" => summary.lagging += 1,
                    _ => summary.pending += 1,
                }
            }
            let instrument_type = history
                .map(|h| h.instrument_type.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| {
                    if is_mainland_fund(&symbol) {
                        "FUND"
                    } else {
                        ""
                    }
                    .into()
                });
            rows.push(PriceCollectionRow {
                market: market.into(),
                scope: scope.into(),
                instrument_type,
                currency: history.map_or(String::new(), |h| h.currency.clone()),
                tracked,
                symbol,
                name,
                status: status.into(),
                issue_codes: codes,
                error,
                attempted_at: (attempted > 0).then_some(attempted / 1000),
                succeeded_at: history
                    .and_then(|h| DateTime::parse_from_rfc3339(&h.fetched_at).ok())
                    .map(|t| t.timestamp()),
                first_on: history.and_then(|h| h.first_on),
                latest_on: history.map(|h| h.adjustment_anchor),
                bars: history.map_or(0, |h| h.bars),
                provider: history.map_or(String::new(), |h| h.provider_name.clone()),
            });
        }
        let mut daily = snapshot.daily;
        daily.retain(|d| d.date != summary.date);
        daily.insert(0, summary.clone());
        daily.truncate(days as usize);
        Ok(PriceCollectionDashboard {
            generated_at: now.timestamp(),
            years: HISTORY_YEARS,
            target_through: target,
            universe: snapshot.scope.universe,
            universe_attempted_at: (snapshot.scope.attempted_at > 0)
                .then_some(snapshot.scope.attempted_at / 1000),
            universe_error: snapshot.scope.last_error,
            summary,
            rows,
            daily,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::PublicPrices,
        models::market::{
            history::tests::fixture,
            monitor::{PriceMember, PriceUniverse, UNIVERSE_URL},
        },
    };
    use std::sync::Arc;
    fn scope() -> PriceUniverse {
        PriceUniverse {
            as_of: "2026-11-11".parse().unwrap(),
            fetched_at: Utc::now().timestamp(),
            source_url: UNIVERSE_URL.into(),
            members: (1..=300)
                .map(|i| PriceMember {
                    symbol: format!("{i:06}.SZ"),
                    name: format!("测试{i}"),
                })
                .collect(),
        }
    }
    fn seed(store: &PublicPrices, now: i64) {
        assert!(store.claim_universe(now, "scope").unwrap());
        store.finish_universe("scope", &Ok(scope())).unwrap();
        assert_eq!(store.symbols().unwrap().len(), 301);
    }
    #[test]
    fn dashboard_distinguishes_success_failure_backfill_lag_and_interruption_without_network() {
        let store = Arc::new(PublicPrices::test());
        let now: DateTime<Utc> = "2026-11-16T08:00:00Z".parse().unwrap();
        seed(&store, now.timestamp_millis());
        let day = "2026-11-16".parse().unwrap();
        for symbol in [
            "000001.SS",
            "000001.SZ",
            "000002.SZ",
            "000003.SZ",
            "000004.SZ",
            "000006.SZ",
        ] {
            assert!(store
                .claim(symbol, now.timestamp_millis() - 121000, symbol)
                .unwrap());
            let mut h = fixture(symbol, day);
            if symbol == "000002.SZ" {
                h.bars.pop();
                h = h.normalize().unwrap();
            }
            if symbol == "000003.SZ" {
                h.window_start = day.checked_sub_months(chrono::Months::new(60)).unwrap();
                h.bars.retain(|b| b.day >= h.window_start);
                h = h.normalize().unwrap();
            }
            store
                .finish_at(symbol, symbol, &Ok(h), now.timestamp_millis())
                .unwrap();
        }
        assert!(store
            .claim("000004.SZ", now.timestamp_millis(), "failed")
            .unwrap());
        store
            .finish(
                "000004.SZ",
                "failed",
                &Err(AppError::MarketData("HTTP 429".into())),
            )
            .unwrap();
        assert!(store
            .claim("000005.SZ", now.timestamp_millis() - 121000, "interrupted")
            .unwrap());
        let service = PublicPriceService::new(
            store.clone(),
            Arc::new(crate::market_data::PublicMarketSource::new().unwrap()),
        );
        let result = service.collection_dashboard_at(7, now).unwrap();
        // 300 members plus the Shanghai composite that supplies the trading calendar.
        assert_eq!(result.summary.expected, 301);
        assert_eq!(result.summary.successful, 3);
        assert_eq!(result.summary.failed, 2);
        assert_eq!(result.summary.lagging, 1);
        assert_eq!(result.summary.pending, 295);
        assert_eq!(
            result.rows.iter().filter(|r| r.scope == "csi300").count(),
            300
        );
        let row = |symbol| result.rows.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(row("000002.SZ")
            .issue_codes
            .contains(&"latest_close_lagging".into()));
        assert!(row("000003.SZ")
            .issue_codes
            .contains(&"backfill_pending".into()));
        assert!(row("000004.SZ").error.contains("429"));
        assert!(
            row("000004.SZ").bars > 0,
            "failure retains the successful history"
        );
        assert!(row("000005.SZ")
            .issue_codes
            .contains(&"fetch_interrupted".into()));
        assert!(row("000006.SZ").succeeded_at.is_some());
        assert!(store
            .claim("000006.SZ", now.timestamp_millis(), "rechecking")
            .unwrap());
        let during = service.collection_dashboard_at(7, now).unwrap();
        let during_row = during
            .rows
            .iter()
            .find(|r| r.symbol == "000006.SZ")
            .unwrap();
        assert_eq!(
            during_row.status, "success",
            "rechecking must not erase today's result"
        );
        assert!(during_row.issue_codes.contains(&"fetch_in_progress".into()));
        let restarted = PublicPriceService::new(
            store.clone(),
            Arc::new(crate::market_data::PublicMarketSource::new().unwrap()),
        );
        let after_restart = restarted.collection_dashboard_at(7, now).unwrap();
        assert_eq!(
            serde_json::to_value(&during).unwrap(),
            serde_json::to_value(&after_restart).unwrap()
        );
        store.save_monitor_day(&result.summary).unwrap();
        let next = service
            .collection_dashboard_at(7, now + chrono::Duration::days(1))
            .unwrap();
        assert_eq!(next.daily[1].successful, 3);
        assert_eq!(
            next.summary.successful, 0,
            "yesterday's prices never claim today's fetch succeeded"
        );
        assert_eq!(
            store.monitor_snapshot(7, "2026-11-17").unwrap().daily.len(),
            1
        );
    }
    #[test]
    fn holidays_use_actual_index_date_and_no_prior_day_fetch_is_reported_today() {
        let store = Arc::new(PublicPrices::test());
        let now: DateTime<Utc> = "2026-11-16T08:00:00Z".parse().unwrap();
        seed(&store, now.timestamp_millis());
        for symbol in ["000001.SS", "000001.SZ"] {
            assert!(store.claim(symbol, now.timestamp_millis(), symbol).unwrap());
            let mut h = fixture(symbol, "2026-11-16".parse().unwrap());
            h.bars.pop();
            h = h.normalize().unwrap();
            store
                .finish_at(symbol, symbol, &Ok(h), now.timestamp_millis())
                .unwrap();
        }
        let service = PublicPriceService::new(
            store.clone(),
            Arc::new(crate::market_data::PublicMarketSource::new().unwrap()),
        );
        let result = service.collection_dashboard_at(7, now).unwrap();
        assert_eq!(result.summary.lagging, 0);
        assert_eq!(result.summary.successful, 2);
    }
    #[test]
    fn registered_overseas_and_fund_symbols_are_shown_with_their_own_market_close() {
        let store = Arc::new(PublicPrices::test());
        // Monday 16:00 Shanghai: New York has not completed Monday, so Friday is current.
        let now: DateTime<Utc> = "2026-11-16T08:00:00Z".parse().unwrap();
        seed(&store, now.timestamp_millis());
        let ms = now.timestamp_millis() - 121000;
        let finish = |symbol: &str, through: &str| {
            assert!(store.claim(symbol, ms, symbol).unwrap());
            let h = fixture(symbol, through.parse().unwrap());
            store.finish_at(symbol, symbol, &Ok(h), ms + 1000).unwrap();
        };
        finish("000001.SS", "2026-11-16");
        for symbol in ["QQQ", "0700.HK", "510300.SS"] {
            store.register(symbol).unwrap();
        }
        finish("QQQ", "2026-11-13");
        finish("0700.HK", "2026-11-13");
        finish("510300.SS", "2026-11-16");
        // Claimed without a registration: kept on record but outside the collection scope.
        finish("VOO", "2026-11-06");
        let service = PublicPriceService::new(
            store.clone(),
            Arc::new(crate::market_data::PublicMarketSource::new().unwrap()),
        );
        let result = service.collection_dashboard_at(7, now).unwrap();
        let row = |symbol| result.rows.iter().find(|r| r.symbol == symbol).unwrap();
        let qqq = row("QQQ");
        assert_eq!((qqq.scope.as_str(), qqq.market.as_str()), ("other", "us"));
        assert_eq!(qqq.status, "success", "{:?}", qqq.issue_codes);
        assert_eq!(qqq.currency, "USD");
        assert!(qqq.tracked);
        assert!(!qqq.issue_codes.contains(&"trading_calendar_pending".into()));
        // Hong Kong closes at 16:15, so Friday is still its completed day.
        let hk = row("0700.HK");
        assert_eq!(hk.market, "hk");
        assert_eq!(hk.status, "success", "{:?}", hk.issue_codes);
        let fund = row("510300.SS");
        assert_eq!(
            (fund.market.as_str(), fund.status.as_str()),
            ("cn", "success")
        );
        assert_eq!(row("000001.SS").scope, "reference");
        let voo = row("VOO");
        assert!(!voo.tracked);
        assert_eq!(voo.status, "inactive");
        assert!(voo.bars > 0, "inactive rows keep their stored prices");
        // 300 members + reference + three registered symbols; VOO is not counted.
        assert_eq!(result.summary.expected, 304);
        assert_eq!(result.summary.successful, 4);
    }
}
