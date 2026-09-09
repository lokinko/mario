use super::{params, AppResult, InvestmentRule, Transaction};

pub(super) fn average(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

pub(super) fn rule_effectiveness_signal(
    reviewed_count: usize,
    followed_average: Option<f64>,
    deviated_average: Option<f64>,
    followed_reviewed: usize,
    deviated_reviewed: usize,
) -> String {
    if reviewed_count < 3 {
        return "样本不足：继续记录过程".into();
    }
    match (followed_average, deviated_average) {
        (Some(followed), Some(deviated)) if followed >= deviated + 0.5 => {
            "遵守时过程评分更高".into()
        }
        (Some(followed), Some(deviated)) if followed + 0.5 < deviated => {
            "反常信号：需要复核规则".into()
        }
        (Some(_), Some(_)) => "暂无明显过程差异".into(),
        (Some(_), None) if followed_reviewed >= 3 => "只有遵守样本，缺少偏离对照".into(),
        (None, Some(_)) if deviated_reviewed >= 3 => "只有偏离样本，无法判断规则作用".into(),
        _ => "样本不足：继续记录过程".into(),
    }
}

pub(super) fn store_rule_revision(
    transaction: &Transaction<'_>,
    rule: &InvestmentRule,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO investment_rule_revisions (rule_id, revision, payload, changed_at)
         VALUES (?1,?2,?3,?4)",
        params![
            rule.id,
            rule.revision,
            serde_json::to_string(rule)?,
            rule.updated_at
        ],
    )?;
    Ok(())
}
