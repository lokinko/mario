//! Account-owned configuration and frozen connection checks through execution ports.
use crate::{
    db::{model_key_binding, Database},
    models::{
        ModelConfig, ModelConfigInput, ModelConfigSource, ModelConnectionTest, ModelProviderKind,
        ModelTarget,
    },
    ports::model::{BridgeRegistry, ModelConnector},
    AppError, AppResult,
};
pub(crate) struct ModelServices {
    pub connector: std::sync::Arc<dyn ModelConnector>,
    pub bridges: std::sync::Arc<dyn BridgeRegistry>,
    pub server: Option<ServerOpenAi>,
}

/// Process-owned credential. Never serialized, logged, or stored in tenant settings.
pub(crate) struct ServerOpenAi {
    model: String,
    credential: String,
}
const OPENAI_URL: &str = "https://api.openai.com/v1";
impl ServerOpenAi {
    pub(crate) fn new(model: String, credential: String) -> AppResult<Self> {
        if model.trim().is_empty()
            || credential.trim().is_empty()
            || credential.contains(['\r', '\n'])
        {
            return Err(AppError::Validation(
                "站点 OpenAI 的模型名称或密钥配置无效".into(),
            ));
        }
        Ok(Self {
            model: model.trim().into(),
            credential: credential.trim().into(),
        })
    }
}

