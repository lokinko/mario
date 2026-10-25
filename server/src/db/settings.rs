use super::{params, AppError, AppResult, Database, ModelConfig, OptionalRow};
use crate::models::{AnalysisSettings, ModelProviderKind};

impl Database {
    pub fn investment_methodology(
        &self,
    ) -> AppResult<crate::models::methodology::InvestmentMethodology> {
        use crate::models::methodology::{InvestmentMethodology, DEFAULT_INVESTMENT_METHODOLOGY};
        Ok(InvestmentMethodology::new(
            self.setting("investment.methodology")?
                .unwrap_or_else(|| DEFAULT_INVESTMENT_METHODOLOGY.into()),
        ))
    }

    pub fn save_investment_methodology(
        &self,
        input: crate::models::methodology::InvestmentMethodologyInput,
    ) -> AppResult<crate::models::methodology::InvestmentMethodology> {
        input.validate()?;
        self.set_setting("investment.methodology", &input.prompt)?;
        self.investment_methodology()
    }

    pub fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        let settings = self
            .setting("analysis.settings")?
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .unwrap_or_else(|| Ok(AnalysisSettings::default()))?;
        Ok(normalize_analysis_settings(settings))
    }

    pub fn save_analysis_settings(
        &self,
        settings: AnalysisSettings,
    ) -> AppResult<AnalysisSettings> {
        if !matches!(settings.workflow.as_str(), "quick" | "deep") {
            return Err(AppError::Validation("分析方式无效".into()));
        }
        let settings = normalize_analysis_settings(settings);
        self.set_setting("analysis.settings", &serde_json::to_string(&settings)?)?;
        Ok(settings)
    }

    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self
            .conn()?
            .query_row("SELECT value FROM settings WHERE key=$1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO settings (key,value) VALUES ($1,$2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM settings WHERE key=$1", [key])?;
        Ok(())
    }

    pub fn model_config(&self) -> AppResult<ModelConfig> {
        let has_key = self.secret("model").is_ok();
        let conn = self.conn()?;
        let value = |key: &str, fallback: &str| -> AppResult<String> {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key=$1", [key], |row| {
                    row.get(0)
                })
                .optional()?
                .unwrap_or_else(|| fallback.into()))
        };
        let provider: ModelProviderKind = value(
            "model.provider",
            ModelProviderKind::OpenaiResponses.as_str(),
        )?
        .parse()?;
        Ok(ModelConfig {
            provider,
            base_url: value("model.base_url", "https://api.openai.com/v1")?,
            model: value("model.name", "gpt-4.1-mini")?,
            has_api_key: match provider {
                ModelProviderKind::CodexBridge => !value("model.codex_bridge_id", "")?.is_empty(),
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
            _ => crate::models::validate_endpoint(base_url)?,
        }
        if model.trim().is_empty() {
            return Err(AppError::Validation("请填写模型名称".into()));
        }
        let base_url = base_url.trim_end_matches('/');
        let credential = match api_key {
            Some(key) => Some((
                self.encrypted_secret("model", key)?,
                model_key_binding(provider, base_url),
            )),
            None => None,
        };
        let mut writes = vec![
            ("model.provider", provider.as_str().to_string()),
            ("model.base_url", base_url.to_string()),
            ("model.name", model.to_string()),
        ];
        if let Some((secret, binding)) = credential {
            writes.push(("secret.model", secret));
            writes.push(("model.key_binding", binding));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for (key, value) in &writes {
            tx.execute("INSERT INTO settings (key,value) VALUES ($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![*key, value.as_str()])?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// The destination a stored model credential may be sent to.
pub(crate) fn model_key_binding(provider: ModelProviderKind, base_url: &str) -> String {
    format!("{} {}", provider, base_url.trim_end_matches('/'))
}

// Keep legacy presets compatible while excluding data no longer offered in settings.
fn normalize_analysis_settings(mut settings: AnalysisSettings) -> AnalysisSettings {
    settings.workflow = "quick".into();
    settings.reflect = false;
    settings.explore_alternatives = false;
    let selection = &mut settings.context_selection;
    selection.include_planning = false;
    selection.include_risk_findings = false;
    selection.include_rules = false;
    selection.include_system_reviews = false;
    selection.include_daily_assets = false;
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methodology_is_account_scoped_persistent_and_restorable() {
        use crate::models::methodology::{
            InvestmentMethodologyInput, DEFAULT_INVESTMENT_METHODOLOGY,
        };
        let account = crate::db::TestAccount::new();
        let db = account.open();
        let other = Database::test();
        let initial = db.export_sync_data().unwrap();
        assert_eq!(
            db.investment_methodology().unwrap().prompt,
            DEFAULT_INVESTMENT_METHODOLOGY
        );
        let prompt = "按季度评估，优先解释现金流和费用。\n保留我自己的补充。";
        db.save_investment_methodology(InvestmentMethodologyInput {
            prompt: prompt.into(),
        })
        .unwrap();
        for invalid in ["".to_string(), "  \n".to_string(), "中".repeat(4001)] {
            assert!(db
                .save_investment_methodology(InvestmentMethodologyInput { prompt: invalid })
                .is_err());
        }
        let saved = db.export_sync_data().unwrap();
        drop(db);
        let db = account.open();
        assert_eq!(db.investment_methodology().unwrap().prompt, prompt);
        assert_eq!(
            other.investment_methodology().unwrap().prompt,
            DEFAULT_INVESTMENT_METHODOLOGY
        );
        other.import_sync_data(&saved).unwrap();
        assert_eq!(other.investment_methodology().unwrap().prompt, prompt);
        let mut invalid = saved.clone();
        let row = invalid.tables.iter_mut().find(|table| table.name == "settings").unwrap().rows.iter_mut().find(|row| matches!(&row[0], crate::db::SyncValue::Text(key) if key == "investment.methodology")).unwrap();
        row[1] = crate::db::SyncValue::Text(" ".into());
        assert!(other.import_sync_data(&invalid).is_err());
        assert_eq!(other.investment_methodology().unwrap().prompt, prompt);
        other.import_sync_data(&initial).unwrap();
        assert_eq!(
            other.investment_methodology().unwrap().prompt,
            DEFAULT_INVESTMENT_METHODOLOGY
        );
    }

    #[test]
    fn legacy_presets_only_keep_supported_data_and_use_quick_analysis() {
        let db = Database::test();
        let mut legacy = AnalysisSettings {
            workflow: "deep".into(),
            reflect: true,
            explore_alternatives: true,
            web_search: true,
            context_selection: crate::models::ContextSelection::default(),
        };
        legacy.context_selection.include_holdings = false;
        db.set_setting(
            "analysis.settings",
            &serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        let restored = db.analysis_settings().unwrap();
        assert_eq!(restored.workflow, "quick");
        assert!(restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_holdings);
        assert!(restored.context_selection.include_evidence);
        assert!(!restored.reflect);
        assert!(!restored.explore_alternatives);
        assert!(restored.web_search);
        let selection = &restored.context_selection;
        assert!(!selection.include_planning);
        assert!(!selection.include_risk_findings);
        assert!(!selection.include_rules);
        assert!(!selection.include_system_reviews);
        assert!(!selection.include_daily_assets);

        // Older clients must not re-enable removed options when saving.
        let saved = db.save_analysis_settings(legacy).unwrap();
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
        assert_eq!(settings.workflow, "quick");
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
        assert_eq!(db.analysis_settings().unwrap().workflow, "quick");
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
        assert_eq!(db.secret("model").unwrap(), "key-a");
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
        assert_eq!(db.secret("model").unwrap(), "key-a");
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
            assert!(serde_json::from_value::<crate::models::ModelConfigInput>(
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
