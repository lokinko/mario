use super::{
    params, truncate_chars, AppError, AppResult, Database, HashMap, MemoryItem,
    MemoryPreferenceInput, Utc,
};

impl Database {
    pub fn memories(&self) -> AppResult<Vec<MemoryItem>> {
        let preferences = self.memory_preference_map()?;
        let mut items = Vec::new();
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

    pub(super) fn memory_preference_map(
        &self,
    ) -> AppResult<HashMap<String, (String, String, String)>> {
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
}
