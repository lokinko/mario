//! AI authorization, execution, frozen reports and analysis history contracts.
use super::{
    analysis_history::{self, StoredAnalysisReport},
    council,
    portfolio::HoldingValuationEvidence,
    provider::ModelProviderKind,
    research::{ResearchAttachment, ResearchEvidence},
};
use serde::{Deserialize, Serialize};

pub const PREVIOUS_TURN_LIMIT: usize = 800;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviousTurn {
    pub question: String,
    pub answer: String,
    pub suggestions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<String>,
}

impl PreviousTurn {
    pub fn validate(&self) -> crate::AppResult<()> {
        let length = self.question.chars().count()
            + self.answer.chars().count()
            + self
                .suggestions
                .iter()
                .map(|s| s.chars().count())
                .sum::<usize>();
        if self.earlier_questions.len() > 5
            || self
                .earlier_questions
                .iter()
                .any(|question| question.trim().is_empty() || question.chars().count() > 300)
            || self.pending_question.as_ref().is_some_and(|question| {
                question.trim().is_empty() || question.chars().count() > 200
            })
        {
            return Err(crate::AppError::Validation(
                "早期用户问题最多5条、每条不超过300字符；上一轮追问不超过200字符，正文须非空"
                    .into(),
            ));
        }
        if length > PREVIOUS_TURN_LIMIT
            || self.question.trim().is_empty()
            || self.answer.trim().is_empty()
            || self.suggestions.len() > 3
            || self.suggestions.iter().any(|s| s.trim().is_empty())
        {
            return Err(crate::AppError::Validation(
                "上一轮摘要须非空，总长度不超过800字符，建议最多3条".into(),
            ));
        }
        Ok(())
    }
}

/// The only production workflow identifier: research agent, proposal agents and a vote.
pub const ANALYSIS_WORKFLOW: &str = "council";

