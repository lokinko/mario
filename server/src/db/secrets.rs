//! Account-bound encrypted credential persistence.
use super::Database;
use crate::{
    secrets::{decrypt, encrypt},
    AppError, AppResult,
};

impl Database {
    fn secret_scope(&self, name: &str) -> AppResult<String> {
        Ok(format!(
            "{}:{name}",
            self.conn()?
                .query_row("SELECT current_schema()::text", [], |r| r
                    .get::<_, String>(0))?
        ))
    }
    pub fn secret(&self, name: &str) -> AppResult<String> {
        let encrypted = self
            .setting(&format!("secret.{name}"))?
            .ok_or_else(|| AppError::Validation("请先配置 API Key".into()))?;
        decrypt(&encrypted, &self.secret_scope(name)?)
    }
    pub fn save_secret(&self, name: &str, value: &str) -> AppResult<()> {
        self.set_setting(
            &format!("secret.{name}"),
            &self.encrypted_secret(name, value)?,
        )
    }
    pub(super) fn encrypted_secret(&self, name: &str, value: &str) -> AppResult<String> {
        if value.trim().is_empty() {
            return Err(AppError::Validation("API Key 不能为空".into()));
        }
        encrypt(value.trim(), &self.secret_scope(name)?)
    }
    pub fn remove_secret(&self, name: &str) -> AppResult<()> {
        self.delete_setting(&format!("secret.{name}"))
    }
}
