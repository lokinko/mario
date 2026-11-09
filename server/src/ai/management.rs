use super::{bridge, ChatMessage, ModelProvider, NativeModelProvider};
use crate::{
    models::{
        BridgePairing, CodexBridgeStatus, ModelConnectionTest, ModelProviderKind, ModelTarget,
    },
    ports::model::{BridgeRegistry, ModelConnector},
    AppResult,
};

pub(crate) struct ModelConnectorAdapter;
#[async_trait::async_trait]
impl ModelConnector for ModelConnectorAdapter {
    async fn test(&self, target: ModelTarget) -> AppResult<ModelConnectionTest> {
        let config = target.config;
        let provider: Box<dyn ModelProvider> = if config.provider == ModelProviderKind::CodexBridge
        {
            Box::new(bridge::BridgeProvider::new(
                target.credential,
                config.model.clone(),
            ))
        } else {
            Box::new(NativeModelProvider::new(
                config.provider,
                config.base_url,
                config.model.clone(),
                target.credential,
            )?)
        };
        let started = std::time::Instant::now();
        provider
            .complete(vec![
                ChatMessage::system("这是连接测试。"),
                ChatMessage::user("请只回复 OK"),
            ])
            .await?;
        Ok(ModelConnectionTest {
            ok: true,
            model: config.model,
            latency_ms: started.elapsed().as_millis(),
        })
    }
}

pub(crate) struct BridgeRegistryAdapter;
impl BridgeRegistry for BridgeRegistryAdapter {
    fn status(&self, id: Option<&str>) -> CodexBridgeStatus {
        id.map(bridge::status).unwrap_or_default()
    }
    fn create(&self) -> AppResult<BridgePairing> {
        let (id, code) = bridge::create()?;
        Ok(BridgePairing { id, code })
    }
    fn revoke(&self, id: &str) {
        bridge::revoke(id);
    }
}
