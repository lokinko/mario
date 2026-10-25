//! Read-only decoding of reports frozen before the concise report contract.
use super::{AnalysisClaim, StructuredAnalysis};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StoredAnalysisReport {
    Current(StructuredAnalysis),
    Legacy(LegacyStructuredAnalysis),
}

impl StoredAnalysisReport {
    pub fn headline(&self) -> &str {
        match self {
            Self::Current(report) => &report.answer,
            Self::Legacy(report) => &report.verdict,
        }
    }
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
