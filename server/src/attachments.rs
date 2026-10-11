use std::io::{Cursor, Read};

use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::{events::Event, Reader};

use crate::{
    models::{ResearchAttachment, ResearchAttachmentInput},
    AppError, AppResult,
};

pub const MAX_FILES: usize = 5;
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 20_000;

pub fn decode(input: &ResearchAttachmentInput) -> AppResult<Vec<u8>> {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 240
        || input
            .name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err(AppError::Validation("附件文件名无效".into()));
    }
    if input.data_base64.len() > MAX_BYTES.div_ceil(3) * 4 {
        return Err(AppError::Validation("附件总大小不能超过 5 MB".into()));
    }
    let bytes = STANDARD
        .decode(&input.data_base64)
        .map_err(|_| AppError::Validation("附件内容编码无效".into()))?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::Validation("附件总大小不能超过 5 MB".into()));
    }
    Ok(bytes)
}

pub fn validate(inputs: &[ResearchAttachmentInput]) -> AppResult<()> {
    if inputs.len() > MAX_FILES {
        return Err(AppError::Validation("每条资料最多附带 5 个文件".into()));
    }
    let mut total = 0;
    for input in inputs {
        total += decode(input)?.len();
        if total > MAX_BYTES {
            return Err(AppError::Validation("附件总大小不能超过 5 MB".into()));
        }
    }
    Ok(())
}

pub fn prepare(input: &ResearchAttachmentInput) -> AppResult<ResearchAttachment> {
    let bytes = decode(input)?;
    let extension = input
        .name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extracted = match extension.as_str() {
        "txt" | "md" | "csv" | "tsv" | "json" | "log" => decode_text(&bytes),
        "pdf" => std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes))
            .ok()
            .and_then(Result::ok),
        "docx" => docx_text(&bytes).ok(),
        _ => None,
    };
    let (extracted_text, extraction_note) = match extracted {
        Some(text) if !text.trim().is_empty() => {
            let truncated = text.chars().count() > MAX_TEXT_CHARS;
            (
                text.chars().take(MAX_TEXT_CHARS).collect(),
                if truncated {
                    "已提取前 20000 字供问答使用；完整内容保留在原文件中"
                } else {
                    "已提取文字，可用于问答"
                }
                .into(),
            )
        }
        _ => (
            String::new(),
            "原文件已保存；未提取到可供问答使用的文字".into(),
        ),
    };
    Ok(ResearchAttachment {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name.clone(),
        byte_size: bytes.len(),
        extracted_text,
        extraction_note,
    })
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        if rest.len() % 2 != 0 {
            return None;
        }
        return String::from_utf16(
            &rest
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        if rest.len() % 2 != 0 {
            return None;
        }
        return String::from_utf16(
            &rest
                .chunks_exact(2)
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok();
    }
    String::from_utf8(
        bytes
            .strip_prefix(&[0xef, 0xbb, 0xbf])
            .unwrap_or(bytes)
            .to_vec(),
    )
    .ok()
}

