mod sync_format;
use sync_format::{sync_specs_for_version, SYNC_TABLES};
pub use sync_format::{SyncDataset, SyncTable, SyncValue, SYNC_DATASET_SCHEMA_VERSION};
mod text;
use text::truncate_chars;
mod sync_validation;
use sync_validation::{
    quote_identifier, validate_synced_event_identities, validate_synced_event_reversals,
    validate_synced_fx_provenance, validate_synced_holding_valuations,
    validate_synced_memory_preferences,
};
mod snapshot;
use snapshot::build_snapshot;
mod validation;
use validation::{
    normalized_fx_provenance, normalized_holding_fx_provenance,
    validate_and_canonicalize_rule_checks, validate_currency, validate_goal, validate_holding,
    validate_holding_valuation_evidence, validate_investment_rule, validate_non_negative,
    validate_portfolio_event, validate_research_evidence, validate_stored_holding_valuation,
    validate_system_review,
};
mod ledger_values;
use ledger_values::{
    insert_portfolio_event, portfolio_event_content_hash, portfolio_event_fingerprint,
    portfolio_event_identity_fingerprint, portfolio_event_import_row, portfolio_event_record,
    portfolio_event_record_from_row, portfolio_import_revision,
};
mod rule_values;
use rule_values::{average, rule_effectiveness_signal, store_rule_revision};
pub(crate) mod daily;
mod decisions;
mod ledger;
mod memory;
mod portfolio;
mod reminders;
mod research;
mod reviews;
mod settings;
#[cfg(test)]
mod sync_merge;
mod sync_store;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};

use crate::storage::{Connection, Row, Transaction};
use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, types::ValueRef};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    event_import,
    models::{
        AnalysisHistoryItem, AnalysisResult, DecisionEntry, DecisionRecord, DecisionReview,
        DecisionReviewInput, DecisionRuleCheck, FinancialProfile, Goal, GoalInput, Holding,
        HoldingInput, HoldingValuationEvidence, InvestmentRule, InvestmentRuleInput,
        InvestmentRuleRevision, MemoryItem, MemoryPreferenceInput, ModelConfig,
        PortfolioCheckInInput, PortfolioCheckInRecord, PortfolioEventImportCommitRequest,
        PortfolioEventImportPreview, PortfolioEventImportRequest, PortfolioEventImportResult,
        PortfolioEventImportRow, PortfolioEventInput, PortfolioEventRecord,
        PortfolioEventReversalInput, PortfolioEventType, ReminderSettings, ResearchEvidence,
        ResearchEvidenceInput, ReviewReminderSummary, RuleEffectivenessItem,
        RuleEffectivenessSummary, SecurityPriceQuote, Snapshot, StoredAnalysis, SystemReviewInput,
        SystemReviewRecord, SystemReviewSnapshot,
    },
    performance, planning, risk, valuation,
};

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn is_empty_for_import(&self) -> AppResult<bool> {
        if self.export_sync_data()?.tables.iter().any(|t| {
            !["daily_settings", "daily_entries"].contains(&t.name.as_str()) && !t.rows.is_empty()
        }) {
            return Ok(false);
        }
        daily::only_initial_entries(&*self.conn()?)
    }
    pub fn postgres(url: &str, user_id: &str) -> AppResult<Self> {
        let id = Uuid::parse_str(user_id).map_err(|_| AppError::Auth("无效账户".into()))?;
        let connection = Connection::postgres(url, &format!("user_{}", id.simple()))?;
        connection
            .execute_batch("SELECT pg_advisory_lock(hashtextextended(current_schema(),0));")?;
        let initialized = connection
            .query_row("SELECT to_regclass('data_revision')::text", [], |r| {
                r.get::<_, Option<String>>(0)
            })?
            .is_some();
        if !initialized {
            connection.execute_batch("BEGIN")?;
            schema::initialize(&connection)?;
            connection.execute_batch(include_str!("../migrations/tenant-revisions.sql"))?;
            connection.execute_batch("COMMIT")?;
        }
        connection
            .execute_batch("SELECT pg_advisory_unlock(hashtextextended(current_schema(),0));")?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
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

    pub fn lock_account(&self) -> AppResult<()> {
        if self.hosted() {
            self.conn()?
                .execute_batch("SELECT pg_advisory_lock(hashtextextended(current_schema(),0));")?;
        }
        Ok(())
    }
    pub fn unlock_account(&self) -> AppResult<()> {
        if self.hosted() {
            self.conn()?.execute_batch(
                "SELECT pg_advisory_unlock(hashtextextended(current_schema(),0));",
            )?;
        }
        Ok(())
    }
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        schema::initialize(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AppError::Database(rusqlite::Error::InvalidQuery))
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

mod schema;
#[cfg(test)]
mod tests;
