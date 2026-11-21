//! Long-term memory notes and the dreaming watermark, owned by the account schema.
use crate::models::memory::{
    AcceptedOperation, DreamInput, DreamSourceAnalysis, DreamState, DreamSummary, MemoryKind,
    MemoryNote, MemoryOrigin, DREAM_ANSWER_CHARS, DREAM_BATCH, MEMORY_ACTIVE_LIMIT,
    MEMORY_QUOTE_CHARS, MEMORY_RATIONALE_CHARS, MEMORY_SOURCES_MAX, MEMORY_STATEMENT_CHARS,
};
use crate::storage::{Connection, Row};
use {
    super::settings::ai::AiSetting,
    crate::{
        db::Database,
        storage::{params, OptionalRow},
        AppError, AppResult,
    },
};

const DREAM_STATE_KEY: AiSetting = AiSetting::DreamState;
const SELECT_NOTES: &str = "SELECT id,kind,origin,statement,rationale,source_analysis_ids,quote,active,created_at,updated_at FROM analysis_memories";

impl Database {
    /// Active notes first, then most recently updated.
    pub fn memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        let conn = self.conn()?;
        read_notes(&conn)
    }

    pub fn active_memory_notes(&self) -> AppResult<Vec<MemoryNote>> {
        Ok(self
            .memory_notes()?
            .into_iter()
            .filter(|note| note.active)
            .collect())
    }

    pub fn set_memory_status(&self, id: &str, active: bool) -> AppResult<MemoryNote> {
        let conn = self.conn()?;
        if active {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM analysis_memories WHERE active=1 AND id<>$1",
                [id],
                |row| row.get(0),
            )?;
            if count as usize >= MEMORY_ACTIVE_LIMIT {
                return Err(AppError::Validation(format!(
                    "有效记忆最多 {MEMORY_ACTIVE_LIMIT} 条，请先归档其他记忆"
                )));
            }
        }
        conn.execute(
            "UPDATE analysis_memories SET active=$2, updated_at=$3 WHERE id=$1 AND active<>$2",
            params![id, i64::from(active), chrono::Utc::now().to_rfc3339()],
        )?;
        drop(conn);
        self.memory_notes()?
            .into_iter()
            .find(|note| note.id == id)
            .ok_or_else(|| AppError::NotFound("找不到这条记忆".into()))
    }

    pub fn delete_memory(&self, id: &str) -> AppResult<()> {
        if self
            .conn()?
            .execute("DELETE FROM analysis_memories WHERE id=$1", [id])?
            == 0
        {
            return Err(AppError::NotFound("找不到这条记忆".into()));
        }
        Ok(())
    }

    pub fn dream_state(&self) -> AppResult<DreamState> {
        let conn = self.conn()?;
        read_dream_state(&conn)
    }

    /// Completed analyses after the watermark, oldest first, and how many remain in total.
    pub fn undreamed_analyses(&self) -> AppResult<(Vec<DreamSourceAnalysis>, usize)> {
        let conn = self.conn()?;
        let through = read_dream_state(&conn)?.dreamed_through.unwrap_or_default();
        let pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM analyses WHERE created_at > $1",
            [&through],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT id,created_at,question,answer FROM analyses WHERE created_at > $1 ORDER BY created_at, id LIMIT $2",
        )?;
        let analyses = statement
            .query_map(params![through, DREAM_BATCH as i64], |row| {
                Ok(DreamSourceAnalysis {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    question: row.get(2)?,
                    answer: row
                        .get::<_, String>(3)?
                        .chars()
                        .take(DREAM_ANSWER_CHARS)
                        .collect(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((analyses, pending as usize))
    }

    /// Apply screened operations and advance the watermark atomically.
    pub fn apply_dream(
        &self,
        input: &DreamInput,
        operations: &[AcceptedOperation],
        rejected: Vec<String>,
    ) -> AppResult<DreamSummary> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut summary = DreamSummary {
            analyses: input.analyses.len(),
            rejected,
            ..DreamSummary::default()
        };
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for operation in operations {
            match operation {
                AcceptedOperation::Add {
                    kind,
                    origin,
                    statement,
                    rationale,
                    source_analysis_ids,
                    quote,
                } => {
                    tx.execute(
                        "INSERT INTO analysis_memories(id,kind,origin,statement,rationale,source_analysis_ids,quote,active,created_at,updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,1,$8,$8)",
                        params![
                            format!("mem-{}", uuid::Uuid::new_v4()),
                            kind.as_str(),
                            origin.as_str(),
                            statement,
                            rationale,
                            serde_json::to_string(source_analysis_ids)?,
                            quote,
                            now
                        ],
                    )?;
                    summary.added += 1;
                }
                AcceptedOperation::Update {
                    id,
                    statement,
                    rationale,
                    source_analysis_ids,
                    quote,
                } => {
                    tx.execute(
                        "UPDATE analysis_memories SET statement=$2, rationale=$3, source_analysis_ids=$4, quote=$5, updated_at=$6 WHERE id=$1 AND active=1",
                        params![
                            id,
                            statement,
                            rationale,
                            serde_json::to_string(source_analysis_ids)?,
                            quote,
                            now
                        ],
                    )?;
                    summary.updated += 1;
                }
                AcceptedOperation::Archive { id } => {
                    tx.execute(
                        "UPDATE analysis_memories SET active=0, updated_at=$2 WHERE id=$1 AND active=1",
                        params![id, now],
                    )?;
                    summary.archived += 1;
                }
            }
        }
        let mut state = read_dream_state(&tx)?;
        if let Some(newest) = input.analyses.iter().map(|a| a.created_at.clone()).max() {
            state.dreamed_through = Some(newest);
        }
        state.last_run_at = Some(now);
        state.last_result = Some(summary.clone());
        tx.execute(
            "INSERT INTO settings(key,value) VALUES ($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![DREAM_STATE_KEY.key(), serde_json::to_string(&state)?],
        )?;
        validate(&tx)?;
        tx.commit()?;
        Ok(summary)
    }
}

fn read_dream_state(conn: &Connection) -> AppResult<DreamState> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=$1",
            [DREAM_STATE_KEY.key()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match stored {
        Some(value) => serde_json::from_str(&value)?,
        None => DreamState::default(),
    })
}

fn read_notes(conn: &Connection) -> AppResult<Vec<MemoryNote>> {
    let mut statement = conn.prepare(&format!(
        "{SELECT_NOTES} ORDER BY active DESC, updated_at DESC, id"
    ))?;
    let rows = statement
        .query_map([], |row| Ok(raw_note(row)))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(|row| row?.into_note()).collect()
}

struct RawNote {
    id: String,
    kind: String,
    origin: String,
    statement: String,
    rationale: String,
    sources: String,
    quote: String,
    active: i64,
    created_at: String,
    updated_at: String,
}

fn raw_note(row: &Row) -> crate::storage::Result<RawNote> {
    Ok(RawNote {
        id: row.get(0)?,
        kind: row.get(1)?,
        origin: row.get(2)?,
        statement: row.get(3)?,
        rationale: row.get(4)?,
        sources: row.get(5)?,
        quote: row.get(6)?,
        active: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

impl RawNote {
    fn into_note(self) -> AppResult<MemoryNote> {
        let invalid = || AppError::Validation(format!("记忆 {} 的格式无效", self.id));
        Ok(MemoryNote {
            kind: MemoryKind::parse(&self.kind).ok_or_else(invalid)?,
            origin: MemoryOrigin::parse(&self.origin).ok_or_else(invalid)?,
            source_analysis_ids: serde_json::from_str(&self.sources).map_err(|_| invalid())?,
            active: self.active == 1,
            id: self.id,
            statement: self.statement,
            rationale: self.rationale,
            quote: self.quote,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// Backup imports and dreaming both end here; a malformed note rolls back the transaction.
pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    let notes = read_notes(conn)?;
    let bounded = |value: &str, max: usize| (1..=max).contains(&value.trim().chars().count());
    for note in &notes {
        let ok = bounded(&note.statement, MEMORY_STATEMENT_CHARS)
            && bounded(&note.rationale, MEMORY_RATIONALE_CHARS)
            && bounded(&note.quote, MEMORY_QUOTE_CHARS)
            && (1..=MEMORY_SOURCES_MAX).contains(&note.source_analysis_ids.len())
            && !(note.origin == MemoryOrigin::ModelInferred && note.kind != MemoryKind::Viewpoint)
            && [&note.created_at, &note.updated_at]
                .iter()
                .all(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok());
        if !ok {
            return Err(AppError::Validation(format!("记忆 {} 的内容无效", note.id)));
        }
    }
    if notes.iter().filter(|note| note.active).count() > MEMORY_ACTIVE_LIMIT {
        return Err(AppError::Validation(format!(
            "有效记忆超过 {MEMORY_ACTIVE_LIMIT} 条上限"
        )));
    }
    read_dream_state(conn)?.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::screen_operations;
    use crate::models::memory::{DreamOperation, DreamProposal};

    fn analysis(db: &Database, id: &str, at: &str, question: &str) {
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO analyses(id,question,answer,created_at) VALUES ($1,$2,'回答原文',$3)",
                params![id, question, at],
            )
            .unwrap();
    }

    fn preference(quote: &str) -> DreamOperation {
        DreamOperation::Add {
            kind: MemoryKind::Preference,
            origin: MemoryOrigin::UserStated,
            statement: format!("偏好：{quote}"),
            rationale: "用户原话".into(),
            source_analysis_ids: vec!["a1".into()],
            quote: quote.into(),
        }
    }

    #[test]
    fn dreaming_applies_atomically_advances_the_watermark_and_round_trips_backups() {
        let db = Database::test();
        analysis(
            &db,
            "a1",
            "2026-11-16T01:00:00+00:00",
            "我只想先讨论分红持续性",
        );
        analysis(&db, "a2", "2026-11-16T02:00:00+00:00", "第二个问题");
        let (analyses, pending) = db.undreamed_analyses().unwrap();
        assert_eq!((analyses.len(), pending), (2, 2));
        let input = DreamInput {
            analyses,
            memories: db.active_memory_notes().unwrap(),
        };
        let proposal = DreamProposal {
            operations: vec![preference("只想先讨论分红持续性")],
        };
        let (accepted, rejected) = screen_operations(&input, &proposal);
        let summary = db.apply_dream(&input, &accepted, rejected).unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(db.undreamed_analyses().unwrap().1, 0);
        let state = db.dream_state().unwrap();
        assert_eq!(
            state.dreamed_through.as_deref(),
            Some("2026-11-16T02:00:00+00:00")
        );

        let exported = db.export_sync_data().unwrap();
        let target = Database::test();
        target.import_sync_data(&exported).unwrap();
        assert_eq!(target.memory_notes().unwrap(), db.memory_notes().unwrap());
        assert_eq!(target.dream_state().unwrap(), state);

        let id = db.memory_notes().unwrap()[0].id.clone();
        assert!(!db.set_memory_status(&id, false).unwrap().active);
        assert!(db.active_memory_notes().unwrap().is_empty());
        db.delete_memory(&id).unwrap();
        assert!(matches!(db.delete_memory(&id), Err(AppError::NotFound(_))));
    }

    #[test]
    fn a_failed_write_keeps_notes_and_watermark_unchanged() {
        let db = Database::test();
        analysis(
            &db,
            "a1",
            "2026-11-16T01:00:00+00:00",
            "我只想先讨论分红持续性",
        );
        let (analyses, _) = db.undreamed_analyses().unwrap();
        let input = DreamInput {
            analyses,
            memories: Vec::new(),
        };
        let (accepted, _) = screen_operations(
            &input,
            &DreamProposal {
                operations: vec![preference("只想先讨论分红持续性")],
            },
        );
        crate::db::fail_inserts(&db.conn().unwrap(), "analysis_memories", "fixture failure");
        assert!(db.apply_dream(&input, &accepted, Vec::new()).is_err());
        assert!(db.memory_notes().unwrap().is_empty());
        assert_eq!(db.dream_state().unwrap(), DreamState::default());
        assert_eq!(db.undreamed_analyses().unwrap().1, 1);
    }

    #[test]
    fn imports_reject_model_inferred_preferences_and_outdated_backups() {
        let db = Database::test();
        db.conn().unwrap().execute_batch(
            "INSERT INTO analysis_memories VALUES ('m1','preference','modelInferred','偏好','理由','[\"a1\"]','原文',1,'2026-11-16T00:00:00+00:00','2026-11-16T00:00:00+00:00')",
        ).unwrap();
        let exported = db.export_sync_data().unwrap();
        assert!(Database::test().import_sync_data(&exported).is_err());

        let mut old = exported;
        old.schema_version = 30;
        let target = Database::test();
        assert!(target.import_sync_data(&old).is_err());
    }
}
