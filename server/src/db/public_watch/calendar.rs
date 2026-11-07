//! Public forecast cache only. Account scopes and implemented-dividend storage stay separate.
use super::PublicWatch;
use crate::{
    models::watch::calendar::PlanSnapshot,
    storage::{params, OptionalRow},
    AppError, AppResult,
};
impl PublicWatch {
    pub fn calendar_snapshot(&self, symbol: &str) -> AppResult<PlanSnapshot> {
        let payload: Option<String> = self
            .conn()?
            .query_row(
                "SELECT payload FROM dividend_plan_snapshots WHERE symbol=$1",
                [symbol],
                |r| r.get(0),
            )
            .optional()?;
        Ok(payload
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default())
    }
    pub fn save_calendar_snapshot(&self, symbol: &str, snapshot: &PlanSnapshot) -> AppResult<()> {
        if snapshot
            .attempted_at
            .as_ref()
            .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot
                .fetched_at
                .as_ref()
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            || snapshot.plans.iter().any(|p| p.symbol != symbol)
        {
            return Err(AppError::Validation("分红计划来源或采集时间无效".into()));
        }
        self.conn()?.execute("INSERT INTO dividend_plan_snapshots(symbol,payload) VALUES ($1,$2)
            ON CONFLICT(symbol) DO UPDATE SET payload=(CASE
              WHEN dividend_plan_snapshots.payload::jsonb->>'fetchedAt' IS NOT NULL
                AND (excluded.payload::jsonb->>'fetchedAt' IS NULL OR
                  (dividend_plan_snapshots.payload::jsonb->>'fetchedAt')::timestamptz > (excluded.payload::jsonb->>'fetchedAt')::timestamptz)
              THEN jsonb_set(jsonb_set(excluded.payload::jsonb,'{plans}',dividend_plan_snapshots.payload::jsonb->'plans'),'{fetchedAt}',dividend_plan_snapshots.payload::jsonb->'fetchedAt')
              ELSE excluded.payload::jsonb END)::text
            WHERE (dividend_plan_snapshots.payload::jsonb->>'attemptedAt')::timestamptz <= (excluded.payload::jsonb->>'attemptedAt')::timestamptz",
            params![symbol, serde_json::to_string(snapshot)?])?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_forecasts_survive_reopen_and_late_failures_cannot_restore_old_plans() {
        let store = PublicWatch::test();
        let good = PlanSnapshot {
            attempted_at: Some("2026-11-07T10:00:00Z".into()),
            fetched_at: Some("2026-11-07T10:00:01Z".into()),
            plans: vec![crate::models::watch::calendar::tests::plan(
                "600000.SS",
                crate::models::watch::calendar::PlanStage::Scheduled,
            )],
            refresh_failed: false,
        };
        store.save_calendar_snapshot("600000.SS", &good).unwrap();
        let failed = PlanSnapshot {
            attempted_at: Some("2026-11-07T10:00:02Z".into()),
            refresh_failed: true,
            ..Default::default()
        };
        store.save_calendar_snapshot("600000.SS", &failed).unwrap();
        let reopened = PublicWatch::open(&store.url, &store.schema).unwrap();
        let retained = reopened.calendar_snapshot("600000.SS").unwrap();
        assert!(retained.refresh_failed);
        assert_eq!(retained.plans.len(), 1);
        assert_eq!(retained.fetched_at, good.fetched_at);
        store.save_calendar_snapshot("600000.SS", &good).unwrap();
        assert!(store.calendar_snapshot("600000.SS").unwrap().refresh_failed);
        assert!(store
            .calendar_snapshot("600001.SS")
            .unwrap()
            .fetched_at
            .is_none());
    }
}
