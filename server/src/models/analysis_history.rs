//! Read-only decoding of reports frozen before the concise report contract.
use crate::models::analysis::{AnalysisClaim, StructuredAnalysis};
use serde::{Deserialize, Serialize};

/// Trace fields only workflows before the concise report carried. Current runs never
/// write them; history keeps the v7 grounding review readable. Plans, alternatives and
/// critiques of v2–v7 stay in the stored JSON but have no reader.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyTraceArtifacts {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advice_grounding: Vec<AdviceGrounding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundingCheck {
    pub fact_index: usize,
    pub source_kind: String,
    pub source_ref: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdviceGrounding {
    pub action_index: usize,
    pub status: String,
    pub reason: String,
    pub checks: Vec<GroundingCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StoredAnalysisReport {
    Current(StructuredAnalysis),
    Legacy(LegacyStructuredAnalysis),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyStructuredAnalysis {
    pub verdict: String,
    pub facts: Vec<AnalysisClaim>,
    pub inferences: Vec<AnalysisClaim>,
    pub unknowns: Vec<String>,
    pub options: Vec<AnalysisOption>,
    pub actions: Vec<AnalysisAction>,
    pub review_triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisOption {
    pub name: String,
    pub suitable_when: String,
    pub tradeoffs: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisAction {
    // Defaults keep older frozen reports readable; new output is validated strictly.
    #[serde(default)]
    pub supporting_fact_indices: Vec<usize>,
    #[serde(default)]
    pub evidence_limits: Vec<String>,
    pub action: String,
    pub rationale: String,
    pub reversible: bool,
    pub review_trigger: String,
}
