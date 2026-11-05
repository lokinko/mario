//! Shared public market facts. Never reads tenant schemas, holdings or accounts.
use crate::{
    models::watch::{DividendHistory, WatchInput},
    storage::{params, Connection, OptionalRow},
    AppError, AppResult,
};
use std::sync::Mutex;

pub(crate) struct PublicDividends {
    connection: Mutex<Connection>,
    url: String,
    schema: String,
}

impl PublicDividends {
    pub fn postgres(url: &str, seeds: &[(String, DividendHistory)]) -> AppResult<Self> {
        Self::open(url, "mario_market", seeds)
    }

    fn open(url: &str, schema: &str, seeds: &[(String, DividendHistory)]) -> AppResult<Self> {
        let mut connection = Connection::postgres(url, schema)?;
        let tx = connection.transaction()?;
        tx.execute_batch(
            "SELECT pg_advisory_xact_lock(hashtextextended(current_schema(),0));
            CREATE TABLE IF NOT EXISTS dividend_histories (
                symbol TEXT PRIMARY KEY, payload TEXT NOT NULL
            );",
        )?;
        for (symbol, history) in seeds {
            save(&tx, symbol, history)?;
        }
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(connection),
            url: url.into(),
            schema: schema.into(),
        })
    }

    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Recover before a new operation after a database restart. Never replay a write.
        if connection
            .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .is_err()
        {
            *connection = Connection::postgres(&self.url, &self.schema)?;
        }
        Ok(connection)
    }

    pub fn get(&self, symbol: &str) -> AppResult<Option<DividendHistory>> {
        let connection = self.conn()?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM dividend_histories WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .as_deref()
            .map(|p| {
                let history: DividendHistory = serde_json::from_str(p)?;
                history.validate()?;
                Ok(history)
            })
            .transpose()
    }

    pub fn save(&self, symbol: &str, history: &DividendHistory) -> AppResult<()> {
        let connection = self.conn()?;
        save(&connection, symbol, history)
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self::open(
            &std::env::var("MARIO_TEST_DATABASE_URL").unwrap(),
            &format!("market_test_{}", uuid::Uuid::new_v4().simple()),
            &[],
        )
        .unwrap()
    }
}

fn save(connection: &Connection, symbol: &str, history: &DividendHistory) -> AppResult<()> {
    let input = WatchInput {
        symbol: symbol.into(),
        name: symbol.into(),
        thesis: String::new(),
    }
    .normalize()?;
    if input.symbol != symbol {
        return Err(AppError::Validation("公开分红证券代码须为规范形式".into()));
    }
    history.validate()?;
    connection.execute("INSERT INTO dividend_histories(symbol,payload) VALUES ($1,$2)
        ON CONFLICT(symbol) DO UPDATE SET payload=excluded.payload
        WHERE (dividend_histories.payload::jsonb->>'asOf') < (excluded.payload::jsonb->>'asOf')
           OR ((dividend_histories.payload::jsonb->>'asOf') = (excluded.payload::jsonb->>'asOf')
               AND (dividend_histories.payload::jsonb->>'fetchedAt')::timestamptz < (excluded.payload::jsonb->>'fetchedAt')::timestamptz)",
        params![symbol, serde_json::to_string(history)?])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::watch::DividendBasis;
    #[test]
    fn public_history_survives_reopen_and_startup_seeds_never_overwrite_newer_facts() {
        let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
        let schema = format!("market_test_{}", uuid::Uuid::new_v4().simple());
        let mut history = DividendHistory {
            basis: DividendBasis {
                as_of: chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
                fetched_at: "2026-11-04T15:00:00Z".into(),
            },
            sources: vec!["https://example.test/dividends".into()],
            events: vec![],
        };
        let store =
            PublicDividends::open(&url, &schema, &[("600036.SS".into(), history.clone())]).unwrap();
        let old = history.clone();
        history.basis.as_of = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        history.basis.fetched_at = "2026-11-05T15:00:00Z".into();
        store.save("600036.SS", &history).unwrap();
        drop(store);
        let reopened =
            PublicDividends::open(&url, &schema, &[("600036.SS".into(), old.clone())]).unwrap();
        assert_eq!(reopened.get("600036.SS").unwrap(), Some(history.clone()));
        let mut delayed = old;
        delayed.basis.fetched_at = "2026-11-06T15:00:00Z".into();
        reopened.save("600036.SS", &delayed).unwrap();
        assert_eq!(reopened.get("600036.SS").unwrap(), Some(history));
        let mut invalid = delayed;
        invalid.sources.clear();
        assert!(reopened.save("600036.SS", &invalid).is_err());
        assert!(reopened.save("600036.SH", &invalid).is_err());
        let before = reopened.get("600036.SS").unwrap().unwrap();
        assert!(PublicDividends::open(
            &url,
            &schema,
            &[
                ("600000.SS".into(), before.clone()),
                ("600036.SS".into(), invalid),
            ]
        )
        .is_err());
        assert!(reopened.get("600000.SS").unwrap().is_none());
        assert_eq!(reopened.get("600036.SS").unwrap(), Some(before));
        let pid = reopened
            .conn()
            .unwrap()
            .query_row("SELECT pg_backend_pid()", [], |r| r.get::<_, i64>(0))
            .unwrap();
        let admin = Connection::postgres(&url, &schema).unwrap();
        assert!(admin
            .query_row("SELECT pg_terminate_backend($1)", [pid], |r| r
                .get::<_, bool>(0))
            .unwrap());
        for _ in 0..100 {
            if !admin
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)",
                    [pid],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(reopened.get("600036.SS").unwrap().is_some());
    }
}
