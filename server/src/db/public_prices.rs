//! Shared public price revisions and refresh leases. Never reads tenant data.
use crate::{
    models::market::{canonical_symbol, history::PriceHistory},
    storage::{params, Connection, OptionalRow},
    AppError, AppResult,
};
use std::sync::Mutex;

pub(crate) struct PriceState {
    pub history: Option<PriceHistory>,
    pub attempted_at: i64,
    pub lease_until: i64,
    pub last_error: String,
}
pub(crate) struct PublicPrices {
    connection: Mutex<Connection>,
    url: String,
    schema: String,
}
impl PublicPrices {
    pub fn postgres(url: &str) -> AppResult<Self> {
        Self::open(url, "mario_market")
    }
    fn open(url: &str, schema: &str) -> AppResult<Self> {
        let mut c = Connection::postgres(url, schema)?;
        let tx = c.transaction()?;
        tx.execute_batch("SELECT pg_advisory_xact_lock(hashtextextended(current_schema(),0));
          CREATE TABLE IF NOT EXISTS price_histories (
            symbol TEXT PRIMARY KEY, payload TEXT, attempted_at BIGINT NOT NULL DEFAULT 0,
            lease_until BIGINT NOT NULL DEFAULT 0, lease_token TEXT NOT NULL DEFAULT '', last_error TEXT NOT NULL DEFAULT '', requested_at BIGINT NOT NULL DEFAULT 0
          );")?;
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(c),
            url: url.into(),
            schema: schema.into(),
        })
    }
    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        let mut c = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_err() {
            *c = Connection::postgres(&self.url, &self.schema)?;
        }
        Ok(c)
    }
    pub fn state(&self, symbol: &str) -> AppResult<Option<PriceState>> {
        let row=self.conn()?.query_row("SELECT payload,attempted_at,lease_until,last_error FROM price_histories WHERE symbol=$1",[symbol],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?))).optional()?;
        row.map(|(payload, attempted_at, lease_until, last_error)| {
            let history = payload
                .map(|p| serde_json::from_str::<PriceHistory>(&p))
                .transpose()?;
            if let Some(h) = &history {
                h.validate()?;
                if h.symbol != symbol {
                    return Err(AppError::Validation("公共行情证券身份不一致".into()));
                }
            }
            Ok(PriceState {
                history,
                attempted_at,
                lease_until,
                last_error,
            })
        })
        .transpose()
    }
    pub fn register(&self, symbol: &str) -> AppResult<()> {
        if canonical_symbol(symbol)? != symbol {
            return Err(AppError::Validation("公共行情需要规范证券代码".into()));
        }
        self.conn()?.execute(
            "INSERT INTO price_histories(symbol,requested_at) VALUES($1,$2) ON CONFLICT(symbol) DO UPDATE SET requested_at=excluded.requested_at",
            params![symbol,chrono::Utc::now().timestamp_millis()],
        )?;
        Ok(())
    }
    pub fn symbols(&self) -> AppResult<Vec<String>> {
        Ok(self
            .conn()?
            .prepare("SELECT symbol FROM price_histories WHERE requested_at >= $1 ORDER BY attempted_at,symbol")?
            .query_map([chrono::Utc::now().timestamp_millis()-172_800_000], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    /// A persistent lease coordinates API processes without holding a DB transaction over I/O.
    pub fn claim(&self, symbol: &str, now: i64, token: &str) -> AppResult<bool> {
        if canonical_symbol(symbol)? != symbol {
            return Err(AppError::Validation("公共行情需要规范证券代码".into()));
        }
        self.conn()?.execute(
            "INSERT INTO price_histories(symbol) VALUES($1) ON CONFLICT(symbol) DO NOTHING",
            [symbol],
        )?;
        Ok(self.conn()?.execute("UPDATE price_histories SET attempted_at=$2,lease_until=$2+120000,lease_token=$3 WHERE symbol=$1 AND lease_until<=$2 AND attempted_at<=$2-60000",params![symbol,now,token])?>0)
    }
    pub fn finish(
        &self,
        symbol: &str,
        token: &str,
        result: &AppResult<PriceHistory>,
    ) -> AppResult<()> {
        match result {
            Ok(h) => {
                h.validate()?;
                if h.symbol != symbol {
                    return Err(AppError::Validation("公共行情写入证券不一致".into()));
                }
                self.conn()?.execute("UPDATE price_histories SET payload=CASE WHEN payload IS NULL OR ((payload::jsonb->>'completedThrough') <= $4 AND (payload::jsonb->>'adjustmentAnchor') <= $5) THEN $3 ELSE payload END,lease_until=0,lease_token='',last_error='' WHERE symbol=$1 AND lease_token=$2",params![symbol,token,serde_json::to_string(h)?,h.completed_through.to_string(), h.adjustment_anchor.to_string()])?;
            }
            Err(error) => {
                self.conn()?.execute("UPDATE price_histories SET lease_until=0,lease_token='',last_error=$3 WHERE symbol=$1 AND lease_token=$2",params![symbol,token,error.to_string()])?;
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self::open(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
            &format!("market_prices_{}", uuid::Uuid::new_v4().simple()),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::history::tests::fixture;
    #[test]
    fn leases_fence_late_writers_failures_preserve_data_and_reopen_keeps_public_history() {
        let store = PublicPrices::test();
        let now = chrono::Utc::now().timestamp_millis();
        assert!(store.claim("600941.SS", now, "old").unwrap());
        assert!(!store.claim("600941.SS", now + 1, "other").unwrap());
        assert!(store.claim("600941.SS", now + 121000, "new").unwrap());
        let day = crate::models::market::today().parse().unwrap();
        let h = fixture("600941.SS", day);
        store.finish("600941.SS", "old", &Ok(h.clone())).unwrap();
        assert!(store.state("600941.SS").unwrap().unwrap().history.is_none());
        store.finish("600941.SS", "new", &Ok(h.clone())).unwrap();
        assert!(store.claim("600941.SS", now + 242000, "failed").unwrap());
        store
            .finish(
                "600941.SS",
                "failed",
                &Err(AppError::MarketData("offline".into())),
            )
            .unwrap();
        let state = store.state("600941.SS").unwrap().unwrap();
        assert_eq!(state.history, Some(h.clone()));
        assert!(state.last_error.contains("offline"));
        assert_eq!(state.lease_until, 0);
        let reopened = PublicPrices::open(&store.url, &store.schema).unwrap();
        assert_eq!(
            reopened.state("600941.SS").unwrap().unwrap().history,
            Some(h)
        );
        assert!(store.claim("600941.SH", now, "invalid").is_err());
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn older_observation_never_replaces_latest_data_and_inactive_symbols_stop_refreshing() {
        let store = PublicPrices::test();
        let now = chrono::Utc::now().timestamp_millis();
        store.register("600941.SS").unwrap();
        assert_eq!(store.symbols().unwrap(), vec!["600941.SS"]);
        assert!(store.claim("600941.SS", now, "first").unwrap());
        let day = crate::models::market::today().parse().unwrap();
        let h = crate::models::market::history::tests::fixture("600941.SS", day);
        store.finish("600941.SS", "first", &Ok(h.clone())).unwrap();
        let mut stale = h.clone();
        stale.bars.pop();
        stale = stale.normalize().unwrap();
        assert!(store.claim("600941.SS", now + 121000, "stale").unwrap());
        store.finish("600941.SS", "stale", &Ok(stale)).unwrap();
        assert_eq!(
            store.state("600941.SS").unwrap().unwrap().history,
            Some(h.clone())
        );
        store
            .conn()
            .unwrap()
            .execute("UPDATE price_histories SET requested_at=0", [])
            .unwrap();
        assert!(store.symbols().unwrap().is_empty());
        assert_eq!(store.state("600941.SS").unwrap().unwrap().history, Some(h));
    }
}
