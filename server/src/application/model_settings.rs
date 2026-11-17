//! Account-owned configuration and frozen connection checks through execution ports.
#[cfg(test)]
use crate::db::Database;
use crate::{
    models::{
        connection::{
            ModelConfig, ModelConfigInput, ModelConfigSource, ModelConnectionTest,
            ServerModelCatalog,
        },
        provider::{model_key_binding, ModelProviderKind, ModelTarget},
    },
    ports::{
        account::AccountSession,
        ai_account::{ModelConfiguration, ModelSettingsStore},
        model::{BridgeRegistry, ModelCatalog, ModelConnector},
    },
    AppError, AppResult,
};
pub(crate) struct ModelServices {
    pub connector: std::sync::Arc<dyn ModelConnector>,
    pub bridges: std::sync::Arc<dyn BridgeRegistry>,
    pub catalog: std::sync::Arc<dyn ModelCatalog>,
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
    db: &impl ModelConfiguration,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelConfig> {
    let mut config = db.model_config()?;
    if config.source == ModelConfigSource::Server
        || (server.is_some() && db.model_source()?.is_none())
    {
        config.source = ModelConfigSource::Server;
        config.provider = ModelProviderKind::OpenaiResponses;
        config.base_url = OPENAI_URL.into();
        config.model = db
            .selected_server_model()?
            .or_else(|| server.map(|s| s.model.clone()))
            .unwrap_or_default();
        config.has_api_key = server.is_some();
    }
    config.server_model = server.map(|s| s.model.clone());
    Ok(config)
}

pub(crate) fn delete_key(
    db: &impl ModelSettingsStore,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelConfig> {
    if configuration(db, server)?.source == ModelConfigSource::Server {
        return Err(AppError::Validation(
            "站点密钥由管理员管理，无法从个人账号移除".into(),
        ));
    }
    db.remove_model_credential()?;
    configuration(db, server)
}

pub(crate) async fn server_models(
    db: &(impl ModelConfiguration + AccountSession),
    services: &ModelServices,
) -> AppResult<ServerModelCatalog> {
    let server = services
        .server
        .as_ref()
        .ok_or_else(|| AppError::Validation("站点尚未启用 OpenAI 服务".into()))?;
    let models = db
        .without_account_lock(services.catalog.list(&server.credential))
        .await??;
    Ok(ServerModelCatalog {
        models,
        default_model: server.model.clone(),
    })
}
pub(crate) async fn test_connection(
    db: &(impl ModelConfiguration + AccountSession),
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
    db: &impl ModelConfiguration,
    config: &ModelConfig,
    server: Option<&ServerOpenAi>,
) -> AppResult<ModelTarget> {
    let credential = if config.provider == ModelProviderKind::CodexBridge {
        super::codex_bridge::id(db)?
    } else {
        crate::models::endpoint::validate_endpoint(&config.base_url)?;
        configured_model_key(db, config, server)?
    };
    Ok(ModelTarget {
        config: config.clone(),
        credential,
    })
}

pub(crate) fn configured_model_key(
    db: &impl ModelConfiguration,
    config: &ModelConfig,
    server: Option<&ServerOpenAi>,
) -> AppResult<String> {
    if config.source == ModelConfigSource::Server {
        let server = server.ok_or_else(|| {
            AppError::Validation("站点尚未启用 OpenAI 服务，请联系管理员或配置个人模型".into())
        })?;
        if config.provider != ModelProviderKind::OpenaiResponses
            || config.base_url != OPENAI_URL
            || config.model != configuration(db, Some(server))?.model
        {
            return Err(AppError::Validation(
                "站点 OpenAI 配置已变化，请重新加载".into(),
            ));
        }
        return Ok(server.credential.clone());
    }
    if db
        .model_key_binding()?
        .is_none_or(|binding| binding != model_key_binding(config.provider, &config.base_url))
    {
        return Err(AppError::Validation(
            "当前密钥不属于这个模型接口，请重新保存对应 API Key".into(),
        ));
    }
    db.model_credential()
}

pub(crate) async fn save(
    db: &impl ModelSettingsStore,
    input: ModelConfigInput,
    services: &ModelServices,
) -> AppResult<ModelConfig> {
    let server = services.server.as_ref();
    let bridges = services.bridges.as_ref();
    if input.source == ModelConfigSource::Server {
        let server =
            server.ok_or_else(|| AppError::Validation("站点尚未启用 OpenAI 服务".into()))?;
        if input.provider != ModelProviderKind::OpenaiResponses
            || input.base_url != OPENAI_URL
            || input.api_key.is_some()
            || input.model.trim().is_empty()
        {
            return Err(AppError::Validation(
                "请选择站点模型，地址和密钥由管理员配置".into(),
            ));
        }
        let current = configuration(db, Some(server))?;
        if input.model != server.model
            && !(current.source == ModelConfigSource::Server && input.model == current.model)
        {
            let revision = db.revision()?;
            let catalog = server_models(db, services).await?;
            if db.revision()? != revision {
                return Err(AppError::Conflict(
                    "读取模型目录期间账号资料已变化，请刷新后重试".into(),
                ));
            }
            if !catalog.models.contains(&input.model) {
                return Err(AppError::Validation(
                    "此模型不在站点返回的文本模型目录中，请重新选择".into(),
                ));
            }
        }
        db.save_server_model(&input.model)?;
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

    struct ReadOnlyModel;
    impl ModelConfiguration for ReadOnlyModel {
        fn model_config(&self) -> AppResult<ModelConfig> {
            Ok(ModelConfig {
                provider: ModelProviderKind::OpenaiChat,
                base_url: "https://example.test/v1".into(),
                model: "chosen".into(),
                has_api_key: false,
                source: ModelConfigSource::Account,
                server_model: None,
            })
        }
        fn model_source(&self) -> AppResult<Option<String>> {
            Ok(Some("account".into()))
        }
        fn selected_server_model(&self) -> AppResult<Option<String>> {
            Ok(None)
        }
        fn model_key_binding(&self) -> AppResult<Option<String>> {
            Ok(None)
        }
        fn model_credential(&self) -> AppResult<String> {
            Err(AppError::Validation("no credential".into()))
        }
        fn bridge_id(&self) -> AppResult<Option<String>> {
            Ok(None)
        }
    }
    #[test]
    fn configuration_uses_a_read_only_port_without_database_or_mutation_capabilities() {
        let server = ServerOpenAi::new("site-model".into(), "site-key".into()).unwrap();
        let result = configuration(&ReadOnlyModel, Some(&server)).unwrap();
        assert_eq!(result.model, "chosen");
        assert_eq!(result.source, ModelConfigSource::Account);
        assert_eq!(result.server_model.as_deref(), Some("site-model"));
        assert!(!result.has_api_key);
    }

    #[test]
    fn removing_a_site_key_is_rejected_without_deleting_the_saved_personal_credential() {
        let db = Database::test();
        db.save_model_configuration(
            ModelProviderKind::OpenaiResponses,
            OPENAI_URL,
            "personal",
            Some("personal-key"),
        )
        .unwrap();
        let server = ServerOpenAi::new("site-model".into(), "site-key".into()).unwrap();
        db.save_server_model("site-model").unwrap();
        assert!(delete_key(&db, Some(&server)).is_err());
        assert_eq!(db.secret("model").unwrap(), "personal-key");
        db.save_model_configuration(
            ModelProviderKind::OpenaiResponses,
            OPENAI_URL,
            "personal",
            None,
        )
        .unwrap();
        assert!(!delete_key(&db, Some(&server)).unwrap().has_api_key);
        assert!(db.secret("model").is_err());
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
        // Older settings without an explicit source now use the site default,
        // while the personal credential remains available for an explicit opt-out.
        a.delete_setting("model.source").unwrap();
        assert_eq!(
            configuration(&a, Some(&server)).unwrap().source,
            ModelConfigSource::Server
        );
        assert_eq!(a.secret("model").unwrap(), "personal-secret");
        a.set_setting("model.source", "account").unwrap();
        // Losing a personal key never silently switches its billing to the site.
        a.remove_secret("model").unwrap();
        assert!(!configuration(&a, Some(&server)).unwrap().has_api_key);
        assert_eq!(
            configuration(&b, Some(&server)).unwrap().source,
            ModelConfigSource::Server
        );
        assert!(!configuration(&b, None).unwrap().has_api_key);
    }

    #[tokio::test]
    async fn shared_selection_rejects_redirection_and_disabled_service_without_writes() {
        let db = Database::test();
        let server = server();
        let model_services = services(Some(self::server()));
        let _lock = db.lock_account().unwrap();
        let input = || ModelConfigInput {
            source: ModelConfigSource::Server,
            provider: ModelProviderKind::OpenaiResponses,
            base_url: OPENAI_URL.into(),
            model: server.model.clone(),
            api_key: None,
        };
        let revision = db.revision().unwrap();
        assert!(save(&db, input(), &services(None)).await.is_err());
        for change in 0..4 {
            let mut request = input();
            match change {
                0 => request.base_url = "https://untrusted.example.test/v1".into(),
                1 => request.model = "arbitrary-model".into(),
                2 => request.provider = ModelProviderKind::OpenaiChat,
                _ => request.api_key = Some("supplied-key".into()),
            }
            assert!(save(&db, request, &model_services).await.is_err());
        }
        assert_eq!(db.revision().unwrap(), revision);
        let selected = save(&db, input(), &model_services).await.unwrap();
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

    #[tokio::test]
    async fn catalog_choices_are_account_scoped_persistent_and_atomic() {
        crate::secrets::use_test_master_key();
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let other = Database::test();
        let services = services(Some(server()));
        let input = |name: &str| ModelConfigInput {
            source: ModelConfigSource::Server,
            provider: ModelProviderKind::OpenaiResponses,
            base_url: OPENAI_URL.into(),
            model: name.into(),
            api_key: None,
        };
        {
            let _lock = db.lock_account().unwrap();
            let revision = db.revision().unwrap();
            let catalog = server_models(&db, &services).await.unwrap();
            assert_eq!(catalog.models, ["catalog-fixture", "catalog-second"]);
            assert_eq!(db.revision().unwrap(), revision);
            let selected = save(&db, input("catalog-fixture"), &services)
                .await
                .unwrap();
            assert_eq!(selected.model, "catalog-fixture");
            assert_eq!(
                configured_model_key(&db, &selected, services.server.as_ref()).unwrap(),
                "site-secret-never-export"
            );
            assert_eq!(
                configuration(&other, services.server.as_ref())
                    .unwrap()
                    .model,
                "shared-fixture"
            );
        }
        drop(db);
        let db = account.open();
        assert_eq!(
            configuration(&db, services.server.as_ref()).unwrap().model,
            "catalog-fixture"
        );
        assert!(!serde_json::to_string(&db.export_sync_data().unwrap())
            .unwrap()
            .contains("model.server_name"));
        db.save_model_configuration(
            ModelProviderKind::OpenaiChat,
            "https://personal.example.test/v1",
            "personal",
            Some("personal-secret"),
        )
        .unwrap();
        crate::db::fail_inserts_when(
            &account.connection(),
            "settings",
            "NEW.key = 'model.server_name'",
            "site selection write failed",
        );
        let _lock = db.lock_account().unwrap();
        assert!(save(&db, input("shared-fixture"), &services).await.is_err());
        let unchanged = configuration(&db, services.server.as_ref()).unwrap();
        assert_eq!(unchanged.source, ModelConfigSource::Account);
        assert_eq!(unchanged.model, "personal");
        assert_eq!(
            db.setting("model.server_name").unwrap().as_deref(),
            Some("catalog-fixture")
        );
        assert_eq!(db.secret("model").unwrap(), "personal-secret");
    }

    struct UnavailableCatalog;
    #[async_trait::async_trait]
    impl ModelCatalog for UnavailableCatalog {
        async fn list(&self, _: &str) -> AppResult<Vec<String>> {
            Err(AppError::Model("catalog unavailable".into()))
        }
    }
    #[tokio::test]
    async fn unavailable_catalog_preserves_current_choice_and_allows_the_site_default() {
        let db = Database::test();
        let mut services = services(Some(server()));
        db.save_server_model("saved-choice").unwrap();
        services.catalog = std::sync::Arc::new(UnavailableCatalog);
        let input = |name: &str| ModelConfigInput {
            source: ModelConfigSource::Server,
            provider: ModelProviderKind::OpenaiResponses,
            base_url: OPENAI_URL.into(),
            model: name.into(),
            api_key: None,
        };
        let _lock = db.lock_account().unwrap();
        let before = db.revision().unwrap();
        assert!(server_models(&db, &services).await.is_err());
        assert!(save(&db, input("new-choice"), &services).await.is_err());
        assert_eq!(db.revision().unwrap(), before);
        assert_eq!(
            save(&db, input("saved-choice"), &services)
                .await
                .unwrap()
                .model,
            "saved-choice"
        );
        assert_eq!(
            save(&db, input("shared-fixture"), &services)
                .await
                .unwrap()
                .model,
            "shared-fixture"
        );
    }

    struct TestCatalog;
    #[async_trait::async_trait]
    impl ModelCatalog for TestCatalog {
        async fn list(&self, key: &str) -> AppResult<Vec<String>> {
            assert_eq!(key, "site-secret-never-export");
            Ok(vec!["catalog-fixture".into(), "catalog-second".into()])
        }
    }
    fn services(server: Option<ServerOpenAi>) -> ModelServices {
        ModelServices {
            server,
            connector: std::sync::Arc::new(SharedConnector),
            bridges: std::sync::Arc::new(crate::ai::management::BridgeRegistryAdapter),
            catalog: std::sync::Arc::new(TestCatalog),
        }
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
