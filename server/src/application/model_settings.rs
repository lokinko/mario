//! Model credentials and destination binding; independent of HTTP and process startup.
use crate::{
    ai::{ChatMessage, CodexProvider, ModelProvider, NativeModelProvider},
    db::Database,
    models::{ModelConfig, ModelConfigInput, ModelConnectionTest},
    AppError, AppResult,
};
pub(crate) fn save_codex_config(db: &Database, model: &str) -> AppResult<()> {
    let previous = db.model_config()?;
    if previous.provider != "codex" && db.setting("model.key_binding")?.is_none() {
        db.set_setting(
            "model.key_binding",
            &model_key_binding(&previous.provider, &previous.base_url),
        )?;
    }
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
    let protocol = if provider == "openai-compatible" {
        "openai-responses"
    } else {
        provider
    };
    format!("{} {}", protocol, base_url.trim_end_matches('/'))
}

pub(crate) fn configured_model_key(db: &Database, config: &ModelConfig) -> AppResult<String> {
    if db
        .setting("model.key_binding")?
        .is_some_and(|binding| binding != model_key_binding(&config.provider, &config.base_url))
    {
        return Err(AppError::Validation(
            "当前密钥不属于这个模型接口，请重新保存对应 API Key".into(),
        ));
    }
    db.secret("model")
}

pub(crate) async fn save(db: &Database, input: ModelConfigInput) -> AppResult<ModelConfig> {
    if input.provider == "codex" {
        if db.hosted() {
            return Err(AppError::Validation(
                "托管服务不允许使用 Codex 本机登录".into(),
            ));
        }
        CodexProvider::detect().await?;
        save_codex_config(db, &input.model)?;
        return db.model_config();
    }
    let previous = db.model_config()?;
    let protocol = if input.provider == "openai-compatible" {
        "openai-responses"
    } else {
        &input.provider
    };
    if previous.has_api_key
        && (protocol != previous.provider
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
    // Bind legacy credentials before changing metadata. A failed secret write must
    // never leave an old provider's key usable at the newly selected destination.
    if db.setting("model.key_binding")?.is_none() {
        db.set_setting(
            "model.key_binding",
            &model_key_binding(&previous.provider, &previous.base_url),
        )?;
    }
    db.save_model_metadata(&input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input.api_key.as_deref() {
        db.save_secret("model", key)?;
        db.set_setting(
            "model.key_binding",
            &model_key_binding(protocol, &input.base_url),
        )?;
    }
    db.model_config()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_key_destination_is_rejected_before_reading_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("key-binding.db")).unwrap();
        db.set_setting(
            "model.key_binding",
            &model_key_binding("openai-compatible", "https://api.openai.com/v1/"),
        )
        .unwrap();
        let config = ModelConfig {
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            model: "fixture".into(),
            has_api_key: true,
        };
        assert!(configured_model_key(&db, &config)
            .unwrap_err()
            .to_string()
            .contains("当前密钥不属于"));
        assert_eq!(
            model_key_binding("openai-compatible", "https://api.openai.com/v1/"),
            model_key_binding("openai-responses", "https://api.openai.com/v1")
        );
    }
}
