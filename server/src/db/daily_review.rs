//! AI-owned persistence for daily review settings, run logs and feed items.
use crate::{
    db::{settings::ai::AiSetting, Database},
    models::daily_review::{
        DailyReviewFeedItem, DailyReviewFeedQuery, DailyReviewItemKind, DailyReviewRun,
        DailyReviewSettings,
    },
    storage::{params, OptionalRow},
    AppError, AppResult,
};
use chrono::{NaiveDate, Utc};

impl Database {
    pub fn daily_review_settings(&self) -> AppResult<DailyReviewSettings> {
        let Some(raw) = self.ai_setting(AiSetting::DailyReviewSettings)? else {
            return Ok(DailyReviewSettings::default());
        };
        let settings: DailyReviewSettings = serde_json::from_str(&raw)?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save_daily_review_settings(
        &self,
        settings: &DailyReviewSettings,
    ) -> AppResult<DailyReviewSettings> {
        settings.validate()?;
        let normalized = DailyReviewSettings {
            enabled: settings.enabled,
            schedule_time: settings.schedule_time.trim().to_string(),
            scope: settings.scope,
            web_search: settings.web_search,
        };
        self.set_ai_setting(
            AiSetting::DailyReviewSettings,
            &serde_json::to_string(&normalized)?,
        )?;
        Ok(normalized)
    }

    pub fn daily_review_runs(&self, limit: usize) -> AppResult<Vec<DailyReviewRun>> {
        let limit = limit.clamp(1, 60) as i64;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT payload FROM daily_review_runs
             ORDER BY review_day DESC, created_at DESC, id DESC
             LIMIT $1",
        )?;
        let rows = stmt
            .query_map([limit], |r| r.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<DailyReviewRun>(&row?)?))
            .collect::<AppResult<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn has_daily_review_for_day(&self, review_day: &str) -> AppResult<bool> {
        let count: i64 = self.conn()?.query_row(
            "SELECT COUNT(*) FROM daily_review_runs WHERE review_day=$1 AND status != 'failed'",
            [review_day],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn daily_review_unread_count(&self) -> AppResult<usize> {
        let count: i64 = self.conn()?.query_row(
            "SELECT COUNT(*) FROM daily_review_items WHERE read_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(count.max(0) as usize)
    }

    pub fn daily_review_items(
        &self,
        query: &DailyReviewFeedQuery,
    ) -> AppResult<(Vec<DailyReviewFeedItem>, bool, Option<String>)> {
        let limit = query.limit.unwrap_or(40).clamp(1, 100);
        let unread_only = i64::from(query.unread_only.unwrap_or(false));
        let day = query
            .day
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(value) = day {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| AppError::Validation("复盘日期必须为 YYYY-MM-DD".into()))?;
        }
        let kind = query
            .kind
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(value) = kind {
            serde_json::from_value::<DailyReviewItemKind>(serde_json::Value::String(
                value.to_string(),
            ))
            .map_err(|_| AppError::Validation(format!("不支持的复盘动态类型：{value}")))?;
        }
        let symbol = query
            .symbol
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let before = query
            .before
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT payload, read_at FROM daily_review_items
             WHERE ($1::text IS NULL OR review_day = $1)
               AND ($2::text IS NULL OR kind = $2)
               AND ($3::text IS NULL OR symbol = $3)
               AND ($4 = 0 OR read_at IS NULL)
               AND ($5::text IS NULL OR created_at < $5)
             ORDER BY review_day DESC,
               CASE importance WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END ASC,
               created_at DESC, id ASC
             LIMIT $6",
        )?;
        let mut items = stmt
            .query_map(
                params![day, kind, symbol, unread_only, before, (limit + 1) as i64],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )?
            .map(|row| {
                let (payload, read_at) = row?;
                let mut item: DailyReviewFeedItem = serde_json::from_str(&payload)?;
                item.read_at = read_at;
                Ok(item)
            })
            .collect::<AppResult<Vec<_>>>()?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_before = if has_more {
            items.last().map(|item| item.created_at.clone())
        } else {
            None
        };
        Ok((items, has_more, next_before))
    }

    pub fn save_daily_review_run(
        &self,
        run: &DailyReviewRun,
        items: &[DailyReviewFeedItem],
    ) -> AppResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        // Replace prior auto/manual items for the same review day so re-running stays clean
        // while keeping read states of unchanged item ids if present.
        let mut prior_reads = std::collections::HashMap::<String, String>::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, read_at FROM daily_review_items WHERE review_day=$1 AND read_at IS NOT NULL",
            )?;
            for row in stmt.query_map([&run.review_day], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (id, read_at) = row?;
                prior_reads.insert(id, read_at);
            }
        }
        tx.execute(
            "DELETE FROM daily_review_items WHERE review_day=$1",
            [&run.review_day],
        )?;
        tx.execute(
            "INSERT INTO daily_review_runs (id, review_day, trigger, status, payload, created_at)
             VALUES ($1,$2,$3,$4,$5,$6)",
            params![
                run.id,
                run.review_day,
                serde_json::to_string(&run.trigger)?.trim_matches('"'),
                serde_json::to_string(&run.status)?.trim_matches('"'),
                serde_json::to_string(run)?,
                run.created_at
            ],
        )?;
        for item in items {
            let mut stored = item.clone();
            if stored.read_at.is_none() {
                stored.read_at = prior_reads.get(&stored.id).cloned();
            }
            tx.execute(
                "INSERT INTO daily_review_items
                 (id, run_id, review_day, kind, symbol, importance, payload, read_at, created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                params![
                    stored.id,
                    stored.run_id,
                    stored.review_day,
                    stored.kind.as_str(),
                    stored.symbol.as_deref().unwrap_or(""),
                    stored.importance.as_str(),
                    serde_json::to_string(&stored)?,
                    stored.read_at,
                    stored.created_at
                ],
            )?;
        }
        // Keep the latest 90 runs per account to avoid unbounded growth.
        tx.execute(
            "DELETE FROM daily_review_runs WHERE id NOT IN (
               SELECT id FROM daily_review_runs ORDER BY created_at DESC, id DESC LIMIT 90
             )",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_daily_review_read(&self, id: Option<&str>, all: bool) -> AppResult<usize> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn()?;
        if all {
            conn.execute(
                "UPDATE daily_review_items SET read_at=$1 WHERE read_at IS NULL",
                [&now],
            )?;
        } else if let Some(item_id) = id.map(str::trim).filter(|s| !s.is_empty()) {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM daily_review_items WHERE id=$1",
                    [item_id],
                    |r| r.get(0),
                )
                .optional()?;
            if existing.is_none() {
                return Err(AppError::NotFound("找不到对应的复盘动态".into()));
            }
            conn.execute(
                "UPDATE daily_review_items SET read_at=COALESCE(read_at,$1) WHERE id=$2",
                params![now, item_id],
            )?;
        } else {
            return Err(AppError::Validation(
                "请指定要标为已读的动态或选择全部已读".into(),
            ));
        }
        drop(conn);
        self.daily_review_unread_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::daily_review::{
        DailyReviewAiStatus, DailyReviewImportance, DailyReviewItemKind, DailyReviewItemOrigin,
        DailyReviewRunStatus, DailyReviewScope, DailyReviewTrigger,
    };

    #[test]
    fn persists_daily_review_settings_runs_items_and_read_states() {
        let db = Database::test();
        assert_eq!(db.daily_review_settings().unwrap().schedule_time, "07:30");
        let saved = db
            .save_daily_review_settings(&DailyReviewSettings {
                enabled: true,
                schedule_time: "08:15".into(),
                scope: DailyReviewScope::HoldingsOnly,
                web_search: false,
            })
            .unwrap();
        assert_eq!(saved.schedule_time, "08:15");
        assert_eq!(
            db.daily_review_settings().unwrap().scope,
            DailyReviewScope::HoldingsOnly
        );

        let run = DailyReviewRun {
            id: "run-1".into(),
            review_day: "2026-11-20".into(),
            trigger: DailyReviewTrigger::Scheduled,
            status: DailyReviewRunStatus::Completed,
            ai_status: DailyReviewAiStatus::Completed,
            ai_model: Some("test-model".into()),
            degraded_reason: None,
            item_count: 1,
            announcement_count: 0,
            created_at: "2026-11-21T00:00:00+00:00".into(),
        };
        let item = DailyReviewFeedItem {
            id: "item-1".into(),
            run_id: "run-1".into(),
            review_day: "2026-11-20".into(),
            kind: DailyReviewItemKind::PortfolioSummary,
            origin: DailyReviewItemOrigin::DeterministicFact,
            importance: DailyReviewImportance::High,
            title: "昨日组合总览".into(),
            summary: "组合上涨 1.2%".into(),
            body: "详细信息".into(),
            symbol: None,
            asset_name: None,
            metrics: Vec::new(),
            sources: Vec::new(),
            tags: vec!["总览".into()],
            ask_prompt: "请分析昨日变化".into(),
            read_at: None,
            created_at: "2026-11-21T00:00:00+00:00".into(),
        };
        db.save_daily_review_run(&run, &[item]).unwrap();
        assert!(db.has_daily_review_for_day("2026-11-20").unwrap());
        assert_eq!(db.daily_review_unread_count().unwrap(), 1);

        let unread = db.mark_daily_review_read(Some("item-1"), false).unwrap();
        assert_eq!(unread, 0);
        let (items, _, _) = db
            .daily_review_items(&DailyReviewFeedQuery::default())
            .unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].read_at.is_some());

        assert!(db
            .daily_review_items(&DailyReviewFeedQuery {
                day: Some("2026-02-30".into()),
                ..Default::default()
            })
            .is_err());
        assert!(db
            .daily_review_items(&DailyReviewFeedQuery {
                kind: Some("forgedKind".into()),
                ..Default::default()
            })
            .is_err());
    }
}
