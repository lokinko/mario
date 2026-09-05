use std::{path::Path, sync::Mutex};

use chrono::Utc;
use rusqlite::{params, types::ValueRef, Connection, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        AnalysisHistoryItem, AnalysisResult, DecisionEntry, DecisionRecord, DecisionReview,
        DecisionReviewInput, FinancialProfile, Goal, GoalInput, Holding, HoldingInput,
        InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, MemoryItem, ModelConfig,
        ResearchEvidence, ResearchEvidenceInput, Snapshot, SystemReviewInput, SystemReviewRecord,
        SystemReviewSnapshot,
    },
    planning, risk,
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

struct SyncTableSpec {
    name: &'static str,
    columns: &'static [&'static str],
    order_by: &'static str,
}

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
        columns: &[
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
        ],
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
];

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
        if self.schema_version != 1 {
            return Err(AppError::Validation(format!(
                "不支持的数据快照版本 {}",
                self.schema_version
            )));
        }
        chrono::DateTime::parse_from_rfc3339(&self.exported_at)
            .map_err(|_| AppError::Validation("数据快照时间格式无效".into()))?;
        if self.tables.len() != SYNC_TABLES.len() {
            return Err(AppError::Validation("数据快照缺少必要数据表".into()));
        }
        let mut total_rows = 0_usize;
        for spec in SYNC_TABLES {
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
            schema_version: 1,
            exported_at: Utc::now().to_rfc3339(),
            tables,
        };
        dataset.validate()?;
        Ok(dataset)
    }

    pub fn import_sync_data(&self, dataset: &SyncDataset) -> AppResult<()> {
        dataset.validate()?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute_batch("PRAGMA defer_foreign_keys=ON;")?;

        for spec in SYNC_TABLES.iter().rev() {
            transaction.execute(&format!("DELETE FROM {}", quote_identifier(spec.name)), [])?;
        }
        for spec in SYNC_TABLES {
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

        let mut holding_stmt = conn.prepare("SELECT id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency FROM holdings ORDER BY created_at")?;
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
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

        Ok(build_snapshot(profile, goals, holdings, updated_at))
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
        let payload = serde_json::to_string(profile)?;
        self.conn()?.execute(
            "INSERT INTO profile (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![payload, Utc::now().to_rfc3339()],
        )?;
        self.snapshot()
    }

    pub fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot> {
        if input.name.trim().is_empty() || input.market_value <= 0.0 {
            return Err(AppError::Validation("资产名称和正数市值为必填项".into()));
        }
        validate_non_negative(&[input.market_value, input.cost_basis, input.target_pct])?;
        validate_percentage(input.target_pct, "目标权重")?;
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO holdings (id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![Uuid::new_v4().to_string(), input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency, now],
        )?;
        self.snapshot()
    }

    pub fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot> {
        if input.name.trim().is_empty() || input.market_value <= 0.0 {
            return Err(AppError::Validation("资产名称和正数市值为必填项".into()));
        }
        validate_non_negative(&[input.market_value, input.cost_basis, input.target_pct])?;
        validate_percentage(input.target_pct, "目标权重")?;
        let affected = self.conn()?.execute(
            "UPDATE holdings SET symbol=?2, name=?3, asset_class=?4, market_value=?5, cost_basis=?6, target_pct=?7, currency=?8, updated_at=?9 WHERE id=?1",
            params![id, input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency, Utc::now().to_rfc3339()],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        }
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
        let id = entry
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut stored = entry.clone();
        stored.id = Some(id.clone());
        self.conn()?.execute(
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
                asset_name: entry.asset_name,
                thesis: entry.thesis,
                counter_thesis: entry.counter_thesis,
                expected_return_pct: entry.expected_return_pct,
                downside_pct: entry.downside_pct,
                confidence_pct: entry.confidence_pct,
                position_pct: entry.position_pct,
                invalidation: entry.invalidation,
                review_date: entry.review_date,
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

    pub fn save_system_review(&self, input: &SystemReviewInput) -> AppResult<SystemReviewRecord> {
        validate_system_review(input)?;
        let portfolio = self.snapshot()?;
        let decisions = self.decisions()?;
        let rules = self.investment_rules()?;
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let snapshot = SystemReviewSnapshot {
            portfolio_value: portfolio.total_value,
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
        let mut items = Vec::new();
        for record in self.decisions()?.into_iter().take(50) {
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
                    "review": review_payload,
                }),
                created_at: record.created_at,
                occurred_at,
                status: status.clone(),
                reviewed,
                contradiction,
                tags: vec![record.asset_name, "投资决策".into(), status],
                selected: true,
                retrieval: None,
            });
        }
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, question, answer, trace, created_at FROM analyses ORDER BY created_at DESC LIMIT 20")?;
        for item in stmt.query_map([], |row| {
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
                selected: true,
                retrieval: None,
            })
        })? {
            items.push(item?);
        }
        Ok(items)
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
            history.push(AnalysisHistoryItem {
                id,
                question,
                created_at,
                transparency: audit
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
                workflow_trace: trace
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
            });
        }
        Ok(history)
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
    updated_at: String,
) -> Snapshot {
    let raw_total = holdings.iter().map(|h| h.market_value).sum::<f64>();
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
    let largest = holdings
        .iter()
        .map(|h| h.market_value)
        .fold(0.0_f64, f64::max);
    let concentration_pct = if total_value > 0.0 {
        largest / total_value * 100.0
    } else {
        0.0
    };
    let mut findings = risk::analyze(&profile, &holdings);
    let plan = planning::analyze(&profile, &goals, &holdings);
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
                "计划每月投入 {:.0} 元，但当前月度结余约 {:.0} 元。",
                plan.committed_monthly, plan.monthly_surplus
            ),
            action: "调整目标优先级、期限或月度投入，避免计划依赖新增负债。".into(),
        });
    }
    Snapshot {
        profile,
        goals,
        holdings,
        findings,
        total_value,
        emergency_months,
        concentration_pct,
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
        })
        .unwrap();
        let snapshot = db.snapshot().unwrap();
        assert_eq!(snapshot.holdings.len(), 1);
        assert_eq!(snapshot.emergency_months, 6.0);
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
            })
            .unwrap();
        source.set_setting("model.name", "never-sync-this").unwrap();

        let dataset = source.export_sync_data().unwrap();
        let second_export = source.export_sync_data().unwrap();
        assert_eq!(
            dataset.content_hash().unwrap(),
            second_export.content_hash().unwrap()
        );

        let target = Database::open(&directory.path().join("target.db")).unwrap();
        target
            .set_setting("model.name", "keep-local-model")
            .unwrap();
        target.import_sync_data(&dataset).unwrap();
        let restored = target.snapshot().unwrap();
        assert_eq!(restored.holdings.len(), 1);
        assert_eq!(restored.profile.emergency_fund, 48_000.0);
        assert_eq!(
            target.setting("model.name").unwrap().as_deref(),
            Some("keep-local-model")
        );
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
    fn stores_decision_review_separately_from_original_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.save_decision(&DecisionEntry {
            id: None,
            asset_name: "指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值过高".into(),
            expected_return_pct: 12.0,
            downside_pct: 20.0,
            confidence_pct: 65.0,
            position_pct: 30.0,
            invalidation: "风险容量改变".into(),
            review_date: "2026-12-01".into(),
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
            asset_name: "指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值过高".into(),
            expected_return_pct: 8.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 30.0,
            invalidation: "风险容量下降".into(),
            review_date: "".into(),
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
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
                 INSERT INTO goals VALUES ('g1','养老',1000000,'2036-12-31','重要','2026-01-01');
                 INSERT INTO holdings VALUES ('h1','IDX','指数','基金',100000,90000,'CNY','2026-01-01');
                 INSERT INTO analyses VALUES ('a1','旧问题','旧回答','2026-01-01');",
            )
            .unwrap();
        drop(connection);

        let db = Database::open(&path).unwrap();
        let snapshot = db.snapshot().unwrap();
        assert_eq!(snapshot.goals[0].current_amount, 0.0);
        assert_eq!(snapshot.goals[0].monthly_contribution, 0.0);
        assert_eq!(snapshot.holdings[0].target_pct, 0.0);
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
            history[0].workflow_trace.as_ref().unwrap().version,
            "investment-workflow-v4"
        );
        assert!(history[0]
            .workflow_trace
            .as_ref()
            .unwrap()
            .structured_report
            .is_some());
        assert!(audit.structured_output_validated);
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
        assert!(db.analysis_history().unwrap()[0].workflow_trace.is_none());
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
        assert!(db.analysis_history().unwrap()[0].workflow_trace.is_none());
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

        let trace = db.analysis_history().unwrap()[0]
            .workflow_trace
            .clone()
            .unwrap();
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