pub(crate) fn configuration(
    db: &Database,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelConfig> {
    let mut config = db.model_config()?;
    if config.source == ModelConfigSource::Server
        || (server.is_some()
            && !config.has_api_key
            && db.setting("model.provider")?.is_none()
            && db.setting("model.source")?.is_none())
    {
        config.source = ModelConfigSource::Server;
        config.provider = ModelProviderKind::OpenaiResponses;
        config.base_url = OPENAI_URL.into();
        config.model = server.map(|s| s.model.clone()).unwrap_or_default();
        config.has_api_key = server.is_some();
    }
    config.server_model = server.map(|s| s.model.clone());
    Ok(config)
}
pub(crate) async fn test_connection(
    db: &Database,
    connector: &dyn ModelConnector,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelConnectionTest> {
    let config = configuration(db, server)?;
    let target = frozen_target(db, &config, server)?;
    let started = std::time::Instant::now();
    let result = db
        .without_account_lock(connector.test(target.clone()))
        .await?;
    tracing::debug!(target: "mario::timing", stage = "supplier", kind = "model_test", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
    let result = result?;
    let current = configuration(db, server)?;
    if serde_json::to_value(&current)? != serde_json::to_value(&config)?
        || frozen_target(db, &current, server)
            .ok()
            .is_none_or(|current| current.credential != target.credential)
    {
        return Err(AppError::Conflict(
            "测试期间模型配置已变化，请测试当前配置".into(),
        ));
    }
    Ok(result)
}
fn frozen_target(
    db: &Database,
    config: &ModelConfig,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelTarget> {
    let credential = if config.provider == ModelProviderKind::CodexBridge {
        super::codex_bridge::id(db)?
    } else {
        crate::models::validate_endpoint(&config.base_url)?;
        configured_model_key(db, config, server)?
    };
    Ok(ModelTarget {
        config: config.clone(),
        credential,
    })
}

pub(crate) fn configured_model_key(
    db: &Database,
    config: &ModelConfig,
    server: Option<&ServerOpenAi>,
) -> AppResult<String> {
    if config.source == ModelConfigSource::Server {
        let server = server.ok_or_else(|| {
            AppError::Validation("站点尚未启用 OpenAI 服务，请联系管理员或配置个人模型".into())
        })?;
        if config.provider != ModelProviderKind::OpenaiResponses
            || config.base_url != OPENAI_URL
            || config.model != server.model
        {
            return Err(AppError::Validation(
                "站点 OpenAI 配置已变化，请重新加载".into(),
            ));
        }
        return Ok(server.credential.clone());
    }
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
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelConfig> {
    if input.source == ModelConfigSource::Server {
        let server =
            server.ok_or_else(|| AppError::Validation("站点尚未启用 OpenAI 服务".into()))?;
        if input.provider != ModelProviderKind::OpenaiResponses
            || input.base_url != OPENAI_URL
            || input.model != server.model
            || input.api_key.is_some()
        {
            return Err(AppError::Validation(
                "站点 OpenAI 的地址、模型和密钥由管理员配置".into(),
            ));
        }
        db.set_setting("model.source", "server")?;
        return configuration(db, Some(server));
    }
    if input.provider == ModelProviderKind::CodexBridge {
        super::codex_bridge::save(db, &input.model, bridges)?;
        return configuration(db, server);
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
    configuration(db, server)
}
#[cfg(test)]
#[path = "model_settings_tests.rs"]
mod concurrency_tests;
#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> ServerOpenAi {
        ServerOpenAi::new("shared-fixture".into(), "site-secret-never-export".into()).unwrap()
    }

    #[test]
    fn shared_model_defaults_are_read_only_and_do_not_replace_personal_destinations() {
        crate::secrets::use_test_master_key();
        let a = Database::test();
        let b = Database::test();
        let server = server();
        let revision = a.revision().unwrap();
        for db in [&a, &b] {
            let config = configuration(db, Some(&server)).unwrap();
            assert_eq!(config.source, ModelConfigSource::Server);
            assert_eq!(config.base_url, OPENAI_URL);
            assert_eq!(
                configured_model_key(db, &config, Some(&server)).unwrap(),
                server.credential
            );
            assert!(db.setting("model.source").unwrap().is_none());
            assert!(db.setting("secret.model").unwrap().is_none());
            let exported = serde_json::to_string(&db.export_sync_data().unwrap()).unwrap();
            assert!(!exported.contains(&server.credential));
            assert!(!serde_json::to_string(&config)
                .unwrap()
                .contains(&server.credential));
        }
        assert_eq!(a.revision().unwrap(), revision);
        a.save_model_configuration(
            ModelProviderKind::OpenaiChat,
            "https://personal.example.test/v1",
            "personal",
            Some("personal-secret"),
        )
        .unwrap();
        let personal = configuration(&a, Some(&server)).unwrap();
        assert_eq!(personal.source, ModelConfigSource::Account);
        assert_eq!(
            configured_model_key(&a, &personal, Some(&server)).unwrap(),
            "personal-secret"
        );
        // Losing a personal key never silently switches its billing to the site.
        a.remove_secret("model").unwrap();
        assert!(!configuration(&a, Some(&server)).unwrap().has_api_key);
        assert_eq!(
            configuration(&b, Some(&server)).unwrap().source,
            ModelConfigSource::Server
        );
        assert!(!configuration(&b, None).unwrap().has_api_key);
    }

    #[test]
    fn shared_selection_rejects_redirection_and_disabled_service_without_writes() {
        let db = Database::test();
        let server = server();
        let registry = crate::ai::management::BridgeRegistryAdapter;
        let input = || ModelConfigInput {
            source: ModelConfigSource::Server,
            provider: ModelProviderKind::OpenaiResponses,
            base_url: OPENAI_URL.into(),
            model: server.model.clone(),
            api_key: None,
        };
        let revision = db.revision().unwrap();
        assert!(save(&db, input(), &registry, None).is_err());
        for change in 0..4 {
            let mut request = input();
            match change {
                0 => request.base_url = "https://untrusted.example.test/v1".into(),
                1 => request.model = "arbitrary-model".into(),
                2 => request.provider = ModelProviderKind::OpenaiChat,
                _ => request.api_key = Some("supplied-key".into()),
            }
            assert!(save(&db, request, &registry, Some(&server)).is_err());
        }
        assert_eq!(db.revision().unwrap(), revision);
        let selected = save(&db, input(), &registry, Some(&server)).unwrap();
        let mut redirected = selected.clone();
        redirected.base_url = "https://untrusted.example.test/v1".into();
        assert!(configured_model_key(&db, &redirected, Some(&server)).is_err());
        let disabled = configuration(&db, None).unwrap();
        assert_eq!(disabled.source, ModelConfigSource::Server);
        assert!(!disabled.has_api_key);
        assert!(configured_model_key(&db, &disabled, None).is_err());
        assert!(db.setting("secret.model").unwrap().is_none());
        // Source preference is stored in account settings, but stays out of personal backups.
        assert_eq!(
            db.setting("model.source").unwrap().as_deref(),
            Some("server")
        );
        assert!(!serde_json::to_string(&db.export_sync_data().unwrap())
            .unwrap()
            .contains("model.source"));
    }

    struct SharedConnector;
    #[async_trait::async_trait]
    impl ModelConnector for SharedConnector {
        async fn test(&self, target: ModelTarget) -> AppResult<ModelConnectionTest> {
            assert_eq!(target.config.base_url, OPENAI_URL);
            assert_eq!(target.config.provider, ModelProviderKind::OpenaiResponses);
            assert_eq!(target.credential, "site-secret-never-export");
            Ok(ModelConnectionTest {
                ok: true,
                model: target.config.model,
                latency_ms: 1,
            })
        }
    }

    #[tokio::test]
    async fn shared_connection_uses_injected_secret_and_disabled_service_never_calls_connector() {
        let db = Database::test();
        let server = server();
        let _lock = db.lock_account().unwrap();
        assert!(
            test_connection(&db, &SharedConnector, Some(&server))
                .await
                .unwrap()
                .ok
        );
        db.set_setting("model.source", "server").unwrap();
        assert!(test_connection(&db, &SharedConnector, None).await.is_err());
        assert!(ServerOpenAi::new("".into(), "key".into()).is_err());
        assert!(ServerOpenAi::new("model".into(), "key\nheader".into()).is_err());
    }

    #[test]
    fn missing_or_mismatched_key_binding_is_rejected_before_reading_credentials() {
        let db = Database::test();
        let config = ModelConfig {
            source: ModelConfigSource::Account,
            server_model: None,
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
            assert!(configured_model_key(&db, &config, None)
                .unwrap_err()
                .to_string()
                .contains("当前密钥不属于"));
        }
    }
}
