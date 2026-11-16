//! Persistent public collection scope and daily collection outcomes.
use super::PublicPrices;
use crate::{
    models::market::monitor::{PriceCollectionDay, PriceUniverse},
    storage::{params, OptionalRow},
    AppResult,
};

pub(crate) struct UniverseState {
    pub universe: Option<PriceUniverse>,
    pub attempted_at: i64,
    pub last_error: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PriceCoverage {
    pub window_start: chrono::NaiveDate,
    pub completed_through: chrono::NaiveDate,
    pub adjustment_anchor: chrono::NaiveDate,
    pub fetched_at: String,
    pub provider_name: String,
    pub first_on: Option<chrono::NaiveDate>,
    pub bars: usize,
    pub missing_close_values: bool,
}
pub(crate) struct MonitorPrice {
    pub history: Option<PriceCoverage>,
    pub attempted_at: i64,
    pub lease_until: i64,
    pub last_error: String,
}
pub(crate) struct MonitorSnapshot {
    pub scope: UniverseState,
    pub prices: Vec<(String, MonitorPrice)>,
    pub successes: std::collections::BTreeSet<String>,
    pub daily: Vec<PriceCollectionDay>,
}
impl PublicPrices {
    pub fn claim_universe(&self, now: i64, token: &str) -> AppResult<bool> {
        Ok(self.conn()?.execute("UPDATE price_universe SET attempted_at=$1::bigint,lease_until=$1::bigint+60000,lease_token=$2 WHERE id=1 AND attempted_at<=$1::bigint-CASE WHEN last_error<>'' OR payload IS NULL OR lease_until>0 THEN 300000 ELSE 86400000 END AND lease_until<=$1::bigint", params![now,token])? > 0)
    }
    pub fn finish_universe(&self, token: &str, result: &AppResult<PriceUniverse>) -> AppResult<()> {
        match result {
            Ok(scope) => {
                scope.validate()?;
                let c = self.conn()?;
                let accepted = c.execute("UPDATE price_universe SET payload=$2,lease_until=0,lease_token='',last_error='' WHERE id=1 AND lease_token=$1 AND (payload IS NULL OR payload::jsonb->>'asOf'<=$3)",params![token,serde_json::to_string(scope)?,scope.as_of.to_string()])?;
                if accepted == 0 {
                    c.execute("UPDATE price_universe SET lease_until=0,lease_token='',last_error='成分表日期倒退，保留原范围' WHERE id=1 AND lease_token=$1",[token])?;
                }
            }
            Err(error) => {
                self.conn()?.execute("UPDATE price_universe SET lease_until=0,lease_token='',last_error=$2 WHERE id=1 AND lease_token=$1",params![token,error.to_string()])?;
            }
        }
        Ok(())
    }
    pub fn monitor_snapshot(&self, days: i64, date: &str) -> AppResult<MonitorSnapshot> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        tx.execute_batch("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")?;
        let (payload, attempted_at, last_error) = tx.query_row(
            "SELECT payload,attempted_at,last_error FROM price_universe WHERE id=1",
            [],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let scope = UniverseState {
            universe: payload
                .map(|s| serde_json::from_str::<PriceUniverse>(&s))
                .transpose()?,
            attempted_at,
            last_error,
        };
        // Extract a small projection in PostgreSQL; never load 300 complete ten-year histories into the dashboard process.
        let prices = tx.prepare(r#"SELECT p.symbol,
            CASE WHEN p.payload IS NULL THEN NULL ELSE jsonb_build_object(
                'windowStart',h."windowStart",'completedThrough',h."completedThrough",
                'adjustmentAnchor',h."adjustmentAnchor",'fetchedAt',h."fetchedAt",
                'providerName',h."providerName",'firstOn',h.bars->0->>'day',
                'bars',jsonb_array_length(h.bars),'missingCloseValues',EXISTS(
                    SELECT 1 FROM jsonb_array_elements(h.bars) AS b WHERE b->>'rawClose' IS NULL OR b->>'adjustedClose' IS NULL
                ))::text END,p.attempted_at,p.lease_until,p.last_error
            FROM price_histories AS p LEFT JOIN LATERAL jsonb_to_record(p.payload::jsonb) AS h(
                "windowStart" TEXT,"completedThrough" TEXT,"adjustmentAnchor" TEXT,"fetchedAt" TEXT,"providerName" TEXT,bars JSONB
            ) ON TRUE
            WHERE p.symbol='000001.SS' OR p.symbol IN (
                SELECT value->>'symbol' FROM price_universe,jsonb_array_elements(payload::jsonb->'members')
            ) ORDER BY p.symbol"#)?
            .query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?
            .collect::<Result<Vec<_>,_>>()?.into_iter().map(|(s,p,attempted_at,lease_until,last_error)|Ok((s,MonitorPrice { history:p.map(|v|serde_json::from_str(&v)).transpose()?,attempted_at,lease_until,last_error }))).collect::<AppResult<Vec<_>>>()?;
        let successes = tx
            .prepare("SELECT symbol FROM price_fetch_days WHERE day=$1 AND successes>0")?
            .query_map([date], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        let daily = tx.prepare("SELECT payload FROM price_monitor_days WHERE day >= ($1::text::date-($2::bigint-1)::integer)::text ORDER BY day DESC")?
            .query_map(params![date,days], |r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?.into_iter().map(|s|Ok(serde_json::from_str(&s)?)).collect::<AppResult<Vec<_>>>()?;
        tx.commit()?;
        Ok(MonitorSnapshot {
            scope,
            prices,
            successes,
            daily,
        })
    }
    pub fn save_monitor_day(&self, day: &PriceCollectionDay) -> AppResult<()> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        tx.execute("INSERT INTO price_monitor_days(day,payload) VALUES($1,$2) ON CONFLICT(day) DO UPDATE SET payload=excluded.payload WHERE (price_monitor_days.payload::jsonb->>'updatedAt')::bigint<=(excluded.payload::jsonb->>'updatedAt')::bigint",params![day.date,serde_json::to_string(day)?])?;
        tx.execute(
            "DELETE FROM price_monitor_days WHERE day<($1::text::date-89)::text",
            [day.date.as_str()],
        )?;
        tx.execute(
            "DELETE FROM price_fetch_days WHERE day<($1::text::date-89)::text",
            [day.date.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub(super) fn record_attempt(
        tx: &crate::storage::Transaction<'_>,
        symbol: &str,
        now: i64,
    ) -> AppResult<()> {
        tx.execute("INSERT INTO price_fetch_days(day,symbol,attempts) VALUES((to_timestamp($2::double precision/1000) AT TIME ZONE 'Asia/Shanghai')::date::text,$1,1) ON CONFLICT(day,symbol) DO UPDATE SET attempts=price_fetch_days.attempts+1",params![symbol,now])?;
        Ok(())
    }
    pub(super) fn record_success(
        tx: &crate::storage::Transaction<'_>,
        symbol: &str,
        attempted_at: i64,
    ) -> AppResult<()> {
        tx.execute("UPDATE price_fetch_days SET successes=successes+1 WHERE symbol=$1 AND day=(to_timestamp($2::double precision/1000) AT TIME ZONE 'Asia/Shanghai')::date::text",params![symbol,attempted_at])?;
        Ok(())
    }
    pub(super) fn owned_attempt(
        tx: &crate::storage::Transaction<'_>,
        symbol: &str,
        token: &str,
    ) -> AppResult<Option<i64>> {
        Ok(tx.query_row("SELECT attempted_at FROM price_histories WHERE symbol=$1 AND lease_token=$2 FOR UPDATE",params![symbol,token], |r|r.get(0)).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::monitor::{PriceMember, UNIVERSE_URL};
    #[test]
    fn scope_replacement_is_atomic_failed_updates_keep_prior_scope_and_members_never_expire() {
        let store = PublicPrices::test();
        let now = chrono::Utc::now().timestamp_millis();
        let mut scope = PriceUniverse {
            as_of: "2026-11-11".parse().unwrap(),
            fetched_at: now / 1000,
            source_url: UNIVERSE_URL.into(),
            members: (1..=300)
                .map(|i| PriceMember {
                    symbol: format!("{i:06}.SZ"),
                    name: format!("公司{i}"),
                })
                .collect(),
        };
        assert!(store.claim_universe(now, "first").unwrap());
        assert!(!store.claim_universe(now, "duplicate").unwrap());
        store.finish_universe("first", &Ok(scope.clone())).unwrap();
        assert_eq!(store.symbols().unwrap().len(), 301);
        store
            .conn()
            .unwrap()
            .execute("UPDATE price_histories SET requested_at=0", [])
            .unwrap();
        assert_eq!(store.symbols().unwrap().len(), 301);
        assert!(store.claim_universe(now + 86400001, "invalid").unwrap());
        scope.members.pop();
        assert!(store.finish_universe("invalid", &Ok(scope)).is_err());
        store
            .finish_universe(
                "invalid",
                &Err(crate::AppError::MarketData("offline".into())),
            )
            .unwrap();
        let snapshot = store.monitor_snapshot(7, "2026-11-16").unwrap();
        assert_eq!(snapshot.scope.universe.unwrap().members.len(), 300);
        assert!(snapshot.scope.last_error.contains("offline"));
        assert!(store
            .claim_universe(now + 86400001 + 300000, "retry")
            .unwrap());
        let mut replacement = store
            .monitor_snapshot(7, "2026-11-16")
            .unwrap()
            .scope
            .universe
            .unwrap();
        replacement.members.last_mut().unwrap().symbol = "600001.SS".into();
        store.finish_universe("retry", &Ok(replacement)).unwrap();
        let symbols = store.symbols().unwrap();
        assert!(symbols.contains(&"600001.SS".into()));
        assert!(!symbols.contains(&"000300.SZ".into()));
        assert!(
            store.state("000300.SZ").unwrap().is_some(),
            "leaving the scope must not delete stored data"
        );
        let reopened = PublicPrices::open(&store.url, &store.schema).unwrap();
        assert_eq!(reopened.symbols().unwrap().len(), 301);
    }
    #[test]
    fn upgrading_existing_five_year_rows_preserves_data_and_adds_monitoring_idempotently() {
        let store = PublicPrices::test();
        let now = chrono::Utc::now().timestamp_millis();
        let day = crate::models::market::today().parse().unwrap();
        let mut h = crate::models::market::history::tests::fixture("600941.SS", day);
        h.window_start = day.checked_sub_months(chrono::Months::new(60)).unwrap();
        h.bars.retain(|b| b.day >= h.window_start);
        h = h.normalize().unwrap();
        assert!(store.claim("600941.SS", now, "old").unwrap());
        store.finish("600941.SS", "old", &Ok(h.clone())).unwrap();
        // Model the old release's schema: only the existing history table.
        store
            .conn()
            .unwrap()
            .execute_batch("DROP TABLE price_universe,price_fetch_days,price_monitor_days")
            .unwrap();
        let reopened = PublicPrices::open(&store.url, &store.schema).unwrap();
        assert_eq!(
            reopened.state("600941.SS").unwrap().unwrap().history,
            Some(h)
        );
        assert!(reopened
            .monitor_snapshot(7, &day.to_string())
            .unwrap()
            .scope
            .universe
            .is_none());
        let again = PublicPrices::open(&store.url, &store.schema).unwrap();
        assert!(again.state("600941.SS").unwrap().unwrap().history.is_some());
    }
}
