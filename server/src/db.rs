use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};

use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, types::ValueRef, Connection, Row, Transaction};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTable {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SyncValue>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDataset {
    pub schema_version: u32,
    pub exported_at: String,
    pub tables: Vec<SyncTable>,
}

#[derive(Clone, Copy)]
struct SyncTableSpec {
    name: &'static str,
    columns: &'static [&'static str],
    order_by: &'static str,
}

const LEGACY_HOLDING_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "name",
    "asset_class",
    "market_value",
    "cost_basis",
    "target_pct",
    "currency",
    "created_at",
    "updated_at",
];

const V3_HOLDING_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "name",
    "asset_class",
    "market_value",
    "cost_basis",
    "target_pct",
    "currency",
    "fx_rate_to_base",
    "valuation_date",
    "created_at",
    "updated_at",
];

const CURRENT_HOLDING_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "name",
    "asset_class",
    "market_value",
    "cost_basis",
    "target_pct",
    "currency",
    "fx_rate_to_base",
    "valuation_date",
    "fx_rate_source",
    "fx_rate_observed_on",
    "created_at",
    "updated_at",
];

const LEGACY_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const V5_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const V6_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "fx_rate_source",
    "fx_rate_observed_on",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const CURRENT_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "fx_rate_source",
    "fx_rate_observed_on",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
    "reversal_of_event_id",
];

const V7_SYNC_TABLE_COUNT: usize = 12;

