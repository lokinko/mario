//! AI history and historical web excerpts; never promotes excerpts into user originals.
use crate::models::{
    analysis::{AnalysisHistoryItem, AnalysisResult, StoredAnalysis},
    research::ResearchEvidence,
};
use {
    crate::{
        db::Database,
        storage::{params, OptionalRow},
        AppError, AppResult,
    },
    chrono::Utc,
};
impl Database {
    /// Reuse provider search records from saved answers, without promoting an
    /// assistant's verdict or personal-data inference into a verified fact.
    pub fn automatic_research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT trace FROM analyses WHERE created_at >= $1 ORDER BY created_at DESC, id DESC LIMIT 50",
        )?;
        let cutoff = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let traces = statement.query_map([cutoff], |row| row.get::<_, Option<String>>(0))?;
        let mut seen = std::collections::HashSet::new();
        let mut evidence = Vec::new();
        for trace in traces {
            let Some(trace) = trace?.and_then(|text| {
                serde_json::from_str::<crate::models::analysis::AnalysisWorkflowTrace>(&text).ok()
            }) else {
                continue;
            };
            for source in trace.evidence_catalog {
                // Provider-written summaries from older runs are leads, never evidence.
                if !source.id.starts_with("web-")
                    || source.evidence_type != "native_web_excerpt"
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
                    category: String::new(),
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
                    attachments: Vec::new(),
                });
                if evidence.len() >= 200 {
                    return Ok(evidence);
                }
            }
        }
        Ok(evidence)
    }

    pub fn save_analysis(&self, result: &AnalysisResult, question: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO analyses (id, question, answer, audit, trace, created_at) VALUES ($1,$2,$3,$4,$5,$6)",
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
        // Project only the small summary fields in PostgreSQL so large evidence catalogs
        // and multi-stage reasoning payloads are never transferred or deserialized for lists.
        let mut statement = conn.prepare(
            "SELECT id, question, created_at, audit,
                    CASE WHEN trace IS NOT NULL AND left(ltrim(trace), 1) = '{'
                         THEN jsonb_build_object(
                           'version', trace::jsonb->'version',
                           'verdict', trace::jsonb->'structuredReport'->>'answer'
                         )::text
                         ELSE NULL
                    END
             FROM analyses
             ORDER BY created_at DESC, id DESC
             LIMIT 50",
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
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TraceSummary {
            version: Option<String>,
            verdict: Option<String>,
        }
        let mut history = Vec::new();
        for row in rows {
            let (id, question, created_at, audit, summary) = row?;
            let parsed = summary
                .as_deref()
                .and_then(|value| serde_json::from_str::<TraceSummary>(value).ok());
            history.push(AnalysisHistoryItem {
                id,
                question,
                created_at,
                transparency: audit
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
                workflow_version: parsed
                    .as_ref()
                    .and_then(|item| item.version.clone())
                    .filter(|value| !value.is_empty()),
                verdict: parsed.and_then(|item| item.verdict),
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
                "SELECT id, question, answer, created_at, audit, trace FROM analyses WHERE id=$1",
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
    fn automatic_library_deduplicates_sources_without_promoting_answers_summaries_or_old_captures()
    {
        let db = Database::test();
        let now = Utc::now().to_rfc3339();
        let old = (Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        let source = json!({"id":"web-original","assetName":"","evidenceType":"native_web_excerpt","claim":"Provider-returned original excerpt","notes":"Original qualification","stance":"背景","capturedAt":now,"title":"Documentation","publisher":"example.com","sourceUrl":"https://example.com/docs","sourceTier":"网页来源（待核实）","asOfDate":""});
        let mut summary = source.clone();
        summary["id"] = json!("web-summary");
        summary["evidenceType"] = json!("native_web_summary");
        summary["claim"] = json!("Search summary, not verified original text");
        let mut stale = source.clone();
        stale["id"] = json!("web-old");
        stale["capturedAt"] = json!(old);
        stale["claim"] = json!("Old quote copied into a new answer");
        let mut manual = source.clone();
        manual["id"] = json!("manual-source");
        manual["claim"] = json!("Manual source is not automatically reactivated");
        let mut unsafe_url = source.clone();
        unsafe_url["sourceUrl"] = json!("file:///private");
        let trace = crate::models::analysis::AnalysisWorkflowTrace {
            evidence_catalog: serde_json::from_value(json!([
                source.clone(),
                source,
                summary,
                stale,
                manual,
                unsafe_url
            ]))
            .unwrap(),
            ..Default::default()
        };
        db.conn().unwrap().execute("INSERT INTO analyses (id,question,answer,trace,created_at) VALUES ('a','question','An invented model conclusion',$1,$2)", params![serde_json::to_string(&trace).unwrap(),now]).unwrap();
        let before = db.snapshot().unwrap().holdings;
        let results = db.automatic_research_evidence().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].evidence_type, "native_web_excerpt");
        assert_eq!(results[0].captured_at, now);
        assert!(results[0].as_of_date.is_empty());
        assert!(results[0].notes.contains("不是当前行情"));
        assert!(db.research_evidence().unwrap().is_empty());
        assert_eq!(db.snapshot().unwrap().holdings.len(), before.len());
    }

    #[test]
    fn analysis_history_projection_handles_current_malformed_and_limit() {
        let db = Database::test();
        let conn = db.conn().unwrap();
        let rows: Vec<(&str, &str, Option<String>)> = vec![
            (
                "a-current",
                "2026-11-20T10:00:01Z",
                Some(
                    json!({
                        "version": "investment-workflow-v1",
                        "structuredReport": { "answer": "当前结论标题" },
                        "evidenceCatalog": [{ "id": "huge", "claim": "x".repeat(4000) }]
                    })
                    .to_string(),
                ),
            ),
            (
                "a-empty-version",
                "2026-11-20T10:00:02Z",
                Some(json!({ "version": "" }).to_string()),
            ),
            ("a-null-trace", "2026-11-20T10:00:03Z", None),
            (
                "a-malformed",
                "2026-11-20T10:00:04Z",
                Some("not-json-at-all".into()),
            ),
        ];
        for (id, created_at, trace) in rows {
            conn.execute(
                "INSERT INTO analyses (id, question, answer, audit, trace, created_at) VALUES ($1,$2,$3,NULL,$4,$5)",
                params![id, format!("q-{id}"), "answer", trace, created_at],
            )
            .unwrap();
        }
        drop(conn);
        let items = db.analysis_history().unwrap();
        let by_id: std::collections::HashMap<_, _> =
            items.iter().map(|item| (item.id.as_str(), item)).collect();
        assert_eq!(by_id["a-current"].verdict.as_deref(), Some("当前结论标题"));
        assert_eq!(
            by_id["a-current"].workflow_version.as_deref(),
            Some("investment-workflow-v1")
        );
        assert_eq!(by_id["a-empty-version"].workflow_version, None);
        assert_eq!(by_id["a-empty-version"].verdict, None);
        assert_eq!(by_id["a-null-trace"].workflow_version, None);
        assert_eq!(by_id["a-malformed"].verdict, None);

        // Inserting more than 50 rows must return only the latest 50 ordered by (created_at DESC, id DESC).
        let conn = db.conn().unwrap();
        for index in 0..55 {
            let id = format!("bulk-{index:02}");
            let ts = format!("2026-11-21T00:00:{:02}Z", index % 60);
            conn.execute(
                "INSERT INTO analyses (id, question, answer, created_at) VALUES ($1,$2,'a',$3)",
                params![id, "bulk", ts],
            )
            .unwrap();
        }
        drop(conn);
        let limited = db.analysis_history().unwrap();
        assert_eq!(limited.len(), 50);
        assert_eq!(limited[0].id, "bulk-54");
        assert_eq!(limited[49].id, "bulk-05");
    }
}
