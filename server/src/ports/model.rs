//! Model management capabilities; implementations have no access to account storage.
use crate::{
    models::{BridgePairing, CodexBridgeStatus, ModelConnectionTest, ModelTarget},
    AppResult,
};

#[async_trait::async_trait]
pub(crate) trait ModelConnector: Send + Sync {
    async fn test(&self, target: ModelTarget) -> AppResult<ModelConnectionTest>;
}

pub(crate) trait BridgeRegistry: Send + Sync {
    fn status(&self, id: Option<&str>) -> CodexBridgeStatus;
    fn create(&self) -> AppResult<BridgePairing>;
    fn revoke(&self, id: &str);
}