const SYNC_TABLES: &[SyncTableSpec] = &[
    SyncTableSpec {
        name: "profile",
        columns: &["id", "payload", "updated_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "goals",
        columns: &[
            "id",
            "name",
            "target_amount",
            "current_amount",
            "monthly_contribution",
            "target_date",
            "priority",
            "created_at",
            "updated_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holdings",
        columns: CURRENT_HOLDING_COLUMNS,
        order_by: "id",
    },
    SyncTableSpec {
        name: "decisions",
        columns: &["id", "asset_name", "payload", "review_date", "created_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "decision_reviews",
        columns: &[
            "decision_id",
            "outcome_summary",
            "actual_return_pct",
            "thesis_status",
            "process_rating",
            "lessons",
            "reviewed_at",
        ],
        order_by: "decision_id",
    },
    SyncTableSpec {
        name: "analyses",
        columns: &["id", "question", "answer", "audit", "trace", "created_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "system_reviews",
        columns: &[
            "id",
            "payload",
            "snapshot",
            "next_review_date",
            "created_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "investment_rules",
        columns: &[
            "id",
            "category",
            "statement",
            "trigger",
            "rationale",
            "active",
            "source_review_id",
            "revision",
            "created_at",
            "updated_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "investment_rule_revisions",
        columns: &["rule_id", "revision", "payload", "changed_at"],
        order_by: "rule_id, revision",
    },
    SyncTableSpec {
        name: "research_evidence",
        columns: &[
            "id",
            "asset_name",
            "title",
            "publisher",
            "source_url",
            "source_tier",
            "evidence_type",
            "stance",
            "as_of_date",
            "claim",
            "notes",
            "active",
            "captured_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "portfolio_checkins",
        columns: &["id", "payload", "created_at"],
        order_by: "created_at, id",
    },
    SyncTableSpec {
        name: "portfolio_events",
        columns: CURRENT_EVENT_COLUMNS,
        order_by: "occurred_on, created_at, id",
    },
    SyncTableSpec {
        name: "memory_preferences",
        columns: &["memory_id", "preference", "note", "updated_at"],
        order_by: "memory_id",
    },
    SyncTableSpec {
        name: "holding_valuations",
        columns: &[
            "holding_id",
            "symbol",
            "quantity",
            "unit_price",
            "market_value",
            "currency",
            "requested_on",
            "observed_on",
            "staleness_days",
            "provider_code",
            "provider_name",
            "exchange_name",
            "mic_code",
            "instrument_type",
            "price_basis",
            "source_url",
            "methodology_url",
            "disclaimer",
            "captured_at",
        ],
        order_by: "holding_id",
    },
];

pub const SYNC_DATASET_SCHEMA_VERSION: u32 = 9;
const V1_SYNC_TABLE_COUNT: usize = 10;
const V2_V3_SYNC_TABLE_COUNT: usize = 11;
const V8_SYNC_TABLE_COUNT: usize = 13;

fn sync_specs_for_version(version: u32) -> AppResult<Vec<SyncTableSpec>> {
    let mut specs = SYNC_TABLES.to_vec();
    match version {
        1 => {
            specs.truncate(V1_SYNC_TABLE_COUNT);
            specs[2].columns = LEGACY_HOLDING_COLUMNS;
            Ok(specs)
        }
        2 => {
            specs.truncate(V2_V3_SYNC_TABLE_COUNT);
            specs[2].columns = LEGACY_HOLDING_COLUMNS;
            Ok(specs)
        }
        3 => {
            specs.truncate(V2_V3_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            Ok(specs)
        }
        4 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            specs[11].columns = LEGACY_EVENT_COLUMNS;
            Ok(specs)
        }
        5 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            specs[11].columns = V5_EVENT_COLUMNS;
            Ok(specs)
        }
        6 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[11].columns = V6_EVENT_COLUMNS;
            Ok(specs)
        }
        7 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            Ok(specs)
        }
        8 => {
            specs.truncate(V8_SYNC_TABLE_COUNT);
            Ok(specs)
        }
        SYNC_DATASET_SCHEMA_VERSION => Ok(specs),
        _ => Err(AppError::Validation(format!(
            "不支持的数据快照版本 {version}"
        ))),
    }
}

impl SyncDataset {
    pub fn content_hash(&self) -> AppResult<String> {
        Ok(crate::cloud_sync::hash_bytes(&serde_json::to_vec(
            &self.tables,
        )?))
    }

    pub fn record_count(&self) -> usize {
        self.tables.iter().map(|table| table.rows.len()).sum()
    }

    pub fn validate(&self) -> AppResult<()> {
        let specs = sync_specs_for_version(self.schema_version)?;
        chrono::DateTime::parse_from_rfc3339(&self.exported_at)
            .map_err(|_| AppError::Validation("数据快照时间格式无效".into()))?;
        if self.tables.len() != specs.len() {
            return Err(AppError::Validation("数据快照缺少必要数据表".into()));
        }
        let mut total_rows = 0_usize;
        for spec in &specs {
            let table = self
                .tables
                .iter()
                .find(|table| table.name == spec.name)
                .ok_or_else(|| AppError::Validation(format!("数据快照缺少 {}", spec.name)))?;
            let expected_columns = spec
                .columns
                .iter()
                .copied()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if table.columns != expected_columns {
                return Err(AppError::Validation(format!(
                    "数据表 {} 的字段结构不匹配",
                    spec.name
                )));
            }
            total_rows += table.rows.len();
            if total_rows > 50_000 {
                return Err(AppError::Validation("同步记录超过 50000 条安全上限".into()));
            }
            for row in &table.rows {
                if row.len() != spec.columns.len() {
                    return Err(AppError::Validation(format!(
                        "数据表 {} 存在字段数量错误的记录",
                        spec.name
                    )));
                }
                for value in row {
                    match value {
                        SyncValue::Real(number) if !number.is_finite() => {
                            return Err(AppError::Validation("同步数据包含无效数字".into()));
                        }
                        SyncValue::Text(text) if text.len() > 2 * 1024 * 1024 => {
                            return Err(AppError::Validation(
                                "单个同步字段超过 2 MB 安全上限".into(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS profile (
               id INTEGER PRIMARY KEY CHECK (id = 1),
               payload TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS goals (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               target_amount REAL NOT NULL,
               current_amount REAL NOT NULL DEFAULT 0,
               monthly_contribution REAL NOT NULL DEFAULT 0,
               target_date TEXT NOT NULL,
               priority TEXT NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS holdings (
               id TEXT PRIMARY KEY,
               symbol TEXT NOT NULL,
               name TEXT NOT NULL,
               asset_class TEXT NOT NULL,
               market_value REAL NOT NULL,
               cost_basis REAL NOT NULL,
               target_pct REAL NOT NULL DEFAULT 0,
               currency TEXT NOT NULL,
               fx_rate_to_base REAL,
               valuation_date TEXT NOT NULL DEFAULT '',
               fx_rate_source TEXT NOT NULL DEFAULT '',
               fx_rate_observed_on TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS decisions (
               id TEXT PRIMARY KEY,
               asset_name TEXT NOT NULL,
               payload TEXT NOT NULL,
               review_date TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS decision_reviews (
               decision_id TEXT PRIMARY KEY REFERENCES decisions(id) ON DELETE CASCADE,
               outcome_summary TEXT NOT NULL,
               actual_return_pct REAL,
               thesis_status TEXT NOT NULL,
               process_rating INTEGER NOT NULL,
               lessons TEXT NOT NULL,
               reviewed_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS analyses (
               id TEXT PRIMARY KEY,
               question TEXT NOT NULL,
               answer TEXT NOT NULL,
               audit TEXT,
               trace TEXT,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS system_reviews (
               id TEXT PRIMARY KEY,
               payload TEXT NOT NULL,
               snapshot TEXT NOT NULL,
               next_review_date TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS investment_rules (
               id TEXT PRIMARY KEY,
               category TEXT NOT NULL,
               statement TEXT NOT NULL,
               trigger TEXT NOT NULL,
               rationale TEXT NOT NULL,
               active INTEGER NOT NULL,
               source_review_id TEXT,
               revision INTEGER NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               FOREIGN KEY(source_review_id) REFERENCES system_reviews(id)
             );
             CREATE TABLE IF NOT EXISTS investment_rule_revisions (
               rule_id TEXT NOT NULL,
               revision INTEGER NOT NULL,
               payload TEXT NOT NULL,
               changed_at TEXT NOT NULL,
               PRIMARY KEY(rule_id, revision),
               FOREIGN KEY(rule_id) REFERENCES investment_rules(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS research_evidence (
               id TEXT PRIMARY KEY,
               asset_name TEXT NOT NULL,
               title TEXT NOT NULL,
               publisher TEXT NOT NULL,
               source_url TEXT NOT NULL,
               source_tier TEXT NOT NULL,
               evidence_type TEXT NOT NULL,
               stance TEXT NOT NULL,
               as_of_date TEXT NOT NULL,
               claim TEXT NOT NULL,
               notes TEXT NOT NULL,
               active INTEGER NOT NULL,
               captured_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS portfolio_checkins (
               id TEXT PRIMARY KEY,
               payload TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS portfolio_events (
               id TEXT PRIMARY KEY,
               event_type TEXT NOT NULL,
               source TEXT NOT NULL DEFAULT 'manual',
               external_id TEXT NOT NULL DEFAULT '',
               fingerprint TEXT NOT NULL DEFAULT '',
               asset_name TEXT NOT NULL,
               amount REAL NOT NULL,
               currency TEXT NOT NULL,
               fx_rate_to_base REAL,
               fx_rate_source TEXT NOT NULL DEFAULT '',
               fx_rate_observed_on TEXT NOT NULL DEFAULT '',
               base_currency TEXT NOT NULL,
               base_amount REAL NOT NULL,
               occurred_on TEXT NOT NULL,
               note TEXT NOT NULL,
               created_at TEXT NOT NULL,
               reversal_of_event_id TEXT REFERENCES portfolio_events(id)
             );
             CREATE TABLE IF NOT EXISTS memory_preferences (
               memory_id TEXT PRIMARY KEY,
               preference TEXT NOT NULL,
               note TEXT NOT NULL DEFAULT '',
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS holding_valuations (
               holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE,
               symbol TEXT NOT NULL,
               quantity REAL NOT NULL,
               unit_price REAL NOT NULL,
               market_value REAL NOT NULL,
               currency TEXT NOT NULL,
               requested_on TEXT NOT NULL,
               observed_on TEXT NOT NULL,
               staleness_days INTEGER NOT NULL,
               provider_code TEXT NOT NULL,
               provider_name TEXT NOT NULL,
               exchange_name TEXT NOT NULL,
               mic_code TEXT NOT NULL,
               instrument_type TEXT NOT NULL,
               price_basis TEXT NOT NULL,
               source_url TEXT NOT NULL,
               methodology_url TEXT NOT NULL,
               disclaimer TEXT NOT NULL,
               captured_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
        )?;
        ensure_column(
            &connection,
            "goals",
            "current_amount",
            "REAL NOT NULL DEFAULT 0",
        )?;
        ensure_column(&connection, "analyses", "audit", "TEXT")?;
        ensure_column(&connection, "analyses", "trace", "TEXT")?;
        ensure_column(
            &connection,
            "goals",
            "updated_at",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "goals",
            "monthly_contribution",
            "REAL NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &connection,
            "holdings",
            "target_pct",
            "REAL NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &connection,
            "holdings",
            "updated_at",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "source",
            "TEXT NOT NULL DEFAULT 'manual'",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "external_id",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "fingerprint",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_portfolio_events_occurred_on
               ON portfolio_events(occurred_on, created_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_portfolio_events_fingerprint
               ON portfolio_events(fingerprint) WHERE fingerprint <> '';",
        )?;
        ensure_column(&connection, "holdings", "fx_rate_to_base", "REAL")?;
        ensure_column(
            &connection,
            "holdings",
            "valuation_date",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "holdings",
            "fx_rate_source",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "holdings",
            "fx_rate_observed_on",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "fx_rate_source",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "fx_rate_observed_on",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "portfolio_events",
            "reversal_of_event_id",
            "TEXT REFERENCES portfolio_events(id)",
        )?;
        connection.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_portfolio_events_single_reversal
               ON portfolio_events(reversal_of_event_id) WHERE reversal_of_event_id IS NOT NULL;",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AppError::Database(rusqlite::Error::InvalidQuery))
    }

    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self
            .conn()?
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO settings (key,value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM settings WHERE key=?1", [key])?;
        Ok(())
    }

    pub fn reminder_settings(&self) -> AppResult<ReminderSettings> {
        Ok(ReminderSettings {
            enabled: self.setting("reminders.enabled")?.as_deref() == Some("true"),
        })
    }

    pub fn save_reminder_settings(&self, enabled: bool) -> AppResult<ReminderSettings> {
        self.set_setting("reminders.enabled", if enabled { "true" } else { "false" })?;
        Ok(ReminderSettings { enabled })
    }

    pub fn review_reminder_summary(&self, today: NaiveDate) -> AppResult<ReviewReminderSummary> {
        let enabled = self.reminder_settings()?.enabled;
        let mut due_ids = self
            .decisions()?
            .into_iter()
            .filter(|decision| {
                decision.review.is_none()
                    && NaiveDate::parse_from_str(&decision.review_date, "%Y-%m-%d")
                        .is_ok_and(|date| date <= today)
            })
            .map(|decision| decision.id)
            .collect::<Vec<_>>();
        due_ids.sort();

        let latest_review = self.system_reviews()?.into_iter().next();
        let periodic_review_due = latest_review.as_ref().is_none_or(|review| {
            NaiveDate::parse_from_str(&review.next_review_date, "%Y-%m-%d")
                .map(|date| date <= today)
                .unwrap_or(true)
        });
        let periodic_marker = latest_review
            .as_ref()
            .map(|review| format!("{}:{}", review.id, review.next_review_date))
            .unwrap_or_else(|| "never-reviewed".into());
        let fingerprint_source = format!(
            "decisions={};periodic={periodic_review_due}:{periodic_marker}",
            due_ids.join(",")
        );
        let fingerprint = format!("{:x}", Sha256::digest(fingerprint_source.as_bytes()));
        let checked_on = today.format("%Y-%m-%d").to_string();
        let already_notified = self.setting("reminders.last_notified_on")?.as_deref()
            == Some(checked_on.as_str())
            && self
                .setting("reminders.last_notified_fingerprint")?
                .as_deref()
                == Some(fingerprint.as_str());
        let has_due_work = !due_ids.is_empty() || periodic_review_due;

        Ok(ReviewReminderSummary {
            enabled,
            due_decision_count: due_ids.len(),
            periodic_review_due,
            fingerprint,
            should_notify: enabled && has_due_work && !already_notified,
            checked_on,
        })
    }

    pub fn acknowledge_review_reminder(
        &self,
        today: NaiveDate,
        fingerprint: &str,
    ) -> AppResult<ReviewReminderSummary> {
        let current = self.review_reminder_summary(today)?;
        if fingerprint.trim().is_empty() || fingerprint != current.fingerprint {
            return Err(AppError::Validation(
                "复盘提醒状态已经变化，请刷新后重试".into(),
            ));
        }
        self.set_setting("reminders.last_notified_on", &current.checked_on)?;
        self.set_setting("reminders.last_notified_fingerprint", &current.fingerprint)?;
        self.review_reminder_summary(today)
    }

    pub fn export_sync_data(&self) -> AppResult<SyncDataset> {
        let conn = self.conn()?;
        let mut tables = Vec::with_capacity(SYNC_TABLES.len());
        for spec in SYNC_TABLES {
            let columns = spec
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>();
            let sql = format!(
                "SELECT {} FROM {} ORDER BY {}",
                columns.join(","),
                quote_identifier(spec.name),
                spec.order_by
            );
            let mut statement = conn.prepare(&sql)?;
            let rows = statement
                .query_map([], |row| {
                    let mut values = Vec::with_capacity(spec.columns.len());
                    for index in 0..spec.columns.len() {
                        values.push(match row.get_ref(index)? {
                            ValueRef::Null => SyncValue::Null,
                            ValueRef::Integer(value) => SyncValue::Integer(value),
                            ValueRef::Real(value) => SyncValue::Real(value),
                            ValueRef::Text(value) => SyncValue::Text(
                                String::from_utf8(value.to_vec()).map_err(|error| {
                                    rusqlite::Error::FromSqlConversionFailure(
                                        index,
                                        rusqlite::types::Type::Text,
                                        Box::new(error),
                                    )
                                })?,
                            ),
                            ValueRef::Blob(_) => {
                                return Err(rusqlite::Error::InvalidColumnType(
                                    index,
                                    spec.columns[index].into(),
                                    rusqlite::types::Type::Blob,
                                ));
                            }
                        });
                    }
                    Ok(values)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            tables.push(SyncTable {
                name: spec.name.into(),
                columns: spec.columns.iter().copied().map(str::to_owned).collect(),
                rows,
            });
        }
        let dataset = SyncDataset {
            schema_version: SYNC_DATASET_SCHEMA_VERSION,
            exported_at: Utc::now().to_rfc3339(),
            tables,
        };
        dataset.validate()?;
        Ok(dataset)
    }

    pub fn import_sync_data(&self, dataset: &SyncDataset) -> AppResult<()> {
        dataset.validate()?;
        validate_synced_event_identities(dataset)?;
        validate_synced_fx_provenance(dataset)?;
        validate_synced_event_reversals(dataset)?;
        validate_synced_memory_preferences(dataset)?;
        validate_synced_holding_valuations(dataset)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute_batch("PRAGMA defer_foreign_keys=ON;")?;

        for spec in SYNC_TABLES.iter().rev() {
            transaction.execute(&format!("DELETE FROM {}", quote_identifier(spec.name)), [])?;
        }
        for spec in sync_specs_for_version(dataset.schema_version)? {
            let table = dataset
                .tables
                .iter()
                .find(|table| table.name == spec.name)
                .ok_or_else(|| AppError::Validation(format!("数据快照缺少 {}", spec.name)))?;
            let columns = spec
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>();
            let placeholders = (1..=spec.columns.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>();
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                quote_identifier(spec.name),
                columns.join(","),
                placeholders.join(",")
            );
            let mut statement = transaction.prepare(&sql)?;
            for row in &table.rows {
                let values = row
                    .iter()
                    .map(|value| match value {
                        SyncValue::Null => rusqlite::types::Value::Null,
                        SyncValue::Integer(value) => rusqlite::types::Value::Integer(*value),
                        SyncValue::Real(value) => rusqlite::types::Value::Real(*value),
                        SyncValue::Text(value) => rusqlite::types::Value::Text(value.clone()),
                    })
                    .collect::<Vec<_>>();
                statement.execute(rusqlite::params_from_iter(values.iter()))?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn snapshot(&self) -> AppResult<Snapshot> {
        let conn = self.conn()?;
        let profile = conn
            .query_row("SELECT payload FROM profile WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();

        let mut goal_stmt = conn.prepare(
            "SELECT id, name, target_amount, current_amount, monthly_contribution, target_date, priority FROM goals ORDER BY created_at",
        )?;
        let goals = goal_stmt
            .query_map([], |row| {
                Ok(Goal {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    target_amount: row.get(2)?,
                    current_amount: row.get(3)?,
                    monthly_contribution: row.get(4)?,
                    target_date: row.get(5)?,
                    priority: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut holding_stmt = conn.prepare("SELECT id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on FROM holdings ORDER BY created_at")?;
        let holdings = holding_stmt
            .query_map([], |row| {
                Ok(Holding {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    name: row.get(2)?,
                    asset_class: row.get(3)?,
                    market_value: row.get(4)?,
                    cost_basis: row.get(5)?,
                    target_pct: row.get(6)?,
                    currency: row.get(7)?,
                    fx_rate_to_base: row.get(8)?,
                    valuation_date: row.get(9)?,
                    fx_rate_source: row.get(10)?,
                    fx_rate_observed_on: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut valuation_stmt = conn.prepare(
            "SELECT holding_id, symbol, quantity, unit_price, market_value, currency,
                    requested_on, observed_on, staleness_days, provider_code, provider_name,
                    exchange_name, mic_code, instrument_type, price_basis, source_url,
                    methodology_url, disclaimer, captured_at
             FROM holding_valuations ORDER BY holding_id",
        )?;
        let holding_valuations = valuation_stmt
            .query_map([], |row| {
                Ok(HoldingValuationEvidence {
                    holding_id: row.get(0)?,
                    symbol: row.get(1)?,
                    quantity: row.get(2)?,
                    unit_price: row.get(3)?,
                    market_value: row.get(4)?,
                    currency: row.get(5)?,
                    requested_on: row.get(6)?,
                    observed_on: row.get(7)?,
                    staleness_days: row.get(8)?,
                    provider_code: row.get(9)?,
                    provider_name: row.get(10)?,
                    exchange: row.get(11)?,
                    mic_code: row.get(12)?,
                    instrument_type: row.get(13)?,
                    price_basis: row.get(14)?,
                    source_url: row.get(15)?,
                    methodology_url: row.get(16)?,
                    disclaimer: row.get(17)?,
                    captured_at: row.get(18)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(valuation_stmt);
        drop(holding_stmt);
        drop(goal_stmt);

        let updated_at = conn
            .query_row(
                "SELECT MAX(value) FROM (
                   SELECT updated_at AS value FROM profile
                   UNION ALL
                   SELECT CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM goals
                   UNION ALL
                   SELECT CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM holdings
                 )",
                [],
                |row| row.get::<_, Option<String>>(0),
            )?
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        Ok(build_snapshot(
            profile,
            goals,
            holdings,
            holding_valuations,
            updated_at,
        ))
    }

    pub fn save_profile(&self, profile: &FinancialProfile) -> AppResult<Snapshot> {
        validate_non_negative(&[
            profile.monthly_income,
            profile.monthly_expense,
            profile.emergency_fund,
            profile.liabilities,
            profile.investable_assets,
            profile.max_drawdown_pct,
        ])?;
        if !(0..=80).contains(&profile.horizon_years) {
            return Err(AppError::Validation("投资期限应在 0—80 年之间".into()));
        }
        validate_currency(&profile.base_currency)?;
        let mut normalized = profile.clone();
        normalized.base_currency = profile.base_currency.trim().to_ascii_uppercase();
        let payload = serde_json::to_string(&normalized)?;
        let previous_base_currency = self.snapshot()?.profile.base_currency;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO profile (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![payload, Utc::now().to_rfc3339()],
        )?;
        if !previous_base_currency.eq_ignore_ascii_case(&normalized.base_currency) {
            transaction.execute(
                "UPDATE holdings SET fx_rate_to_base=NULL, fx_rate_source='',
                    fx_rate_observed_on='', updated_at=?1",
                params![Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot> {
        let base_currency = self.snapshot()?.profile.base_currency;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO holdings (id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
            params![Uuid::new_v4().to_string(), input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, now],
        )?;
        self.snapshot()
    }

    pub fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot> {
        let base_currency = self.snapshot()?.profile.base_currency;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let mut conn = self.conn()?;
        let previous = conn
            .query_row(
                "SELECT symbol, market_value, currency, valuation_date FROM holdings WHERE id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some(previous) = previous else {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        };
        let transaction = conn.transaction()?;
        let affected = transaction.execute(
            "UPDATE holdings SET symbol=?2, name=?3, asset_class=?4, market_value=?5, cost_basis=?6, target_pct=?7, currency=?8, fx_rate_to_base=?9, valuation_date=?10, fx_rate_source=?11, fx_rate_observed_on=?12, updated_at=?13 WHERE id=?1",
            params![id, input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, Utc::now().to_rfc3339()],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        }
        let valuation_changed = previous.0 != input.symbol.trim()
            || (previous.1 - input.market_value).abs() > 0.005
            || !previous.2.eq_ignore_ascii_case(input.currency.trim())
            || previous.3 != input.valuation_date.trim();
        if valuation_changed {
            transaction.execute("DELETE FROM holding_valuations WHERE holding_id=?1", [id])?;
        }
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn apply_verified_holding_valuation(
        &self,
        id: &str,
        quantity: f64,
        quote: &SecurityPriceQuote,
    ) -> AppResult<Snapshot> {
        validate_holding_valuation_evidence(quantity, quote)?;
        let market_value = quantity * quote.close;
        if !market_value.is_finite() || market_value <= 0.0 || market_value > 1e15 {
            return Err(AppError::Validation("数量乘以单位价格后的市值无效".into()));
        }
        let captured_at = Utc::now().to_rfc3339();
        let mut conn = self.conn()?;
        let holding_currency = conn
            .query_row("SELECT currency FROM holdings WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .ok_or_else(|| AppError::Validation("找不到要估值的资产".into()))?;
        if !holding_currency.eq_ignore_ascii_case(&quote.currency) {
            return Err(AppError::Validation(format!(
                "行情以 {} 计价，但持仓币种是 {}；请先核对代码和币种",
                quote.currency, holding_currency
            )));
        }
        let transaction = conn.transaction()?;
        transaction.execute(
            "UPDATE holdings
             SET symbol=?2, market_value=?3, valuation_date=?4,
                 fx_rate_to_base=CASE WHEN valuation_date=?4 THEN fx_rate_to_base ELSE NULL END,
                 fx_rate_source=CASE WHEN valuation_date=?4 THEN fx_rate_source ELSE '' END,
                 fx_rate_observed_on=CASE WHEN valuation_date=?4 THEN fx_rate_observed_on ELSE '' END,
                 updated_at=?5
             WHERE id=?1",
            params![id, quote.symbol.trim(), market_value, quote.requested_on, captured_at],
        )?;
        transaction.execute(
            "INSERT INTO holding_valuations (
               holding_id, symbol, quantity, unit_price, market_value, currency,
               requested_on, observed_on, staleness_days, provider_code, provider_name,
               exchange_name, mic_code, instrument_type, price_basis, source_url,
               methodology_url, disclaimer, captured_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
             ON CONFLICT(holding_id) DO UPDATE SET
               symbol=excluded.symbol, quantity=excluded.quantity,
               unit_price=excluded.unit_price, market_value=excluded.market_value,
               currency=excluded.currency, requested_on=excluded.requested_on,
               observed_on=excluded.observed_on, staleness_days=excluded.staleness_days,
               provider_code=excluded.provider_code, provider_name=excluded.provider_name,
               exchange_name=excluded.exchange_name, mic_code=excluded.mic_code,
               instrument_type=excluded.instrument_type, price_basis=excluded.price_basis,
               source_url=excluded.source_url, methodology_url=excluded.methodology_url,
               disclaimer=excluded.disclaimer, captured_at=excluded.captured_at",
            params![
                id,
                quote.symbol.trim(),
                quantity,
                quote.close,
                market_value,
                quote.currency.trim().to_ascii_uppercase(),
                quote.requested_on,
                quote.observed_on,
                quote.staleness_days,
                quote.provider_code,
                quote.provider_name,
                quote.exchange,
                quote.mic_code,
                quote.instrument_type,
                quote.price_basis,
                quote.source_url,
                quote.methodology_url,
                quote.disclaimer,
                captured_at,
            ],
        )?;
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn delete_holding(&self, id: &str) -> AppResult<Snapshot> {
        let affected = self
            .conn()?
            .execute("DELETE FROM holdings WHERE id=?1", [id])?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要删除的资产".into()));
        }
        self.snapshot()
    }

    pub fn portfolio_events(&self) -> AppResult<Vec<PortfolioEventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             ORDER BY event.occurred_on DESC, event.created_at DESC, event.id DESC LIMIT 500",
        )?;
        let rows = statement.query_map([], portfolio_event_record_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn add_portfolio_event(
        &self,
        input: &PortfolioEventInput,
    ) -> AppResult<PortfolioEventRecord> {
        let snapshot = self.snapshot()?;
        let base_currency = snapshot.profile.base_currency.trim().to_ascii_uppercase();
        validate_portfolio_event(input, &base_currency)?;

        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        if !latest.base_currency.eq_ignore_ascii_case(&base_currency) {
            return Err(AppError::Validation(
                "基准币种已改变；请先建立新的组合基线，再记录流水".into(),
            ));
        }
        let latest_date = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先明确建立新的比较基线".into())
        })?;
        if input.occurred_on.trim() <= latest_date.as_str() {
            return Err(AppError::Validation(format!(
                "流水日期必须晚于最近一次组合检查点 {latest_date}；已冻结期间不能回填"
            )));
        }

        let record = portfolio_event_record(input, &base_currency)?;
        let fingerprint = portfolio_event_fingerprint(input);
        let conn = self.conn()?;
        if !fingerprint.is_empty()
            && conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM portfolio_events WHERE fingerprint=?1)",
                [&fingerprint],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(AppError::Conflict(
                "相同来源与交易 ID 的流水已经存在".into(),
            ));
        }
        insert_portfolio_event(&conn, &record, &fingerprint)?;
        Ok(record)
    }

    pub fn reverse_portfolio_event(
        &self,
        id: &str,
        input: &PortfolioEventReversalInput,
    ) -> AppResult<PortfolioEventRecord> {
        let occurred_on = NaiveDate::parse_from_str(input.occurred_on.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::Validation("冲正日期必须使用 YYYY-MM-DD".into()))?;
        if occurred_on > Local::now().date_naive() {
            return Err(AppError::Validation("冲正日期不能晚于今天".into()));
        }
        if input.note.trim().is_empty() {
            return Err(AppError::Validation("冲正说明为必填项".into()));
        }
        if input.note.chars().count() > 2_000 {
            return Err(AppError::Validation("冲正说明不能超过 2000 个字符".into()));
        }

        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        let frozen_through = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先明确建立新的比较基线".into())
        })?;
        if input.occurred_on.trim() <= frozen_through.as_str() {
            return Err(AppError::Validation(format!(
                "冲正日期必须晚于最近一次组合检查点 {frozen_through}"
            )));
        }

        let conn = self.conn()?;
        let source = conn
            .query_row(
                "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                        event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                        event.fx_rate_observed_on, event.base_currency, event.base_amount,
                        event.occurred_on, event.note, event.reversal_of_event_id,
                        reversal.id, event.created_at
                 FROM portfolio_events event
                 LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
                 WHERE event.id=?1",
                [id],
                portfolio_event_record_from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("找不到要冲正的组合流水".into()))?;

        if source.reversal_of_event_id.is_some() || source.amount <= 0.0 {
            return Err(AppError::Validation("冲正记录不能再次冲正".into()));
        }
        if source.reversed_by_event_id.is_some() {
            return Err(AppError::Conflict("这笔流水已经冲正".into()));
        }
        if source.occurred_on <= frozen_through {
            return Err(AppError::Validation(
                "这笔流水已进入冻结周期；请建立纠正后的新基线并保留说明".into(),
            ));
        }
        if input.occurred_on.trim() < source.occurred_on.as_str() {
            return Err(AppError::Validation("冲正日期不能早于原流水日期".into()));
        }

        let reversal_id = Uuid::new_v4().to_string();
        let record = PortfolioEventRecord {
            id: reversal_id.clone(),
            event_type: source.event_type,
            source: "mario-reversal".into(),
            external_id: format!("reversal:{id}"),
            asset_name: source.asset_name,
            amount: -source.amount,
            currency: source.currency,
            fx_rate_to_base: source.fx_rate_to_base,
            fx_rate_source: source.fx_rate_source,
            fx_rate_observed_on: source.fx_rate_observed_on,
            base_currency: source.base_currency,
            base_amount: -source.base_amount,
            occurred_on: input.occurred_on.trim().into(),
            note: input.note.trim().into(),
            reversal_of_event_id: Some(id.into()),
            reversed_by_event_id: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let fingerprint = portfolio_event_identity_fingerprint(&record.source, &record.external_id);
        insert_portfolio_event(&conn, &record, &fingerprint)?;
        Ok(record)
    }

    pub fn preview_portfolio_event_import(
        &self,
        request: &PortfolioEventImportRequest,
    ) -> AppResult<PortfolioEventImportPreview> {
        let parsed = event_import::parse(&request.csv_text)?;
        let snapshot = self.snapshot()?;
        let base_currency = snapshot.profile.base_currency.trim().to_ascii_uppercase();
        let latest = self
            .portfolio_checkins()?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("请先在决策总览建立组合基线".into()))?;
        if !latest.base_currency.eq_ignore_ascii_case(&base_currency) {
            return Err(AppError::Validation(
                "基准币种已改变；请先建立新的组合基线，再导入流水".into(),
            ));
        }
        let frozen_through = latest.valuation_date.ok_or_else(|| {
            AppError::Validation("旧组合检查点没有估值日期；请先建立新的比较基线".into())
        })?;
        let existing = self.portfolio_event_identities()?;
        let mut seen = HashMap::<String, String>::new();
        let mut rows = parsed.issues;

        for parsed_row in parsed.rows {
            let input = &parsed_row.input;
            let mut status = "ready";
            let mut message = "校验通过，确认后写入".to_owned();
            let validation =
                if input.source.trim().is_empty() || input.external_id.trim().is_empty() {
                    Err(AppError::Validation(
                        "CSV 导入必须填写 source 和 external_id".into(),
                    ))
                } else if input.occurred_on.trim() <= frozen_through.as_str() {
                    Err(AppError::Validation(format!(
                        "发生日期必须晚于冻结边界 {frozen_through}"
                    )))
                } else {
                    validate_portfolio_event(input, &base_currency)
                };

            if let Err(error) = validation {
                status = "error";
                message = error.to_string();
            } else {
                let record = portfolio_event_record(input, &base_currency)?;
                let identity = portfolio_event_fingerprint(input);
                let content = portfolio_event_content_hash(&record)?;
                if let Some(existing_content) = existing.get(&identity) {
                    if existing_content == &content {
                        status = "duplicate";
                        message = "相同来源交易 ID 与内容已经存在，将跳过".into();
                    } else {
                        status = "error";
                        message = "相同来源交易 ID 已存在，但内容不同".into();
                    }
                } else if let Some(previous_content) = seen.get(&identity) {
                    if previous_content == &content {
                        status = "duplicate";
                        message = "CSV 内存在完全相同的重复行，将跳过".into();
                    } else {
                        status = "error";
                        message = "CSV 内同一来源交易 ID 对应不同内容".into();
                    }
                } else {
                    seen.insert(identity, content);
                }
            }

            rows.push(portfolio_event_import_row(
                parsed_row.row_number,
                input,
                status,
                message,
                &base_currency,
            ));
        }
        rows.sort_by_key(|row| row.row_number);
        let ready_count = rows.iter().filter(|row| row.status == "ready").count();
        let duplicate_count = rows.iter().filter(|row| row.status == "duplicate").count();
        let error_count = rows.iter().filter(|row| row.status == "error").count();
        let preview_revision =
            portfolio_import_revision(&rows, &base_currency, &frozen_through, &request.csv_text)?;
        Ok(PortfolioEventImportPreview {
            rows,
            ready_count,
            duplicate_count,
            error_count,
            preview_revision,
            base_currency,
            frozen_through,
        })
    }

    pub fn commit_portfolio_event_import(
        &self,
        request: &PortfolioEventImportCommitRequest,
    ) -> AppResult<PortfolioEventImportResult> {
        let preview = self.preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: request.csv_text.clone(),
        })?;
        if request.preview_revision != preview.preview_revision {
            return Err(AppError::Conflict(
                "流水或组合基线已变化，请重新预览 CSV".into(),
            ));
        }
        if preview.error_count > 0 {
            return Err(AppError::Validation(
                "CSV 仍有错误行，修正并重新预览后才能导入".into(),
            ));
        }
        let ready_rows = preview
            .rows
            .iter()
            .filter(|row| row.status == "ready")
            .map(|row| row.row_number)
            .collect::<HashSet<_>>();
        let parsed = event_import::parse(&request.csv_text)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let mut inserted_count = 0;
        for row in parsed.rows {
            if !ready_rows.contains(&row.row_number) {
                continue;
            }
            let record = portfolio_event_record(&row.input, &preview.base_currency)?;
            let fingerprint = portfolio_event_fingerprint(&row.input);
            insert_portfolio_event(&transaction, &record, &fingerprint)?;
            inserted_count += 1;
        }
        transaction.commit()?;
        Ok(PortfolioEventImportResult {
            inserted_count,
            duplicate_count: preview.duplicate_count,
            preview_revision: preview.preview_revision,
        })
    }

    fn portfolio_event_identities(&self) -> AppResult<HashMap<String, String>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             WHERE event.external_id <> ''",
        )?;
        let rows = statement.query_map([], portfolio_event_record_from_row)?;
        let mut identities = HashMap::new();
        for row in rows {
            let record = row?;
            identities.insert(
                portfolio_event_fingerprint(&PortfolioEventInput {
                    event_type: record.event_type.clone(),
                    source: record.source.clone(),
                    external_id: record.external_id.clone(),
                    asset_name: record.asset_name.clone(),
                    amount: record.amount,
                    currency: record.currency.clone(),
                    fx_rate_to_base: record.fx_rate_to_base,
                    fx_rate_source: record.fx_rate_source.clone(),
                    fx_rate_observed_on: record.fx_rate_observed_on.clone(),
                    occurred_on: record.occurred_on.clone(),
                    note: record.note.clone(),
                }),
                portfolio_event_content_hash(&record)?,
            );
        }
        Ok(identities)
    }

    fn portfolio_events_between(
        &self,
        period_start: &str,
        period_end: &str,
    ) -> AppResult<Vec<PortfolioEventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT event.id, event.event_type, event.source, event.external_id, event.asset_name,
                    event.amount, event.currency, event.fx_rate_to_base, event.fx_rate_source,
                    event.fx_rate_observed_on, event.base_currency, event.base_amount,
                    event.occurred_on, event.note, event.reversal_of_event_id,
                    reversal.id, event.created_at
             FROM portfolio_events event
             LEFT JOIN portfolio_events reversal ON reversal.reversal_of_event_id = event.id
             WHERE event.occurred_on > ?1 AND event.occurred_on <= ?2
             ORDER BY event.occurred_on, event.created_at, event.id",
        )?;
        let rows = statement.query_map(
            params![period_start, period_end],
            portfolio_event_record_from_row,
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn portfolio_checkins(&self) -> AppResult<Vec<PortfolioCheckInRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT payload FROM portfolio_checkins
             ORDER BY created_at DESC, id DESC LIMIT 100",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(serde_json::from_str::<PortfolioCheckInRecord>(&row?)?);
        }
        Ok(records)
    }

    pub fn save_portfolio_checkin(
        &self,
        input: &PortfolioCheckInInput,
    ) -> AppResult<PortfolioCheckInRecord> {
        if input.period_label.trim().is_empty() {
            return Err(AppError::Validation("组合快照周期为必填项".into()));
        }
        if input.period_label.chars().count() > 100 || input.note.chars().count() > 2_000 {
            return Err(AppError::Validation("组合快照周期或说明过长".into()));
        }
        if !input.external_cash_flow.is_finite() || input.external_cash_flow.abs() > 1e15 {
            return Err(AppError::Validation("期间净入金必须是有效金额".into()));
        }
        if !input.use_ledger_cash_flows
            && input.external_cash_flow.abs() > 0.005
            && input.note.trim().is_empty()
        {
            return Err(AppError::Validation(
                "存在净入金或出金时，必须说明现金流来源".into(),
            ));
        }

        let snapshot = self.snapshot()?;
        if snapshot.holdings.is_empty() {
            return Err(AppError::Validation(
                "请先录入当前持仓，再建立组合变化基线".into(),
            ));
        }
        if !snapshot.valuation_status.comparable {
            return Err(AppError::Validation(format!(
                "请先补齐外币持仓汇率：{}",
                snapshot.valuation_status.missing_fx_holdings.join("、")
            )));
        }
        let valuation_date = snapshot
            .valuation_status
            .aligned_valuation_date
            .clone()
            .ok_or_else(|| {
                AppError::Validation("全部持仓必须使用同一个估值日期，才能冻结组合检查点".into())
            })?;
        if snapshot.total_value <= 0.0 {
            return Err(AppError::Validation("组合折算后的总市值必须大于 0".into()));
        }
        let previous = if input.reset_baseline {
            None
        } else {
            self.portfolio_checkins()?.into_iter().next()
        };
        if previous.is_none() && input.external_cash_flow.abs() > 0.005 {
            return Err(AppError::Validation(
                "第一条记录是组合基线，期间净入金应填写 0".into(),
            ));
        }
        if previous.as_ref().is_some_and(|record| {
            !record
                .base_currency
                .eq_ignore_ascii_case(&snapshot.valuation_status.base_currency)
        }) {
            return Err(AppError::Validation(
                "基准币种已经改变；请明确选择按新币种重新建立基线".into(),
            ));
        }

        let previous_date = previous
            .as_ref()
            .and_then(|record| record.valuation_date.as_deref());
        if previous_date.is_some_and(|date| valuation_date.as_str() <= date) {
            return Err(AppError::Validation(
                "本次估值日期必须晚于上一条组合检查点".into(),
            ));
        }

        let period_events = if input.use_ledger_cash_flows {
            if let Some(period_start) = previous_date {
                let events = self.portfolio_events_between(period_start, &valuation_date)?;
                if events.iter().any(|event| {
                    !event
                        .base_currency
                        .eq_ignore_ascii_case(&snapshot.valuation_status.base_currency)
                }) {
                    return Err(AppError::Validation(
                        "期间流水的基准币种与当前组合不一致，请重新建立基线".into(),
                    ));
                }
                events
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let event_summary = performance::summarize(&period_events);
        let external_cash_flow = if input.use_ledger_cash_flows {
            event_summary.external_cash_flow
        } else {
            input.external_cash_flow
        };

        let total_change = previous
            .as_ref()
            .map(|record| snapshot.total_value - record.total_value);
        let valuation_residual = total_change.map(|change| change - external_cash_flow);
        let modified_dietz_return_pct = if input.use_ledger_cash_flows {
            previous.as_ref().and_then(|record| {
                let period_start =
                    NaiveDate::parse_from_str(record.valuation_date.as_deref()?, "%Y-%m-%d")
                        .ok()?;
                let period_end = NaiveDate::parse_from_str(&valuation_date, "%Y-%m-%d").ok()?;
                performance::modified_dietz_return_pct(
                    record.total_value,
                    snapshot.total_value,
                    period_start,
                    period_end,
                    &period_events,
                )
            })
        } else {
            None
        };
        let allocation_changes = previous
            .as_ref()
            .map(|record| {
                valuation::allocation_changes(
                    &record.holdings,
                    &snapshot.holdings,
                    &snapshot.valuation_status.base_currency,
                )
            })
            .unwrap_or_default();
        let record = PortfolioCheckInRecord {
            id: Uuid::new_v4().to_string(),
            period_label: input.period_label.trim().into(),
            external_cash_flow,
            note: input.note.trim().into(),
            total_value: snapshot.total_value,
            previous_check_in_id: previous.as_ref().map(|record| record.id.clone()),
            previous_total_value: previous.as_ref().map(|record| record.total_value),
            total_change,
            valuation_residual,
            base_currency: snapshot.valuation_status.base_currency,
            valuation_date: Some(valuation_date),
            cash_flow_source: if previous.is_none() {
                "baseline".into()
            } else if input.use_ledger_cash_flows {
                "ledger".into()
            } else {
                "manual".into()
            },
            event_ids: event_summary.event_ids,
            income: event_summary.income,
            costs: event_summary.costs,
            turnover: event_summary.turnover,
            modified_dietz_return_pct,
            holdings: snapshot.holdings,
            holding_valuations: snapshot.holding_valuations,
            allocation_changes,
            created_at: Utc::now().to_rfc3339(),
        };
        self.conn()?.execute(
            "INSERT INTO portfolio_checkins (id, payload, created_at) VALUES (?1,?2,?3)",
            params![
                record.id,
                serde_json::to_string(&record)?,
                record.created_at
            ],
        )?;
        Ok(record)
    }

    pub fn add_goal(&self, input: &GoalInput) -> AppResult<Snapshot> {
        validate_goal(input)?;
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO goals (id, name, target_amount, current_amount, monthly_contribution, target_date, priority, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![Uuid::new_v4().to_string(), input.name.trim(), input.target_amount, input.current_amount, input.monthly_contribution, input.target_date, input.priority, now],
        )?;
        self.snapshot()
    }

    pub fn update_goal(&self, id: &str, input: &GoalInput) -> AppResult<Snapshot> {
        validate_goal(input)?;
        let affected = self.conn()?.execute(
            "UPDATE goals SET name=?2, target_amount=?3, current_amount=?4, monthly_contribution=?5, target_date=?6, priority=?7, updated_at=?8 WHERE id=?1",
            params![id, input.name.trim(), input.target_amount, input.current_amount, input.monthly_contribution, input.target_date, input.priority, Utc::now().to_rfc3339()],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的目标".into()));
        }
        self.snapshot()
    }

    pub fn delete_goal(&self, id: &str) -> AppResult<Snapshot> {
        let affected = self
            .conn()?
            .execute("DELETE FROM goals WHERE id=?1", [id])?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要删除的目标".into()));
        }
        self.snapshot()
    }

    pub fn save_decision(&self, entry: &DecisionEntry) -> AppResult<()> {
        if entry.asset_name.trim().is_empty()
            || entry.thesis.trim().is_empty()
            || entry.counter_thesis.trim().is_empty()
            || entry.invalidation.trim().is_empty()
            || entry.review_date.trim().is_empty()
        {
            return Err(AppError::Validation(
                "投资对象、正反逻辑、证伪条件和复盘日期为必填项".into(),
            ));
        }
        validate_non_negative(&[
            entry.expected_return_pct,
            entry.downside_pct,
            entry.confidence_pct,
            entry.position_pct,
        ])?;
        if entry.confidence_pct > 100.0 || entry.position_pct > 100.0 {
            return Err(AppError::Validation("置信度和仓位不能超过 100%".into()));
        }
        chrono::NaiveDate::parse_from_str(&entry.review_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("复盘日期格式无效".into()))?;
        let active_rules = self
            .investment_rules()?
            .into_iter()
            .filter(|rule| rule.active)
            .collect::<Vec<_>>();
        let canonical_rule_checks =
            validate_and_canonicalize_rule_checks(&entry.rule_checks, &active_rules)?;
        if entry.source_action_index.is_some() && entry.source_analysis_id.is_none() {
            return Err(AppError::Validation(
                "AI 行动序号必须关联一条分析记录".into(),
            ));
        }
        let conn = self.conn()?;
        if let Some(source_id) = entry.source_analysis_id.as_deref() {
            if source_id.trim().is_empty() || source_id.len() > 128 {
                return Err(AppError::Validation("AI 分析来源 ID 无效".into()));
            }
            let source_trace = conn
                .query_row(
                    "SELECT trace FROM analyses WHERE id=?1",
                    [source_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    AppError::Validation("找不到关联的 AI 分析；请保留原分析后再冻结决策".into())
                })?;
            if let Some(action_index) = entry.source_action_index {
                let trace = source_trace
                    .as_deref()
                    .and_then(|value| {
                        serde_json::from_str::<crate::models::AnalysisWorkflowTrace>(value).ok()
                    })
                    .ok_or_else(|| AppError::Validation("关联的 AI 分析缺少可验证工作流".into()))?;
                let actions = trace
                    .structured_report
                    .map(|report| report.actions)
                    .unwrap_or_default();
                if action_index >= actions.len() {
                    return Err(AppError::Validation("AI 行动序号超出原分析范围".into()));
                }
            }
        }
        let id = entry
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut stored = entry.clone();
        stored.id = Some(id.clone());
        stored.rule_checks = canonical_rule_checks;
        conn.execute(
            "INSERT INTO decisions (id, asset_name, payload, review_date, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, entry.asset_name.trim(), serde_json::to_string(&stored)?, entry.review_date, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn decisions(&self) -> AppResult<Vec<DecisionRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT d.id, d.payload, d.created_at,
                    r.outcome_summary, r.actual_return_pct, r.thesis_status,
                    r.process_rating, r.lessons, r.reviewed_at
             FROM decisions d
             LEFT JOIN decision_reviews r ON r.decision_id = d.id
             ORDER BY d.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let review = row
                .get::<_, Option<String>>(3)?
                .map(|outcome_summary| DecisionReview {
                    outcome_summary,
                    actual_return_pct: row.get(4).ok().flatten(),
                    thesis_status: row.get::<_, String>(5).unwrap_or_default(),
                    process_rating: row.get::<_, i64>(6).unwrap_or_default(),
                    lessons: row.get::<_, String>(7).unwrap_or_default(),
                    reviewed_at: row.get::<_, String>(8).unwrap_or_default(),
                });
            Ok((id, payload, created_at, review))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (id, payload, created_at, review) = row?;
            let entry: DecisionEntry = serde_json::from_str(&payload)?;
            records.push(DecisionRecord {
                id,
                source_analysis_id: entry.source_analysis_id,
                source_action_index: entry.source_action_index,
                asset_name: entry.asset_name,
                thesis: entry.thesis,
                counter_thesis: entry.counter_thesis,
                expected_return_pct: entry.expected_return_pct,
                downside_pct: entry.downside_pct,
                confidence_pct: entry.confidence_pct,
                position_pct: entry.position_pct,
                invalidation: entry.invalidation,
                review_date: entry.review_date,
                rule_checks: entry.rule_checks,
                created_at,
                review,
            });
        }
        Ok(records)
    }

    pub fn save_decision_review(&self, id: &str, input: &DecisionReviewInput) -> AppResult<()> {
        if input.outcome_summary.trim().is_empty() || input.lessons.trim().is_empty() {
            return Err(AppError::Validation("结果摘要和经验修正为必填项".into()));
        }
        if !(1..=5).contains(&input.process_rating) {
            return Err(AppError::Validation("过程评分必须在 1—5 之间".into()));
        }
        if !matches!(
            input.thesis_status.as_str(),
            "成立" | "部分成立" | "失效" | "尚不明确"
        ) {
            return Err(AppError::Validation("未知的原始逻辑结果".into()));
        }
        if let Some(value) = input.actual_return_pct {
            if !value.is_finite() {
                return Err(AppError::Validation("实际收益率必须是有效数字".into()));
            }
        }
        let conn = self.conn()?;
        let exists: i64 =
            conn.query_row("SELECT COUNT(*) FROM decisions WHERE id=?1", [id], |row| {
                row.get(0)
            })?;
        if exists == 0 {
            return Err(AppError::Validation("找不到要复盘的决策".into()));
        }
        conn.execute(
            "INSERT INTO decision_reviews
             (decision_id, outcome_summary, actual_return_pct, thesis_status, process_rating, lessons, reviewed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(decision_id) DO UPDATE SET
               outcome_summary=excluded.outcome_summary,
               actual_return_pct=excluded.actual_return_pct,
               thesis_status=excluded.thesis_status,
               process_rating=excluded.process_rating,
               lessons=excluded.lessons,
               reviewed_at=excluded.reviewed_at",
            params![id, input.outcome_summary.trim(), input.actual_return_pct, input.thesis_status, input.process_rating, input.lessons.trim(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn investment_rules(&self) -> AppResult<Vec<InvestmentRule>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, category, statement, trigger, rationale, active, source_review_id,
                    revision, created_at, updated_at
             FROM investment_rules
             ORDER BY active DESC, updated_at DESC, id ASC",
        )?;
        let rules = statement
            .query_map([], |row| {
                Ok(InvestmentRule {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    statement: row.get(2)?,
                    trigger: row.get(3)?,
                    rationale: row.get(4)?,
                    active: row.get::<_, i64>(5)? != 0,
                    source_review_id: row.get(6)?,
                    revision: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rules)
    }

    pub fn add_investment_rule(&self, input: &InvestmentRuleInput) -> AppResult<InvestmentRule> {
        validate_investment_rule(input)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let rule = InvestmentRule {
            id: id.clone(),
            category: input.category.trim().into(),
            statement: input.statement.trim().into(),
            trigger: input.trigger.trim().into(),
            rationale: input.rationale.trim().into(),
            active: input.active,
            source_review_id: input.source_review_id.clone(),
            revision: 1,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO investment_rules
             (id, category, statement, trigger, rationale, active, source_review_id, revision, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                rule.id,
                rule.category,
                rule.statement,
                rule.trigger,
                rule.rationale,
                i64::from(rule.active),
                rule.source_review_id,
                rule.revision,
                rule.created_at,
                rule.updated_at
            ],
        )?;
        store_rule_revision(&transaction, &rule)?;
        transaction.commit()?;
        Ok(rule)
    }

    pub fn update_investment_rule(
        &self,
        id: &str,
        input: &InvestmentRuleInput,
    ) -> AppResult<InvestmentRule> {
        validate_investment_rule(input)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT created_at, revision FROM investment_rules WHERE id=?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("找不到要修订的投资规则".into()))?;
        let now = Utc::now().to_rfc3339();
        let rule = InvestmentRule {
            id: id.into(),
            category: input.category.trim().into(),
            statement: input.statement.trim().into(),
            trigger: input.trigger.trim().into(),
            rationale: input.rationale.trim().into(),
            active: input.active,
            source_review_id: input.source_review_id.clone(),
            revision: existing.1 + 1,
            created_at: existing.0,
            updated_at: now,
        };
        transaction.execute(
            "UPDATE investment_rules SET category=?2, statement=?3, trigger=?4, rationale=?5,
                    active=?6, source_review_id=?7, revision=?8, updated_at=?9 WHERE id=?1",
            params![
                rule.id,
                rule.category,
                rule.statement,
                rule.trigger,
                rule.rationale,
                i64::from(rule.active),
                rule.source_review_id,
                rule.revision,
                rule.updated_at
            ],
        )?;
        store_rule_revision(&transaction, &rule)?;
        transaction.commit()?;
        Ok(rule)
    }

    pub fn investment_rule_history(&self, id: &str) -> AppResult<Vec<InvestmentRuleRevision>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT payload, changed_at FROM investment_rule_revisions
             WHERE rule_id=?1 ORDER BY revision DESC",
        )?;
        let rows = statement.query_map([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut revisions = Vec::new();
        for row in rows {
            let (payload, changed_at) = row?;
            let rule: InvestmentRule = serde_json::from_str(&payload)?;
            revisions.push(InvestmentRuleRevision {
                rule_id: rule.id,
                revision: rule.revision,
                category: rule.category,
                statement: rule.statement,
                trigger: rule.trigger,
                rationale: rule.rationale,
                active: rule.active,
                source_review_id: rule.source_review_id,
                changed_at,
            });
        }
        Ok(revisions)
    }

    pub fn rule_effectiveness(&self) -> AppResult<RuleEffectivenessSummary> {
        let decisions = self.decisions()?;
        let rules = self.investment_rules()?;
        let mut items = Vec::with_capacity(rules.len());

        for rule in rules {
            let checks = decisions
                .iter()
                .filter_map(|decision| {
                    decision
                        .rule_checks
                        .iter()
                        .find(|check| check.rule_id == rule.id)
                        .map(|check| (check, decision.review.as_ref()))
                })
                .collect::<Vec<_>>();
            let applicable = checks
                .iter()
                .filter(|(check, _)| check.status != "不适用")
                .collect::<Vec<_>>();
            let followed = applicable
                .iter()
                .filter(|(check, _)| check.status == "遵守")
                .collect::<Vec<_>>();
            let deviated = applicable
                .iter()
                .filter(|(check, _)| check.status == "偏离")
                .collect::<Vec<_>>();
            let followed_process = followed
                .iter()
                .filter_map(|(_, review)| review.map(|value| value.process_rating as f64))
                .collect::<Vec<_>>();
            let deviated_process = deviated
                .iter()
                .filter_map(|(_, review)| review.map(|value| value.process_rating as f64))
                .collect::<Vec<_>>();
            let mut observed_revisions = checks
                .iter()
                .map(|(check, _)| check.rule_revision)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            observed_revisions.sort_unstable();
            let reviewed_count = applicable
                .iter()
                .filter(|(_, review)| review.is_some())
                .count();
            let followed_process_average = average(&followed_process);
            let deviated_process_average = average(&deviated_process);

            items.push(RuleEffectivenessItem {
                rule_id: rule.id,
                current_revision: rule.revision,
                observed_revisions,
                category: rule.category,
                statement: rule.statement,
                active: rule.active,
                decision_count: checks.len(),
                applicable_count: applicable.len(),
                followed_count: followed.len(),
                deviated_count: deviated.len(),
                reviewed_count,
                followed_process_average,
                deviated_process_average,
                signal: rule_effectiveness_signal(
                    reviewed_count,
                    followed_process_average,
                    deviated_process_average,
                    followed_process.len(),
                    deviated_process.len(),
                ),
            });
        }

        let evaluated_decisions = decisions
            .iter()
            .filter(|decision| !decision.rule_checks.is_empty())
            .count();
        let applicable_checks = items.iter().map(|item| item.applicable_count).sum();
        let followed_checks = items.iter().map(|item| item.followed_count).sum();
        let reviewed_checks = items.iter().map(|item| item.reviewed_count).sum();
        let adherence_pct = (applicable_checks > 0)
            .then(|| followed_checks as f64 / applicable_checks as f64 * 100.0);

        Ok(RuleEffectivenessSummary {
            total_decisions: decisions.len(),
            evaluated_decisions,
            applicable_checks,
            followed_checks,
            adherence_pct,
            reviewed_checks,
            rules: items,
        })
    }

    pub fn save_system_review(&self, input: &SystemReviewInput) -> AppResult<SystemReviewRecord> {
        validate_system_review(input)?;
        let portfolio = self.snapshot()?;
        let decisions = self.decisions()?;
        let rules = self.investment_rules()?;
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let snapshot = SystemReviewSnapshot {
            portfolio_value: portfolio.total_value,
            base_currency: portfolio.profile.base_currency.clone(),
            portfolio_comparable: portfolio.valuation_status.comparable,
            emergency_months: portfolio.emergency_months,
            concentration_pct: portfolio.concentration_pct,
            risk_status: portfolio.plan.risk_status,
            high_risk_findings: portfolio
                .findings
                .iter()
                .filter(|finding| finding.level == "high")
                .count(),
            goal_total: portfolio.goals.len(),
            goals_on_track: portfolio
                .plan
                .goal_projections
                .iter()
                .filter(|goal| matches!(goal.status.as_str(), "on-track" | "reached"))
                .count(),
            decision_total: decisions.len(),
            reviewed_decisions: decisions
                .iter()
                .filter(|decision| decision.review.is_some())
                .count(),
            active_rules: rules.iter().filter(|rule| rule.active).count(),
        };
        let record = SystemReviewRecord {
            id,
            period_label: input.period_label.trim().into(),
            adherence_score: input.adherence_score,
            process_summary: input.process_summary.trim().into(),
            rule_violations: input.rule_violations.trim().into(),
            lessons: input.lessons.trim().into(),
            next_actions: input.next_actions.trim().into(),
            next_review_date: input.next_review_date.clone(),
            snapshot,
            created_at,
        };
        let normalized_input = SystemReviewInput {
            period_label: record.period_label.clone(),
            adherence_score: record.adherence_score,
            process_summary: record.process_summary.clone(),
            rule_violations: record.rule_violations.clone(),
            lessons: record.lessons.clone(),
            next_actions: record.next_actions.clone(),
            next_review_date: record.next_review_date.clone(),
        };
        self.conn()?.execute(
            "INSERT INTO system_reviews (id, payload, snapshot, next_review_date, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                record.id,
                serde_json::to_string(&normalized_input)?,
                serde_json::to_string(&record.snapshot)?,
                record.next_review_date,
                record.created_at
            ],
        )?;
        Ok(record)
    }

    pub fn system_reviews(&self) -> AppResult<Vec<SystemReviewRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, payload, snapshot, created_at FROM system_reviews
             ORDER BY created_at DESC, id ASC LIMIT 50",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut reviews = Vec::new();
        for row in rows {
            let (id, payload, snapshot, created_at) = row?;
            let input: SystemReviewInput = serde_json::from_str(&payload)?;
            reviews.push(SystemReviewRecord {
                id,
                period_label: input.period_label,
                adherence_score: input.adherence_score,
                process_summary: input.process_summary,
                rule_violations: input.rule_violations,
                lessons: input.lessons,
                next_actions: input.next_actions,
                next_review_date: input.next_review_date,
                snapshot: serde_json::from_str(&snapshot)?,
                created_at,
            });
        }
        Ok(reviews)
    }

    pub fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, asset_name, title, publisher, source_url, source_tier, evidence_type,
                    stance, as_of_date, claim, notes, active, captured_at
             FROM research_evidence
             ORDER BY active DESC, as_of_date DESC, captured_at DESC, id ASC",
        )?;
        let evidence = statement
            .query_map([], |row| {
                Ok(ResearchEvidence {
                    id: row.get(0)?,
                    asset_name: row.get(1)?,
                    title: row.get(2)?,
                    publisher: row.get(3)?,
                    source_url: row.get(4)?,
                    source_tier: row.get(5)?,
                    evidence_type: row.get(6)?,
                    stance: row.get(7)?,
                    as_of_date: row.get(8)?,
                    claim: row.get(9)?,
                    notes: row.get(10)?,
                    active: row.get::<_, i64>(11)? != 0,
                    captured_at: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(evidence)
    }

    pub fn add_research_evidence(
        &self,
        input: &ResearchEvidenceInput,
    ) -> AppResult<ResearchEvidence> {
        validate_research_evidence(input)?;
        let evidence = ResearchEvidence {
            id: Uuid::new_v4().to_string(),
            asset_name: input.asset_name.trim().into(),
            title: input.title.trim().into(),
            publisher: input.publisher.trim().into(),
            source_url: input.source_url.trim().into(),
            source_tier: input.source_tier.clone(),
            evidence_type: input.evidence_type.clone(),
            stance: input.stance.clone(),
            as_of_date: input.as_of_date.clone(),
            claim: input.claim.trim().into(),
            notes: input.notes.trim().into(),
            active: true,
            captured_at: Utc::now().to_rfc3339(),
        };
        self.conn()?.execute(
            "INSERT INTO research_evidence
             (id, asset_name, title, publisher, source_url, source_tier, evidence_type, stance,
              as_of_date, claim, notes, active, captured_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12)",
            params![
                evidence.id,
                evidence.asset_name,
                evidence.title,
                evidence.publisher,
                evidence.source_url,
                evidence.source_tier,
                evidence.evidence_type,
                evidence.stance,
                evidence.as_of_date,
                evidence.claim,
                evidence.notes,
                evidence.captured_at
            ],
        )?;
        Ok(evidence)
    }

    pub fn set_research_evidence_status(
        &self,
        id: &str,
        active: bool,
    ) -> AppResult<ResearchEvidence> {
        let affected = self.conn()?.execute(
            "UPDATE research_evidence SET active=?2 WHERE id=?1",
            params![id, i64::from(active)],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的研究证据".into()));
        }
        self.research_evidence()?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::Validation("找不到要更新的研究证据".into()))
    }

    pub fn model_config(&self) -> AppResult<ModelConfig> {
        let conn = self.conn()?;
        let value = |key: &str, fallback: &str| -> AppResult<String> {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                    row.get(0)
                })
                .optional()?
                .unwrap_or_else(|| fallback.into()))
        };
        Ok(ModelConfig {
            provider: value("model.provider", "openai-compatible")?,
            base_url: value("model.base_url", "https://api.openai.com/v1")?,
            model: value("model.name", "gpt-4.1-mini")?,
            has_api_key: crate::secrets::has_api_key(),
        })
    }

    pub fn save_model_metadata(
        &self,
        provider: &str,
        base_url: &str,
        model: &str,
    ) -> AppResult<()> {
        if provider != "openai-compatible" {
            return Err(AppError::Validation(
                "当前版本只支持 OpenAI-compatible 接口".into(),
            ));
        }
        if !base_url.starts_with("https://")
            && !base_url.starts_with("http://127.0.0.1")
            && !base_url.starts_with("http://localhost")
        {
            return Err(AppError::Validation(
                "模型地址必须使用 HTTPS；仅本机服务允许 HTTP".into(),
            ));
        }
        let conn = self.conn()?;
        for (key, value) in [
            ("model.provider", provider),
            ("model.base_url", base_url.trim_end_matches('/')),
            ("model.name", model),
        ] {
            conn.execute("INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        Ok(())
    }

    pub fn memories(&self) -> AppResult<Vec<MemoryItem>> {
        let preferences = self.memory_preference_map()?;
        let mut items = Vec::new();
        for (index, record) in self.decisions()?.into_iter().enumerate() {
            if index >= 50 && !preferences.contains_key(&record.id) {
                continue;
            }
            let (summary, occurred_at, status, reviewed, contradiction, review_payload) =
                if let Some(review) = &record.review {
                    (
                        format!(
                            "复盘结论：{}；经验修正：{}",
                            review.outcome_summary, review.lessons
                        ),
                        if review.reviewed_at.is_empty() {
                            record.created_at.clone()
                        } else {
                            review.reviewed_at.clone()
                        },
                        format!("复盘：{}", review.thesis_status),
                        true,
                        matches!(review.thesis_status.as_str(), "失效" | "部分成立"),
                        serde_json::to_value(review)?,
                    )
                } else {
                    (
                        format!(
                            "尚未复盘；原始置信度 {:.0}%，计划仓位 {:.1}%",
                            record.confidence_pct, record.position_pct
                        ),
                        record.created_at.clone(),
                        "待复盘的原始判断".into(),
                        false,
                        false,
                        serde_json::Value::Null,
                    )
                };
            items.push(MemoryItem {
                id: record.id,
                kind: "decision".into(),
                title: record.asset_name.clone(),
                summary,
                content: serde_json::json!({
                    "originalThesis": record.thesis,
                    "counterThesis": record.counter_thesis,
                    "invalidation": record.invalidation,
                    "confidencePct": record.confidence_pct,
                    "expectedReturnPct": record.expected_return_pct,
                    "downsidePct": record.downside_pct,
                    "positionPct": record.position_pct,
                    "reviewDate": record.review_date,
                    "sourceAnalysisId": record.source_analysis_id,
                    "sourceActionIndex": record.source_action_index,
                    "ruleChecks": record.rule_checks,
                    "review": review_payload,
                }),
                created_at: record.created_at,
                occurred_at,
                status: status.clone(),
                reviewed,
                contradiction,
                tags: vec![record.asset_name, "投资决策".into(), status],
                preference: "default".into(),
                preference_note: String::new(),
                preference_updated_at: None,
                selected: true,
                retrieval: None,
            });
        }
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, question, answer, trace, created_at FROM analyses ORDER BY created_at DESC",
        )?;
        for (index, item) in stmt
            .query_map([], |row| {
                let answer: String = row.get(2)?;
                let trace: Option<String> = row.get(3)?;
                let workflow_version = trace
                    .as_deref()
                    .and_then(|value| {
                        serde_json::from_str::<crate::models::AnalysisWorkflowTrace>(value).ok()
                    })
                    .map(|value| value.version)
                    .filter(|value| !value.is_empty());
                Ok(MemoryItem {
                    id: row.get(0)?,
                    kind: "analysis".into(),
                    title: row.get(1)?,
                    summary: truncate_chars(&answer, 180),
                    content: serde_json::json!({
                        "answer": answer,
                        "workflowVersion": workflow_version,
                    }),
                    created_at: row.get(4)?,
                    occurred_at: row.get(4)?,
                    status: "历史 AI 分析（未经结果验证）".into(),
                    reviewed: false,
                    contradiction: false,
                    tags: vec!["AI 分析".into(), "历史建议".into()],
                    preference: "default".into(),
                    preference_note: String::new(),
                    preference_updated_at: None,
                    selected: true,
                    retrieval: None,
                })
            })?
            .enumerate()
        {
            let item = item?;
            if index < 20 || preferences.contains_key(&item.id) {
                items.push(item);
            }
        }
        drop(stmt);
        drop(conn);
        for item in &mut items {
            if let Some((preference, note, updated_at)) = preferences.get(&item.id) {
                item.preference.clone_from(preference);
                item.preference_note.clone_from(note);
                item.preference_updated_at = Some(updated_at.clone());
                item.selected = preference != "hidden";
            }
        }
        Ok(items)
    }

    fn memory_preference_map(&self) -> AppResult<HashMap<String, (String, String, String)>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT memory_id, preference, note, updated_at FROM memory_preferences ORDER BY memory_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        Ok(rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, preference, note, updated_at)| (id, (preference, note, updated_at)))
            .collect())
    }

    pub fn save_memory_preference(
        &self,
        id: &str,
        input: &MemoryPreferenceInput,
    ) -> AppResult<MemoryItem> {
        if id.trim().is_empty() || id.chars().count() > 128 {
            return Err(AppError::Validation("长期记忆 ID 无效".into()));
        }
        if !matches!(input.preference.as_str(), "default" | "pinned" | "hidden") {
            return Err(AppError::Validation("长期记忆偏好无效".into()));
        }
        if input.note.chars().count() > 1_000 {
            return Err(AppError::Validation(
                "长期记忆备注不能超过 1000 个字符".into(),
            ));
        }
        if !self.memories()?.iter().any(|item| item.id == id) {
            return Err(AppError::NotFound("找不到要管理的长期记忆".into()));
        }

        if input.preference == "default" {
            self.conn()?
                .execute("DELETE FROM memory_preferences WHERE memory_id=?1", [id])?;
        } else {
            self.conn()?.execute(
                "INSERT INTO memory_preferences (memory_id, preference, note, updated_at)
                 VALUES (?1,?2,?3,?4)
                 ON CONFLICT(memory_id) DO UPDATE SET
                   preference=excluded.preference,
                   note=excluded.note,
                   updated_at=excluded.updated_at",
                params![
                    id,
                    input.preference,
                    input.note.trim(),
                    Utc::now().to_rfc3339()
                ],
            )?;
        }
        self.memories()?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::NotFound("找不到要管理的长期记忆".into()))
    }

    pub fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO analyses (id, question, answer, audit, trace, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                result.id,
                question,
                result.answer,
                serde_json::to_string(&result.transparency)?,
                serde_json::to_string(&result.workflow_trace)?,
                result.created_at
            ],
        )?;
        Ok(())
    }

    pub fn analysis_history(&self) -> AppResult<Vec<AnalysisHistoryItem>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, question, created_at, audit, trace FROM analyses ORDER BY created_at DESC LIMIT 50",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut history = Vec::new();
        for row in rows {
            let (id, question, created_at, audit, trace) = row?;
            let parsed_trace = trace.as_deref().and_then(|value| {
                serde_json::from_str::<crate::models::AnalysisWorkflowTrace>(value).ok()
            });
            history.push(AnalysisHistoryItem {
                id,
                question,
                created_at,
                transparency: audit
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
                workflow_version: parsed_trace
                    .as_ref()
                    .map(|item| item.version.clone())
                    .filter(|value| !value.is_empty()),
                verdict: parsed_trace
                    .and_then(|item| item.structured_report)
                    .map(|report| report.verdict),
            });
        }
        Ok(history)
    }

    pub fn analysis(&self, id: &str) -> AppResult<StoredAnalysis> {
        if id.trim().is_empty() || id.len() > 128 {
            return Err(AppError::Validation("分析 ID 无效".into()));
        }
        let row = self
            .conn()?
            .query_row(
                "SELECT id, question, answer, created_at, audit, trace FROM analyses WHERE id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("找不到这条历史 AI 分析".into()))?;
        Ok(StoredAnalysis {
            id: row.0,
            question: row.1,
            answer: row.2,
            created_at: row.3,
            transparency: row
                .4
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok()),
            workflow_trace: row
                .5
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok()),
        })
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut result = value.chars().take(max).collect::<String>();
    if value.chars().count() > max {
        result.push('…');
    }
    result
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn build_snapshot(
    profile: FinancialProfile,
    goals: Vec<Goal>,
    holdings: Vec<Holding>,
    holding_valuations: Vec<HoldingValuationEvidence>,
    updated_at: String,
) -> Snapshot {
    let valuation_status = valuation::status(&profile.base_currency, &holdings);
    let normalized_holdings = if valuation_status.comparable {
        holdings
            .iter()
            .filter_map(|holding| valuation::normalize_holding(holding, &profile.base_currency))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let raw_total = normalized_holdings
        .iter()
        .map(|holding| holding.market_value)
        .sum::<f64>();
    let total_value = if raw_total.abs() < f64::EPSILON {
        0.0
    } else {
        raw_total
    };
    let emergency_months = if profile.monthly_expense > 0.0 {
        profile.emergency_fund / profile.monthly_expense
    } else {
        0.0
    };
    let largest = normalized_holdings
        .iter()
        .map(|h| h.market_value)
        .fold(0.0_f64, f64::max);
    let concentration_pct = if total_value > 0.0 {
        largest / total_value * 100.0
    } else {
        0.0
    };
    let mut findings = risk::analyze(
        &profile,
        if valuation_status.comparable {
            &normalized_holdings
        } else {
            &holdings
        },
        valuation_status.comparable,
    );
    if !valuation_status.missing_fx_holdings.is_empty() {
        findings.push(crate::models::RiskFinding {
            level: "high".into(),
            title: "组合缺少外币折算汇率".into(),
            detail: format!(
                "{} 尚未折算为基准币种 {}，组合总值、集中度和规划已暂停。",
                valuation_status.missing_fx_holdings.join("、"),
                valuation_status.base_currency
            ),
            action: "补充估值日对应的汇率；不要把不同币种的原始金额直接相加。".into(),
        });
    }
    if valuation_status.undated_holding_count > 0 || valuation_status.valuation_dates.len() > 1 {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "持仓估值日期尚未对齐".into(),
            detail: if valuation_status.undated_holding_count > 0 {
                format!(
                    "有 {} 项持仓缺少估值日期，不能建立可靠的周期比较基线。",
                    valuation_status.undated_holding_count
                )
            } else {
                format!(
                    "当前持仓使用了 {} 个不同估值日期，组合变化可能混入时间错位。",
                    valuation_status.valuation_dates.len()
                )
            },
            action: "把全部持仓更新到同一估值日后，再冻结组合检查点。".into(),
        });
    }
    let verified_ids = holding_valuations
        .iter()
        .map(|valuation| valuation.holding_id.as_str())
        .collect::<HashSet<_>>();
    let unverified_holdings = holdings
        .iter()
        .filter(|holding| !verified_ids.contains(holding.id.as_str()))
        .map(|holding| holding.name.as_str())
        .collect::<Vec<_>>();
    if !unverified_holdings.is_empty() {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "部分持仓仍是用户声明估值".into(),
            detail: format!(
                "{} 没有冻结数量、单位价格与外部价格来源；组合计算可继续，但不能视为已核验业绩。",
                unverified_holdings.join("、")
            ),
            action: "对有公开代码的证券查询并采用指定估值日收盘价；现金和非上市资产继续保留人工口径说明。".into(),
        });
    }
    let mut plan = planning::analyze(&profile, &goals, &normalized_holdings);
    if !valuation_status.comparable {
        plan.goal_projections.clear();
        plan.rebalancing.clear();
        plan.assumptions = format!(
            "存在未折算到 {} 的外币持仓，组合风险、目标路径和再平衡计算已暂停。",
            valuation_status.base_currency
        );
    } else if !holdings.is_empty() {
        plan.assumptions = format!(
            "组合数值已按 {} 折算；{}/{} 项持仓冻结了带来源证券价格，其余市值、汇率与日期仍依赖用户确认。{}",
            valuation_status.base_currency,
            holding_valuations.len(),
            holdings.len(),
            plan.assumptions
        );
    }
    let target_total: f64 = holdings.iter().map(|holding| holding.target_pct).sum();
    if target_total > 0.0 && !(99.0..=101.0).contains(&target_total) {
        findings.push(crate::models::RiskFinding {
            level: "medium".into(),
            title: "目标权重尚未闭合".into(),
            detail: format!(
                "当前持仓目标权重合计为 {target_total:.1}%，完成到 100% 后才能计算再平衡动作。"
            ),
            action: "检查每项持仓目标权重，避免无意中放大或遗漏风险预算。".into(),
        });
    }
    if plan.committed_monthly > plan.monthly_surplus
        && plan.committed_monthly > 0.0
        && (profile.monthly_income > 0.0 || profile.monthly_expense > 0.0)
    {
        findings.push(crate::models::RiskFinding {
            level: "high".into(),
            title: "目标投入超过月度结余".into(),
            detail: format!(
                "计划每月投入 {:.0} {}，但当前月度结余约 {:.0} {}。",
                plan.committed_monthly,
                profile.base_currency,
                plan.monthly_surplus,
                profile.base_currency
            ),
            action: "调整目标优先级、期限或月度投入，避免计划依赖新增负债。".into(),
        });
    }
    Snapshot {
        profile,
        goals,
        holdings,
        holding_valuations,
        findings,
        total_value,
        emergency_months,
        concentration_pct,
        valuation_status,
        plan,
        updated_at,
    }
}

fn validate_non_negative(values: &[f64]) -> AppResult<()> {
    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(AppError::Validation(
            "金额、比例和期限必须是有效的非负数".into(),
        ));
    }
    Ok(())
}

fn validate_currency(value: &str) -> AppResult<()> {
    let currency = value.trim();
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(AppError::Validation(
            "币种必须使用三个英文字母，例如 CNY、USD 或 HKD".into(),
        ));
    }
    Ok(())
}

fn portfolio_event_type_to_db(event_type: &PortfolioEventType) -> &'static str {
    match event_type {
        PortfolioEventType::Deposit => "deposit",
        PortfolioEventType::Withdrawal => "withdrawal",
        PortfolioEventType::Dividend => "dividend",
        PortfolioEventType::Interest => "interest",
        PortfolioEventType::Fee => "fee",
        PortfolioEventType::Tax => "tax",
        PortfolioEventType::Buy => "buy",
        PortfolioEventType::Sell => "sell",
    }
}

fn portfolio_event_type_from_db(value: &str) -> AppResult<PortfolioEventType> {
    match value {
        "deposit" => Ok(PortfolioEventType::Deposit),
        "withdrawal" => Ok(PortfolioEventType::Withdrawal),
        "dividend" => Ok(PortfolioEventType::Dividend),
        "interest" => Ok(PortfolioEventType::Interest),
        "fee" => Ok(PortfolioEventType::Fee),
        "tax" => Ok(PortfolioEventType::Tax),
        "buy" => Ok(PortfolioEventType::Buy),
        "sell" => Ok(PortfolioEventType::Sell),
        _ => Err(AppError::Validation("流水类型无效".into())),
    }
}

fn portfolio_event_record_from_row(row: &Row<'_>) -> rusqlite::Result<PortfolioEventRecord> {
    let event_type = portfolio_event_type_from_db(row.get::<_, String>(1)?.as_str())
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(PortfolioEventRecord {
        id: row.get(0)?,
        event_type,
        source: row.get(2)?,
        external_id: row.get(3)?,
        asset_name: row.get(4)?,
        amount: row.get(5)?,
        currency: row.get(6)?,
        fx_rate_to_base: row.get(7)?,
        fx_rate_source: row.get(8)?,
        fx_rate_observed_on: row.get(9)?,
        base_currency: row.get(10)?,
        base_amount: row.get(11)?,
        occurred_on: row.get(12)?,
        note: row.get(13)?,
        reversal_of_event_id: row.get(14)?,
        reversed_by_event_id: row.get(15)?,
        created_at: row.get(16)?,
    })
}

fn portfolio_event_record(
    input: &PortfolioEventInput,
    base_currency: &str,
) -> AppResult<PortfolioEventRecord> {
    let currency = input.currency.trim().to_ascii_uppercase();
    let fx_rate_to_base = if currency == base_currency {
        None
    } else {
        input.fx_rate_to_base
    };
    let base_amount = input.amount * fx_rate_to_base.unwrap_or(1.0);
    if !base_amount.is_finite() || base_amount > 1e15 {
        return Err(AppError::Validation("流水折算金额超出有效范围".into()));
    }
    let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
        &currency,
        base_currency,
        fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )?;
    Ok(PortfolioEventRecord {
        id: Uuid::new_v4().to_string(),
        event_type: input.event_type.clone(),
        source: if input.source.trim().is_empty() {
            "manual".into()
        } else {
            input.source.trim().into()
        },
        external_id: input.external_id.trim().into(),
        asset_name: input.asset_name.trim().into(),
        amount: input.amount,
        currency,
        fx_rate_to_base,
        fx_rate_source,
        fx_rate_observed_on,
        base_currency: base_currency.into(),
        base_amount,
        occurred_on: input.occurred_on.trim().into(),
        note: input.note.trim().into(),
        reversal_of_event_id: None,
        reversed_by_event_id: None,
        created_at: Utc::now().to_rfc3339(),
    })
}

fn portfolio_event_fingerprint(input: &PortfolioEventInput) -> String {
    portfolio_event_identity_fingerprint(&input.source, &input.external_id)
}

fn portfolio_event_identity_fingerprint(source: &str, external_id: &str) -> String {
    if external_id.trim().is_empty() {
        return String::new();
    }
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\u{1f}{}",
                source.trim().to_ascii_lowercase(),
                external_id.trim()
            )
            .as_bytes()
        )
    )
}

fn validate_synced_event_identities(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 5 {
        return Ok(());
    }
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    let mut seen = HashSet::new();
    for row in &table.rows {
        let SyncValue::Text(source) = &row[2] else {
            return Err(AppError::Validation("同步流水来源字段类型无效".into()));
        };
        let SyncValue::Text(external_id) = &row[3] else {
            return Err(AppError::Validation("同步流水交易 ID 字段类型无效".into()));
        };
        let SyncValue::Text(fingerprint) = &row[4] else {
            return Err(AppError::Validation("同步流水去重指纹字段类型无效".into()));
        };
        if (!external_id.is_empty() && source.trim().is_empty())
            || source.chars().count() > 120
            || external_id.chars().count() > 200
        {
            return Err(AppError::Validation("同步流水导入标识无效".into()));
        }
        let expected = portfolio_event_identity_fingerprint(source, external_id);
        if fingerprint != &expected {
            return Err(AppError::Validation("同步流水去重指纹不匹配".into()));
        }
        if !fingerprint.is_empty() && !seen.insert(fingerprint) {
            return Err(AppError::Validation("同步数据包含重复流水交易 ID".into()));
        }
    }
    Ok(())
}

fn sync_text<'a>(row: &'a [SyncValue], index: usize, label: &str) -> AppResult<&'a str> {
    match row.get(index) {
        Some(SyncValue::Text(value)) => Ok(value),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

fn sync_optional_real(row: &[SyncValue], index: usize, label: &str) -> AppResult<Option<f64>> {
    match row.get(index) {
        Some(SyncValue::Null) => Ok(None),
        Some(SyncValue::Real(value)) => Ok(Some(*value)),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

fn validate_synced_fx_row(
    row: &[SyncValue],
    currency_index: usize,
    rate_index: usize,
    source_index: usize,
    observed_index: usize,
    base_currency: &str,
    effective_index: usize,
) -> AppResult<()> {
    let currency = sync_text(row, currency_index, "币种")?;
    let rate = sync_optional_real(row, rate_index, "汇率")?;
    let source = sync_text(row, source_index, "汇率来源")?;
    let observed_on = sync_text(row, observed_index, "汇率观察日")?;
    let effective_on = sync_text(row, effective_index, "估值或流水日期")?;
    if currency.eq_ignore_ascii_case(base_currency) && rate.is_some() {
        return Err(AppError::Validation(
            "同步数据中的同币种记录不能携带折算汇率".into(),
        ));
    }
    let canonical = normalized_fx_provenance(
        currency,
        base_currency,
        rate,
        source,
        observed_on,
        effective_on,
    )?;
    if canonical.0 != source || canonical.1 != observed_on {
        return Err(AppError::Validation("同步汇率来源不是规范形式".into()));
    }
    Ok(())
}

fn validate_synced_fx_provenance(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 6 {
        return Ok(());
    }
    let profile_table = dataset
        .tables
        .iter()
        .find(|table| table.name == "profile")
        .ok_or_else(|| AppError::Validation("数据快照缺少 profile".into()))?;
    let profile = match profile_table.rows.first() {
        Some(profile_row) => {
            serde_json::from_str::<FinancialProfile>(sync_text(profile_row, 1, "财务档案")?)
                .map_err(|_| AppError::Validation("同步财务档案格式无效".into()))?
        }
        None => FinancialProfile::default(),
    };

    let holdings = dataset
        .tables
        .iter()
        .find(|table| table.name == "holdings")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holdings".into()))?;
    for row in &holdings.rows {
        validate_synced_fx_row(row, 7, 8, 10, 11, &profile.base_currency, 9)?;
    }

    let events = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    for row in &events.rows {
        let base_currency = sync_text(row, 11, "流水基准币种")?;
        validate_synced_fx_row(row, 7, 8, 9, 10, base_currency, 13)?;
    }
    Ok(())
}

fn validate_synced_event_reversals(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 7 {
        return Ok(());
    }
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_events")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_events".into()))?;
    let rows_by_id = table
        .rows
        .iter()
        .map(|row| Ok((sync_text(row, 0, "流水 ID")?, row)))
        .collect::<AppResult<HashMap<_, _>>>()?;
    let mut reversed_targets = HashSet::new();

    for row in &table.rows {
        let target_id = match row.get(16) {
            Some(SyncValue::Null) => continue,
            Some(SyncValue::Text(value)) if !value.trim().is_empty() => value,
            _ => return Err(AppError::Validation("同步流水冲正关联无效".into())),
        };
        let event_id = sync_text(row, 0, "流水 ID")?;
        if target_id == event_id || !reversed_targets.insert(target_id.as_str()) {
            return Err(AppError::Validation(
                "同步数据包含重复或自引用的流水冲正".into(),
            ));
        }
        let target = rows_by_id
            .get(target_id.as_str())
            .ok_or_else(|| AppError::Validation("同步流水冲正找不到原记录".into()))?;
        if !matches!(target.get(16), Some(SyncValue::Null)) {
            return Err(AppError::Validation(
                "同步数据不能对冲正记录再次冲正".into(),
            ));
        }
        let amount = sync_real(row, 6, "冲正金额")?;
        let target_amount = sync_real(target, 6, "原流水金额")?;
        let base_amount = sync_real(row, 12, "冲正折算金额")?;
        let target_base_amount = sync_real(target, 12, "原流水折算金额")?;
        if amount >= 0.0
            || amount != -target_amount
            || base_amount != -target_base_amount
            || row[1] != target[1]
            || row[5] != target[5]
            || row[7] != target[7]
            || row[8] != target[8]
            || row[9] != target[9]
            || row[10] != target[10]
            || row[11] != target[11]
            || sync_text(row, 13, "冲正日期")? < sync_text(target, 13, "原流水日期")?
        {
            return Err(AppError::Validation(
                "同步流水冲正内容与原记录不匹配".into(),
            ));
        }
    }
    Ok(())
}

fn validate_synced_memory_preferences(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 8 {
        return Ok(());
    }
    let source_ids = dataset
        .tables
        .iter()
        .filter(|table| matches!(table.name.as_str(), "decisions" | "analyses"))
        .flat_map(|table| table.rows.iter())
        .map(|row| sync_text(row, 0, "记忆来源 ID"))
        .collect::<AppResult<HashSet<_>>>()?;
    let table = dataset
        .tables
        .iter()
        .find(|table| table.name == "memory_preferences")
        .ok_or_else(|| AppError::Validation("数据快照缺少 memory_preferences".into()))?;
    let mut seen = HashSet::new();
    for row in &table.rows {
        let memory_id = sync_text(row, 0, "长期记忆 ID")?;
        let preference = sync_text(row, 1, "长期记忆偏好")?;
        let note = sync_text(row, 2, "长期记忆备注")?;
        let updated_at = sync_text(row, 3, "长期记忆更新时间")?;
        if memory_id.trim().is_empty()
            || memory_id.chars().count() > 128
            || !seen.insert(memory_id)
            || !source_ids.contains(memory_id)
            || !matches!(preference, "pinned" | "hidden")
            || note.chars().count() > 1_000
            || chrono::DateTime::parse_from_rfc3339(updated_at).is_err()
        {
            return Err(AppError::Validation("同步长期记忆偏好无效".into()));
        }
    }
    Ok(())
}

fn validate_synced_holding_valuations(dataset: &SyncDataset) -> AppResult<()> {
    if dataset.schema_version < 9 {
        return Ok(());
    }
    let holdings = dataset
        .tables
        .iter()
        .find(|table| table.name == "holdings")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holdings".into()))?;
    let holdings_by_id = holdings
        .rows
        .iter()
        .map(|row| Ok((sync_text(row, 0, "持仓 ID")?, row)))
        .collect::<AppResult<HashMap<_, _>>>()?;
    let valuations = dataset
        .tables
        .iter()
        .find(|table| table.name == "holding_valuations")
        .ok_or_else(|| AppError::Validation("数据快照缺少 holding_valuations".into()))?;
    let mut seen = HashSet::new();
    for row in &valuations.rows {
        let holding_id = sync_text(row, 0, "估值持仓 ID")?;
        if !seen.insert(holding_id) {
            return Err(AppError::Validation("同步数据包含重复持仓估值".into()));
        }
        let quantity = sync_real(row, 2, "持仓数量")?;
        let market_value = sync_real(row, 4, "持仓估值")?;
        let staleness = sync_real(row, 8, "价格回退天数")?;
        if staleness.fract() != 0.0 {
            return Err(AppError::Validation("同步价格回退天数无效".into()));
        }
        let quote = SecurityPriceQuote {
            symbol: sync_text(row, 1, "行情代码")?.into(),
            currency: sync_text(row, 5, "行情币种")?.into(),
            close: sync_real(row, 3, "单位价格")?,
            requested_on: sync_text(row, 6, "请求日期")?.into(),
            observed_on: sync_text(row, 7, "观察日期")?.into(),
            staleness_days: staleness as i64,
            provider_code: sync_text(row, 9, "价格来源代码")?.into(),
            provider_name: sync_text(row, 10, "价格来源名称")?.into(),
            exchange: sync_text(row, 11, "交易所")?.into(),
            mic_code: sync_text(row, 12, "MIC")?.into(),
            instrument_type: sync_text(row, 13, "证券类型")?.into(),
            price_basis: sync_text(row, 14, "价格口径")?.into(),
            source_url: sync_text(row, 15, "价格来源地址")?.into(),
            methodology_url: sync_text(row, 16, "价格方法地址")?.into(),
            disclaimer: sync_text(row, 17, "价格限制")?.into(),
        };
        validate_holding_valuation_evidence(quantity, &quote)?;
        let expected_market_value = quantity * quote.close;
        if (market_value - expected_market_value).abs()
            > 0.005_f64.max(expected_market_value.abs() * 1e-10)
        {
            return Err(AppError::Validation(
                "同步持仓估值与数量、单位价格不一致".into(),
            ));
        }
        chrono::DateTime::parse_from_rfc3339(sync_text(row, 18, "行情采集时间")?)
            .map_err(|_| AppError::Validation("同步行情采集时间无效".into()))?;
        let holding = holdings_by_id
            .get(holding_id)
            .ok_or_else(|| AppError::Validation("同步持仓估值找不到对应持仓".into()))?;
        if sync_text(holding, 1, "持仓代码")? != quote.symbol
            || !sync_text(holding, 7, "持仓币种")?.eq_ignore_ascii_case(&quote.currency)
            || sync_text(holding, 9, "持仓估值日")? != quote.requested_on
            || (sync_real(holding, 4, "持仓市值")? - market_value).abs() > 0.005
        {
            return Err(AppError::Validation("同步持仓与价格估值证据不一致".into()));
        }
    }

    let checkins = dataset
        .tables
        .iter()
        .find(|table| table.name == "portfolio_checkins")
        .ok_or_else(|| AppError::Validation("数据快照缺少 portfolio_checkins".into()))?;
    for row in &checkins.rows {
        let record: PortfolioCheckInRecord = serde_json::from_str(sync_text(row, 1, "组合检查点")?)
            .map_err(|_| AppError::Validation("同步组合检查点格式无效".into()))?;
        for evidence in &record.holding_valuations {
            validate_stored_holding_valuation(evidence)?;
            let holding = record
                .holdings
                .iter()
                .find(|holding| holding.id == evidence.holding_id)
                .ok_or_else(|| AppError::Validation("检查点行情证据找不到对应持仓".into()))?;
            if holding.symbol != evidence.symbol
                || !holding.currency.eq_ignore_ascii_case(&evidence.currency)
                || holding.valuation_date != evidence.requested_on
                || (holding.market_value - evidence.market_value).abs() > 0.005
            {
                return Err(AppError::Validation("检查点持仓与行情证据不一致".into()));
            }
        }
    }
    Ok(())
}

fn sync_real(row: &[SyncValue], index: usize, label: &str) -> AppResult<f64> {
    match row.get(index) {
        Some(SyncValue::Real(value)) => Ok(*value),
        Some(SyncValue::Integer(value)) => Ok(*value as f64),
        _ => Err(AppError::Validation(format!("同步{label}字段类型无效"))),
    }
}

fn portfolio_event_content_hash(record: &PortfolioEventRecord) -> AppResult<String> {
    let canonical = serde_json::json!({
        "eventType": record.event_type,
        "source": record.source.trim().to_ascii_lowercase(),
        "externalId": record.external_id.trim(),
        "assetName": record.asset_name.trim(),
        "amount": record.amount,
        "currency": record.currency.trim().to_ascii_uppercase(),
        "fxRateToBase": record.fx_rate_to_base,
        "fxRateSource": record.fx_rate_source.trim(),
        "fxRateObservedOn": record.fx_rate_observed_on.trim(),
        "baseCurrency": record.base_currency.trim().to_ascii_uppercase(),
        "baseAmount": record.base_amount,
        "occurredOn": record.occurred_on.trim(),
        "note": record.note.trim(),
        "reversalOfEventId": record.reversal_of_event_id,
    });
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

fn portfolio_event_import_row(
    row_number: usize,
    input: &PortfolioEventInput,
    status: &str,
    message: String,
    base_currency: &str,
) -> PortfolioEventImportRow {
    let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )
    .unwrap_or_else(|_| {
        (
            input.fx_rate_source.trim().into(),
            input.fx_rate_observed_on.trim().into(),
        )
    });
    PortfolioEventImportRow {
        row_number,
        status: status.into(),
        message,
        source: input.source.trim().into(),
        external_id: input.external_id.trim().into(),
        event_type: portfolio_event_type_to_db(&input.event_type).into(),
        occurred_on: input.occurred_on.trim().into(),
        amount: Some(input.amount),
        currency: input.currency.trim().to_ascii_uppercase(),
        fx_rate_to_base: input.fx_rate_to_base,
        fx_rate_source,
        fx_rate_observed_on,
        asset_name: input.asset_name.trim().into(),
        note: input.note.trim().into(),
    }
}

fn portfolio_import_revision(
    rows: &[PortfolioEventImportRow],
    base_currency: &str,
    frozen_through: &str,
    csv_text: &str,
) -> AppResult<String> {
    let canonical = serde_json::json!({
        "rows": rows,
        "baseCurrency": base_currency,
        "frozenThrough": frozen_through,
        "csvSha256": format!("{:x}", Sha256::digest(csv_text.as_bytes())),
    });
    Ok(format!(
        "import-{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

fn insert_portfolio_event(
    connection: &Connection,
    record: &PortfolioEventRecord,
    fingerprint: &str,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO portfolio_events
         (id, event_type, source, external_id, fingerprint, asset_name, amount, currency,
          fx_rate_to_base, fx_rate_source, fx_rate_observed_on, base_currency, base_amount,
          occurred_on, note, created_at, reversal_of_event_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            record.id,
            portfolio_event_type_to_db(&record.event_type),
            record.source,
            record.external_id,
            fingerprint,
            record.asset_name,
            record.amount,
            record.currency,
            record.fx_rate_to_base,
            record.fx_rate_source,
            record.fx_rate_observed_on,
            record.base_currency,
            record.base_amount,
            record.occurred_on,
            record.note,
            record.created_at,
            record.reversal_of_event_id,
        ],
    )?;
    Ok(())
}

fn normalized_fx_provenance(
    currency: &str,
    base_currency: &str,
    fx_rate_to_base: Option<f64>,
    source: &str,
    observed_on: &str,
    effective_on: &str,
) -> AppResult<(String, String)> {
    if currency.trim().eq_ignore_ascii_case(base_currency.trim()) {
        return Ok((String::new(), String::new()));
    }
    if fx_rate_to_base.is_none_or(|rate| !rate.is_finite() || rate <= 0.0 || rate > 1e9) {
        return Err(AppError::Validation("外币折算汇率必须是有效正数".into()));
    }
    let source = if source.trim().is_empty() {
        "user_declared"
    } else {
        source.trim()
    };
    if source.chars().count() > 120 {
        return Err(AppError::Validation("汇率来源不能超过 120 个字符".into()));
    }
    let effective_date = NaiveDate::parse_from_str(effective_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("估值或流水日期必须使用 YYYY-MM-DD".into()))?;
    let observed_on = if observed_on.trim().is_empty() {
        effective_on.trim()
    } else {
        observed_on.trim()
    };
    let observed_date = NaiveDate::parse_from_str(observed_on, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("汇率观察日期必须使用 YYYY-MM-DD".into()))?;
    if observed_date > effective_date {
        return Err(AppError::Validation(
            "汇率观察日期不能晚于对应的估值或流水日期".into(),
        ));
    }
    if observed_date > Local::now().date_naive() {
        return Err(AppError::Validation("汇率观察日期不能晚于今天".into()));
    }
    Ok((source.into(), observed_on.into()))
}

fn validate_portfolio_event(input: &PortfolioEventInput, base_currency: &str) -> AppResult<()> {
    if !input.amount.is_finite() || input.amount <= 0.0 || input.amount > 1e15 {
        return Err(AppError::Validation(
            "流水金额必须是大于 0 的有效数字".into(),
        ));
    }
    let currency = input.currency.trim();
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(AppError::Validation("流水币种必须是 3 位字母代码".into()));
    }
    if !currency.eq_ignore_ascii_case(base_currency)
        && !input
            .fx_rate_to_base
            .is_some_and(|rate| rate.is_finite() && rate > 0.0 && rate <= 1e9)
    {
        return Err(AppError::Validation(
            "外币流水必须填写折算到当前基准币种的有效汇率".into(),
        ));
    }
    let occurred_on = NaiveDate::parse_from_str(input.occurred_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("流水日期必须使用 YYYY-MM-DD".into()))?;
    if occurred_on > Local::now().date_naive() {
        return Err(AppError::Validation("流水日期不能晚于今天".into()));
    }
    if input.note.trim().is_empty() {
        return Err(AppError::Validation(
            "流水说明为必填项，便于未来核对".into(),
        ));
    }
    let source = input.source.trim();
    let external_id = input.external_id.trim();
    if source.is_empty() && !external_id.is_empty() {
        return Err(AppError::Validation(
            "填写来源交易 ID 时必须同时填写流水来源".into(),
        ));
    }
    if matches!(
        input.event_type,
        PortfolioEventType::Buy
            | PortfolioEventType::Sell
            | PortfolioEventType::Dividend
            | PortfolioEventType::Interest
    ) && input.asset_name.trim().is_empty()
    {
        return Err(AppError::Validation(
            "买卖、分红或利息流水必须填写关联资产".into(),
        ));
    }
    if source.chars().count() > 120
        || external_id.chars().count() > 200
        || input.asset_name.chars().count() > 200
        || input.note.chars().count() > 2_000
    {
        return Err(AppError::Validation(
            "流水来源、交易 ID、资产名称或说明过长".into(),
        ));
    }
    normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )?;
    Ok(())
}

fn validate_holding(input: &HoldingInput, base_currency: &str) -> AppResult<()> {
    if input.name.trim().is_empty() || input.market_value <= 0.0 {
        return Err(AppError::Validation("资产名称和正数市值为必填项".into()));
    }
    validate_non_negative(&[input.market_value, input.cost_basis, input.target_pct])?;
    validate_percentage(input.target_pct, "目标权重")?;
    validate_currency(&input.currency)?;
    let valuation_date = NaiveDate::parse_from_str(input.valuation_date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("持仓估值日期必须使用 YYYY-MM-DD".into()))?;
    if valuation_date > Local::now().date_naive() {
        return Err(AppError::Validation("持仓估值日期不能晚于今天".into()));
    }
    if input.currency.eq_ignore_ascii_case(base_currency) {
        if input
            .fx_rate_to_base
            .is_some_and(|rate| !rate.is_finite() || rate <= 0.0)
        {
            return Err(AppError::Validation("汇率必须是有效正数".into()));
        }
    } else if input
        .fx_rate_to_base
        .is_none_or(|rate| !rate.is_finite() || rate <= 0.0)
    {
        return Err(AppError::Validation(format!(
            "{} 持仓必须填写折算到基准币种 {} 的汇率",
            input.currency.trim().to_ascii_uppercase(),
            base_currency.trim().to_ascii_uppercase()
        )));
    }
    normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.valuation_date,
    )?;
    Ok(())
}

fn validate_holding_valuation_evidence(quantity: f64, quote: &SecurityPriceQuote) -> AppResult<()> {
    if !quantity.is_finite() || quantity <= 0.0 || quantity > 1e15 {
        return Err(AppError::Validation("持仓数量必须是有效正数".into()));
    }
    if !quote.close.is_finite() || quote.close <= 0.0 || quote.close > 1e15 {
        return Err(AppError::Validation("证券单位价格必须是有效正数".into()));
    }
    let requested_on = NaiveDate::parse_from_str(quote.requested_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("证券价格请求日期无效".into()))?;
    let observed_on = NaiveDate::parse_from_str(quote.observed_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("证券价格观察日期无效".into()))?;
    if requested_on > Local::now().date_naive()
        || observed_on > requested_on
        || (requested_on - observed_on).num_days() != quote.staleness_days
        || !(0..=10).contains(&quote.staleness_days)
    {
        return Err(AppError::Validation("证券价格日期或回退天数不一致".into()));
    }
    validate_currency(&quote.currency)?;
    let source_url = reqwest::Url::parse(&quote.source_url)
        .map_err(|_| AppError::Validation("证券价格来源地址无效".into()))?;
    let source_symbol = source_url
        .query_pairs()
        .find(|(key, _)| key == "symbol")
        .map(|(_, value)| value.into_owned());
    let source_is_canonical = source_url.scheme() == "https"
        && source_url.host_str() == Some("api.twelvedata.com")
        && source_url.path() == "/time_series"
        && source_url.username().is_empty()
        && source_url.password().is_none()
        && source_symbol.as_deref() == Some(quote.symbol.as_str())
        && !source_url
            .query_pairs()
            .any(|(key, _)| key.eq_ignore_ascii_case("apikey"));
    if quote.symbol.trim().is_empty()
        || quote.symbol.chars().count() > 80
        || quote.provider_code != "twelve_data_raw_close"
        || quote.provider_name != "Twelve Data"
        || quote.price_basis != "unadjusted_daily_close"
        || !source_is_canonical
        || quote.methodology_url != "https://twelvedata.com/docs/market-data/time-series"
        || quote.disclaimer.trim().is_empty()
        || quote.exchange.chars().count() > 120
        || quote.mic_code.chars().count() > 32
        || quote.instrument_type.chars().count() > 120
    {
        return Err(AppError::Validation(
            "证券价格来源或元数据不是受支持的规范形式".into(),
        ));
    }
    Ok(())
}

fn validate_stored_holding_valuation(evidence: &HoldingValuationEvidence) -> AppResult<()> {
    let quote = SecurityPriceQuote {
        symbol: evidence.symbol.clone(),
        currency: evidence.currency.clone(),
        close: evidence.unit_price,
        requested_on: evidence.requested_on.clone(),
        observed_on: evidence.observed_on.clone(),
        staleness_days: evidence.staleness_days,
        provider_code: evidence.provider_code.clone(),
        provider_name: evidence.provider_name.clone(),
        exchange: evidence.exchange.clone(),
        mic_code: evidence.mic_code.clone(),
        instrument_type: evidence.instrument_type.clone(),
        price_basis: evidence.price_basis.clone(),
        source_url: evidence.source_url.clone(),
        methodology_url: evidence.methodology_url.clone(),
        disclaimer: evidence.disclaimer.clone(),
    };
    validate_holding_valuation_evidence(evidence.quantity, &quote)?;
    let expected = evidence.quantity * evidence.unit_price;
    if (evidence.market_value - expected).abs() > 0.005_f64.max(expected.abs() * 1e-10)
        || chrono::DateTime::parse_from_rfc3339(&evidence.captured_at).is_err()
    {
        return Err(AppError::Validation("持仓估值证据内容无效".into()));
    }
    Ok(())
}

fn validate_and_canonicalize_rule_checks(
    checks: &[DecisionRuleCheck],
    active_rules: &[InvestmentRule],
) -> AppResult<Vec<DecisionRuleCheck>> {
    if checks.len() != active_rules.len() {
        return Err(AppError::Validation(
            "请逐条确认当前所有有效投资规则后再冻结决策".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut canonical = Vec::with_capacity(active_rules.len());
    for rule in active_rules {
        let check = checks
            .iter()
            .find(|check| check.rule_id == rule.id)
            .ok_or_else(|| AppError::Validation("缺少当前有效规则的确认结果".into()))?;
        if !seen.insert(check.rule_id.as_str()) {
            return Err(AppError::Validation("投资规则确认不能重复".into()));
        }
        if check.rule_revision != rule.revision {
            return Err(AppError::Validation(
                "投资规则已修订，请刷新并按最新版本重新确认".into(),
            ));
        }
        if !matches!(check.status.as_str(), "遵守" | "偏离" | "不适用") {
            return Err(AppError::Validation(
                "每条有效规则必须明确标记为遵守、偏离或不适用".into(),
            ));
        }
        if check.status == "偏离" && check.note.trim().is_empty() {
            return Err(AppError::Validation("偏离规则时必须记录原因".into()));
        }
        if check.note.chars().count() > 1_000 {
            return Err(AppError::Validation(
                "规则确认备注不能超过 1000 个字符".into(),
            ));
        }
        canonical.push(DecisionRuleCheck {
            rule_id: rule.id.clone(),
            rule_revision: rule.revision,
            category: rule.category.clone(),
            statement: rule.statement.clone(),
            trigger: rule.trigger.clone(),
            status: check.status.clone(),
            note: check.note.trim().into(),
        });
    }
    Ok(canonical)
}

fn average(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn rule_effectiveness_signal(
    reviewed_count: usize,
    followed_average: Option<f64>,
    deviated_average: Option<f64>,
    followed_reviewed: usize,
    deviated_reviewed: usize,
) -> String {
    if reviewed_count < 3 {
        return "样本不足：继续记录过程".into();
    }
    match (followed_average, deviated_average) {
        (Some(followed), Some(deviated)) if followed >= deviated + 0.5 => {
            "遵守时过程评分更高".into()
        }
        (Some(followed), Some(deviated)) if followed + 0.5 < deviated => {
            "反常信号：需要复核规则".into()
        }
        (Some(_), Some(_)) => "暂无明显过程差异".into(),
        (Some(_), None) if followed_reviewed >= 3 => "只有遵守样本，缺少偏离对照".into(),
        (None, Some(_)) if deviated_reviewed >= 3 => "只有偏离样本，无法判断规则作用".into(),
        _ => "样本不足：继续记录过程".into(),
    }
}

fn validate_percentage(value: f64, label: &str) -> AppResult<()> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(AppError::Validation(format!("{label}必须在 0—100% 之间")));
    }
    Ok(())
}

fn validate_goal(input: &GoalInput) -> AppResult<()> {
    if input.name.trim().is_empty()
        || input.target_amount <= 0.0
        || input.target_date.trim().is_empty()
    {
        return Err(AppError::Validation("目标名称、金额和日期为必填项".into()));
    }
    validate_non_negative(&[
        input.target_amount,
        input.current_amount,
        input.monthly_contribution,
    ])?;
    chrono::NaiveDate::parse_from_str(&input.target_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("目标日期格式无效".into()))?;
    Ok(())
}

fn validate_investment_rule(input: &InvestmentRuleInput) -> AppResult<()> {
    if input.category.trim().is_empty()
        || input.statement.trim().is_empty()
        || input.trigger.trim().is_empty()
        || input.rationale.trim().is_empty()
    {
        return Err(AppError::Validation(
            "规则类别、内容、触发条件和依据均为必填项".into(),
        ));
    }
    if !matches!(
        input.category.as_str(),
        "资产配置" | "风险" | "研究" | "仓位" | "行为" | "复盘"
    ) {
        return Err(AppError::Validation("未知的投资规则类别".into()));
    }
    Ok(())
}

fn validate_system_review(input: &SystemReviewInput) -> AppResult<()> {
    if input.period_label.trim().is_empty()
        || input.process_summary.trim().is_empty()
        || input.lessons.trim().is_empty()
        || input.next_actions.trim().is_empty()
        || input.next_review_date.trim().is_empty()
    {
        return Err(AppError::Validation(
            "复盘周期、过程事实、经验、下一步和下次复盘日均为必填项".into(),
        ));
    }
    if !(1..=5).contains(&input.adherence_score) {
        return Err(AppError::Validation("纪律执行评分必须在 1—5 之间".into()));
    }
    chrono::NaiveDate::parse_from_str(&input.next_review_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("下次复盘日期格式无效".into()))?;
    Ok(())
}

fn validate_research_evidence(input: &ResearchEvidenceInput) -> AppResult<()> {
    if input.asset_name.trim().is_empty()
        || input.title.trim().is_empty()
        || input.publisher.trim().is_empty()
        || input.source_url.trim().is_empty()
        || input.as_of_date.trim().is_empty()
        || input.claim.trim().is_empty()
    {
        return Err(AppError::Validation(
            "资产、标题、发布方、来源链接、资料日期和证据摘要均为必填项".into(),
        ));
    }
    if !matches!(
        input.source_tier.as_str(),
        "一手来源" | "二手研究" | "媒体报道"
    ) {
        return Err(AppError::Validation("未知的来源层级".into()));
    }
    if !matches!(
        input.evidence_type.as_str(),
        "公司披露" | "监管文件" | "数据发布" | "研究报告" | "新闻" | "其他"
    ) {
        return Err(AppError::Validation("未知的证据类型".into()));
    }
    if !matches!(input.stance.as_str(), "支持" | "反驳" | "背景") {
        return Err(AppError::Validation("未知的证据立场".into()));
    }
    let as_of_date = chrono::NaiveDate::parse_from_str(&input.as_of_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("资料日期格式无效".into()))?;
    if as_of_date > Utc::now().date_naive() {
        return Err(AppError::Validation("资料日期不能晚于今天".into()));
    }
    let source = reqwest::Url::parse(input.source_url.trim())
        .map_err(|_| AppError::Validation("来源链接格式无效".into()))?;
    if source.scheme() != "https"
        || source.host_str().is_none()
        || !source.username().is_empty()
        || source.password().is_some()
    {
        return Err(AppError::Validation(
            "研究证据必须使用有效的 HTTPS 来源链接".into(),
        ));
    }
    for (label, value, limit) in [
        ("资产或主题", input.asset_name.as_str(), 120),
        ("资料标题", input.title.as_str(), 300),
        ("发布方", input.publisher.as_str(), 200),
        ("来源链接", input.source_url.as_str(), 2_048),
        ("证据摘要", input.claim.as_str(), 4_000),
        ("限制与待核实项", input.notes.as_str(), 4_000),
    ] {
        if value.chars().count() > limit {
            return Err(AppError::Validation(format!(
                "{label}不能超过 {limit} 个字符"
            )));
        }
    }
    Ok(())
}

fn store_rule_revision(transaction: &Transaction<'_>, rule: &InvestmentRule) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO investment_rule_revisions (rule_id, revision, payload, changed_at)
         VALUES (?1,?2,?3,?4)",
        params![
            rule.id,
            rule.revision,
            serde_json::to_string(rule)?,
            rule.updated_at
        ],
    )?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> AppResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
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
mod tests {
    use super::*;

    fn verified_quote() -> SecurityPriceQuote {
        SecurityPriceQuote {
            symbol: "AAPL".into(),
            currency: "USD".into(),
            close: 101.25,
            requested_on: "2026-09-05".into(),
            observed_on: "2026-09-04".into(),
            staleness_days: 1,
            provider_code: "twelve_data_raw_close".into(),
            provider_name: "Twelve Data".into(),
            exchange: "NASDAQ".into(),
            mic_code: "XNAS".into(),
            instrument_type: "Common Stock".into(),
            price_basis: "unadjusted_daily_close".into(),
            source_url: "https://api.twelvedata.com/time_series?symbol=AAPL&interval=1day".into(),
            methodology_url: "https://twelvedata.com/docs/market-data/time-series".into(),
            disclaimer: "测试用的明确限制".into(),
        }
    }

    #[test]
    fn persists_profile_and_holding() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let profile = FinancialProfile {
            monthly_expense: 10_000.0,
            emergency_fund: 60_000.0,
            ..Default::default()
        };
        db.save_profile(&profile).unwrap();
        db.add_holding(&HoldingInput {
            symbol: "IDX".into(),
            name: "指数".into(),
            asset_class: "基金".into(),
            market_value: 100_000.0,
            cost_basis: 90_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        let snapshot = db.snapshot().unwrap();
        assert_eq!(snapshot.holdings.len(), 1);
        assert_eq!(snapshot.emergency_months, 6.0);
    }

    #[test]
    fn freezes_verified_holding_valuation_and_invalidates_it_on_manual_change() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("verified-valuation.db")).unwrap();
        db.save_profile(&FinancialProfile {
            base_currency: "USD".into(),
            ..Default::default()
        })
        .unwrap();
        let created = db
            .add_holding(&HoldingInput {
                symbol: "AAPL".into(),
                name: "Apple".into(),
                asset_class: "股票".into(),
                market_value: 900.0,
                cost_basis: 800.0,
                target_pct: 100.0,
                currency: "USD".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-09-05".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        let holding_id = created.holdings[0].id.clone();
        let valued = db
            .apply_verified_holding_valuation(&holding_id, 10.0, &verified_quote())
            .unwrap();
        assert_eq!(valued.holdings[0].market_value, 1_012.5);
        assert_eq!(valued.holding_valuations.len(), 1);
        assert_eq!(valued.holding_valuations[0].quantity, 10.0);
        assert_eq!(valued.holding_valuations[0].observed_on, "2026-09-04");

        let checkin = db
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "带来源基线".into(),
                external_cash_flow: 0.0,
                note: String::new(),
                reset_baseline: false,
                use_ledger_cash_flows: true,
            })
            .unwrap();
        assert_eq!(checkin.holding_valuations.len(), 1);

        let unchanged = HoldingInput {
            symbol: "AAPL".into(),
            name: "Apple Inc.".into(),
            asset_class: "股票".into(),
            market_value: 1_012.5,
            cost_basis: 810.0,
            target_pct: 100.0,
            currency: "USD".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-09-05".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        };
        assert_eq!(
            db.update_holding(&holding_id, &unchanged)
                .unwrap()
                .holding_valuations
                .len(),
            1
        );
        let changed = HoldingInput {
            market_value: 1_020.0,
            ..unchanged
        };
        assert!(db
            .update_holding(&holding_id, &changed)
            .unwrap()
            .holding_valuations
            .is_empty());
    }

    #[test]
    fn rejects_verified_price_when_provider_currency_differs_from_holding() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("valuation-currency.db")).unwrap();
        let created = db
            .add_holding(&HoldingInput {
                symbol: "AAPL".into(),
                name: "错误币种".into(),
                asset_class: "股票".into(),
                market_value: 1_000.0,
                cost_basis: 900.0,
                target_pct: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-09-05".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        assert!(matches!(
            db.apply_verified_holding_valuation(&created.holdings[0].id, 10.0, &verified_quote()),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn reverses_only_open_period_events_and_preserves_the_audit_chain() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("reversal.db")).unwrap();
        db.add_holding(&HoldingInput {
            symbol: "CASH".into(),
            name: "现金".into(),
            asset_class: "现金".into(),
            market_value: 100_000.0,
            cost_basis: 100_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "冲正测试基线".into(),
            external_cash_flow: 0.0,
            note: "冻结测试基线".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        })
        .unwrap();
        let source = db
            .add_portfolio_event(&PortfolioEventInput {
                event_type: PortfolioEventType::Deposit,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: String::new(),
                amount: 1_000.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-08-02".into(),
                note: "误录入金".into(),
            })
            .unwrap();
        let reversal = db
            .reverse_portfolio_event(
                &source.id,
                &PortfolioEventReversalInput {
                    occurred_on: "2026-08-03".into(),
                    note: "与银行流水核对后确认重复".into(),
                },
            )
            .unwrap();

        assert_eq!(reversal.amount, -1_000.0);
        assert_eq!(reversal.base_amount, -1_000.0);
        assert_eq!(
            reversal.reversal_of_event_id.as_deref(),
            Some(source.id.as_str())
        );
        let events = db.portfolio_events().unwrap();
        let restored_source = events.iter().find(|event| event.id == source.id).unwrap();
        assert_eq!(
            restored_source.reversed_by_event_id.as_deref(),
            Some(reversal.id.as_str())
        );
        assert_eq!(performance::summarize(&events).external_cash_flow, 0.0);
        assert!(matches!(
            db.reverse_portfolio_event(
                &source.id,
                &PortfolioEventReversalInput {
                    occurred_on: "2026-08-03".into(),
                    note: "再次冲正".into(),
                }
            ),
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            db.reverse_portfolio_event(
                &reversal.id,
                &PortfolioEventReversalInput {
                    occurred_on: "2026-08-03".into(),
                    note: "冲正冲正记录".into(),
                }
            ),
            Err(AppError::Validation(_))
        ));

        let dataset = db.export_sync_data().unwrap();
        let target = Database::open(&directory.path().join("reversal-target.db")).unwrap();
        target.import_sync_data(&dataset).unwrap();
        let restored = target.portfolio_events().unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(performance::summarize(&restored).external_cash_flow, 0.0);
    }

    #[test]
    fn sync_snapshot_round_trips_domain_data_but_never_settings() {
        let directory = tempfile::tempdir().unwrap();
        let source = Database::open(&directory.path().join("source.db")).unwrap();
        source
            .save_profile(&FinancialProfile {
                monthly_expense: 8_000.0,
                emergency_fund: 48_000.0,
                ..Default::default()
            })
            .unwrap();
        source
            .add_holding(&HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 120_000.0,
                cost_basis: 100_000.0,
                target_pct: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        let holding_id = source.snapshot().unwrap().holdings[0].id.clone();
        let mut quote = verified_quote();
        quote.symbol = "IDX".into();
        quote.currency = "CNY".into();
        quote.requested_on = "2026-01-01".into();
        quote.observed_on = "2026-01-01".into();
        quote.staleness_days = 0;
        quote.close = 12.0;
        quote.source_url = "https://api.twelvedata.com/time_series?symbol=IDX&interval=1day".into();
        source
            .apply_verified_holding_valuation(&holding_id, 10_000.0, &quote)
            .unwrap();
        let rule = source
            .add_investment_rule(&InvestmentRuleInput {
                category: "仓位".into(),
                statement: "单一主动仓位不超过 8%".into(),
                trigger: "任何新建或加仓决定".into(),
                rationale: "限制永久损失".into(),
                active: true,
                source_review_id: None,
            })
            .unwrap();
        source
            .save_decision(&DecisionEntry {
                id: Some("synced-decision".into()),
                source_analysis_id: None,
                source_action_index: None,
                asset_name: "宽基指数".into(),
                thesis: "长期风险溢价".into(),
                counter_thesis: "估值偏高".into(),
                expected_return_pct: 8.0,
                downside_pct: 20.0,
                confidence_pct: 60.0,
                position_pct: 8.0,
                invalidation: "风险容量下降".into(),
                review_date: "2026-12-01".into(),
                rule_checks: vec![DecisionRuleCheck {
                    rule_id: rule.id,
                    rule_revision: rule.revision,
                    category: rule.category,
                    statement: rule.statement,
                    trigger: rule.trigger,
                    status: "遵守".into(),
                    note: "".into(),
                }],
            })
            .unwrap();
        let hidden_memory = source
            .save_memory_preference(
                "synced-decision",
                &MemoryPreferenceInput {
                    preference: "hidden".into(),
                    note: "先验证永久屏蔽".into(),
                },
            )
            .unwrap();
        assert!(!hidden_memory.selected);
        source
            .save_memory_preference(
                "synced-decision",
                &MemoryPreferenceInput {
                    preference: "pinned".into(),
                    note: "跨设备保留的能力圈经验".into(),
                },
            )
            .unwrap();
        source
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "同步基线".into(),
                external_cash_flow: 0.0,
                note: "首次组合快照".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            })
            .unwrap();
        source
            .add_portfolio_event(&PortfolioEventInput {
                event_type: PortfolioEventType::Deposit,
                source: "同步券商".into(),
                external_id: "sync-event-1".into(),
                asset_name: String::new(),
                amount: 700.0,
                currency: "USD".into(),
                fx_rate_to_base: Some(7.0),
                fx_rate_source: "ecb_reference".into(),
                fx_rate_observed_on: "2026-01-02".into(),
                occurred_on: "2026-01-02".into(),
                note: "同步测试入金".into(),
            })
            .unwrap();
        source.set_setting("model.name", "never-sync-this").unwrap();

        let dataset = source.export_sync_data().unwrap();
        let second_export = source.export_sync_data().unwrap();
        assert_eq!(
            dataset.content_hash().unwrap(),
            second_export.content_hash().unwrap()
        );

        let mut orphaned_preference = dataset.clone();
        let preference_table = orphaned_preference
            .tables
            .iter_mut()
            .find(|table| table.name == "memory_preferences")
            .unwrap();
        preference_table.rows[0][0] = SyncValue::Text("missing-memory".into());
        let preference_target =
            Database::open(&directory.path().join("preference-target.db")).unwrap();
        assert!(matches!(
            preference_target.import_sync_data(&orphaned_preference),
            Err(AppError::Validation(_))
        ));

        let mut corrupted_identity = dataset.clone();
        let event_table = corrupted_identity
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        event_table.rows[0][4] = SyncValue::Text("wrong-fingerprint".into());
        let corrupted_target =
            Database::open(&directory.path().join("corrupted-target.db")).unwrap();
        assert!(matches!(
            corrupted_target.import_sync_data(&corrupted_identity),
            Err(AppError::Validation(_))
        ));

        let mut corrupted_fx_date = dataset.clone();
        let event_table = corrupted_fx_date
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        event_table.rows[0][10] = SyncValue::Text("2026-01-03".into());
        assert!(matches!(
            corrupted_target.import_sync_data(&corrupted_fx_date),
            Err(AppError::Validation(_))
        ));

        let mut corrupted_valuation = dataset.clone();
        let valuation_table = corrupted_valuation
            .tables
            .iter_mut()
            .find(|table| table.name == "holding_valuations")
            .unwrap();
        valuation_table.rows[0][4] = SyncValue::Real(999.0);
        assert!(matches!(
            corrupted_target.import_sync_data(&corrupted_valuation),
            Err(AppError::Validation(_))
        ));

        let target = Database::open(&directory.path().join("target.db")).unwrap();
        target
            .set_setting("model.name", "keep-local-model")
            .unwrap();
        target.import_sync_data(&dataset).unwrap();
        let restored = target.snapshot().unwrap();
        assert_eq!(restored.holdings.len(), 1);
        assert_eq!(restored.holding_valuations.len(), 1);
        assert_eq!(restored.holding_valuations[0].quantity, 10_000.0);
        assert_eq!(restored.profile.emergency_fund, 48_000.0);
        let restored_decision = target.decisions().unwrap().remove(0);
        assert_eq!(restored_decision.rule_checks.len(), 1);
        assert_eq!(restored_decision.rule_checks[0].status, "遵守");
        let restored_memory = target
            .memories()
            .unwrap()
            .into_iter()
            .find(|item| item.id == "synced-decision")
            .unwrap();
        assert_eq!(restored_memory.preference, "pinned");
        assert_eq!(restored_memory.preference_note, "跨设备保留的能力圈经验");
        assert_eq!(target.portfolio_checkins().unwrap().len(), 1);
        let restored_events = target.portfolio_events().unwrap();
        assert_eq!(restored_events.len(), 1);
        assert_eq!(restored_events[0].source, "同步券商");
        assert_eq!(restored_events[0].external_id, "sync-event-1");
        assert_eq!(restored_events[0].fx_rate_source, "ecb_reference");
        assert_eq!(restored_events[0].fx_rate_observed_on, "2026-01-02");
        assert_eq!(
            target.setting("model.name").unwrap().as_deref(),
            Some("keep-local-model")
        );

        let mut legacy_v8 = dataset.clone();
        legacy_v8.schema_version = 8;
        assert_eq!(legacy_v8.tables.pop().unwrap().name, "holding_valuations");
        legacy_v8.validate().unwrap();
        let v8_target = Database::open(&directory.path().join("v8-target.db")).unwrap();
        v8_target.import_sync_data(&legacy_v8).unwrap();
        assert_eq!(v8_target.memories().unwrap().len(), 1);

        let mut legacy_v7 = legacy_v8;
        legacy_v7.schema_version = 7;
        assert_eq!(legacy_v7.tables.pop().unwrap().name, "memory_preferences");
        legacy_v7.validate().unwrap();
        let v7_target = Database::open(&directory.path().join("v7-target.db")).unwrap();
        v7_target.import_sync_data(&legacy_v7).unwrap();
        assert_eq!(v7_target.portfolio_events().unwrap().len(), 1);

        let mut legacy_v6 = legacy_v7;
        legacy_v6.schema_version = 6;
        let events = legacy_v6
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        events.columns.remove(16);
        for row in &mut events.rows {
            row.remove(16);
        }
        legacy_v6.validate().unwrap();
        let v6_target = Database::open(&directory.path().join("v6-target.db")).unwrap();
        v6_target.import_sync_data(&legacy_v6).unwrap();
        assert_eq!(v6_target.portfolio_events().unwrap().len(), 1);

        let mut legacy_v5 = legacy_v6;
        legacy_v5.schema_version = 5;
        let holdings = legacy_v5
            .tables
            .iter_mut()
            .find(|table| table.name == "holdings")
            .unwrap();
        for index in [11, 10] {
            holdings.columns.remove(index);
            for row in &mut holdings.rows {
                row.remove(index);
            }
        }
        let events = legacy_v5
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        for index in [10, 9] {
            events.columns.remove(index);
            for row in &mut events.rows {
                row.remove(index);
            }
        }
        legacy_v5.validate().unwrap();
        let v5_target = Database::open(&directory.path().join("v5-target.db")).unwrap();
        v5_target.import_sync_data(&legacy_v5).unwrap();
        assert!(v5_target.portfolio_events().unwrap()[0]
            .fx_rate_source
            .is_empty());

        let mut legacy_v4 = legacy_v5;
        legacy_v4.schema_version = 4;
        let events = legacy_v4
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        for index in [4, 3, 2] {
            events.columns.remove(index);
            for row in &mut events.rows {
                row.remove(index);
            }
        }
        legacy_v4.validate().unwrap();
        let v4_target = Database::open(&directory.path().join("v4-target.db")).unwrap();
        v4_target.import_sync_data(&legacy_v4).unwrap();
        let v4_events = v4_target.portfolio_events().unwrap();
        assert_eq!(v4_events.len(), 1);
        assert_eq!(v4_events[0].source, "manual");
        assert!(v4_events[0].external_id.is_empty());

        let mut legacy_v3 = legacy_v4;
        legacy_v3.schema_version = 3;
        assert_eq!(legacy_v3.tables.pop().unwrap().name, "portfolio_events");
        legacy_v3.validate().unwrap();
        let v3_target = Database::open(&directory.path().join("v3-target.db")).unwrap();
        v3_target.import_sync_data(&legacy_v3).unwrap();
        assert!(v3_target.portfolio_events().unwrap().is_empty());

        let mut legacy_v2 = legacy_v3;
        legacy_v2.schema_version = 2;
        let holdings = legacy_v2
            .tables
            .iter_mut()
            .find(|table| table.name == "holdings")
            .unwrap();
        for index in [9, 8] {
            holdings.columns.remove(index);
            for row in &mut holdings.rows {
                row.remove(index);
            }
        }
        legacy_v2.validate().unwrap();
        let v2_target = Database::open(&directory.path().join("v2-target.db")).unwrap();
        v2_target.import_sync_data(&legacy_v2).unwrap();
        assert_eq!(v2_target.portfolio_checkins().unwrap().len(), 1);
        assert_eq!(
            v2_target
                .snapshot()
                .unwrap()
                .valuation_status
                .undated_holding_count,
            1
        );

        let mut legacy_dataset = legacy_v2;
        legacy_dataset.schema_version = 1;
        assert_eq!(
            legacy_dataset.tables.pop().unwrap().name,
            "portfolio_checkins"
        );
        legacy_dataset.validate().unwrap();
        let legacy_target = Database::open(&directory.path().join("legacy-target.db")).unwrap();
        legacy_target.import_sync_data(&legacy_dataset).unwrap();
        assert_eq!(legacy_target.snapshot().unwrap().holdings.len(), 1);
        assert!(legacy_target.portfolio_checkins().unwrap().is_empty());
    }

    #[test]
    fn sync_snapshot_rejects_unknown_or_incomplete_schema() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("source.db")).unwrap();
        let mut dataset = db.export_sync_data().unwrap();
        dataset.tables.pop();
        assert!(matches!(dataset.validate(), Err(AppError::Validation(_))));
    }

    #[test]
    fn updates_and_deletes_holding() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let input = HoldingInput {
            symbol: "IDX".into(),
            name: "指数".into(),
            asset_class: "基金".into(),
            market_value: 100_000.0,
            cost_basis: 90_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        };
        let created = db.add_holding(&input).unwrap();
        let id = &created.holdings[0].id;
        let mut updated = input;
        updated.market_value = 120_000.0;
        let snapshot = db.update_holding(id, &updated).unwrap();
        assert_eq!(snapshot.holdings[0].market_value, 120_000.0);
        assert!(db.delete_holding(id).unwrap().holdings.is_empty());
    }

    #[test]
    fn portfolio_checkins_separate_external_flows_from_valuation_residuals() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        assert!(matches!(
            db.save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "基线".into(),
                external_cash_flow: 0.0,
                note: "".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            }),
            Err(AppError::Validation(_))
        ));
        let snapshot = db
            .add_holding(&HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 100_000.0,
                cost_basis: 90_000.0,
                target_pct: 90.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        assert!(matches!(
            db.save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "错误基线".into(),
                external_cash_flow: 10_000.0,
                note: "首次入金".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            }),
            Err(AppError::Validation(_))
        ));
        let baseline = db
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "2026-08 基线".into(),
                external_cash_flow: 0.0,
                note: "首次冻结组合".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            })
            .unwrap();
        assert_eq!(baseline.total_value, 100_000.0);
        assert_eq!(baseline.total_change, None);

        let fund_id = snapshot.holdings[0].id.clone();
        db.update_holding(
            &fund_id,
            &HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 102_000.0,
                cost_basis: 90_000.0,
                target_pct: 90.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-02-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();
        db.add_holding(&HoldingInput {
            symbol: "CASH".into(),
            name: "新增现金".into(),
            asset_class: "现金".into(),
            market_value: 10_000.0,
            cost_basis: 10_000.0,
            target_pct: 10.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-02-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        let next = db
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "2026-09".into(),
                external_cash_flow: 10_000.0,
                note: "工资结余入金".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            })
            .unwrap();
        assert_eq!(
            next.previous_check_in_id.as_deref(),
            Some(baseline.id.as_str())
        );
        assert_eq!(next.total_value, 112_000.0);
        assert_eq!(next.total_change, Some(12_000.0));
        assert_eq!(next.valuation_residual, Some(2_000.0));
        assert_eq!(next.allocation_changes.len(), 2);

        db.update_holding(
            &fund_id,
            &HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 120_000.0,
                cost_basis: 90_000.0,
                target_pct: 90.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-03-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();
        let history = db.portfolio_checkins().unwrap();
        assert_eq!(
            history[0].holdings[0]
                .market_value
                .max(history[0].holdings[1].market_value),
            102_000.0
        );
        assert_eq!(history[1].holdings[0].market_value, 100_000.0);
    }

    #[test]
    fn ledger_drives_checkin_cash_flows_and_preserves_frozen_periods() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("ledger.db")).unwrap();
        assert!(matches!(
            db.add_portfolio_event(&PortfolioEventInput {
                event_type: PortfolioEventType::Deposit,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: String::new(),
                amount: 1_000.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-01".into(),
                note: "没有基线".into(),
            }),
            Err(AppError::Validation(_))
        ));
        let created = db
            .add_holding(&HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 100_000.0,
                cost_basis: 90_000.0,
                target_pct: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "一月基线".into(),
            external_cash_flow: 0.0,
            note: "开始记录".into(),
            reset_baseline: false,
            use_ledger_cash_flows: true,
        })
        .unwrap();

        for input in [
            PortfolioEventInput {
                event_type: PortfolioEventType::Deposit,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: String::new(),
                amount: 10_000.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-16".into(),
                note: "工资结余入金".into(),
            },
            PortfolioEventInput {
                event_type: PortfolioEventType::Dividend,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: "宽基指数".into(),
                amount: 500.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-20".into(),
                note: "现金分红".into(),
            },
            PortfolioEventInput {
                event_type: PortfolioEventType::Fee,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: "宽基指数".into(),
                amount: 50.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-21".into(),
                note: "交易费用".into(),
            },
            PortfolioEventInput {
                event_type: PortfolioEventType::Buy,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: "宽基指数".into(),
                amount: 20_000.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-22".into(),
                note: "账户内部调仓".into(),
            },
        ] {
            db.add_portfolio_event(&input).unwrap();
        }

        db.update_holding(
            &created.holdings[0].id,
            &HoldingInput {
                symbol: "IDX".into(),
                name: "宽基指数".into(),
                asset_class: "基金".into(),
                market_value: 120_450.0,
                cost_basis: 110_000.0,
                target_pct: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-01-31".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();
        let checkin = db
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "一月".into(),
                external_cash_flow: 999_999.0,
                note: "流水自动汇总".into(),
                reset_baseline: false,
                use_ledger_cash_flows: true,
            })
            .unwrap();
        assert_eq!(checkin.cash_flow_source, "ledger");
        assert_eq!(checkin.external_cash_flow, 10_000.0);
        assert_eq!(checkin.income, 500.0);
        assert_eq!(checkin.costs, 50.0);
        assert_eq!(checkin.turnover, 20_000.0);
        assert_eq!(checkin.event_ids.len(), 4);
        assert!((checkin.modified_dietz_return_pct.unwrap() - 9.952_380_952).abs() < 1e-6);

        assert!(matches!(
            db.add_portfolio_event(&PortfolioEventInput {
                event_type: PortfolioEventType::Withdrawal,
                source: "manual".into(),
                external_id: String::new(),
                asset_name: String::new(),
                amount: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
                occurred_on: "2026-01-30".into(),
                note: "迟到记录".into(),
            }),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn csv_event_import_is_previewed_atomic_deduplicated_and_revision_bound() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("event-import.db")).unwrap();
        db.add_holding(&HoldingInput {
            symbol: "CASH".into(),
            name: "现金".into(),
            asset_class: "现金".into(),
            market_value: 10_000.0,
            cost_basis: 10_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "导入基线".into(),
            external_cash_flow: 0.0,
            note: String::new(),
            reset_baseline: false,
            use_ledger_cash_flows: true,
        })
        .unwrap();

        let conflicting = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,trade-001,deposit,2026-09-01,2000,CNY,,,冲突入金\n";
        let conflict_preview = db
            .preview_portfolio_event_import(&PortfolioEventImportRequest {
                csv_text: conflicting.into(),
            })
            .unwrap();
        assert_eq!(conflict_preview.ready_count, 1);
        assert_eq!(conflict_preview.error_count, 1);
        assert!(matches!(
            db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
                csv_text: conflicting.into(),
                preview_revision: conflict_preview.preview_revision,
            }),
            Err(AppError::Validation(_))
        ));
        assert!(db.portfolio_events().unwrap().is_empty());

        let valid = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,fee-001,fee,2026-09-02,5,CNY,,指数基金,交易费用\n";
        let preview = db
            .preview_portfolio_event_import(&PortfolioEventImportRequest {
                csv_text: valid.into(),
            })
            .unwrap();
        assert_eq!(preview.ready_count, 2);
        assert_eq!(preview.duplicate_count, 1);
        assert_eq!(preview.error_count, 0);
        let changed_after_preview = valid.replace("交易费用", "费用说明已改变");
        assert!(matches!(
            db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
                csv_text: changed_after_preview,
                preview_revision: preview.preview_revision.clone(),
            }),
            Err(AppError::Conflict(_))
        ));
        assert!(db.portfolio_events().unwrap().is_empty());
        let result = db
            .commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
                csv_text: valid.into(),
                preview_revision: preview.preview_revision,
            })
            .unwrap();
        assert_eq!(result.inserted_count, 2);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(db.portfolio_events().unwrap().len(), 2);

        let repeated = db
            .preview_portfolio_event_import(&PortfolioEventImportRequest {
                csv_text: valid.into(),
            })
            .unwrap();
        assert_eq!(repeated.ready_count, 0);
        assert_eq!(repeated.duplicate_count, 3);
        let repeated_result = db
            .commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
                csv_text: valid.into(),
                preview_revision: repeated.preview_revision,
            })
            .unwrap();
        assert_eq!(repeated_result.inserted_count, 0);

        let changed = valid.replace("1000,CNY,,,首次入金", "1200,CNY,,,首次入金");
        let changed_preview = db
            .preview_portfolio_event_import(&PortfolioEventImportRequest { csv_text: changed })
            .unwrap();
        assert!(changed_preview.error_count > 0);

        let late = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-003,interest,2026-09-03,20,CNY,,现金,利息\n";
        let late_preview = db
            .preview_portfolio_event_import(&PortfolioEventImportRequest {
                csv_text: late.into(),
            })
            .unwrap();
        db.add_portfolio_event(&PortfolioEventInput {
            event_type: PortfolioEventType::Interest,
            source: "券商甲".into(),
            external_id: "trade-003".into(),
            asset_name: "现金".into(),
            amount: 20.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-09-03".into(),
            note: "利息".into(),
        })
        .unwrap();
        assert!(matches!(
            db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
                csv_text: late.into(),
                preview_revision: late_preview.preview_revision,
            }),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn foreign_currency_events_require_and_freeze_the_declared_rate() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("event-fx.db")).unwrap();
        db.add_holding(&HoldingInput {
            symbol: "CASH".into(),
            name: "现金".into(),
            asset_class: "现金".into(),
            market_value: 10_000.0,
            cost_basis: 10_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "基线".into(),
            external_cash_flow: 0.0,
            note: String::new(),
            reset_baseline: false,
            use_ledger_cash_flows: true,
        })
        .unwrap();
        let mut input = PortfolioEventInput {
            event_type: PortfolioEventType::Withdrawal,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount: 100.0,
            currency: "USD".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-09-01".into(),
            note: "美元出金".into(),
        };
        assert!(matches!(
            db.add_portfolio_event(&input),
            Err(AppError::Validation(_))
        ));
        input.fx_rate_to_base = Some(7.0);
        input.fx_rate_source = "ecb_reference".into();
        input.fx_rate_observed_on = "2026-09-02".into();
        assert!(matches!(
            db.add_portfolio_event(&input),
            Err(AppError::Validation(_))
        ));
        input.fx_rate_observed_on = "2026-08-31".into();
        let event = db.add_portfolio_event(&input).unwrap();
        assert_eq!(event.base_currency, "CNY");
        assert_eq!(event.base_amount, 700.0);
        assert_eq!(event.fx_rate_to_base, Some(7.0));
        assert_eq!(event.fx_rate_source, "ecb_reference");
        assert_eq!(event.fx_rate_observed_on, "2026-08-31");
    }

    #[test]
    fn foreign_currency_is_normalized_and_base_changes_invalidate_old_rates() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("currency.db")).unwrap();
        let mut profile = FinancialProfile {
            base_currency: "CNY".into(),
            ..Default::default()
        };
        db.save_profile(&profile).unwrap();

        assert!(matches!(
            db.add_holding(&HoldingInput {
                symbol: "USD".into(),
                name: "美元资产".into(),
                asset_class: "股票".into(),
                market_value: 100.0,
                cost_basis: 90.0,
                target_pct: 70.0,
                currency: "USD".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-08-31".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            }),
            Err(AppError::Validation(_))
        ));
        let usd_snapshot = db
            .add_holding(&HoldingInput {
                symbol: "USD".into(),
                name: "美元资产".into(),
                asset_class: "股票".into(),
                market_value: 100.0,
                cost_basis: 90.0,
                target_pct: 70.0,
                currency: "USD".into(),
                fx_rate_to_base: Some(7.0),
                valuation_date: "2026-08-31".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        assert_eq!(usd_snapshot.holdings[0].fx_rate_source, "user_declared");
        assert_eq!(usd_snapshot.holdings[0].fx_rate_observed_on, "2026-08-31");
        let usd_id = usd_snapshot.holdings[0].id.clone();
        let snapshot = db
            .add_holding(&HoldingInput {
                symbol: "CASH".into(),
                name: "人民币现金".into(),
                asset_class: "现金".into(),
                market_value: 300.0,
                cost_basis: 300.0,
                target_pct: 30.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-08-31".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            })
            .unwrap();
        let cny_id = snapshot
            .holdings
            .iter()
            .find(|holding| holding.currency == "CNY")
            .unwrap()
            .id
            .clone();
        assert_eq!(snapshot.total_value, 1_000.0);
        assert_eq!(snapshot.concentration_pct, 70.0);
        assert!(snapshot.valuation_status.comparable);
        assert_eq!(
            snapshot.valuation_status.aligned_valuation_date.as_deref(),
            Some("2026-08-31")
        );
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "CNY 基线".into(),
            external_cash_flow: 0.0,
            note: "统一估值日".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        })
        .unwrap();

        db.update_holding(
            &usd_id,
            &HoldingInput {
                symbol: "USD".into(),
                name: "美元资产".into(),
                asset_class: "股票".into(),
                market_value: 100.0,
                cost_basis: 90.0,
                target_pct: 70.0,
                currency: "USD".into(),
                fx_rate_to_base: Some(7.0),
                valuation_date: "2026-09-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();
        assert!(matches!(
            db.save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "错位日期".into(),
                external_cash_flow: 0.0,
                note: "".into(),
                reset_baseline: false,
                use_ledger_cash_flows: false,
            }),
            Err(AppError::Validation(_))
        ));

        profile.base_currency = "USD".into();
        let changed = db.save_profile(&profile).unwrap();
        assert!(!changed.valuation_status.comparable);
        assert_eq!(changed.total_value, 0.0);
        assert_eq!(
            changed.valuation_status.missing_fx_holdings,
            vec!["人民币现金"]
        );
        let usd_after_base_change = changed
            .holdings
            .iter()
            .find(|holding| holding.id == usd_id)
            .unwrap();
        assert_eq!(usd_after_base_change.fx_rate_to_base, None);
        assert!(usd_after_base_change.fx_rate_source.is_empty());
        assert!(usd_after_base_change.fx_rate_observed_on.is_empty());
        let normalized = db
            .update_holding(
                &cny_id,
                &HoldingInput {
                    symbol: "CASH".into(),
                    name: "人民币现金".into(),
                    asset_class: "现金".into(),
                    market_value: 300.0,
                    cost_basis: 300.0,
                    target_pct: 30.0,
                    currency: "CNY".into(),
                    fx_rate_to_base: Some(0.14),
                    valuation_date: "2026-09-01".into(),
                    fx_rate_source: String::new(),
                    fx_rate_observed_on: String::new(),
                },
            )
            .unwrap();
        assert_eq!(normalized.total_value, 142.0);
        let reset = db
            .save_portfolio_checkin(&PortfolioCheckInInput {
                period_label: "USD 新基线".into(),
                external_cash_flow: 0.0,
                note: "切换基准币种后重新开始比较".into(),
                reset_baseline: true,
                use_ledger_cash_flows: false,
            })
            .unwrap();
        assert_eq!(reset.base_currency, "USD");
        assert_eq!(reset.total_change, None);
    }

    #[test]
    fn stores_decision_review_separately_from_original_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.save_decision(&DecisionEntry {
            id: None,
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值过高".into(),
            expected_return_pct: 12.0,
            downside_pct: 20.0,
            confidence_pct: 65.0,
            position_pct: 30.0,
            invalidation: "风险容量改变".into(),
            review_date: "2026-12-01".into(),
            rule_checks: vec![],
        })
        .unwrap();
        let id = db.decisions().unwrap()[0].id.clone();
        db.save_decision_review(
            &id,
            &DecisionReviewInput {
                outcome_summary: "价格下跌但逻辑未破坏".into(),
                actual_return_pct: Some(-8.0),
                thesis_status: "部分成立".into(),
                process_rating: 4,
                lessons: "进一步区分价格与逻辑".into(),
            },
        )
        .unwrap();
        let record = db.decisions().unwrap().remove(0);
        assert_eq!(record.thesis, "长期风险溢价");
        assert_eq!(record.review.unwrap().process_rating, 4);
        let memory = db
            .memories()
            .unwrap()
            .into_iter()
            .find(|item| item.kind == "decision")
            .unwrap();
        assert!(memory.reviewed);
        assert!(memory.contradiction);
        assert_eq!(memory.status, "复盘：部分成立");
        assert_eq!(memory.content["originalThesis"], "长期风险溢价");
        assert_eq!(memory.content["review"]["processRating"], 4);
    }

    #[test]
    fn requires_every_decision_to_have_a_review_date() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let result = db.save_decision(&DecisionEntry {
            id: None,
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值过高".into(),
            expected_return_pct: 8.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 30.0,
            invalidation: "风险容量下降".into(),
            review_date: "".into(),
            rule_checks: vec![],
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn review_reminders_are_opt_in_deduplicated_and_follow_due_work() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 9, 6).unwrap();

        let initial = db.review_reminder_summary(today).unwrap();
        assert!(!initial.enabled);
        assert!(initial.periodic_review_due);
        assert!(!initial.should_notify);

        db.save_reminder_settings(true).unwrap();
        db.save_decision(&DecisionEntry {
            id: Some("due-decision".into()),
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "宽基指数".into(),
            thesis: "长期配置".into(),
            counter_thesis: "风险容量下降".into(),
            expected_return_pct: 7.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 30.0,
            invalidation: "应急金不足".into(),
            review_date: "2026-09-06".into(),
            rule_checks: vec![],
        })
        .unwrap();
        db.save_decision(&DecisionEntry {
            id: Some("future-decision".into()),
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "债券基金".into(),
            thesis: "降低波动".into(),
            counter_thesis: "利率快速上行".into(),
            expected_return_pct: 3.0,
            downside_pct: 5.0,
            confidence_pct: 70.0,
            position_pct: 20.0,
            invalidation: "久期风险变化".into(),
            review_date: "2026-10-01".into(),
            rule_checks: vec![],
        })
        .unwrap();

        let due = db.review_reminder_summary(today).unwrap();
        assert_eq!(due.due_decision_count, 1);
        assert!(due.should_notify);
        assert!(matches!(
            db.acknowledge_review_reminder(today, "stale-fingerprint"),
            Err(AppError::Validation(_))
        ));
        let acknowledged = db
            .acknowledge_review_reminder(today, &due.fingerprint)
            .unwrap();
        assert!(!acknowledged.should_notify);

        db.save_decision_review(
            "due-decision",
            &DecisionReviewInput {
                outcome_summary: "按计划复核".into(),
                actual_return_pct: None,
                thesis_status: "尚不明确".into(),
                process_rating: 4,
                lessons: "继续观察证伪条件".into(),
            },
        )
        .unwrap();
        let changed = db.review_reminder_summary(today).unwrap();
        assert_eq!(changed.due_decision_count, 0);
        assert!(changed.periodic_review_due);
        assert!(changed.should_notify);
        assert_ne!(changed.fingerprint, due.fingerprint);

        db.save_system_review(&SystemReviewInput {
            period_label: "2026 Q3".into(),
            adherence_score: 4,
            process_summary: "按计划执行".into(),
            rule_violations: "无".into(),
            lessons: "保持低频复盘".into(),
            next_actions: "下季度复核".into(),
            next_review_date: "2026-12-31".into(),
        })
        .unwrap();
        let clear = db.review_reminder_summary(today).unwrap();
        assert_eq!(clear.due_decision_count, 0);
        assert!(!clear.periodic_review_due);
        assert!(!clear.should_notify);

        db.save_reminder_settings(false).unwrap();
        assert!(!db.review_reminder_summary(today).unwrap().should_notify);
    }

    #[test]
    fn reminder_settings_are_device_local_and_never_cloud_synced() {
        let directory = tempfile::tempdir().unwrap();
        let source = Database::open(&directory.path().join("source.db")).unwrap();
        source.save_reminder_settings(true).unwrap();
        let dataset = source.export_sync_data().unwrap();

        let target = Database::open(&directory.path().join("target.db")).unwrap();
        target.save_reminder_settings(false).unwrap();
        target.import_sync_data(&dataset).unwrap();

        assert!(!target.reminder_settings().unwrap().enabled);
        assert!(dataset.tables.iter().all(|table| table.name != "settings"));
    }

    #[test]
    fn decision_provenance_must_reference_an_existing_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let base = DecisionEntry {
            id: None,
            source_analysis_id: Some("analysis-1".into()),
            source_action_index: Some(0),
            asset_name: "指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值过高".into(),
            expected_return_pct: 8.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 30.0,
            invalidation: "风险容量下降".into(),
            review_date: "2026-12-01".into(),
            rule_checks: vec![],
        };
        assert!(matches!(
            db.save_decision(&base),
            Err(AppError::Validation(_))
        ));

        let now = Utc::now().to_rfc3339();
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO analyses (id, question, answer, audit, trace, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "analysis-1",
                    "如何处理风险",
                    "先降低脆弱性",
                    "{}",
                    r#"{"structuredReport":{"verdict":"降低脆弱性","facts":[],"inferences":[],"unknowns":["估值"],"options":[],"actions":[{"action":"再平衡","rationale":"集中度过高","reversible":true,"reviewTrigger":"集中度下降"}],"reviewTriggers":["一个月后"]}}"#,
                    now
                ],
            )
            .unwrap();
        let mut invalid_action = base.clone();
        invalid_action.source_action_index = Some(1);
        assert!(matches!(
            db.save_decision(&invalid_action),
            Err(AppError::Validation(_))
        ));
        db.save_decision(&base).unwrap();
        let record = db.decisions().unwrap().remove(0);
        assert_eq!(record.source_analysis_id.as_deref(), Some("analysis-1"));
        assert_eq!(record.source_action_index, Some(0));
    }

    #[test]
    fn old_decision_payload_deserializes_without_provenance() {
        let payload = r#"{
            "id":null,"assetName":"指数","thesis":"长期持有","counterThesis":"估值风险",
            "expectedReturnPct":8,"downsidePct":20,"confidencePct":60,"positionPct":30,
            "invalidation":"目标变化","reviewDate":"2026-12-01"
        }"#;
        let entry: DecisionEntry = serde_json::from_str(payload).unwrap();
        assert_eq!(entry.source_analysis_id, None);
        assert_eq!(entry.source_action_index, None);
        assert!(entry.rule_checks.is_empty());
    }

    #[test]
    fn updates_goal_funding_and_recalculates_projection() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let input = GoalInput {
            name: "养老".into(),
            target_amount: 1_000_000.0,
            current_amount: 100_000.0,
            monthly_contribution: 2_000.0,
            target_date: "2036-12-31".into(),
            priority: "重要".into(),
        };
        let created = db.add_goal(&input).unwrap();
        let id = created.goals[0].id.clone();
        let initial_success = created.plan.goal_projections[0].estimated_success_pct;
        let mut updated = input;
        updated.monthly_contribution = 8_000.0;
        let snapshot = db.update_goal(&id, &updated).unwrap();
        assert_eq!(snapshot.goals[0].monthly_contribution, 8_000.0);
        assert!(snapshot.plan.goal_projections[0].estimated_success_pct > initial_success);
        assert!(db.delete_goal(&id).unwrap().goals.is_empty());
    }

    #[test]
    fn migrates_existing_goal_and_holding_columns_without_losing_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE goals (
                   id TEXT PRIMARY KEY, name TEXT NOT NULL, target_amount REAL NOT NULL,
                   target_date TEXT NOT NULL, priority TEXT NOT NULL, created_at TEXT NOT NULL
                 );
                 CREATE TABLE holdings (
                   id TEXT PRIMARY KEY, symbol TEXT NOT NULL, name TEXT NOT NULL,
                   asset_class TEXT NOT NULL, market_value REAL NOT NULL, cost_basis REAL NOT NULL,
                   currency TEXT NOT NULL, created_at TEXT NOT NULL
                 );
                 CREATE TABLE analyses (
                   id TEXT PRIMARY KEY, question TEXT NOT NULL, answer TEXT NOT NULL,
                   created_at TEXT NOT NULL
                 );
                 CREATE TABLE portfolio_events (
                   id TEXT PRIMARY KEY, event_type TEXT NOT NULL, asset_name TEXT NOT NULL,
                   amount REAL NOT NULL, currency TEXT NOT NULL, fx_rate_to_base REAL,
                   base_currency TEXT NOT NULL, base_amount REAL NOT NULL,
                   occurred_on TEXT NOT NULL, note TEXT NOT NULL, created_at TEXT NOT NULL
                 );
                 INSERT INTO goals VALUES ('g1','养老',1000000,'2036-12-31','重要','2026-01-01');
                 INSERT INTO holdings VALUES ('h1','IDX','指数','基金',100000,90000,'CNY','2026-01-01');
                 INSERT INTO analyses VALUES ('a1','旧问题','旧回答','2026-01-01');
                 INSERT INTO portfolio_events VALUES (
                   'e1','deposit','',1000,'CNY',NULL,'CNY',1000,
                   '2026-01-02','旧版入金','2026-01-02T00:00:00Z'
                 );",
            )
            .unwrap();
        drop(connection);

        let db = Database::open(&path).unwrap();
        let snapshot = db.snapshot().unwrap();
        assert_eq!(snapshot.goals[0].current_amount, 0.0);
        assert_eq!(snapshot.goals[0].monthly_contribution, 0.0);
        assert_eq!(snapshot.holdings[0].target_pct, 0.0);
        assert_eq!(snapshot.holdings[0].fx_rate_to_base, None);
        assert!(snapshot.holdings[0].valuation_date.is_empty());
        assert!(snapshot.holdings[0].fx_rate_source.is_empty());
        assert!(snapshot.holdings[0].fx_rate_observed_on.is_empty());
        let events = db.portfolio_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "manual");
        assert!(events[0].external_id.is_empty());
        assert!(events[0].fx_rate_source.is_empty());
        assert!(events[0].fx_rate_observed_on.is_empty());
        assert_eq!(snapshot.profile.base_currency, "CNY");
        assert_eq!(snapshot.valuation_status.undated_holding_count, 1);
        assert!(db.analysis_history().unwrap()[0].transparency.is_none());
    }

    #[test]
    fn stores_analysis_transparency_audit() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let result = AnalysisResult {
            id: "analysis-1".into(),
            answer: "先控制风险".into(),
            stages: vec!["本地风险检查".into()],
            transparency: crate::models::AnalysisTransparency {
                provider: "mock".into(),
                model: "mock-model".into(),
                context_groups: vec!["本次问题".into(), "规则型风险检查".into()],
                payload_bytes: 512,
                context_revision: "ctx-test".into(),
                memory_items_used: 2,
                reviewed_memory_items_used: 1,
                conflicting_memory_items_used: 1,
                evidence_items_used: 0,
                citations_required: false,
                model_calls: 3,
                total_latency_ms: 1200,
                input_tokens: Some(400),
                output_tokens: Some(100),
                structured_output_validated: true,
                output_repairs: 0,
                external_data_used: false,
                api_key_sent: false,
            },
            workflow_trace: crate::models::AnalysisWorkflowTrace {
                version: "investment-workflow-v4".into(),
                structured_report: Some(crate::models::StructuredAnalysis {
                    verdict: "先控制风险".into(),
                    facts: vec![crate::models::AnalysisClaim {
                        statement: "风险预算已超出".into(),
                        basis: "user_data".into(),
                        evidence_ids: Vec::new(),
                    }],
                    inferences: Vec::new(),
                    unknowns: vec!["未来现金流".into()],
                    options: vec![crate::models::AnalysisOption {
                        name: "分批调整".into(),
                        suitable_when: "风险超限".into(),
                        tradeoffs: vec!["可能错过上涨".into()],
                        risks: vec!["执行偏差".into()],
                    }],
                    actions: vec![crate::models::AnalysisAction {
                        action: "核对目标权重".into(),
                        rationale: "避免错误交易".into(),
                        reversible: true,
                        review_trigger: "一周后".into(),
                    }],
                    review_triggers: vec!["风险回到预算内".into()],
                }),
                output_validation: Some(crate::models::OutputValidationTrace {
                    status: "valid".into(),
                    attempts: 1,
                    errors: Vec::new(),
                }),
                ..crate::models::AnalysisWorkflowTrace::default()
            },
            created_at: "2026-01-01T00:00:00Z".into(),
            disclaimer: "测试".into(),
        };
        db.save_analysis(&result, "如何控制风险？").unwrap();
        let history = db.analysis_history().unwrap();
        let audit = history[0].transparency.as_ref().unwrap();
        assert_eq!(audit.memory_items_used, 2);
        assert_eq!(audit.model_calls, 3);
        assert!(!audit.api_key_sent);
        assert_eq!(
            history[0].workflow_version.as_deref(),
            Some("investment-workflow-v4")
        );
        assert_eq!(history[0].verdict.as_deref(), Some("先控制风险"));
        let history_json = serde_json::to_value(&history[0]).unwrap();
        assert!(history_json.get("workflowTrace").is_none());
        assert_eq!(history_json["workflowVersion"], "investment-workflow-v4");
        assert!(audit.structured_output_validated);
        let stored = db.analysis("analysis-1").unwrap();
        assert_eq!(stored.question, "如何控制风险？");
        assert_eq!(stored.answer, "先控制风险");
        assert_eq!(
            stored.workflow_trace.unwrap().version,
            "investment-workflow-v4"
        );
        assert!(matches!(db.analysis("missing"), Err(AppError::NotFound(_))));
        let memory = db
            .memories()
            .unwrap()
            .into_iter()
            .find(|item| item.kind == "analysis")
            .unwrap();
        assert!(!memory.reviewed);
        assert!(memory.status.contains("未经结果验证"));
        assert_eq!(memory.content["workflowVersion"], "investment-workflow-v4");
    }

    #[test]
    fn reads_analysis_audit_created_before_evidence_fields_existed() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO analyses (id, question, answer, audit, created_at)
                 VALUES ('legacy','旧问题','旧回答',?1,'2026-01-01')",
                [r#"{"provider":"mock","model":"old","contextGroups":[],"payloadBytes":1,"contextRevision":"ctx-old","memoryItemsUsed":0,"externalDataUsed":false,"apiKeySent":false}"#],
            )
            .unwrap();
        let audit = db.analysis_history().unwrap()[0]
            .transparency
            .clone()
            .unwrap();
        assert_eq!(audit.evidence_items_used, 0);
        assert!(!audit.citations_required);
        assert_eq!(audit.model_calls, 0);
        assert_eq!(audit.reviewed_memory_items_used, 0);
        assert_eq!(audit.conflicting_memory_items_used, 0);
        assert!(!audit.structured_output_validated);
        assert_eq!(audit.output_repairs, 0);
        assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
    }

    #[test]
    fn adds_workflow_trace_column_to_an_existing_analysis_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE analyses (
                   id TEXT PRIMARY KEY,
                   question TEXT NOT NULL,
                   answer TEXT NOT NULL,
                   audit TEXT,
                   created_at TEXT NOT NULL
                 );
                 INSERT INTO analyses (id,question,answer,audit,created_at)
                 VALUES ('legacy','旧问题','旧回答',NULL,'2026-01-01');",
            )
            .unwrap();
        drop(connection);

        let db = Database::open(&path).unwrap();
        let trace_columns: i64 = db
            .conn()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('analyses') WHERE name = 'trace'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(trace_columns, 1);
        assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
    }

    #[test]
    fn reads_workflow_trace_created_before_structured_memory() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO analyses (id,question,answer,audit,trace,created_at)
                 VALUES ('old-trace','旧分析','旧回答',NULL,?1,'2026-01-01')",
                [r#"{"version":"investment-workflow-v2","researchPlan":"旧计划","alternatives":[],"critique":null,"calls":[]}"#],
            )
            .unwrap();

        assert_eq!(
            db.analysis_history().unwrap()[0]
                .workflow_version
                .as_deref(),
            Some("investment-workflow-v2")
        );
        let trace = db.analysis("old-trace").unwrap().workflow_trace.unwrap();
        assert_eq!(trace.version, "investment-workflow-v2");
        assert!(trace.memory_items.is_empty());
        assert!(trace.structured_report.is_none());
        assert!(trace.output_validation.is_none());
        assert!(trace.evidence_catalog.is_empty());
    }

    #[test]
    fn versions_investment_rules_without_overwriting_history() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let original = InvestmentRuleInput {
            category: "仓位".into(),
            statement: "单一主动仓位不超过 10%".into(),
            trigger: "任何新建或加仓决定".into(),
            rationale: "限制单一判断错误的永久损失".into(),
            active: true,
            source_review_id: None,
        };
        let created = db.add_investment_rule(&original).unwrap();
        let revised = db
            .update_investment_rule(
                &created.id,
                &InvestmentRuleInput {
                    statement: "单一主动仓位不超过 8%".into(),
                    ..original
                },
            )
            .unwrap();

        assert_eq!(revised.revision, 2);
        let history = db.investment_rule_history(&created.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].statement, "单一主动仓位不超过 8%");
        assert_eq!(history[1].statement, "单一主动仓位不超过 10%");
    }

    #[test]
    fn freezes_rule_checks_and_tracks_process_association_after_review() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let input = InvestmentRuleInput {
            category: "仓位".into(),
            statement: "单一主动仓位不超过 8%".into(),
            trigger: "任何新建或加仓决定".into(),
            rationale: "限制单一判断错误的永久损失".into(),
            active: true,
            source_review_id: None,
        };
        let rule = db.add_investment_rule(&input).unwrap();
        let decision = |id: &str, status: &str, note: &str| DecisionEntry {
            id: Some(id.into()),
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "主动基金".into(),
            thesis: "策略存在长期超额".into(),
            counter_thesis: "超额可能只是风格暴露".into(),
            expected_return_pct: 10.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 8.0,
            invalidation: "连续两期风格调整后仍无超额".into(),
            review_date: "2026-12-01".into(),
            rule_checks: vec![DecisionRuleCheck {
                rule_id: rule.id.clone(),
                rule_revision: rule.revision,
                category: "伪造类别".into(),
                statement: "伪造规则".into(),
                trigger: "伪造触发条件".into(),
                status: status.into(),
                note: note.into(),
            }],
        };

        let mut missing_check = decision("missing-check", "遵守", "");
        missing_check.rule_checks.clear();
        assert!(matches!(
            db.save_decision(&missing_check),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            db.save_decision(&decision("missing-note", "偏离", "")),
            Err(AppError::Validation(_))
        ));

        for (id, status, note, rating) in [
            ("followed-1", "遵守", "", 5),
            ("followed-2", "遵守", "", 4),
            ("deviated-1", "偏离", "因为已有相关敞口", 2),
        ] {
            db.save_decision(&decision(id, status, note)).unwrap();
            db.save_decision_review(
                id,
                &DecisionReviewInput {
                    outcome_summary: "按原计划完成复核".into(),
                    actual_return_pct: None,
                    thesis_status: "尚不明确".into(),
                    process_rating: rating,
                    lessons: "继续记录规则与过程之间的关系".into(),
                },
            )
            .unwrap();
        }

        let stored = db
            .decisions()
            .unwrap()
            .into_iter()
            .find(|item| item.id == "followed-1")
            .unwrap();
        assert_eq!(stored.rule_checks[0].category, "仓位");
        assert_eq!(stored.rule_checks[0].statement, input.statement);
        assert_eq!(stored.rule_checks[0].trigger, input.trigger);

        let revised = db
            .update_investment_rule(
                &rule.id,
                &InvestmentRuleInput {
                    statement: "单一主动仓位不超过 6%".into(),
                    ..input
                },
            )
            .unwrap();
        let summary = db.rule_effectiveness().unwrap();
        assert_eq!(summary.total_decisions, 3);
        assert_eq!(summary.evaluated_decisions, 3);
        assert!((summary.adherence_pct.unwrap() - 200.0 / 3.0).abs() < 1e-10);
        let item = &summary.rules[0];
        assert_eq!(item.current_revision, revised.revision);
        assert_eq!(item.observed_revisions, vec![1]);
        assert_eq!(item.followed_count, 2);
        assert_eq!(item.deviated_count, 1);
        assert_eq!(item.followed_process_average, Some(4.5));
        assert_eq!(item.deviated_process_average, Some(2.0));
        assert_eq!(item.signal, "遵守时过程评分更高");
    }

    #[test]
    fn system_review_freezes_portfolio_and_method_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.add_holding(&HoldingInput {
            symbol: "IDX".into(),
            name: "指数".into(),
            asset_class: "基金".into(),
            market_value: 100_000.0,
            cost_basis: 90_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
        db.add_investment_rule(&InvestmentRuleInput {
            category: "行为".into(),
            statement: "重大决定等待 24 小时".into(),
            trigger: "出现计划外交易冲动".into(),
            rationale: "降低情绪交易".into(),
            active: true,
            source_review_id: None,
        })
        .unwrap();
        let review = db
            .save_system_review(&SystemReviewInput {
                period_label: "2026 Q3".into(),
                adherence_score: 4,
                process_summary: "按计划定投，没有追涨".into(),
                rule_violations: "无".into(),
                lessons: "继续降低无效交易".into(),
                next_actions: "下季度检查再平衡".into(),
                next_review_date: "2026-12-31".into(),
            })
            .unwrap();
        let holding_id = db.snapshot().unwrap().holdings[0].id.clone();
        db.update_holding(
            &holding_id,
            &HoldingInput {
                symbol: "IDX".into(),
                name: "指数".into(),
                asset_class: "基金".into(),
                market_value: 200_000.0,
                cost_basis: 90_000.0,
                target_pct: 100.0,
                currency: "CNY".into(),
                fx_rate_to_base: None,
                valuation_date: "2026-02-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();

        let stored = db.system_reviews().unwrap().remove(0);
        assert_eq!(stored.id, review.id);
        assert_eq!(stored.snapshot.portfolio_value, 100_000.0);
        assert_eq!(stored.snapshot.active_rules, 1);
    }

    #[test]
    fn stores_immutable_research_evidence_and_allows_archiving() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let created = db
            .add_research_evidence(&ResearchEvidenceInput {
                asset_name: "全球指数".into(),
                title: "基金年度报告".into(),
                publisher: "基金管理人".into(),
                source_url: "https://example.com/annual-report".into(),
                source_tier: "一手来源".into(),
                evidence_type: "公司披露".into(),
                stance: "背景".into(),
                as_of_date: "2026-06-30".into(),
                claim: "报告披露了费用与跟踪误差".into(),
                notes: "复核费用变化".into(),
            })
            .unwrap();
        let archived = db.set_research_evidence_status(&created.id, false).unwrap();

        assert!(!archived.active);
        assert_eq!(archived.claim, created.claim);
        assert_eq!(db.research_evidence().unwrap().len(), 1);
    }

    #[test]
    fn rejects_non_https_research_sources() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let result = db.add_research_evidence(&ResearchEvidenceInput {
            asset_name: "指数".into(),
            title: "未知资料".into(),
            publisher: "未知".into(),
            source_url: "http://example.com/report".into(),
            source_tier: "媒体报道".into(),
            evidence_type: "新闻".into(),
            stance: "支持".into(),
            as_of_date: "2026-06-30".into(),
            claim: "未经验证的摘要".into(),
            notes: String::new(),
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
