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
mod sync_store;
mod trades;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};

use crate::storage::{Connection, Transaction};
use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, types::ValueRef};
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
                "UPDATE data_revision SET schema_version=?1 WHERE id=1",
                [schema::POSTGRES_SCHEMA_VERSION],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn hosted(&self) -> bool {
        self.conn().map(|c| c.is_postgres()).unwrap_or(false)
    }

    pub fn revision(&self) -> AppResult<String> {
        if !self.hosted() {
            return Ok("local".into());
        }
        Ok(self
            .conn()?
            .query_row("SELECT revision FROM data_revision WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })?
            .to_string())
    }

    pub fn lock_account(&self) -> AppResult<AccountLock<'_>> {
        if self.hosted() {
            self.conn()?.execute_batch(LOCK_ACCOUNT_SQL)?;
        }
        Ok(AccountLock { db: self })
    }

    fn unlock_account(&self) -> AppResult<()> {
        if self.hosted() {
            self.conn()?.execute_batch(UNLOCK_ACCOUNT_SQL)?;
        }
        Ok(())
    }

    /// Runs slow external work with one level of the caller's account lock released,
    /// then reacquires it. A cancelled future leaves the lock released.
    pub async fn without_account_lock<T>(
        &self,
        work: impl std::future::Future<Output = T>,
    ) -> AppResult<T> {
        self.unlock_account()?;
        let output = work.await;
        if self.hosted() {
            self.conn()?.execute_batch(LOCK_ACCOUNT_SQL)?;
        }
        Ok(output)
    }
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        schema::initialize(&connection)?;
        let tx = connection.transaction()?;
        trades::quarantine_invalid(&tx)?;
        cash::consolidate(&tx)?;
        cash::initialize_balances(&tx)?;
        amounts::initialize(&tx)?;
        tx.commit()?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
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

trait OptionalRow<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod postgres_tests;
mod schema;
#[cfg(test)]
mod tests;
