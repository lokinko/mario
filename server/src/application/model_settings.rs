//! Model credentials and destination binding; independent of HTTP and process startup.
use crate::{
    ai::{ChatMessage, ModelProvider, NativeModelProvider},
    db::Database,
    models::{ModelConfig, ModelConfigInput, ModelConnectionTest, ModelProviderKind},
    AppError, AppResult,
};
pub(crate) async fn test_connection(db: &Database) -> AppResult<ModelConnectionTest> {
    let config = db.model_config()?;
    let provider = configured_model_provider(db, &config)?;
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

pub(crate) fn configured_model_provider(
    db: &Database,
    config: &ModelConfig,
) -> AppResult<Box<dyn ModelProvider>> {
    if config.provider == ModelProviderKind::CodexBridge {
        return Ok(Box::new(crate::ai::bridge::BridgeProvider::new(
            super::codex_bridge::id(db)?,
            config.model.clone(),
        )));
    }
    crate::models::validate_endpoint(&config.base_url)?;
    Ok(Box::new(NativeModelProvider::new(
        config.provider,
        config.base_url.clone(),
        config.model.clone(),
        configured_model_key(db, config)?,
    )?))
}

pub(crate) fn model_key_binding(provider: ModelProviderKind, base_url: &str) -> String {
    format!("{} {}", provider, base_url.trim_end_matches('/'))
}

pub(crate) fn configured_model_key(db: &Database, config: &ModelConfig) -> AppResult<String> {
    if db
        .setting("model.key_binding")?
        .is_none_or(|binding| binding != model_key_binding(config.provider, &config.base_url))
    {
        return Err(AppError::Validation(
            "当前密钥不属于这个模型接口，请重新保存对应 API Key".into(),
        ));
    }
    db.secret("model")
}

pub(crate) async fn save(db: &Database, input: ModelConfigInput) -> AppResult<ModelConfig> {
    if input.provider == ModelProviderKind::CodexBridge {
        return super::codex_bridge::save(db, &input.model);
    }
    let previous = db.model_config()?;
    if previous.has_api_key
        && (input.provider != previous.provider
            || input.base_url.trim_end_matches('/') != previous.base_url.trim_end_matches('/'))
        && input
            .api_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
    {
        return Err(AppError::Validation(
            "切换模型接口或地址时，请填写对应 API Key".into(),
        ));
    }
    db.save_model_metadata(input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input.api_key.as_deref() {
        db.save_secret("model", key)?;
        db.set_setting(
            "model.key_binding",
            &model_key_binding(input.provider, &input.base_url),
        )?;
    }
    db.model_config()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_mismatched_key_binding_is_rejected_before_reading_credentials() {
        let db = Database::test();
        let config = ModelConfig {
            provider: ModelProviderKind::Anthropic,
            base_url: "https://api.anthropic.com/v1".into(),
            model: "fixture".into(),
            has_api_key: true,
        };
        for binding in [
            None,
            Some(model_key_binding(
                ModelProviderKind::OpenaiResponses,
                "https://api.openai.com/v1/",
            )),
        ] {
            if let Some(binding) = binding {
                db.set_setting("model.key_binding", &binding).unwrap();
            }
            assert!(configured_model_key(&db, &config)
                .unwrap_err()
                .to_string()
                .contains("当前密钥不属于"));
        }
    }
}
