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
                    attachments: Vec::new(),
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
             ORDER BY active DESC, captured_at DESC, id ASC",
        )?;
        let mut evidence = statement
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
                    attachments: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut attachment_stmt = conn.prepare(
            "SELECT evidence_id, id, name, byte_size, extracted_text, extraction_note
             FROM research_attachments ORDER BY id",
        )?;
        let attachments = attachment_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                crate::models::ResearchAttachment {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    byte_size: row.get::<_, i64>(3)? as usize,
                    extracted_text: row.get(4)?,
                    extraction_note: row.get(5)?,
                },
            ))
        })?;
        let mut by_evidence = std::collections::HashMap::<String, Vec<_>>::new();
        for attachment in attachments {
            let (id, metadata) = attachment?;
            by_evidence.entry(id).or_default().push(metadata);
        }
        for item in &mut evidence {
            item.attachments = by_evidence.remove(&item.id).unwrap_or_default();
        }
        Ok(evidence)
    }

    pub fn add_research_evidence(
        &self,
        input: &ResearchEvidenceInput,
    ) -> AppResult<ResearchEvidence> {
        validate_research_evidence(input)?;
        let attachments = input
            .attachments
            .iter()
            .map(crate::attachments::prepare)
            .collect::<AppResult<Vec<_>>>()?;
        let mut evidence = ResearchEvidence {
            attachments,
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
        if input.content.is_some() || !input.attachments.is_empty() {
            let content = input.content.as_deref().unwrap_or_default();
            // Reuse the evidence storage and citation pipeline without inventing
            // a publication date, URL, or verified source classification.
            evidence.asset_name.clear();
            evidence.title = content
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .chars()
                .take(80)
                .collect();
            if evidence.title.is_empty() {
                evidence.title = input
                    .attachments
                    .iter()
                    .map(|file| file.name.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
                    .chars()
                    .take(80)
                    .collect();
            }
            evidence.publisher = "用户保存的原始资料".into();
            evidence.source_url.clear();
            evidence.source_tier = "用户资料（待核实）".into();
            evidence.evidence_type = "user_source_text".into();
            evidence.stance = "背景".into();
            evidence.as_of_date.clear();
            evidence.claim = content.into();
            evidence.notes.clear();
        }
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
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
        for (metadata, file) in evidence.attachments.iter().zip(&input.attachments) {
            transaction.execute(
                "INSERT INTO research_attachments
                 (id,evidence_id,name,byte_size,data_base64,extracted_text,extraction_note)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    metadata.id,
                    evidence.id,
                    metadata.name,
                    metadata.byte_size as i64,
                    file.data_base64,
                    metadata.extracted_text,
                    metadata.extraction_note
                ],
            )?;
        }
        transaction.commit()?;
        Ok(evidence)
    }

    pub fn research_attachment(
        &self,
        evidence_id: &str,
        attachment_id: &str,
    ) -> AppResult<crate::models::ResearchAttachmentDownload> {
        self.conn()?
            .query_row(
                "SELECT name,data_base64 FROM research_attachments WHERE evidence_id=?1 AND id=?2",
                params![evidence_id, attachment_id],
                |row| {
                    Ok(crate::models::ResearchAttachmentDownload {
                        name: row.get(0)?,
                        data_base64: row.get(1)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("找不到这份附件".into()))
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

#[cfg(test)]
mod source_text_tests {
    use super::*;
    use crate::evidence::{EvidenceRetriever, LexicalEvidenceRetriever};
    use serde_json::json;

    #[test]
    fn source_text_survives_storage_retrieval_archiving_and_export() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("source.db")).unwrap();
        let content = "  原始资料\n\n黄金储备变化：本期披露的完整原文。\n来源及日期尚未整理。\n  ";
        let input: ResearchEvidenceInput =
            serde_json::from_value(json!({"content": content})).unwrap();
        let created = db.add_research_evidence(&input).unwrap();
        assert_eq!(created.claim, content);
        assert_eq!(created.evidence_type, "user_source_text");
        assert_eq!(created.source_tier, "用户资料（待核实）");
        assert!(created.source_url.is_empty());
        assert!(created.as_of_date.is_empty());
        let items = db.research_evidence().unwrap();
        let found = LexicalEvidenceRetriever.search("黄金储备", &items, 12);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].claim, content);
        let serialized = serde_json::to_value(&found[0]).unwrap();
        assert_eq!(serialized["claim"], content);
        assert_eq!(serialized["asOfDate"], "");

        let target = Database::open(&dir.path().join("restored.db")).unwrap();
        target
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(target.research_evidence().unwrap()[0].claim, content);
        db.set_research_evidence_status(&created.id, false).unwrap();
        assert!(LexicalEvidenceRetriever
            .search("黄金储备", &db.research_evidence().unwrap(), 12)
            .is_empty());
        let restored = db.set_research_evidence_status(&created.id, true).unwrap();
        assert_eq!(restored.claim, content);
    }

    #[test]
    fn source_text_rejects_empty_and_overlong_content_but_accepts_full_documents() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("source.db")).unwrap();
        for content in [" \n\t ".to_string(), "字".repeat(20_001)] {
            let input: ResearchEvidenceInput =
                serde_json::from_value(json!({"content": content})).unwrap();
            assert!(matches!(
                db.add_research_evidence(&input),
                Err(AppError::Validation(_))
            ));
        }
        let content = "字".repeat(20_000);
        let input: ResearchEvidenceInput =
            serde_json::from_value(json!({"content": content})).unwrap();
        assert_eq!(db.add_research_evidence(&input).unwrap().claim, content);
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use crate::{
        db::SyncValue,
        evidence::{EvidenceRetriever, LexicalEvidenceRetriever},
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;

    #[test]
    fn files_are_atomic_downloadable_and_retrievable_without_sending_binary_to_ai() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("files.db")).unwrap();
        let bytes = "黄金储备：本期变动\n\n原始信息".as_bytes();
        let input: ResearchEvidenceInput = serde_json::from_value(json!({
            "content": "", "attachments": [{"name":"报告.txt", "dataBase64":STANDARD.encode(bytes)}]
        }))
        .unwrap();
        let saved = db.add_research_evidence(&input).unwrap();
        assert!(saved.claim.is_empty());
        assert_eq!(saved.title, "报告.txt");
        let file = &saved.attachments[0];
        let downloaded = db.research_attachment(&saved.id, &file.id).unwrap();
        assert_eq!(STANDARD.decode(downloaded.data_base64).unwrap(), bytes);
        assert!(matches!(
            db.research_attachment("other-source", &file.id),
            Err(AppError::NotFound(_))
        ));
        let records = db.research_evidence().unwrap();
        assert_eq!(records[0].attachments[0].extracted_text.as_bytes(), bytes);
        let found = LexicalEvidenceRetriever.search("黄金储备", &records, 12);
        assert_eq!(found.len(), 1);
        assert!(found[0].claim.contains("黄金储备：本期变动"));
        let payload = serde_json::to_string(&found).unwrap();
        assert!(!payload.contains("dataBase64"));
        assert!(!payload.contains(&STANDARD.encode(bytes)));
        db.set_research_evidence_status(&saved.id, false).unwrap();
        assert!(LexicalEvidenceRetriever
            .search("黄金储备", &db.research_evidence().unwrap(), 12)
            .is_empty());
        assert!(db.research_attachment(&saved.id, &file.id).is_ok());

        let bad: ResearchEvidenceInput = serde_json::from_value(json!({"content":"must not save", "attachments":[{"name":"bad.txt","dataBase64":"bad!"}]})).unwrap();
        assert!(db.add_research_evidence(&bad).is_err());
        assert_eq!(db.research_evidence().unwrap().len(), 1);
    }

    #[test]
    fn attachments_survive_export_and_restart_and_legacy_backups_still_import() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("files.db");
        let db = Database::open(&path).unwrap();
        let input: ResearchEvidenceInput = serde_json::from_value(
            json!({"content":"原文", "attachments":[{"name":"image.png","dataBase64":"AAECAwQ="}]}),
        )
        .unwrap();
        let saved = db.add_research_evidence(&input).unwrap();
        let file = &saved.attachments[0];
        let dataset = db.export_sync_data().unwrap();
        let restored = Database::open(&dir.path().join("restored.db")).unwrap();
        restored.import_sync_data(&dataset).unwrap();
        assert_eq!(
            restored
                .research_attachment(&saved.id, &file.id)
                .unwrap()
                .data_base64,
            "AAECAwQ="
        );
        drop(db);
        let reopened = Database::open(&path).unwrap();
        assert_eq!(
            reopened
                .research_attachment(&saved.id, &file.id)
                .unwrap()
                .name,
            "image.png"
        );
        let mut corrupt = dataset.clone();
        let table = corrupt
            .tables
            .iter_mut()
            .find(|t| t.name == "research_attachments")
            .unwrap();
        table.rows[0][1] = SyncValue::Text("wrong-parent".into());
        assert!(restored.import_sync_data(&corrupt).is_err());
        assert!(restored.research_attachment(&saved.id, &file.id).is_ok());
        let mut legacy = dataset.clone();
        super::super::tests::strip_holding_details_for_legacy(&mut legacy);
        legacy.schema_version = 12;
        legacy.tables.retain(|t| t.name != "research_attachments");
        restored.import_sync_data(&legacy).unwrap();
        assert!(restored.research_evidence().unwrap()[0]
            .attachments
            .is_empty());
        assert_eq!(restored.research_evidence().unwrap()[0].claim, "原文");
    }
}
