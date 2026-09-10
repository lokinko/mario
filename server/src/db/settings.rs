use super::{params, AppError, AppResult, Database, ModelConfig, OptionalRow};

impl Database {
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
            has_api_key: crate::secrets::has_api_key(),
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
            "openai-responses" | "anthropic" | "openai-compatible"
        ) {
            return Err(AppError::Validation(
                "请选择 OpenAI Responses 或 Anthropic Messages".into(),
            ));
        }
        let url = reqwest::Url::parse(base_url)
            .map_err(|_| AppError::Validation("模型地址无效".into()))?;
        if !(url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || model.trim().is_empty()
        {
            return Err(AppError::Validation("请填写模型名称和 HTTPS API 基础地址；仅本机允许 HTTP，地址不能包含凭据、查询或片段".into()));
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
        assert!(db
            .save_model_metadata(
                "openai-responses",
                "http://localhost.evil.example/v1",
                "test"
            )
            .is_err());
    }
}
