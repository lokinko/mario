//! Account-owned configuration and frozen connection checks through execution ports.
use crate::{
    db::{model_key_binding, Database},
    models::{ModelConfig, ModelConfigInput, ModelConnectionTest, ModelProviderKind, ModelTarget},
    ports::model::{BridgeRegistry, ModelConnector},
    AppError, AppResult,
};
pub(crate) struct ModelServices {
    pub connector: std::sync::Arc<dyn ModelConnector>,
    pub bridges: std::sync::Arc<dyn BridgeRegistry>,
}
pub(crate) async fn test_connection(
    db: &Database,
    connector: &dyn ModelConnector,
) -> AppResult<ModelConnectionTest> {
    let config = db.model_config()?;
    let target = frozen_target(db, &config)?;
    let started = std::time::Instant::now();
    let result = db
        .without_account_lock(connector.test(target.clone()))
        .await?;
    tracing::debug!(target: "mario::timing", stage = "supplier", kind = "model_test", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
    let result = result?;
    let current = db.model_config()?;
    if serde_json::to_value(&current)? != serde_json::to_value(&config)?
        || frozen_target(db, &current)
            .ok()
            .is_none_or(|current| current.credential != target.credential)
    {
        return Err(AppError::Conflict(
            "测试期间模型配置已变化，请测试当前配置".into(),
        ));
    }
    Ok(result)
}
fn frozen_target(db: &Database, config: &ModelConfig) -> AppResult<ModelTarget> {
    let credential = if config.provider == ModelProviderKind::CodexBridge {
        super::codex_bridge::id(db)?
    } else {
        crate::models::validate_endpoint(&config.base_url)?;
        configured_model_key(db, config)?
    };
    Ok(ModelTarget {
        config: config.clone(),
        credential,
    })
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

pub(crate) fn save(
    db: &Database,
    input: ModelConfigInput,
    bridges: &dyn BridgeRegistry,
) -> AppResult<ModelConfig> {
    if input.provider == ModelProviderKind::CodexBridge {
        return super::codex_bridge::save(db, &input.model, bridges);
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
    db.save_model_configuration(
        input.provider,
        &input.base_url,
        &input.model,
        input.api_key.as_deref(),
    )?;
    db.model_config()
}
#[cfg(test)]
#[path = "model_settings_tests.rs"]
mod concurrency_tests;
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