fn docx_text(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let file = archive.by_name("word/document.xml")?;
    // Bound decompression independently of the uploaded archive size.
    let mut xml = String::new();
    file.take((MAX_BYTES + 1) as u64).read_to_string(&mut xml)?;
    if xml.len() > MAX_BYTES {
        return Err("Word 正文过大".into());
    }
    let mut reader = Reader::from_str(&xml);
    let mut in_text = false;
    let mut text = String::new();
    loop {
        match reader.read_event()? {
            Event::Start(tag) if tag.local_name().as_ref() == b"t" => in_text = true,
            Event::Text(value) if in_text => text.push_str(&value.decode()?),
            Event::GeneralRef(value) if in_text => {
                let name = value.decode()?;
                if let Some(number) = name.strip_prefix("#x") {
                    if let Some(c) = u32::from_str_radix(number, 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        text.push(c);
                    }
                } else if let Some(number) = name.strip_prefix('#') {
                    if let Some(c) = number.parse::<u32>().ok().and_then(char::from_u32) {
                        text.push(c);
                    }
                } else {
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"t" => in_text = false,
            Event::End(tag) if tag.local_name().as_ref() == b"p" => text.push('\n'),
            Event::End(tag) if tag.local_name().as_ref() == b"tc" => text.push('\t'),
            Event::Empty(tag) if tag.local_name().as_ref() == b"tab" => text.push('\t'),
            Event::Empty(tag) if matches!(tag.local_name().as_ref(), b"br" | b"cr") => {
                text.push('\n')
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(text)
}

pub fn validate_sync(dataset: &crate::db::SyncDataset) -> AppResult<()> {
    use crate::db::SyncValue;
    let Some(table) = dataset
        .tables
        .iter()
        .find(|t| t.name == "research_attachments")
    else {
        return Ok(());
    };
    let evidence = dataset
        .tables
        .iter()
        .find(|t| t.name == "research_evidence")
        .ok_or_else(|| AppError::Validation("快照缺少资料表".into()))?;
    let ids = evidence
        .rows
        .iter()
        .filter_map(|row| match &row[0] {
            SyncValue::Text(id) => Some(id.as_str()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let mut seen = std::collections::HashSet::new();
    let mut totals = std::collections::HashMap::<String, (usize, usize)>::new();
    for row in &table.rows {
        let string = |i| match &row[i] {
            SyncValue::Text(s) => Ok(s.as_str()),
            _ => Err(AppError::Validation("附件快照字段无效".into())),
        };
        let id = string(0)?;
        let parent = string(1)?;
        if uuid::Uuid::parse_str(id).is_err() || !seen.insert(id) || !ids.contains(parent) {
            return Err(AppError::Validation("附件快照的资料关联无效".into()));
        }
        let bytes = decode(&ResearchAttachmentInput {
            name: string(2)?.into(),
            data_base64: string(4)?.into(),
        })?;
        if row[3] != SyncValue::Integer(bytes.len() as i64)
            || string(5)?.chars().count() > MAX_TEXT_CHARS
            || string(6)?.chars().count() > 500
        {
            return Err(AppError::Validation("附件快照内容无效".into()));
        }
        let total = totals.entry(parent.into()).or_default();
        total.0 += 1;
        total.1 += bytes.len();
        if total.0 > MAX_FILES || total.1 > MAX_BYTES {
            return Err(AppError::Validation("附件快照超过数量或大小限制".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_text_from_a_real_pdf_document() {
        let content = "BT /F1 12 Tf 72 720 Td (Gold reserves increased this period.) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content),
        ];
        let mut pdf = "%PDF-1.4\n".to_string();
        let mut offsets = vec![0];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, object));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        ));
        let file = prepare(&ResearchAttachmentInput {
            name: "report.pdf".into(),
            data_base64: STANDARD.encode(pdf),
        })
        .unwrap();
        assert!(file
            .extracted_text
            .contains("Gold reserves increased this period."));
    }

    #[test]
    fn extracts_word_paragraphs_tables_and_entities() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(br#"<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>A &amp; B &#x4E2D;</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let file = prepare(&ResearchAttachmentInput {
            name: "report.docx".into(),
            data_base64: STANDARD.encode(bytes),
        })
        .unwrap();
        assert!(file.extracted_text.contains("A & B 中\n"));
        assert!(file.extracted_text.contains("Value\n\t"));
    }

    #[test]
    fn preserves_text_and_exposes_extraction_limits_without_rejecting_other_files() {
        let prepare_bytes = |name: &str, bytes: &[u8]| {
            prepare(&ResearchAttachmentInput {
                name: name.into(),
                data_base64: STANDARD.encode(bytes),
            })
            .unwrap()
        };
        assert_eq!(
            prepare_bytes("report.txt", "  原文\n\n第二段  ".as_bytes()).extracted_text,
            "  原文\n\n第二段  "
        );
        let utf16 = [0xff, 0xfe, 0x2d, 0x4e];
        assert_eq!(prepare_bytes("report.txt", &utf16).extracted_text, "中");
        let long = "字".repeat(20_001);
        let result = prepare_bytes("report.md", long.as_bytes());
        assert_eq!(result.extracted_text.chars().count(), 20_000);
        assert!(result.extraction_note.contains("前 20000"));
        for name in ["image.png", "scan.pdf", "broken.docx"] {
            let file = prepare_bytes(name, b"unreadable data");
            assert!(file.extracted_text.is_empty());
            assert!(file.extraction_note.contains("未提取"));
        }
    }

    #[test]
    fn rejects_invalid_names_encoding_and_limits() {
        for name in ["../file", "a\\file", "\nfile", ""] {
            assert!(decode(&ResearchAttachmentInput {
                name: name.into(),
                data_base64: "YQ==".into()
            })
            .is_err());
        }
        assert!(decode(&ResearchAttachmentInput {
            name: "file.txt".into(),
            data_base64: "bad%%".into()
        })
        .is_err());
        assert!(decode(&ResearchAttachmentInput {
            name: "file.txt".into(),
            data_base64: STANDARD.encode(vec![0; MAX_BYTES + 1])
        })
        .is_err());
        assert!(validate(&vec![
            ResearchAttachmentInput {
                name: "a.txt".into(),
                data_base64: "YQ==".into()
            };
            MAX_FILES + 1
        ])
        .is_err());
    }
}
