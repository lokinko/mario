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
    normalized_fx_provenance, validate_and_canonicalize_rule_checks, validate_currency,
    validate_goal, validate_holding, validate_holding_valuation_evidence, validate_investment_rule,
    validate_non_negative, validate_portfolio_event, validate_research_evidence,
    validate_stored_holding_valuation, validate_system_review,
};
mod ledger_values;
use ledger_values::{
    insert_portfolio_event, portfolio_event_content_hash, portfolio_event_fingerprint,
    portfolio_event_identity_fingerprint, portfolio_event_import_row, portfolio_event_record,
    portfolio_event_record_from_row, portfolio_import_revision,
};
mod rule_values;
use rule_values::{average, rule_effectiveness_signal, store_rule_revision};
mod decisions;
mod ledger;
mod memory;
mod portfolio;
mod reminders;
mod research;
mod reviews;
mod settings;
mod sync_store;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};

use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, types::ValueRef, Connection, Row, Transaction};
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

    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
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
