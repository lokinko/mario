use super::{
    params, validate_and_canonicalize_rule_checks, validate_non_negative, AppError, AppResult,
    Database, DecisionEntry, DecisionRecord, DecisionReview, DecisionReviewInput, OptionalRow, Utc,
    Uuid,
};

impl Database {
    pub fn save_decision(&self, entry: &DecisionEntry) -> AppResult<()> {
        if entry.asset_name.trim().is_empty()
            || entry.thesis.trim().is_empty()
            || entry.counter_thesis.trim().is_empty()
            || entry.invalidation.trim().is_empty()
            || entry.review_date.trim().is_empty()
        {
            return Err(AppError::Validation(
                "投资对象、正反逻辑、证伪条件和复盘日期为必填项".into(),
            ));
        }
        validate_non_negative(&[
            entry.expected_return_pct,
            entry.downside_pct,
            entry.confidence_pct,
            entry.position_pct,
        ])?;
        if entry.confidence_pct > 100.0 || entry.position_pct > 100.0 {
            return Err(AppError::Validation("置信度和仓位不能超过 100%".into()));
        }
        chrono::NaiveDate::parse_from_str(&entry.review_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("复盘日期格式无效".into()))?;
        let active_rules = self
            .investment_rules()?
            .into_iter()
            .filter(|rule| rule.active)
            .collect::<Vec<_>>();
        let canonical_rule_checks =
            validate_and_canonicalize_rule_checks(&entry.rule_checks, &active_rules)?;
        if entry.source_action_index.is_some() && entry.source_analysis_id.is_none() {
            return Err(AppError::Validation(
                "AI 行动序号必须关联一条分析记录".into(),
            ));
        }
        let conn = self.conn()?;
        if let Some(source_id) = entry.source_analysis_id.as_deref() {
            if source_id.trim().is_empty() || source_id.len() > 128 {
                return Err(AppError::Validation("AI 分析来源 ID 无效".into()));
            }
            let source_trace = conn
                .query_row(
                    "SELECT trace FROM analyses WHERE id=?1",
                    [source_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    AppError::Validation("找不到关联的 AI 分析；请保留原分析后再冻结决策".into())
                })?;
            if let Some(action_index) = entry.source_action_index {
                let trace = source_trace
                    .as_deref()
                    .and_then(|value| {
                        serde_json::from_str::<crate::models::AnalysisWorkflowTrace>(value).ok()
                    })
                    .ok_or_else(|| AppError::Validation("关联的 AI 分析缺少可验证工作流".into()))?;
                let actions = trace
                    .structured_report
                    .map(|report| report.actions)
                    .unwrap_or_default();
                if action_index >= actions.len() {
                    return Err(AppError::Validation("AI 行动序号超出原分析范围".into()));
                }
            }
        }
        let id = entry
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut stored = entry.clone();
        stored.id = Some(id.clone());
        stored.rule_checks = canonical_rule_checks;
        conn.execute(
            "INSERT INTO decisions (id, asset_name, payload, review_date, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, entry.asset_name.trim(), serde_json::to_string(&stored)?, entry.review_date, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn decisions(&self) -> AppResult<Vec<DecisionRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT d.id, d.payload, d.created_at,
                    r.outcome_summary, r.actual_return_pct, r.thesis_status,
                    r.process_rating, r.lessons, r.reviewed_at
             FROM decisions d
             LEFT JOIN decision_reviews r ON r.decision_id = d.id
             ORDER BY d.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let review = row
                .get::<_, Option<String>>(3)?
                .map(|outcome_summary| DecisionReview {
                    outcome_summary,
                    actual_return_pct: row.get(4).ok().flatten(),
                    thesis_status: row.get::<_, String>(5).unwrap_or_default(),
                    process_rating: row.get::<_, i64>(6).unwrap_or_default(),
                    lessons: row.get::<_, String>(7).unwrap_or_default(),
                    reviewed_at: row.get::<_, String>(8).unwrap_or_default(),
                });
            Ok((id, payload, created_at, review))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (id, payload, created_at, review) = row?;
            let entry: DecisionEntry = serde_json::from_str(&payload)?;
            records.push(DecisionRecord {
                id,
                source_analysis_id: entry.source_analysis_id,
                source_action_index: entry.source_action_index,
                asset_name: entry.asset_name,
                thesis: entry.thesis,
                counter_thesis: entry.counter_thesis,
                expected_return_pct: entry.expected_return_pct,
                downside_pct: entry.downside_pct,
                confidence_pct: entry.confidence_pct,
                position_pct: entry.position_pct,
                invalidation: entry.invalidation,
                review_date: entry.review_date,
                rule_checks: entry.rule_checks,
                created_at,
                review,
            });
        }
        Ok(records)
    }

    pub fn save_decision_review(&self, id: &str, input: &DecisionReviewInput) -> AppResult<()> {
        if input.outcome_summary.trim().is_empty() || input.lessons.trim().is_empty() {
            return Err(AppError::Validation("结果摘要和经验修正为必填项".into()));
        }
        if !(1..=5).contains(&input.process_rating) {
            return Err(AppError::Validation("过程评分必须在 1—5 之间".into()));
        }
        if !matches!(
            input.thesis_status.as_str(),
            "成立" | "部分成立" | "失效" | "尚不明确"
        ) {
            return Err(AppError::Validation("未知的原始逻辑结果".into()));
        }
        if let Some(value) = input.actual_return_pct {
            if !value.is_finite() {
                return Err(AppError::Validation("实际收益率必须是有效数字".into()));
            }
        }
        let conn = self.conn()?;
        let exists: i64 =
            conn.query_row("SELECT COUNT(*) FROM decisions WHERE id=?1", [id], |row| {
                row.get(0)
            })?;
        if exists == 0 {
            return Err(AppError::Validation("找不到要复盘的决策".into()));
        }
        conn.execute(
            "INSERT INTO decision_reviews
             (decision_id, outcome_summary, actual_return_pct, thesis_status, process_rating, lessons, reviewed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(decision_id) DO UPDATE SET
               outcome_summary=excluded.outcome_summary,
               actual_return_pct=excluded.actual_return_pct,
               thesis_status=excluded.thesis_status,
               process_rating=excluded.process_rating,
               lessons=excluded.lessons,
               reviewed_at=excluded.reviewed_at",
            params![id, input.outcome_summary.trim(), input.actual_return_pct, input.thesis_status, input.process_rating, input.lessons.trim(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}
