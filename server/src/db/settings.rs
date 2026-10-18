use super::{params, AppError, AppResult, Database, ModelConfig, OptionalRow};
use crate::models::{AnalysisSettings, ModelProviderKind};

impl Database {
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
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn()?.execute(
            "INSERT INTO settings (key,value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM settings WHERE key=?1", [key])?;
        Ok(())
    }

    pub fn model_config(&self) -> AppResult<ModelConfig> {
        let has_key = self.secret("model").is_ok();
        let conn = self.conn()?;
        let value = |key: &str, fallback: &str| -> AppResult<String> {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
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
                ModelProviderKind::Codex => value("model.codex_ready", "false")? == "true",
                _ => has_key,
            },
        })
    }

    pub fn save_model_metadata(
        &self,
        provider: ModelProviderKind,
        base_url: &str,
        model: &str,
    ) -> AppResult<()> {
        match provider {
            ModelProviderKind::CodexBridge if base_url != "codex://bridge" => {
                return Err(AppError::Validation("桥接地址无效".into()));
            }
            ModelProviderKind::CodexBridge => {}
            ModelProviderKind::Codex if self.hosted() || base_url != "codex://local" => {
                return Err(AppError::Validation("Codex 登录仅支持本机开发模式".into()));
            }
            ModelProviderKind::Codex => {}
            _ => crate::models::validate_endpoint(base_url)?,
        }
        if model.trim().is_empty() {
            return Err(AppError::Validation("请填写模型名称".into()));
        }
        let conn = self.conn()?;
        for (key, value) in [
            ("model.provider", provider.as_str()),
            ("model.base_url", base_url.trim_end_matches('/')),
            ("model.name", model),
        ] {
            conn.execute("INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        Ok(())
    }
}

// Keep legacy presets compatible while excluding data no longer offered in settings.
fn normalize_analysis_settings(mut settings: AnalysisSettings) -> AnalysisSettings {
    settings.workflow = "deep".into();
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
    fn legacy_presets_only_keep_supported_data_and_use_deep_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("config.db")).unwrap();
        let mut legacy = AnalysisSettings {
            workflow: "quick".into(),
            context_selection: crate::models::ContextSelection::default(),
            ..AnalysisSettings::default()
        };
        legacy.context_selection.include_holdings = false;
        db.set_setting(
            "analysis.settings",
            &serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        let restored = db.analysis_settings().unwrap();
        assert_eq!(restored.workflow, "deep");
        assert!(restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_holdings);
        assert!(restored.context_selection.include_evidence);
        assert!(restored.reflect);
        assert!(restored.explore_alternatives);
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(&path).unwrap();
        let mut settings = db.analysis_settings().unwrap();
        assert_eq!(settings.workflow, "deep");
        assert!(settings.context_selection.include_profile);
        settings.web_search = false;
        settings.context_selection.include_profile = false;
        settings.context_selection.include_evidence = false;
        db.save_analysis_settings(settings).unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        let mut restored = db.analysis_settings().unwrap();
        assert!(!restored.web_search);
        assert!(!restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_evidence);
        restored.workflow = "invalid".into();
        assert!(db.save_analysis_settings(restored).is_err());
        assert_eq!(db.analysis_settings().unwrap().workflow, "deep");
    }

    #[test]
    fn opening_an_old_database_canonicalizes_provider_and_binds_credentials_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy-model.db");
        let db = Database::open(&path).unwrap();
        db.set_setting("model.provider", "openai-compatible")
            .unwrap();
        db.set_setting("model.base_url", "https://models.example.test/v1/")
            .unwrap();
        db.set_setting("secret.model", "encrypted-credential-preserved")
            .unwrap();
        db.set_setting("trading.reset.old", "true").unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
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
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.setting("model.key_binding").unwrap().as_deref(),
            Some("openai-responses https://original.example.test/v1")
        );
    }

    #[test]
    fn saves_current_native_protocols_and_rejects_retired_names() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("config.db")).unwrap();
        db.save_model_metadata(
            ModelProviderKind::Anthropic,
            "https://api.anthropic.com/v1",
            "claude-test",
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
        db.save_model_metadata(
            ModelProviderKind::OpenaiChat,
            "http://models.example.test:8080/custom/v1/",
            "third-party-model",
        )
        .unwrap();
        let config = db.model_config().unwrap();
        assert_eq!(config.provider, ModelProviderKind::OpenaiChat);
        assert_eq!(config.base_url, "http://models.example.test:8080/custom/v1");
        assert_eq!(config.model, "third-party-model");
        assert!(db
            .save_model_metadata(
                ModelProviderKind::OpenaiChat,
                "https://models.example.test?key=secret",
                "test"
            )
            .is_err());
    }
}
