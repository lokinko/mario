//! AI model destinations, bound credentials and analysis presets.
use crate::models::{
    analysis::AnalysisSettings,
    provider::{model_key_binding, ModelProviderKind},
};
use {
    super::settings::ai::AiSetting,
    crate::{
        db::Database,
        models::connection::ModelConfig,
        storage::{params, OptionalRow},
        AppError, AppResult,
    },
};

impl Database {
    pub fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        let settings = self
            .ai_setting(AiSetting::Presets)?
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .unwrap_or_else(|| Ok(AnalysisSettings::default()))?;
        Ok(normalize_analysis_settings(settings))
    }

    pub fn save_analysis_settings(
        &self,
        settings: AnalysisSettings,
    ) -> AppResult<AnalysisSettings> {
        if settings.workflow != crate::models::analysis::ANALYSIS_WORKFLOW {
            return Err(AppError::Validation("分析方式无效".into()));
        }
        let settings = normalize_analysis_settings(settings);
        self.set_ai_setting(AiSetting::Presets, &serde_json::to_string(&settings)?)?;
        Ok(settings)
    }

    pub fn model_config(&self) -> AppResult<ModelConfig> {
        let has_key = self.model_secret().is_ok();
        let conn = self.conn()?;
        let value = |key: AiSetting, fallback: &str| -> AppResult<String> {
            Ok(conn
                .query_row(
                    "SELECT value FROM settings WHERE key=$1",
                    [key.key()],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_else(|| fallback.into()))
        };
        let provider: ModelProviderKind = value(
            AiSetting::Provider,
            ModelProviderKind::OpenaiResponses.as_str(),
        )?
        .parse()?;
        Ok(ModelConfig {
            source: if value(AiSetting::Source, "account")? == "server" {
                crate::models::connection::ModelConfigSource::Server
            } else {
                crate::models::connection::ModelConfigSource::Account
            },
            server_model: None,
            provider,
            base_url: value(AiSetting::BaseUrl, "https://api.openai.com/v1")?,
            model: value(AiSetting::ModelName, "gpt-4.1-mini")?,
            has_api_key: match provider {
                ModelProviderKind::CodexBridge => !value(AiSetting::Bridge, "")?.is_empty(),
                _ => has_key,
            },
        })
    }

    /// Replaces the model destination and, when given, its credential in one transaction, so a
    /// failed save leaves the previous configuration and key binding intact.
    pub fn save_model_configuration(
        &self,
        provider: ModelProviderKind,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
    ) -> AppResult<()> {
        match provider {
            ModelProviderKind::CodexBridge if base_url != "codex://bridge" => {
                return Err(AppError::Validation("桥接地址无效".into()));
            }
            ModelProviderKind::CodexBridge => {}
            _ => crate::models::endpoint::validate_endpoint(base_url)?,
        }
        if model.trim().is_empty() {
            return Err(AppError::Validation("请填写模型名称".into()));
        }
        let base_url = base_url.trim_end_matches('/');
        let credential = match api_key {
            Some(key) => Some((
                self.encrypt_model_secret(key)?,
                model_key_binding(provider, base_url),
            )),
            None => None,
        };
        let mut writes = vec![
            (AiSetting::Source, "account".to_string()),
            (AiSetting::Provider, provider.as_str().to_string()),
            (AiSetting::BaseUrl, base_url.to_string()),
            (AiSetting::ModelName, model.to_string()),
        ];
        if let Some((secret, binding)) = credential {
            writes.push((AiSetting::ModelCredential, secret));
            writes.push((AiSetting::KeyBinding, binding));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for (key, value) in &writes {
            tx.execute("INSERT INTO settings (key,value) VALUES ($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key.key(), value.as_str()])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The site selection is independent from the saved personal connection and secret.
    pub fn save_server_model(&self, model: &str) -> AppResult<()> {
        if model.trim().is_empty() {
            return Err(AppError::Validation("请选择站点模型".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for (key, value) in [
            (AiSetting::Source, "server"),
            (AiSetting::ServerModel, model),
        ] {
            tx.execute("INSERT INTO settings (key,value) VALUES ($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key.key(), value])?;
        }
        tx.commit()?;
        Ok(())
    }
}

// Keep legacy presets compatible while excluding data no longer offered in settings.
fn normalize_analysis_settings(mut settings: AnalysisSettings) -> AnalysisSettings {
    settings.workflow = crate::models::analysis::ANALYSIS_WORKFLOW.into();
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_defaults_on_but_saved_false_is_preserved() {
        assert!(AnalysisSettings::default().web_search);
        let request: crate::models::analysis::AnalysisRequest = serde_json::from_value(
            serde_json::json!({"question":"当前状态", "workflow":"council"}),
        )
        .unwrap();
        assert!(request.web_search);
        let request: crate::models::analysis::AnalysisRequest = serde_json::from_value(
            serde_json::json!({"question":"当前状态", "workflow":"council", "webSearch":false}),
        )
        .unwrap();
        assert!(!request.web_search);
        let account = crate::db::TestAccount::new();
        let db = account.open();
        assert!(db.analysis_settings().unwrap().web_search);
        db.save_analysis_settings(AnalysisSettings {
            web_search: false,
            ..Default::default()
        })
        .unwrap();
        drop(db);
        assert!(!account.open().analysis_settings().unwrap().web_search);
    }

    #[test]
    fn upgrade_strips_retired_preset_options_and_moves_to_the_council_workflow() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let mut legacy = AnalysisSettings {
            workflow: "deep".into(),

            web_search: true,
            context_selection: crate::models::analysis::ContextSelection::default(),
        };
        legacy.context_selection.include_holdings = false;
        let mut stored = serde_json::to_value(&legacy).unwrap();
        stored["reflect"] = true.into();
        stored["exploreAlternatives"] = true.into();
        stored["useMemory"] = true.into();
        stored["contextSelection"]["includeRules"] = true.into();
        stored["contextSelection"]
            .as_object_mut()
            .unwrap()
            .remove("includeMemory");
        db.set_setting(
            "analysis.settings",
            &serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();
        drop(db);
        let db = account.reopen_from_old_release();

        let restored = db.analysis_settings().unwrap();
        assert_eq!(
            restored.workflow,
            crate::models::analysis::ANALYSIS_WORKFLOW
        );
        assert!(restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_holdings);
        assert!(restored.context_selection.include_evidence);
        assert!(restored.context_selection.include_memory);
        assert!(restored.web_search);

        // Older clients must not re-enable removed options when saving.
        assert!(db.save_analysis_settings(legacy).is_err());
        assert!(serde_json::from_value::<AnalysisSettings>(stored).is_err());
        let saved = db.save_analysis_settings(restored.clone()).unwrap();
        let expected = serde_json::to_value(restored).unwrap();
        assert_eq!(serde_json::to_value(saved).unwrap(), expected);
        let persisted = db.setting("analysis.settings").unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&persisted).unwrap(),
            expected
        );
    }

    #[test]
    fn analysis_defaults_persist_across_restarts_and_reject_invalid_workflows() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let mut settings = db.analysis_settings().unwrap();
        assert_eq!(
            settings.workflow,
            crate::models::analysis::ANALYSIS_WORKFLOW
        );
        assert!(settings.context_selection.include_profile);
        settings.web_search = false;
        settings.context_selection.include_profile = false;
        settings.context_selection.include_evidence = false;
        db.save_analysis_settings(settings).unwrap();
        drop(db);
        let db = account.open();
        let mut restored = db.analysis_settings().unwrap();
        assert!(!restored.web_search);
        assert!(!restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_evidence);
        restored.workflow = "invalid".into();
        assert!(db.save_analysis_settings(restored).is_err());
        assert_eq!(
            db.analysis_settings().unwrap().workflow,
            crate::models::analysis::ANALYSIS_WORKFLOW
        );
    }

    #[test]
    fn upgrading_an_old_tenant_canonicalizes_provider_and_binds_credentials_once() {
        let account = crate::db::TestAccount::new();
        let db = account.open();
        db.set_setting("model.provider", "openai-compatible")
            .unwrap();
        db.set_setting("model.base_url", "https://models.example.test/v1/")
            .unwrap();
        db.set_setting("secret.model", "encrypted-credential-preserved")
            .unwrap();
        db.set_setting("trading.reset.old", "true").unwrap();
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(
            db.model_config().unwrap().provider,
            ModelProviderKind::OpenaiResponses
        );
        assert_eq!(
            db.setting("model.key_binding").unwrap().as_deref(),
            Some("openai-responses https://models.example.test/v1")
        );
        assert_eq!(
            db.setting("secret.model").unwrap().as_deref(),
            Some("encrypted-credential-preserved")
        );
        assert!(db.setting("trading.reset.old").unwrap().is_none());
        db.set_setting(
            "model.key_binding",
            "openai-compatible https://original.example.test/v1",
        )
        .unwrap();
        drop(db);
        let db = account.reopen_from_old_release();
        assert_eq!(
            db.setting("model.key_binding").unwrap().as_deref(),
            Some("openai-responses https://original.example.test/v1")
        );
    }

    #[test]
    fn a_failed_save_keeps_the_previous_configuration_and_credential() {
        crate::secrets::use_test_master_key();
        let account = crate::db::TestAccount::new();
        let db = account.open();
        db.save_model_configuration(
            ModelProviderKind::Anthropic,
            "https://api.anthropic.com/v1/",
            "claude-test",
            Some("key-a"),
        )
        .unwrap();
        let saved = db.model_config().unwrap();
        let binding = || db.setting("model.key_binding").unwrap();
        assert_eq!(saved.base_url, "https://api.anthropic.com/v1");
        assert_eq!(db.model_secret().unwrap(), "key-a");
        assert_eq!(
            binding().as_deref(),
            Some("anthropic https://api.anthropic.com/v1")
        );

        crate::db::fail_inserts_when(
            &account.connection(),
            "settings",
            "NEW.key = 'model.key_binding'",
            "binding write failed",
        );
        assert!(db
            .save_model_configuration(
                ModelProviderKind::OpenaiChat,
                "https://models.example.test/v1",
                "other-model",
                Some("key-b"),
            )
            .is_err());
        assert!(db
            .save_model_configuration(
                ModelProviderKind::OpenaiChat,
                "https://models.example.test/v1",
                "other-model",
                Some("   "),
            )
            .unwrap_err()
            .to_string()
            .contains("API Key 不能为空"));
        let after = db.model_config().unwrap();
        assert_eq!(
            (after.provider, after.base_url, after.model),
            (saved.provider, saved.base_url, saved.model)
        );
        assert_eq!(db.model_secret().unwrap(), "key-a");
        assert_eq!(
            binding().as_deref(),
            Some("anthropic https://api.anthropic.com/v1")
        );
    }

    #[test]
    fn saves_current_native_protocols_and_rejects_retired_names() {
        let db = Database::test();
        db.save_model_configuration(
            ModelProviderKind::Anthropic,
            "https://api.anthropic.com/v1",
            "claude-test",
            None,
        )
        .unwrap();
        assert_eq!(
            db.model_config().unwrap().provider,
            ModelProviderKind::Anthropic
        );
        for retired in ["openai-compatible", "other"] {
            assert!(serde_json::from_value::<crate::models::connection::ModelConfigInput>(
                serde_json::json!({"provider": retired, "baseUrl": "https://api.openai.com/v1", "model": "test"})
            )
            .is_err());
        }
        db.save_model_configuration(
            ModelProviderKind::OpenaiChat,
            "http://models.example.test:8080/custom/v1/",
            "third-party-model",
            None,
        )
        .unwrap();
        let config = db.model_config().unwrap();
        assert_eq!(config.provider, ModelProviderKind::OpenaiChat);
        assert_eq!(config.base_url, "http://models.example.test:8080/custom/v1");
        assert_eq!(config.model, "third-party-model");
        assert!(db
            .save_model_configuration(
                ModelProviderKind::OpenaiChat,
                "https://models.example.test?key=secret",
                "test",
                None,
            )
            .is_err());
    }
}
