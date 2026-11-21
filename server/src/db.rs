mod sync_format;
#[cfg(test)]
use sync_format::sync_specs_for_version;
pub use sync_format::SyncDataset;
#[cfg(test)]
pub use sync_format::{SyncTable, SyncValue, SYNC_DATASET_SCHEMA_VERSION};
mod amounts;
mod cash;
mod corporate;
mod financial;
#[cfg(test)]
mod financial_tests;
mod rule_values;
mod snapshot;
mod sync_validation;
mod validation;
#[cfg(test)]
pub(crate) use cash::fund as fund_cash;
mod daily;
mod daily_review;
mod fx;
mod memory;
mod portfolio;
mod public_announcements;
mod public_dividend_plans;
mod public_dividends;
pub(crate) use public_announcements::PublicAnnouncements;
pub(crate) use public_dividend_plans::PublicDividendPlans;
mod public_prices;
mod public_watch;
pub(crate) use public_prices::PublicPrices;
mod account_access;
mod ai_access;
mod ai_records;
mod analysis_history;
mod evidence;
mod methodology;
mod model_settings;
mod reminders;
mod reviews;
mod secrets;
mod settings;
mod user_access;
mod user_records;
mod watch;
mod watch_grid_migration;
pub(crate) use public_dividends::PublicDividends;
pub(crate) use public_watch::PublicWatch;
mod options;
mod sync_store;
mod trades;
mod watchlist_import;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

#[cfg(test)]
use crate::storage::params;
use crate::storage::Connection;
#[cfg(test)]
use chrono::NaiveDate;
use uuid::Uuid;

#[cfg(test)]
use crate::models::{
    analysis::AnalysisResult,
    market::quotes::SecurityPriceQuote,
    portfolio::{FinancialProfile, HoldingInput, Snapshot},
    research::ResearchEvidenceInput,
    reviews::{InvestmentRuleInput, SystemReviewInput},
};
use crate::{AppError, AppResult};

pub struct Database {
    connection: Mutex<Connection>,
    lock_depth: AtomicUsize,
}

const LOCK_ACCOUNT_SQL: &str = "SELECT pg_advisory_lock(hashtextextended(current_schema(),0));";
const UNLOCK_ACCOUNT_SQL: &str = "SELECT pg_advisory_unlock(hashtextextended(current_schema(),0));";

/// Session advisory lock on the account schema; released when dropped.
/// PostgreSQL counts nested acquisitions, so guards may overlap on one connection.
#[must_use = "the account lock is released as soon as the guard is dropped"]
pub struct AccountLock<'a> {
    db: &'a Database,
}

impl Drop for AccountLock<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.db.unlock_account() {
            tracing::warn!(%error, "failed to release account lock");
        }
    }
}

impl Database {
    pub fn is_empty_for_import(&self) -> AppResult<bool> {
        if self.export_sync_data()?.tables.iter().any(|t| {
            !["daily_settings", "daily_entries", "settings"].contains(&t.name.as_str())
                && !t.rows.is_empty()
        }) {
            return Ok(false);
        }
        daily::only_initial_entries(&*self.conn()?)
    }
    #[cfg(test)]
    pub fn postgres(url: &str, user_id: &str) -> AppResult<Self> {
        let id = Uuid::parse_str(user_id).map_err(|_| AppError::Auth("无效账户".into()))?;
        Self::migrated(Connection::postgres(url, &format!("user_{}", id.simple()))?)
    }

    pub(crate) fn pooled(pool: &crate::storage::ConnectionPool, user_id: &str) -> AppResult<Self> {
        let id = Uuid::parse_str(user_id).map_err(|_| AppError::Auth("无效账户".into()))?;
        let schema = format!("user_{}", id.simple());
        let connection = pool.acquire(&schema)?;
        if pool.is_migrated(&schema) {
            return Ok(Self {
                connection: Mutex::new(connection),
                lock_depth: AtomicUsize::new(0),
            });
        }
        let db = Self::migrated(connection)?;
        pool.mark_migrated(&schema);
        Ok(db)
    }

    fn migrated(connection: Connection) -> AppResult<Self> {
        let db = Self {
            connection: Mutex::new(connection),
            lock_depth: AtomicUsize::new(0),
        };
        {
            let _lock = db.lock_account()?;
            db.migrate_tenant()?;
        }
        Ok(db)
    }

