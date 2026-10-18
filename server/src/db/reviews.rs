use super::{
    params, store_rule_revision, validate_investment_rule, validate_system_review, AppError,
    AppResult, Database, InvestmentRule, InvestmentRuleInput, InvestmentRuleRevision, OptionalRow,
    SystemReviewInput, SystemReviewRecord, SystemReviewSnapshot, Utc, Uuid,
};

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredSystemReview {
    content: Option<String>,
    period_label: String,
    adherence_score: Option<i64>,
    process_summary: String,
    rule_violations: String,
    lessons: String,
    next_actions: String,
    next_review_date: String,
}

impl Database {
    pub fn investment_rules(&self) -> AppResult<Vec<InvestmentRule>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, category, statement, trigger, rationale, active, source_review_id,
                    revision, created_at, updated_at
             FROM investment_rules
             ORDER BY active DESC, updated_at DESC, id ASC",
        )?;
        let rules = statement
            .query_map([], |row| {
                Ok(InvestmentRule {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    statement: row.get(2)?,
                    trigger: row.get(3)?,
                    rationale: row.get(4)?,
                    active: row.get::<_, i64>(5)? != 0,
                    source_review_id: row.get(6)?,
                    revision: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rules)
    }

    pub fn add_investment_rule(&self, input: &InvestmentRuleInput) -> AppResult<InvestmentRule> {
        validate_investment_rule(input)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let rule = InvestmentRule {
            id: id.clone(),
            category: input.category.trim().into(),
            statement: input.statement.trim().into(),
            trigger: input.trigger.trim().into(),
            rationale: input.rationale.trim().into(),
            active: input.active,
            source_review_id: input.source_review_id.clone(),
            revision: 1,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO investment_rules
             (id, category, statement, trigger, rationale, active, source_review_id, revision, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                rule.id,
                rule.category,
                rule.statement,
                rule.trigger,
                rule.rationale,
                i64::from(rule.active),
                rule.source_review_id,
                rule.revision,
                rule.created_at,
                rule.updated_at
            ],
        )?;
        store_rule_revision(&transaction, &rule)?;
        transaction.commit()?;
        Ok(rule)
    }

    pub fn update_investment_rule(
        &self,
        id: &str,
        input: &InvestmentRuleInput,
    ) -> AppResult<InvestmentRule> {
        validate_investment_rule(input)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT created_at, revision FROM investment_rules WHERE id=?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("找不到要修订的投资规则".into()))?;
        let now = Utc::now().to_rfc3339();
        let rule = InvestmentRule {
            id: id.into(),
            category: input.category.trim().into(),
            statement: input.statement.trim().into(),
            trigger: input.trigger.trim().into(),
            rationale: input.rationale.trim().into(),
            active: input.active,
            source_review_id: input.source_review_id.clone(),
            revision: existing.1 + 1,
            created_at: existing.0,
            updated_at: now,
        };
        transaction.execute(
            "UPDATE investment_rules SET category=?2, statement=?3, trigger=?4, rationale=?5,
                    active=?6, source_review_id=?7, revision=?8, updated_at=?9 WHERE id=?1",
            params![
                rule.id,
                rule.category,
                rule.statement,
                rule.trigger,
                rule.rationale,
                i64::from(rule.active),
                rule.source_review_id,
                rule.revision,
                rule.updated_at
            ],
        )?;
        store_rule_revision(&transaction, &rule)?;
        transaction.commit()?;
        Ok(rule)
    }

    pub fn investment_rule_history(&self, id: &str) -> AppResult<Vec<InvestmentRuleRevision>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT payload, changed_at FROM investment_rule_revisions
             WHERE rule_id=?1 ORDER BY revision DESC",
        )?;
        let rows = statement.query_map([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut revisions = Vec::new();
        for row in rows {
            let (payload, changed_at) = row?;
            let rule: InvestmentRule = serde_json::from_str(&payload)?;
            revisions.push(InvestmentRuleRevision {
                rule_id: rule.id,
                revision: rule.revision,
                category: rule.category,
                statement: rule.statement,
                trigger: rule.trigger,
                rationale: rule.rationale,
                active: rule.active,
                source_review_id: rule.source_review_id,
                changed_at,
            });
        }
        Ok(revisions)
    }

    pub fn save_system_review(&self, input: &SystemReviewInput) -> AppResult<SystemReviewRecord> {
        validate_system_review(input)?;
        let portfolio = self.snapshot()?;
        let rules = self.investment_rules()?;
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let snapshot = SystemReviewSnapshot {
            portfolio_value: portfolio.total_value,
            base_currency: portfolio.profile.base_currency.clone(),
            portfolio_comparable: portfolio.valuation_status.comparable,
            emergency_months: portfolio.emergency_months,
            concentration_pct: portfolio.concentration_pct,
            risk_status: portfolio.plan.risk_status,
            high_risk_findings: portfolio
                .findings
                .iter()
                .filter(|finding| finding.level == "high")
                .count(),
            active_rules: rules.iter().filter(|rule| rule.active).count(),
        };
        let record = SystemReviewRecord {
            id,
            content: Some(input.content.clone()),
            period_label: String::new(),
            adherence_score: None,
            process_summary: String::new(),
            rule_violations: String::new(),
            lessons: String::new(),
            next_actions: String::new(),
            next_review_date: String::new(),
            snapshot,
            created_at,
        };
        self.conn()?.execute(
            "INSERT INTO system_reviews (id, payload, snapshot, next_review_date, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                record.id,
                serde_json::to_string(input)?,
                serde_json::to_string(&record.snapshot)?,
                record.next_review_date,
                record.created_at
            ],
        )?;
        Ok(record)
    }

    pub fn system_reviews(&self) -> AppResult<Vec<SystemReviewRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id, payload, snapshot, created_at FROM system_reviews
             ORDER BY created_at DESC, id ASC LIMIT 50",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut reviews = Vec::new();
        for row in rows {
            let (id, payload, snapshot, created_at) = row?;
            let input: StoredSystemReview = serde_json::from_str(&payload)?;
            reviews.push(SystemReviewRecord {
                id,
                content: input.content,
                period_label: input.period_label,
                adherence_score: input.adherence_score,
                process_summary: input.process_summary,
                rule_violations: input.rule_violations,
                lessons: input.lessons,
                next_actions: input.next_actions,
                next_review_date: input.next_review_date,
                snapshot: serde_json::from_str(&snapshot)?,
                created_at,
            });
        }
        Ok(reviews)
    }
}
