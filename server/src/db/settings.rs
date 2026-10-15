use super::{params, AppError, AppResult, Database, ModelConfig, OptionalRow};
use crate::models::AnalysisSettings;

impl Database {
    pub fn analysis_settings(&self) -> AppResult<AnalysisSettings> {
        self.setting("analysis.settings")?
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .unwrap_or_else(|| Ok(AnalysisSettings::default()))
    }

    pub fn save_analysis_settings(
        &self,
        settings: AnalysisSettings,
    ) -> AppResult<AnalysisSettings> {
        if !matches!(settings.workflow.as_str(), "quick" | "deep") {
            return Err(AppError::Validation("请选择快速或深度分析".into()));
        }
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
        Ok(ModelConfig {
            provider: match value("model.provider", "openai-responses")?.as_str() {
                "openai-compatible" => "openai-responses".into(),
                other => other.into(),
            },
            base_url: value("model.base_url", "https://api.openai.com/v1")?,
            model: value("model.name", "gpt-4.1-mini")?,
            has_api_key: if value("model.provider", "")? == "codex-bridge" {
                !value("model.codex_bridge_id", "")?.is_empty()
            } else if value("model.provider", "")? == "codex" {
                value("model.codex_ready", "false")? == "true"
            } else {
                has_key
            },
        })
    }

    pub fn save_model_metadata(
        &self,
        provider: &str,
        base_url: &str,
        model: &str,
    ) -> AppResult<()> {
        if !matches!(
            provider,
            "openai-responses"
                | "openai-chat"
                | "anthropic"
                | "openai-compatible"
                | "codex"
                | "codex-bridge"
        ) {
            return Err(AppError::Validation(
                "请选择 OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 或 Codex"
                    .into(),
            ));
        }
        if provider == "codex-bridge" {
            if base_url != "codex://bridge" {
                return Err(AppError::Validation("桥接地址无效".into()));
            }
        } else if provider != "codex" {
            crate::models::validate_endpoint(base_url)?;
        } else if self.hosted() || base_url != "codex://local" {
            return Err(AppError::Validation("Codex 登录仅支持本机开发模式".into()));
        }
        if model.trim().is_empty() {
            return Err(AppError::Validation("请填写模型名称".into()));
        }
        let conn = self.conn()?;
        for (key, value) in [
            ("model.provider", provider),
            ("model.base_url", base_url.trim_end_matches('/')),
            ("model.name", model),
        ] {
            conn.execute("INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analysis_defaults_persist_across_restarts_and_reject_invalid_workflows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(&path).unwrap();
        let mut settings = db.analysis_settings().unwrap();
        assert_eq!(settings.workflow, "deep");
        assert!(settings.context_selection.include_profile);
        settings.web_search = false;
        settings.use_memory = false;
        settings.context_selection.include_profile = false;
        settings.context_selection.include_evidence = false;
        db.save_analysis_settings(settings).unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        let mut restored = db.analysis_settings().unwrap();
        assert!(!restored.web_search);
        assert!(!restored.use_memory);
        assert!(!restored.context_selection.include_profile);
        assert!(!restored.context_selection.include_evidence);
        restored.workflow = "invalid".into();
        assert!(db.save_analysis_settings(restored).is_err());
        assert_eq!(db.analysis_settings().unwrap().workflow, "deep");
    }

    #[test]
    fn saves_both_native_protocols_and_reads_legacy_as_responses() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("config.db")).unwrap();
        db.save_model_metadata("anthropic", "https://api.anthropic.com/v1", "claude-test")
            .unwrap();
        assert_eq!(db.model_config().unwrap().provider, "anthropic");
        db.save_model_metadata(
            "openai-compatible",
            "https://api.openai.com/v1",
            "openai-test",
        )
        .unwrap();
        assert_eq!(db.model_config().unwrap().provider, "openai-responses");
        assert!(db
            .save_model_metadata("other", "https://example.com", "test")
            .is_err());
        db.save_model_metadata(
            "openai-chat",
            "http://models.example.test:8080/custom/v1/",
            "third-party-model",
        )
        .unwrap();
        let config = db.model_config().unwrap();
        assert_eq!(config.provider, "openai-chat");
        assert_eq!(config.base_url, "http://models.example.test:8080/custom/v1");
        assert_eq!(config.model, "third-party-model");
        assert!(db
            .save_model_metadata(
                "openai-chat",
                "https://models.example.test?key=secret",
                "test"
            )
            .is_err());
    }
}
