use super::{
    params, validate_research_evidence, AnalysisHistoryItem, AnalysisResult, AppError, AppResult,
    Database, OptionalRow, ResearchEvidence, ResearchEvidenceInput, StoredAnalysis, Utc, Uuid,
};

impl Database {
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
            let grounding_status = parsed_trace
                .as_ref()
                .map(|trace| {
                    if trace.advice_grounding.is_empty() {
                        "unavailable"
                    } else if trace
                        .advice_grounding
                        .iter()
                        .any(|item| item.status == "contradicted")
                    {
                        "contradicted"
                    } else if trace
                        .advice_grounding
                        .iter()
                        .any(|item| item.status == "insufficient")
                    {
                        "insufficient"
                    } else if trace
                        .advice_grounding
                        .iter()
                        .any(|item| item.status != "supported")
                    {
                        "unavailable"
                    } else {
                        "supported"
                    }
                })
                .map(str::to_string);
            history.push(AnalysisHistoryItem {
                grounding_status,
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