/// Retired request and preset options. Storage strips them once; requests that still
/// send them are rejected instead of silently running a different workflow.
pub const RETIRED_ANALYSIS_OPTIONS: [&str; 4] = [
    "reflect",
    "exploreAlternatives",
    "useMemory",
    "dailyAssetRange",
];
pub const RETIRED_CONTEXT_OPTIONS: [&str; 6] = [
    "includePlanning",
    "includeRiskFindings",
    "includeRules",
    "includeSystemReviews",
    "includeDailyAssets",
    "includePortfolioCheckins",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisRequest {
    #[serde(default = "enabled")]
    pub web_search: bool,
    #[serde(default)]
    pub previous_turn: Option<PreviousTurn>,
    pub question: String,
    pub workflow: String,

    #[serde(default)]
    pub context_selection: ContextSelection,
    #[serde(default)]
    pub preview_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AnalysisSettings {
    pub workflow: String,

    pub web_search: bool,
    pub context_selection: ContextSelection,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            workflow: ANALYSIS_WORKFLOW.into(),

            web_search: true,
            context_selection: ContextSelection::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextSelection {
    #[serde(default = "enabled")]
    pub include_profile: bool,
    #[serde(default = "enabled")]
    pub include_holdings: bool,
    #[serde(default = "enabled")]
    pub include_evidence: bool,
    /// Send relevant long-term memory and allow automatic dreaming after answers.
    #[serde(default = "enabled")]
    pub include_memory: bool,
}

impl ContextSelection {
    /// Every financial data group is authorized; model-derived history may be carried.
    pub fn all_financial_groups(&self) -> bool {
        self.include_profile && self.include_holdings && self.include_evidence
    }
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            include_profile: true,
            include_holdings: true,
            include_evidence: true,
            include_memory: true,
        }
    }
}

fn enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextGroup {
    pub key: String,
    pub label: String,
    pub included: bool,
    pub record_count: usize,
    pub sensitivity: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPreview {
    pub provider: ModelProviderKind,
    pub model: String,
    pub workflow: String,
    pub groups: Vec<ContextGroup>,
    pub evidence_candidates: Vec<ResearchEvidence>,
    pub payload: serde_json::Value,
    pub payload_bytes: usize,
    pub context_revision: String,
    pub system_policy: String,
    pub local_only: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisTransparency {
    pub provider: String,
    pub model: String,
    pub context_groups: Vec<String>,
    pub payload_bytes: usize,
    pub context_revision: String,
    #[serde(default)]
    pub evidence_items_used: usize,
    #[serde(default)]
    pub citations_required: bool,
    #[serde(default)]
    pub model_calls: usize,
    #[serde(default)]
    pub total_latency_ms: u64,
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub structured_output_validated: bool,
    #[serde(default)]
    pub output_repairs: usize,
    #[serde(default)]
    pub memory_notes_used: usize,
    pub external_data_used: bool,
    pub api_key_sent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredAnalysis {
    pub answer: String,
    pub suggestions: Vec<AnalysisSuggestion>,
    pub facts: Vec<AnalysisClaim>,
    pub question: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisSuggestion {
    pub action: String,
    pub why: String,
    pub fact_indices: Vec<usize>,
    pub unless_when: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisClaim {
    pub statement: String,
    pub basis: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
}

/// `repaired` means only that a second, format-only attempt passed structural checks;
/// the rejected first output is kept so a reviewer can compare what changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputValidationTrace {
    pub status: String,
    pub attempts: usize,
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisEvidenceReference {
    #[serde(default)]
    pub attachments: Vec<ResearchAttachment>,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub evidence_type: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub stance: String,
    #[serde(default)]
    pub captured_at: String,
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub source_url: String,
    pub source_tier: String,
    pub as_of_date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisWorkflowTrace {
    #[serde(default)]
    pub valuation_evidence: Vec<HoldingValuationEvidence>,
    #[serde(default)]
    pub web_search: Option<WebSearchTrace>,
    #[serde(default)]
    pub personal_context: Option<serde_json::Value>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub calls: Vec<ModelCallTrace>,
    /// The adopted report: the winning proposal, unchanged.
    #[serde(default)]
    pub structured_report: Option<StoredAnalysisReport>,
    #[serde(default)]
    pub output_validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub evidence_catalog: Vec<AnalysisEvidenceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<council::ResearchTrace>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposals: Vec<council::ProposalTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vote: Option<council::VoteTrace>,
    /// Fields only reports frozen by retired workflows carry; never written now.
    /// Kept as a direct field rather than `#[serde(flatten)]` so `f64` fields in
    /// `valuation_evidence` deserialize directly under `serde_json/arbitrary_precision`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advice_grounding: Vec<analysis_history::AdviceGrounding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCallTrace {
    #[serde(default)]
    pub request_count: Option<usize>,
    pub stage: String,
    pub label: String,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub id: String,
    pub answer: String,
    pub stages: Vec<String>,
    pub transparency: AnalysisTransparency,
    #[serde(default)]
    pub workflow_trace: AnalysisWorkflowTrace,
    pub created_at: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisHistoryItem {
    #[serde(default)]
    pub grounding_status: Option<String>,
    pub id: String,
    pub question: String,
    pub created_at: String,
    pub transparency: Option<AnalysisTransparency>,
    #[serde(default)]
    pub workflow_version: Option<String>,
    #[serde(default)]
    pub verdict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAnalysis {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub created_at: String,
    pub transparency: Option<AnalysisTransparency>,
    #[serde(default)]
    pub workflow_trace: Option<AnalysisWorkflowTrace>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchTrace {
    /// `completed`, `incomplete` (no citable original text) or `failed` (run continued locally).
    pub status: String,
    pub search_calls: usize,
    pub warnings: Vec<String>,
    /// Provider-written summaries: shown as leads to verify, never authorized as evidence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leads: Vec<WebLead>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebLead {
    pub title: String,
    pub url: String,
    pub summary: String,
}

impl AnalysisRequest {
    pub fn validate(&self) -> crate::AppResult<()> {
        if self.question.trim().is_empty() {
            return Err(crate::AppError::Validation("分析问题不能为空".into()));
        }
        if let Some(previous) = &self.previous_turn {
            previous.validate()?;
        }
        if self.workflow != ANALYSIS_WORKFLOW {
            return Err(crate::AppError::Validation("未知的分析工作流".into()));
        }
        Ok(())
    }
}
