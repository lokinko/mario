use super::{params, AppResult, InvestmentRule, Transaction};

pub(super) fn store_rule_revision(
    transaction: &Transaction<'_>,
    rule: &InvestmentRule,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO investment_rule_revisions (rule_id, revision, payload, changed_at)
         VALUES ($1,$2,$3,$4)",
        params![
            rule.id,
            rule.revision,
            serde_json::to_string(rule)?,
            rule.updated_at
        ],
    )?;
    Ok(())
}
