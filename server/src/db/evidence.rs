//! User research originals and attachments; never writes analysis history.
use {
    crate::{
        db::validation::{research_evidence_tags, validate_research_evidence},
        models::research::{ResearchEvidence, ResearchEvidenceInput},
    },
    uuid::Uuid,
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
    pub fn research_evidence(&self) -> AppResult<Vec<ResearchEvidence>> {
        self.load_research_evidence(true)
    }

    /// Lightweight list projection for UI display: omits large attachment `extracted_text`
    /// payloads that are only needed when building AI analysis context.
    pub fn research_evidence_summary(&self) -> AppResult<Vec<ResearchEvidence>> {
        self.load_research_evidence(false)
    }

    fn load_research_evidence(
        &self,
        include_extracted_text: bool,
    ) -> AppResult<Vec<ResearchEvidence>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, asset_name, title, publisher, source_url, source_tier, evidence_type,
                    stance, as_of_date, claim, notes, active, captured_at, category
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
                    category: row.get(13)?,
                    attachments: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let attachment_sql = if include_extracted_text {
            "SELECT evidence_id, id, name, byte_size, extracted_text, extraction_note
             FROM research_attachments ORDER BY id"
        } else {
            "SELECT evidence_id, id, name, byte_size, ''::TEXT, extraction_note
             FROM research_attachments ORDER BY id"
        };
        let mut attachment_stmt = conn.prepare(attachment_sql)?;
        let attachments = attachment_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                crate::models::research::ResearchAttachment {
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
        let tags = research_evidence_tags(&input.asset_name, &input.category)?;
        let attachments = input
            .attachments
            .iter()
            .map(crate::attachments::prepare)
            .collect::<AppResult<Vec<_>>>()?;
        let mut title = input
            .content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .chars()
            .take(80)
            .collect::<String>();
        if title.is_empty() {
            title = input
                .attachments
                .iter()
                .map(|file| file.name.as_str())
                .collect::<Vec<_>>()
                .join("、")
                .chars()
                .take(80)
                .collect();
        }
        let evidence = ResearchEvidence {
            attachments,
            id: Uuid::new_v4().to_string(),
            asset_name: tags.asset_name,
            category: tags.category,
            title,
            publisher: "用户保存的原始资料".into(),
            source_url: String::new(),
            source_tier: "用户资料（待核实）".into(),
            evidence_type: "user_source_text".into(),
            stance: "背景".into(),
            as_of_date: String::new(),
            claim: input.content.clone(),
            notes: String::new(),
            active: true,
            captured_at: Utc::now().to_rfc3339(),
        };
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO research_evidence
             (id, asset_name, title, publisher, source_url, source_tier, evidence_type, stance,
              as_of_date, claim, notes, active, captured_at, category)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12,$13)",
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
                evidence.captured_at,
                evidence.category
            ],
        )?;
        for (metadata, file) in evidence.attachments.iter().zip(&input.attachments) {
            transaction.execute(
                "INSERT INTO research_attachments
                 (id,evidence_id,name,byte_size,data_base64,extracted_text,extraction_note)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
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
    ) -> AppResult<crate::models::research::ResearchAttachmentDownload> {
        self.conn()?
            .query_row(
                "SELECT name,data_base64 FROM research_attachments WHERE evidence_id=$1 AND id=$2",
                params![evidence_id, attachment_id],
                |row| {
                    Ok(crate::models::research::ResearchAttachmentDownload {
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
            "UPDATE research_evidence SET active=$2 WHERE id=$1",
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

    pub fn set_research_evidence_tags(
        &self,
        id: &str,
        input: &crate::models::research::ResearchEvidenceTagsInput,
    ) -> AppResult<ResearchEvidence> {
        let tags = research_evidence_tags(&input.asset_name, &input.category)?;
        let affected = self.conn()?.execute(
            "UPDATE research_evidence SET asset_name=$2, category=$3
             WHERE id=$1 AND evidence_type='user_source_text'",
            params![id, tags.asset_name, tags.category],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的参考资料".into()));
        }
        self.research_evidence()?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::Validation("找不到要更新的参考资料".into()))
    }
}

#[cfg(test)]
mod source_text_tests {
    use super::*;
    use crate::evidence::search_evidence;
    use serde_json::json;

    #[test]
    fn source_text_survives_storage_retrieval_archiving_and_export() {
        let db = Database::test();
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
        let found = search_evidence("黄金储备", &items, 12);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].claim, content);
        let serialized = serde_json::to_value(&found[0]).unwrap();
        assert_eq!(serialized["claim"], content);
        assert_eq!(serialized["asOfDate"], "");

        let target = Database::test();
        target
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(target.research_evidence().unwrap()[0].claim, content);
        db.set_research_evidence_status(&created.id, false).unwrap();
        assert!(search_evidence("黄金储备", &db.research_evidence().unwrap(), 12).is_empty());
        let restored = db.set_research_evidence_status(&created.id, true).unwrap();
        assert_eq!(restored.claim, content);
    }

    #[test]
    fn source_text_rejects_empty_and_overlong_content_but_accepts_full_documents() {
        let db = Database::test();
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
    use crate::{db::SyncValue, evidence::search_evidence};
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;

    #[test]
    fn files_are_atomic_downloadable_and_retrievable_without_sending_binary_to_ai() {
        let db = Database::test();
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
        let found = search_evidence("黄金储备", &records, 12);
        assert_eq!(found.len(), 1);
        assert!(found[0].claim.contains("黄金储备：本期变动"));
        let payload = serde_json::to_string(&found).unwrap();
        assert!(!payload.contains("dataBase64"));
        assert!(!payload.contains(&STANDARD.encode(bytes)));
        db.set_research_evidence_status(&saved.id, false).unwrap();
        assert!(search_evidence("黄金储备", &db.research_evidence().unwrap(), 12).is_empty());
        assert!(db.research_attachment(&saved.id, &file.id).is_ok());

        let bad: ResearchEvidenceInput = serde_json::from_value(json!({"content":"must not save", "attachments":[{"name":"bad.txt","dataBase64":"bad!"}]})).unwrap();
        assert!(db.add_research_evidence(&bad).is_err());
        assert_eq!(db.research_evidence().unwrap().len(), 1);
    }

    #[test]
    fn attachments_survive_export_and_restart_and_legacy_backups_still_import() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let input: ResearchEvidenceInput = serde_json::from_value(
            json!({"content":"原文", "attachments":[{"name":"image.png","dataBase64":"AAECAwQ="}]}),
        )
        .unwrap();
        let saved = db.add_research_evidence(&input).unwrap();
        let file = &saved.attachments[0];
        let dataset = db.export_sync_data().unwrap();
        let restored = Database::test();
        restored.import_sync_data(&dataset).unwrap();
        assert_eq!(
            restored
                .research_attachment(&saved.id, &file.id)
                .unwrap()
                .data_base64,
            "AAECAwQ="
        );
        drop(db);
        let reopened = account.open();
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
    }

    #[test]
    fn summary_omits_attachment_extracted_text_while_full_read_preserves_it() {
        let db = Database::test();
        let bytes = "这是一段很长的附件原文，用于验证摘要列表不会下发全文。".as_bytes();
        let input: ResearchEvidenceInput = serde_json::from_value(json!({
            "content": "主体笔记",
            "attachments": [{"name": "研报.txt", "dataBase64": STANDARD.encode(bytes)}]
        }))
        .unwrap();
        let saved = db.add_research_evidence(&input).unwrap();
        let summary = db.research_evidence_summary().unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].id, saved.id);
        assert_eq!(summary[0].claim, "主体笔记");
        assert_eq!(summary[0].attachments.len(), 1);
        assert_eq!(summary[0].attachments[0].name, "研报.txt");
        assert_eq!(summary[0].attachments[0].byte_size, bytes.len());
        assert!(summary[0].attachments[0].extracted_text.is_empty());

        let full = db.research_evidence().unwrap();
        assert_eq!(full[0].attachments[0].extracted_text.as_bytes(), bytes);
    }
}
