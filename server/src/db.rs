use std::{path::Path, sync::Mutex};

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        DecisionEntry, FinancialProfile, Goal, GoalInput, Holding, HoldingInput, MemoryItem,
        ModelConfig, Snapshot,
    },
    risk,
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
               target_date TEXT NOT NULL,
               priority TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS holdings (
               id TEXT PRIMARY KEY,
               symbol TEXT NOT NULL,
               name TEXT NOT NULL,
               asset_class TEXT NOT NULL,
               market_value REAL NOT NULL,
               cost_basis REAL NOT NULL,
               currency TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS decisions (
               id TEXT PRIMARY KEY,
               asset_name TEXT NOT NULL,
               payload TEXT NOT NULL,
               review_date TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS analyses (
               id TEXT PRIMARY KEY,
               question TEXT NOT NULL,
               answer TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
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
            "SELECT id, name, target_amount, target_date, priority FROM goals ORDER BY created_at",
        )?;
        let goals = goal_stmt
            .query_map([], |row| {
                Ok(Goal {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    target_amount: row.get(2)?,
                    target_date: row.get(3)?,
                    priority: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut holding_stmt = conn.prepare("SELECT id, symbol, name, asset_class, market_value, cost_basis, currency FROM holdings ORDER BY created_at")?;
        let holdings = holding_stmt
            .query_map([], |row| {
                Ok(Holding {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    name: row.get(2)?,
                    asset_class: row.get(3)?,
                    market_value: row.get(4)?,
                    cost_basis: row.get(5)?,
                    currency: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(holding_stmt);
        drop(goal_stmt);

        Ok(build_snapshot(profile, goals, holdings))
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
        validate_non_negative(&[input.market_value, input.cost_basis])?;
        self.conn()?.execute(
            "INSERT INTO holdings (id, symbol, name, asset_class, market_value, cost_basis, currency, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![Uuid::new_v4().to_string(), input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.currency, Utc::now().to_rfc3339()],
        )?;
        self.snapshot()
    }

    pub fn add_goal(&self, input: &GoalInput) -> AppResult<Snapshot> {
        if input.name.trim().is_empty()
            || input.target_amount <= 0.0
            || input.target_date.trim().is_empty()
        {
            return Err(AppError::Validation("目标名称、金额和日期为必填项".into()));
        }
        self.conn()?.execute(
            "INSERT INTO goals (id, name, target_amount, target_date, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![Uuid::new_v4().to_string(), input.name.trim(), input.target_amount, input.target_date, input.priority, Utc::now().to_rfc3339()],
        )?;
        self.snapshot()
    }

    pub fn save_decision(&self, entry: &DecisionEntry) -> AppResult<()> {
        if entry.asset_name.trim().is_empty()
            || entry.thesis.trim().is_empty()
            || entry.counter_thesis.trim().is_empty()
            || entry.invalidation.trim().is_empty()
        {
            return Err(AppError::Validation(
                "投资对象、正反逻辑和证伪条件为必填项".into(),
            ));
        }
        validate_non_negative(&[entry.confidence_pct, entry.position_pct])?;
        if entry.confidence_pct > 100.0 || entry.position_pct > 100.0 {
            return Err(AppError::Validation("置信度和仓位不能超过 100%".into()));
        }
        let id = entry
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        self.conn()?.execute(
            "INSERT INTO decisions (id, asset_name, payload, review_date, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, entry.asset_name.trim(), serde_json::to_string(entry)?, entry.review_date, Utc::now().to_rfc3339()],
        )?;
        Ok(())
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
        let conn = self.conn()?;
        let mut items = Vec::new();
        let mut stmt = conn.prepare("SELECT id, asset_name, payload, created_at FROM decisions ORDER BY created_at DESC LIMIT 50")?;
        for item in stmt.query_map([], |row| {
            Ok(MemoryItem {
                id: row.get(0)?,
                kind: "decision".into(),
                title: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })? {
            items.push(item?);
        }
        let mut stmt = conn.prepare("SELECT id, question, answer, created_at FROM analyses ORDER BY created_at DESC LIMIT 20")?;
        for item in stmt.query_map([], |row| {
            Ok(MemoryItem {
                id: row.get(0)?,
                kind: "analysis".into(),
                title: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })? {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn save_analysis(
        &self,
        id: &str,
        question: &str,
        answer: &str,
        created_at: &str,
    ) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO analyses (id, question, answer, created_at) VALUES (?1,?2,?3,?4)",
            params![id, question, answer, created_at],
        )?;
        Ok(())
    }
}

fn build_snapshot(profile: FinancialProfile, goals: Vec<Goal>, holdings: Vec<Holding>) -> Snapshot {
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
    let findings = risk::analyze(&profile, &holdings);
    Snapshot {
        profile,
        goals,
        holdings,
        findings,
        total_value,
        emergency_months,
        concentration_pct,
        updated_at: Utc::now().to_rfc3339(),
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
            currency: "CNY".into(),
        })
        .unwrap();
        let snapshot = db.snapshot().unwrap();
        assert_eq!(snapshot.holdings.len(), 1);
        assert_eq!(snapshot.emergency_months, 6.0);
    }
}
