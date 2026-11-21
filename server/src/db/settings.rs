//! Typed settings access. Raw string access is private to this storage primitive.
pub(super) mod ai;
pub(super) mod user;
use crate::{
    db::Database,
    storage::{params, OptionalRow},
    AppResult,
};
use ai::AiSetting;
use user::UserSetting;

fn read(db: &Database, key: &str) -> AppResult<Option<String>> {
    Ok(db
        .conn()?
        .query_row("SELECT value FROM settings WHERE key=$1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}
fn write(db: &Database, key: &str, value: &str) -> AppResult<()> {
    db.conn()?.execute("INSERT INTO settings (key,value) VALUES ($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
    Ok(())
}
fn remove(db: &Database, key: &str) -> AppResult<()> {
    db.conn()?
        .execute("DELETE FROM settings WHERE key=$1", [key])?;
    Ok(())
}

/// Shared backup coordination includes user facts and memory progress, excluding model setup.
pub(super) fn backup_predicate() -> String {
    format!(
        "key LIKE '{}%' OR key LIKE '{}%' OR key='{}' OR key='{}' OR key='{}' OR key='{}'",
        UserSetting::FX_BOOK_PREFIX,
        UserSetting::FX_QUOTE_PREFIX,
        UserSetting::Methodology.key(),
        UserSetting::WatchRules.key(),
        AiSetting::DreamState.key(),
        AiSetting::DailyReviewSettings.key()
    )
}
pub(super) fn is_backup_setting(key: &str) -> bool {
    key.starts_with(UserSetting::FX_BOOK_PREFIX)
        || key.starts_with(UserSetting::FX_QUOTE_PREFIX)
        || key == UserSetting::Methodology.key()
        || key == UserSetting::WatchRules.key()
        || key == AiSetting::DreamState.key()
        || key == AiSetting::DailyReviewSettings.key()
}

impl Database {
    pub(super) fn user_setting(&self, key: UserSetting<'_>) -> AppResult<Option<String>> {
        read(self, &key.key())
    }
    pub(super) fn set_user_setting(&self, key: UserSetting<'_>, value: &str) -> AppResult<()> {
        write(self, &key.key(), value)
    }
    pub(super) fn ai_setting(&self, key: AiSetting) -> AppResult<Option<String>> {
        read(self, key.key())
    }
    pub(super) fn set_ai_setting(&self, key: AiSetting, value: &str) -> AppResult<()> {
        write(self, key.key(), value)
    }
    pub(super) fn delete_ai_setting(&self, key: AiSetting) -> AppResult<()> {
        remove(self, key.key())
    }

    // Fixtures seed old layouts and malformed values to verify migrations and rejection.
    #[cfg(test)]
    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        read(self, key)
    }

    #[cfg(test)]
    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        write(self, key, value)
    }

    #[cfg(test)]
    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        remove(self, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        analysis::AnalysisSettings, fx::FxOverrideInput, methodology::InvestmentMethodologyInput,
        provider::ModelProviderKind,
    };

    #[test]
    fn user_settings_restore_without_overwriting_ai_choices_or_account_bound_credentials() {
        crate::secrets::use_test_master_key();
        let account = crate::db::TestAccount::new();
        let db = account.open();
        db.save_model_configuration(
            ModelProviderKind::OpenaiResponses,
            "https://source.example.test/v1",
            "source-model",
            Some("source-key"),
        )
        .unwrap();
        db.save_analysis_settings(AnalysisSettings {
            web_search: false,
            ..Default::default()
        })
        .unwrap();
        db.save_investment_methodology(InvestmentMethodologyInput {
            prompt: "保留我的用户方法论".into(),
        })
        .unwrap();
        db.set_fx_override(&FxOverrideInput {
            currency: "USD".into(),
            exact_rate: Some("7.25000001".into()),
            rate: Some(7.25000001),
        })
        .unwrap();
        let backup = db.export_sync_data().unwrap();
        drop(db);

        let reopened = account.open();
        assert_eq!(reopened.model_secret().unwrap(), "source-key");
        assert!(!reopened.analysis_settings().unwrap().web_search);
        assert_eq!(
            reopened.investment_methodology().unwrap().prompt,
            "保留我的用户方法论"
        );
        let recipient = Database::test();
        recipient
            .save_model_configuration(
                ModelProviderKind::OpenaiResponses,
                "https://recipient.example.test/v1",
                "recipient-model",
                Some("recipient-key"),
            )
            .unwrap();
        recipient
            .save_analysis_settings(AnalysisSettings::default())
            .unwrap();
        recipient.import_sync_data(&backup).unwrap();
        assert_eq!(
            recipient.investment_methodology().unwrap().prompt,
            "保留我的用户方法论"
        );
        assert_eq!(
            recipient.fx_book("CNY").unwrap().manual["USD"]
                .exact_rate
                .as_deref(),
            Some("7.25000001")
        );
        assert_eq!(recipient.model_config().unwrap().model, "recipient-model");
        assert_eq!(recipient.model_secret().unwrap(), "recipient-key");
        assert!(recipient.analysis_settings().unwrap().web_search);

        let before = reopened.export_sync_data().unwrap().content_hash().unwrap();
        reopened.remove_model_secret().unwrap();
        assert!(reopened.model_secret().is_err());
        assert_eq!(
            reopened.export_sync_data().unwrap().content_hash().unwrap(),
            before
        );
        assert!(!reopened.analysis_settings().unwrap().web_search);
    }
}
