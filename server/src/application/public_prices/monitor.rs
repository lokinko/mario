//! Collection monitoring uses public state only. Reading the dashboard never fetches sources.
use super::PublicPriceService;
use crate::{
    models::market::{
        close,
        history::{history_start, HISTORY_YEARS},
        monitor::{PriceCollectionDashboard, PriceCollectionDay, PriceCollectionRow},
    },
    ports::market::PriceUniverseProvider,
    AppError, AppResult,
};
use chrono::{DateTime, Utc};

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
        let target = close::completed_day("000001.SS", date, now);
        let snapshot = self.store.monitor_snapshot(days, &date.to_string())?;
        let mut prices = snapshot
            .prices
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        // An actual source date prevents weekends and exchange holidays becoming false alarms.
        let trading_day = prices
            .remove("000001.SS")
            .and_then(|s| s.history)
            .filter(|h| h.completed_through == target)
            .map(|h| h.adjustment_anchor);
        let mut summary = PriceCollectionDay {
            date: date.to_string(),
            updated_at: now.timestamp(),
            ..Default::default()
        };
        let mut rows = Vec::new();
        if let Some(scope) = &snapshot.scope.universe {
            for member in &scope.members {
                let state = prices.get(&member.symbol);
                let history = state.and_then(|s| s.history.as_ref());
                let attempted = state.map_or(0, |s| s.attempted_at);
                let lease = state.map_or(0, |s| s.lease_until);
                let mut error = state.map_or(String::new(), |s| s.last_error.clone());
                let mut codes = Vec::new();
                let fetching = lease > now.timestamp_millis();
                if fetching {
                    codes.push("fetch_in_progress".into());
                }
                let status = if lease > 0 && !fetching {
                    codes.push("fetch_interrupted".into());
                    error = "上次采集中断，租约已到期，等待自动重试".into();
                    "failed"
                } else if !error.is_empty() {
                    codes.push("source_failed".into());
                    "failed"
                } else if history.is_none() {
                    codes.push("history_pending".into());
                    "pending"
                } else if history
                    .is_some_and(|h| h.window_start > history_start(h.completed_through))
                {
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
                } else if !snapshot.successes.contains(&member.symbol) {
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
                    if trading_day.is_none() {
                        codes.push("trading_calendar_pending".into());
                    }
                }
                summary.expected += 1;
                match status {
                    "success" => summary.successful += 1,
                    "failed" => summary.failed += 1,
                    "lagging" => summary.lagging += 1,
                    _ => summary.pending += 1,
                }
                rows.push(PriceCollectionRow {
                    symbol: member.symbol.clone(),
                    name: member.name.clone(),
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
            store.finish(symbol, symbol, &Ok(h)).unwrap();
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
        assert_eq!(result.summary.expected, 300);
        assert_eq!(result.summary.successful, 2);
        assert_eq!(result.summary.failed, 2);
        assert_eq!(result.summary.lagging, 1);
        assert_eq!(result.summary.pending, 295);
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
        assert_eq!(next.daily[1].successful, 2);
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
            store.finish(symbol, symbol, &Ok(h)).unwrap();
        }
        let service = PublicPriceService::new(
            store.clone(),
            Arc::new(crate::market_data::PublicMarketSource::new().unwrap()),
        );
        let result = service.collection_dashboard_at(7, now).unwrap();
        assert_eq!(result.summary.lagging, 0);
        assert_eq!(result.summary.successful, 1);
    }
}
