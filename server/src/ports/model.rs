//! Model management capabilities; implementations have no access to account storage.
use crate::{
    models::{
        connection::{BridgePairing, CodexBridgeStatus, ModelConnectionTest},
        provider::ModelTarget,
    },
    AppResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CatalogProtocol {
    Openai,
    Anthropic,
}

#[async_trait::async_trait]
pub(crate) trait ModelConnector: Send + Sync {
    async fn test(&self, target: ModelTarget) -> AppResult<ModelConnectionTest>;
}

#[async_trait::async_trait]
pub(crate) trait ModelCatalog: Send + Sync {
    /// Lists candidate text models from the official OpenAI endpoint only.
    async fn list(&self, api_key: &str) -> AppResult<Vec<String>>;
    /// Lists candidate models from a validated native provider endpoint.
    async fn list_provider(
        &self,
        protocol: CatalogProtocol,
        base_url: &str,
        api_key: &str,
    ) -> AppResult<Vec<String>>;
}

pub(crate) trait BridgeRegistry: Send + Sync {
    fn status(&self, id: Option<&str>) -> CodexBridgeStatus;
    fn create(&self) -> AppResult<BridgePairing>;
    fn revoke(&self, id: &str);
}
