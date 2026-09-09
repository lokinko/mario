use super::{
    average, params, rule_effectiveness_signal, store_rule_revision, validate_investment_rule,
    validate_system_review, AppError, AppResult, Database, HashSet, InvestmentRule,
    InvestmentRuleInput, InvestmentRuleRevision, OptionalRow, RuleEffectivenessItem,
    RuleEffectivenessSummary, SystemReviewInput, SystemReviewRecord, SystemReviewSnapshot, Utc,
    Uuid,
};

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

    pub fn rule_effectiveness(&self) -> AppResult<RuleEffectivenessSummary> {
        let decisions = self.decisions()?;
        let rules = self.investment_rules()?;
        let mut items = Vec::with_capacity(rules.len());

        for rule in rules {
            let checks = decisions
                .iter()
                .filter_map(|decision| {
                    decision
                        .rule_checks
                        .iter()
                        .find(|check| check.rule_id == rule.id)
                        .map(|check| (check, decision.review.as_ref()))
                })
                .collect::<Vec<_>>();
            let applicable = checks
                .iter()
                .filter(|(check, _)| check.status != "不适用")
                .collect::<Vec<_>>();
            let followed = applicable
                .iter()
                .filter(|(check, _)| check.status == "遵守")
                .collect::<Vec<_>>();
            let deviated = applicable
                .iter()
                .filter(|(check, _)| check.status == "偏离")
                .collect::<Vec<_>>();
            let followed_process = followed
                .iter()
                .filter_map(|(_, review)| review.map(|value| value.process_rating as f64))
                .collect::<Vec<_>>();
            let deviated_process = deviated
                .iter()
                .filter_map(|(_, review)| review.map(|value| value.process_rating as f64))
                .collect::<Vec<_>>();
            let mut observed_revisions = checks
                .iter()
                .map(|(check, _)| check.rule_revision)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            observed_revisions.sort_unstable();
            let reviewed_count = applicable
                .iter()
                .filter(|(_, review)| review.is_some())
                .count();
            let followed_process_average = average(&followed_process);
            let deviated_process_average = average(&deviated_process);

            items.push(RuleEffectivenessItem {
                rule_id: rule.id,
                current_revision: rule.revision,
                observed_revisions,
                category: rule.category,
                statement: rule.statement,
                active: rule.active,
                decision_count: checks.len(),
                applicable_count: applicable.len(),
                followed_count: followed.len(),
                deviated_count: deviated.len(),
                reviewed_count,
                followed_process_average,
                deviated_process_average,
                signal: rule_effectiveness_signal(
                    reviewed_count,
                    followed_process_average,
                    deviated_process_average,
                    followed_process.len(),
                    deviated_process.len(),
                ),
            });
        }

        let evaluated_decisions = decisions
            .iter()
            .filter(|decision| !decision.rule_checks.is_empty())
            .count();
        let applicable_checks = items.iter().map(|item| item.applicable_count).sum();
        let followed_checks = items.iter().map(|item| item.followed_count).sum();
        let reviewed_checks = items.iter().map(|item| item.reviewed_count).sum();
        let adherence_pct = (applicable_checks > 0)
            .then(|| followed_checks as f64 / applicable_checks as f64 * 100.0);

        Ok(RuleEffectivenessSummary {
            total_decisions: decisions.len(),
            evaluated_decisions,
            applicable_checks,
            followed_checks,
            adherence_pct,
            reviewed_checks,
            rules: items,
        })
    }

    pub fn save_system_review(&self, input: &SystemReviewInput) -> AppResult<SystemReviewRecord> {
        validate_system_review(input)?;
        let portfolio = self.snapshot()?;
        let decisions = self.decisions()?;
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
            goal_total: portfolio.goals.len(),
            goals_on_track: portfolio
                .plan
                .goal_projections
                .iter()
                .filter(|goal| matches!(goal.status.as_str(), "on-track" | "reached"))
                .count(),
            decision_total: decisions.len(),
            reviewed_decisions: decisions
                .iter()
                .filter(|decision| decision.review.is_some())
                .count(),
            active_rules: rules.iter().filter(|rule| rule.active).count(),
        };
        let record = SystemReviewRecord {
            id,
            period_label: input.period_label.trim().into(),
            adherence_score: input.adherence_score,
            process_summary: input.process_summary.trim().into(),
            rule_violations: input.rule_violations.trim().into(),
            lessons: input.lessons.trim().into(),
            next_actions: input.next_actions.trim().into(),
            next_review_date: input.next_review_date.clone(),
            snapshot,
            created_at,
        };
        let normalized_input = SystemReviewInput {
            period_label: record.period_label.clone(),
            adherence_score: record.adherence_score,
            process_summary: record.process_summary.clone(),
            rule_violations: record.rule_violations.clone(),
            lessons: record.lessons.clone(),
            next_actions: record.next_actions.clone(),
            next_review_date: record.next_review_date.clone(),
        };
        self.conn()?.execute(
            "INSERT INTO system_reviews (id, payload, snapshot, next_review_date, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                record.id,
                serde_json::to_string(&normalized_input)?,
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
            let input: SystemReviewInput = serde_json::from_str(&payload)?;
            reviews.push(SystemReviewRecord {
                id,
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
