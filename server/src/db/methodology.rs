//! User-authored methodology, stored independently from model configuration.
use super::{AppResult, Database};
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
}
