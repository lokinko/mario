//! AI model configuration and local bridge contracts.
use super::provider::ModelProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub provider: ModelProviderKind,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
    #[serde(default)]
    pub source: ModelConfigSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_model: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelConfigSource {
    #[default]
    Account,
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerModelCatalog {
    pub models: Vec<String>,
    pub default_model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModel {
    pub model: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexBridgeStatus {
    pub paired: bool,
    pub online: bool,
    pub model: Option<String>,
    pub models: Vec<CodexModel>,
}

pub(crate) struct BridgePairing {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigInput {
    pub provider: ModelProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub source: ModelConfigSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConnectionTest {
    pub ok: bool,
    pub model: String,
    pub latency_ms: u128,
}
