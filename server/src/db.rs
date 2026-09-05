use std::{path::Path, sync::Mutex};

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        AnalysisHistoryItem, AnalysisResult, DecisionEntry, DecisionRecord, DecisionReview,
        DecisionReviewInput, FinancialProfile, Goal, GoalInput, Holding, HoldingInput, MemoryItem,
        ModelConfig, Snapshot,
    },
    planning, risk,
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
               created_at TEXT NOT NULL
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

    pub fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO analyses (id, question, answer, audit, created_at) VALUES (?1,?2,?3,?4,?5)",
            params![
                result.id,
                question,
                result.answer,
                serde_json::to_string(&result.transparency)?,
                result.created_at
            ],
        )?;
        Ok(())
    }

    pub fn analysis_history(&self) -> AppResult<Vec<AnalysisHistoryItem>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, question, created_at, audit FROM analyses ORDER BY created_at DESC LIMIT 50",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut history = Vec::new();
        for row in rows {
            let (id, question, created_at, audit) = row?;
            history.push(AnalysisHistoryItem {
                id,
                question,
                created_at,
                transparency: audit
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
            });
        }
        Ok(history)
    }
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
                external_data_used: false,
                api_key_sent: false,
            },
            created_at: "2026-01-01T00:00:00Z".into(),
            disclaimer: "测试".into(),
        };
        db.save_analysis(&result, "如何控制风险？").unwrap();
        let history = db.analysis_history().unwrap();
        let audit = history[0].transparency.as_ref().unwrap();
        assert_eq!(audit.memory_items_used, 2);
        assert!(!audit.api_key_sent);
    }
}
