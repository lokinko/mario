mod sync_format;
use sync_format::{sync_specs_for_version, SYNC_TABLES};
pub use sync_format::{SyncDataset, SyncTable, SyncValue, SYNC_DATASET_SCHEMA_VERSION};
mod sync_validation;
use sync_validation::{
    quote_identifier, validate_synced_fx_provenance, validate_synced_holding_valuations,
};
mod snapshot;
use snapshot::build_snapshot;
mod validation;
use validation::{
    normalized_fx_provenance, normalized_holding_fx_provenance, validate_currency,
    validate_holding, validate_holding_valuation_evidence, validate_investment_rule,
    validate_non_negative, validate_research_evidence, validate_system_review,
};
mod rule_values;
use rule_values::store_rule_revision;
mod amounts;
mod cash;
mod corporate;
mod financial;
#[cfg(test)]
mod financial_tests;
#[cfg(test)]
pub(crate) use cash::fund as fund_cash;
mod daily;
mod fx;
mod portfolio;
mod reminders;
mod research;
mod reviews;
mod secrets;
mod settings;
pub(crate) use settings::model_key_binding;
mod sync_store;
mod trades;
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use crate::storage::{params, Connection, OptionalRow, Transaction, Value};
use chrono::{Local, NaiveDate, Utc};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        AnalysisHistoryItem, AnalysisResult, FinancialProfile, Holding, HoldingInput,
        HoldingValuationEvidence, InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision,
        ModelConfig, ResearchEvidence, ResearchEvidenceInput, ReviewReminderSummary,
        SecurityPriceQuote, Snapshot, StoredAnalysis, SystemReviewInput, SystemReviewRecord,
        SystemReviewSnapshot,
    },
    planning, risk, valuation,
};

pub struct Database {
    connection: Mutex<Connection>,
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
    pub fn postgres(url: &str, user_id: &str) -> AppResult<Self> {
        let id = Uuid::parse_str(user_id).map_err(|_| AppError::Auth("无效账户".into()))?;
        let db = Self {
            connection: Mutex::new(Connection::postgres(url, &format!("user_{}", id.simple()))?),
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
            cash::consolidate(&tx)?;
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

    pub fn lock_account(&self) -> AppResult<AccountLock<'_>> {
        self.conn()?.execute_batch(LOCK_ACCOUNT_SQL)?;
        Ok(AccountLock { db: self })
    }

    fn unlock_account(&self) -> AppResult<()> {
        Ok(self.conn()?.execute_batch(UNLOCK_ACCOUNT_SQL)?)
    }

    /// Runs slow external work with one level of the caller's account lock released,
    /// then reacquires it. A cancelled future leaves the lock released.
    pub async fn without_account_lock<T>(
        &self,
        work: impl std::future::Future<Output = T>,
    ) -> AppResult<T> {
        self.unlock_account()?;
        let output = work.await;
        self.conn()?.execute_batch(LOCK_ACCOUNT_SQL)?;
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
