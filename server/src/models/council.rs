//! Modular investment analysis workflow contracts (v1).
//! Covers routing, research brief, specialist modules, and chief strategist synthesis.
use crate::models::analysis::{AnalysisClaim, OutputValidationTrace};
use serde::{Deserialize, Serialize};

pub const RESEARCH_FINDINGS_MAX: usize = 10;
pub const RESEARCH_NOTES_MAX: usize = 6;
pub const RESEARCH_NOTE_CHARS: usize = 300;
pub const MODULE_STANCE_CHARS: usize = 120;
pub const MODULE_SUMMARY_CHARS: usize = 600;
pub const MODULE_ITEM_CHARS: usize = 240;
pub const MODULE_GAP_CHARS: usize = 180;
pub const MODULE_FACTS_MAX: usize = 5;

/// Adaptive workflow execution mode selected from the frozen inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Full modular investment analysis: research → specialist modules → strategist synthesis.
    #[default]
    Council,
    /// Single direct structured advisor call for lightweight concept, data-status or lookup queries.
    Fast,
}

/// Deterministic domain and execution-mode routing recorded in the workflow trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteTrace {
    pub domain: String,
    pub domain_label: String,
    pub execution_mode: ExecutionMode,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companies: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCheckStatus {
    Covered,
    NotApplicable,
    MissingSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchCheck {
    pub index: usize,
    pub status: ResearchCheckStatus,
    pub note: String,
}

/// Structured evidence brief produced by the research stage before specialist modules run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchBrief {
    pub findings: Vec<AnalysisClaim>,
    pub counterpoints: Vec<String>,
    pub unknowns: Vec<String>,
    pub checks: Vec<ResearchCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTrace {
    #[serde(default)]
    pub brief: Option<ResearchBrief>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

/// Output of one specialist analysis module (fundamentals/valuation, portfolio/execution, risk/bear).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleAnalysis {
    pub stance: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub data_gaps: Vec<String>,
    #[serde(default)]
    pub facts: Vec<AnalysisClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleTrace {
    pub id: String,
    pub module: String,
    pub label: String,
    #[serde(default)]
    pub analysis: Option<ModuleAnalysis>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisTrace {
    pub status: String,
    pub modules_used: usize,
    pub modules_total: usize,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
}
