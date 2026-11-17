//! Account-bound encrypted credential persistence.
use crate::{
    secrets::{decrypt, encrypt},
    AppError, AppResult,
};
use {super::settings::ai::AiSetting, crate::db::Database};

impl Database {
    fn secret_scope(&self) -> AppResult<String> {
        Ok(format!(
            "{}:model",
            self.conn()?
                .query_row("SELECT current_schema()::text", [], |r| r
                    .get::<_, String>(0))?
        ))
    }
    pub(super) fn model_secret(&self) -> AppResult<String> {
        let encrypted = self
            .ai_setting(AiSetting::ModelCredential)?
            .ok_or_else(|| AppError::Validation("请先配置 API Key".into()))?;
        decrypt(&encrypted, &self.secret_scope()?)
    }
    pub(super) fn encrypt_model_secret(&self, value: &str) -> AppResult<String> {
        if value.trim().is_empty() {
            return Err(AppError::Validation("API Key 不能为空".into()));
        }
        encrypt(value.trim(), &self.secret_scope()?)
    }
    pub(super) fn remove_model_secret(&self) -> AppResult<()> {
        self.delete_ai_setting(AiSetting::ModelCredential)
    }
}
