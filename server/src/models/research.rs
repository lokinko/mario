//! User-owned evidence originals, tags and attachments.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ResearchEvidenceInput {
    pub content: String,
    pub attachments: Vec<ResearchAttachmentInput>,
    pub asset_name: String,
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ResearchEvidenceTagsInput {
    pub asset_name: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidence {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ResearchAttachment>,
    pub id: String,
    pub asset_name: String,
    #[serde(default)]
    pub category: String,
    pub title: String,
    pub publisher: String,
    pub source_url: String,
    pub source_tier: String,
    pub evidence_type: String,
    pub stance: String,
    pub as_of_date: String,
    pub claim: String,
    pub notes: String,
    pub active: bool,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidenceStatusInput {
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachmentInput {
    pub name: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachment {
    pub id: String,
    pub name: String,
    pub byte_size: usize,
    #[serde(default, skip_serializing)]
    pub extracted_text: String,
    pub extraction_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttachmentDownload {
    pub name: String,
    pub data_base64: String,
}