    fn migrate_tenant(&self) -> AppResult<()> {
        let mut connection = self.conn()?;
        let has_schema_version = connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM pg_attribute
             WHERE attrelid=to_regclass('data_revision')
               AND attname='schema_version' AND NOT attisdropped)",
            [],
            |r| r.get::<_, bool>(0),
        )?;
        let version = if has_schema_version {
            connection.query_row(
                "SELECT schema_version FROM data_revision WHERE id=1",
                [],
                |r| r.get::<_, i64>(0),
            )?
        } else {
            0
        };
        if version < schema::POSTGRES_SCHEMA_VERSION {
            let tx = connection.transaction()?;
            schema::initialize(&tx)?;
            trades::quarantine_invalid(&tx)?;
            cash::initialize_balances(&tx)?;
            amounts::initialize(&tx)?;
            // Re-run for existing tenants too, so newly added tables get revision triggers.
            tx.execute_batch(include_str!("../migrations/tenant-revisions.sql"))?;
            tx.execute_batch(
                "ALTER TABLE data_revision ADD COLUMN IF NOT EXISTS schema_version INTEGER NOT NULL DEFAULT 0",
            )?;
            tx.execute(
                "UPDATE data_revision SET schema_version=$1 WHERE id=1",
                [schema::POSTGRES_SCHEMA_VERSION],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn revision(&self) -> AppResult<String> {
        Ok(self
            .conn()?
            .query_row("SELECT revision FROM data_revision WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })?
            .to_string())
    }

    /// Called under the account lock after external work, before accepting a stale write.
    pub(crate) fn require_revision(&self, expected: &str) -> AppResult<()> {
        if self.revision()? != expected {
            return Err(AppError::Conflict(
                "查询期间资料已变化，请核对最新资料后重试；当前输入不会自动覆盖。".into(),
            ));
        }
        Ok(())
    }

    pub fn lock_account(&self) -> AppResult<AccountLock<'_>> {
        self.acquire_account_lock()?;
        Ok(AccountLock { db: self })
    }

    fn acquire_account_lock(&self) -> AppResult<()> {
        let started = std::time::Instant::now();
        let result = self.conn()?.execute_batch(LOCK_ACCOUNT_SQL);
        tracing::debug!(target: "mario::timing", stage = "account_lock", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
        result?;
        self.lock_depth.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn unlock_account(&self) -> AppResult<()> {
        self.conn()?.execute_batch(UNLOCK_ACCOUNT_SQL)?;
        let _ = self
            .lock_depth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |depth| {
                Some(depth.saturating_sub(1))
            });
        Ok(())
    }

    /// Runs slow external work with one level of the caller's account lock released,
    /// then reacquires it. Read-only callers without an active lock run `work` directly.
    /// A cancelled future leaves the lock released.
    pub async fn without_account_lock<T>(
        &self,
        work: impl std::future::Future<Output = T>,
    ) -> AppResult<T> {
        if self.lock_depth.load(Ordering::Acquire) == 0 {
            return Ok(work.await);
        }
        self.unlock_account()?;
        let output = work.await;
        self.acquire_account_lock()?;
        Ok(output)
    }
    /// A panic while the guard was held already rolled back any open transaction
    /// during unwinding, so a poisoned connection is still consistent.
    pub(crate) fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        Ok(self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

/// A fresh tenant in the disposable `MARIO_TEST_DATABASE_URL` database; `open` may be called
/// repeatedly to simulate a restart against the same account schema.
#[cfg(test)]
pub(crate) struct TestAccount {
    url: String,
    user: String,
}

#[cfg(test)]
impl TestAccount {
    pub(crate) fn new() -> Self {
        Self {
            url: std::env::var("MARIO_TEST_DATABASE_URL")
                .expect("set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database (scripts/test.sh starts one)"),
            user: Uuid::new_v4().to_string(),
        }
    }

    pub(crate) fn open(&self) -> Database {
        Database::postgres(&self.url, &self.user).unwrap()
    }

    pub(crate) fn pooled(&self, pool: &crate::storage::ConnectionPool) -> Database {
        Database::pooled(pool, &self.user).unwrap()
    }

    /// The account schema without running tenant migrations, for seeding older layouts.
    pub(crate) fn connection(&self) -> Connection {
        let id = Uuid::parse_str(&self.user).unwrap();
        Connection::postgres(&self.url, &format!("user_{}", id.simple())).unwrap()
    }
}

#[cfg(test)]
impl TestAccount {
    /// Reopens as if an older release had last migrated this tenant, re-running migrations.
    pub(crate) fn reopen_from_old_release(&self) -> Database {
        self.connection()
            .execute_batch("UPDATE data_revision SET schema_version=0")
            .unwrap();
        self.open()
    }
}

/// Makes every later insert into `table` fail, to exercise rollback paths.
#[cfg(test)]
pub(crate) fn fail_inserts(connection: &Connection, table: &str, message: &str) {
    fail_inserts_when(connection, table, "true", message);
}

/// Like `fail_inserts`, but only for rows matching `condition` (an SQL expression over `NEW`).
#[cfg(test)]
pub(crate) fn fail_inserts_when(
    connection: &Connection,
    table: &str,
    condition: &str,
    message: &str,
) {
    connection
        .execute_batch(&format!(
            "CREATE FUNCTION fail_{table}() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION '{message}'; END $$;
             CREATE TRIGGER fail_{table} BEFORE INSERT ON {table}
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION fail_{table}();"
        ))
        .unwrap();
}

#[cfg(test)]
impl Database {
    pub(crate) fn test() -> Self {
        TestAccount::new().open()
    }
}

#[cfg(test)]
mod postgres_tests;
mod schema;
#[cfg(test)]
mod tests;
