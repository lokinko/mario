use super::{
    params, validate_research_evidence, AnalysisHistoryItem, AnalysisResult, AppError, AppResult,
    Database, OptionalRow, ResearchEvidence, ResearchEvidenceInput, StoredAnalysis, Utc, Uuid,
};

impl Database {
    /// Reuse provider search records from saved answers, without promoting an
    /// assistant's verdict or personal-data inference into a verified fact.
    pub fn automatic_research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT trace FROM analyses WHERE created_at >= ?1 ORDER BY created_at DESC, id DESC LIMIT 50",
        )?;
        let cutoff = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let traces = statement.query_map([cutoff], |row| row.get::<_, Option<String>>(0))?;
        let mut seen = std::collections::HashSet::new();
        let mut evidence = Vec::new();
        for trace in traces {
            let Some(trace) = trace?.and_then(|text| {
                serde_json::from_str::<crate::models::AnalysisWorkflowTrace>(&text).ok()
            }) else {
                continue;
            };
            for source in trace.evidence_catalog {
                if !source.id.starts_with("web-")
                    || !matches!(
                        source.evidence_type.as_str(),
                        "native_web_excerpt" | "native_web_summary"
                    )
                    || source.claim.trim().is_empty()
                    || !chrono::DateTime::parse_from_rfc3339(&source.captured_at)
                        .is_ok_and(|date| date >= Utc::now() - chrono::Duration::days(30))
                    || !reqwest::Url::parse(&source.source_url).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https")
                            && url.username().is_empty()
                            && url.password().is_none()
                    })
                    || !seen.insert((source.source_url.clone(), source.claim.clone()))
                {
                    continue;
                }
                evidence.push(ResearchEvidence {
                    id: source.id,
                    asset_name: source.asset_name,
                    title: source.title,
                    publisher: source.publisher,
                    source_url: source.source_url,
                    source_tier: source.source_tier,
                    evidence_type: source.evidence_type,
                    stance: source.stance,
                    as_of_date: source.as_of_date,
                    claim: source.claim,
                    notes: format!(
                        "mario 自动整理的历史搜索资料；不是当前行情，使用前需核对时效。{}",
                        source.notes
                    ),
                    active: true,
                    captured_at: source.captured_at,
                });
                if evidence.len() >= 200 {
                    return Ok(evidence);
                }
            }
        }
        Ok(evidence)
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

#[cfg(test)]
mod automatic_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn automatic_library_deduplicates_sources_without_promoting_answers_or_refreshing_old_captures()
    {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("research.db")).unwrap();
        let now = Utc::now().to_rfc3339();
        let old = (Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        let source = json!({"id":"web-original","assetName":"","evidenceType":"native_web_summary","claim":"Search summary, not verified original text","notes":"Original qualification","stance":"背景","capturedAt":now,"title":"Documentation","publisher":"example.com","sourceUrl":"https://example.com/docs","sourceTier":"网页来源（待核实）","asOfDate":""});
        let mut stale = source.clone();
        stale["id"] = json!("web-old");
        stale["capturedAt"] = json!(old);
        stale["claim"] = json!("Old quote copied into a new answer");
        let mut manual = source.clone();
        manual["id"] = json!("manual-source");
        manual["claim"] = json!("Manual source is not automatically reactivated");
        let mut unsafe_url = source.clone();
        unsafe_url["sourceUrl"] = json!("file:///private");
        let trace = crate::models::AnalysisWorkflowTrace {
            evidence_catalog: serde_json::from_value(json!([
                source.clone(),
                source,
                stale,
                manual,
                unsafe_url
            ]))
            .unwrap(),
            ..Default::default()
        };
        db.conn().unwrap().execute("INSERT INTO analyses (id,question,answer,trace,created_at) VALUES ('a','question','An invented model conclusion',?1,?2)", params![serde_json::to_string(&trace).unwrap(),now]).unwrap();
        let before = db.snapshot().unwrap().holdings;
        let results = db.automatic_research_evidence().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].evidence_type, "native_web_summary");
        assert_eq!(results[0].captured_at, now);
        assert!(results[0].as_of_date.is_empty());
        assert!(results[0].notes.contains("不是当前行情"));
        assert!(db.research_evidence().unwrap().is_empty());
        assert_eq!(db.snapshot().unwrap().holdings.len(), before.len());
    }
}
