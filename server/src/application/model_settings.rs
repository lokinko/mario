//! Model credentials and destination binding; independent of HTTP and process startup.
use crate::{
    ai::{ChatMessage, CodexProvider, ModelProvider, NativeModelProvider},
    db::Database,
    models::{ModelConfig, ModelConfigInput, ModelConnectionTest},
    AppError, AppResult,
};
pub(crate) fn save_codex_config(db: &Database, model: &str) -> AppResult<()> {
    db.save_model_metadata("codex", "codex://local", model)?;
    db.set_setting("model.codex_ready", "true")
}

pub(crate) async fn read_codex(db: &Database) -> AppResult<ModelConfig> {
    if db.hosted() {
        return Err(AppError::Validation(
            "托管服务不允许使用服务器的个人 Codex 登录".into(),
        ));
    }
    let model = CodexProvider::detect().await?;
    save_codex_config(db, &model)?;
    db.model_config()
}

pub(crate) async fn codex_models(db: &Database) -> AppResult<Vec<crate::models::CodexModel>> {
    if db.hosted() {
        return Err(AppError::Validation(
            "请通过本机桥接读取个人电脑的模型列表".into(),
        ));
    }
    CodexProvider::models().await
}

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
    if config.provider == "codex-bridge" {
        return Ok(Box::new(crate::ai::bridge::BridgeProvider::new(
            super::codex_bridge::id(db)?,
            config.model.clone(),
        )));
    }
    if db.hosted() {
        crate::models::validate_endpoint(&config.base_url)?;
    }
    if config.provider == "codex" {
        if db.hosted() {
            return Err(AppError::Validation(
                "托管服务不允许使用 Codex 本机登录".into(),
            ));
        }
        return Ok(Box::new(CodexProvider::new(config.model.clone())));
    }
    Ok(Box::new(NativeModelProvider::new(
        config.provider.clone(),
        config.base_url.clone(),
        config.model.clone(),
        configured_model_key(db, config)?,
    )?))
}

pub(crate) fn model_key_binding(provider: &str, base_url: &str) -> String {
    format!("{} {}", provider, base_url.trim_end_matches('/'))
}

pub(crate) fn configured_model_key(db: &Database, config: &ModelConfig) -> AppResult<String> {
    if db
        .setting("model.key_binding")?
        .is_none_or(|binding| binding != model_key_binding(&config.provider, &config.base_url))
    {
        return Err(AppError::Validation(
            "当前密钥不属于这个模型接口，请重新保存对应 API Key".into(),
        ));
    }
    db.secret("model")
}

pub(crate) async fn save(db: &Database, input: ModelConfigInput) -> AppResult<ModelConfig> {
    if input.provider == "codex-bridge" {
        return super::codex_bridge::save(db, &input.model);
    }
    if input.provider == "codex" {
        if db.hosted() {
            return Err(AppError::Validation(
                "托管服务不允许使用 Codex 本机登录".into(),
            ));
        }
        if !codex_models(db)
            .await?
            .iter()
            .any(|entry| entry.model == input.model)
        {
            return Err(AppError::Validation(
                "此模型不在本机 Codex 返回的模型列表中，请重新选择".into(),
            ));
        }
        save_codex_config(db, &input.model)?;
        return db.model_config();
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
    db.save_model_metadata(&input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input.api_key.as_deref() {
        db.save_secret("model", key)?;
        db.set_setting(
            "model.key_binding",
            &model_key_binding(&input.provider, &input.base_url),
        )?;
    }
    db.model_config()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_mismatched_key_binding_is_rejected_before_reading_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("key-binding.db")).unwrap();
        let config = ModelConfig {
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            model: "fixture".into(),
            has_api_key: true,
        };
        for binding in [
            None,
            Some(model_key_binding(
                "openai-responses",
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
