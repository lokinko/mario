//! Disclosed dividend plans are separate from implemented TTM events and account assets.
use super::WatchInput;
use crate::{AppError, AppResult};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStage {
    Preannounced,
    Proposed,
    Approved,
    Scheduled,
    Cancelled,
    NoDistribution,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanIssue {
    InvalidDates,
    InvalidAmount,
    UnknownProgress,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendPlan {
    pub id: String,
    pub symbol: String,
    pub report_date: NaiveDate,
    pub announced_on: NaiveDate,
    pub stage: PlanStage,
    pub source_progress: String,
    pub profile: String,
    pub cash_per_ten: Option<String>,
    pub bonus_per_ten: Option<String>,
    pub transfer_per_ten: Option<String>,
    pub record_date: Option<NaiveDate>,
    pub ex_date: Option<NaiveDate>,
    pub payment_date: Option<NaiveDate>,
    pub issues: Vec<PlanIssue>,
    pub sources: Vec<String>,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanSnapshot {
    pub plans: Vec<DividendPlan>,
    pub attempted_at: Option<String>,
    pub fetched_at: Option<String>,
    pub refresh_failed: bool,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalendarScope {
    #[default]
    All,
    Favorites,
    Public,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarCompany {
    pub symbol: String,
    pub name: String,
    pub personal: bool,
}
pub fn companies(
    catalog: &super::catalog::Catalog,
    personal: &[WatchInput],
    scope: CalendarScope,
) -> Vec<CalendarCompany> {
    let mut result = BTreeMap::new();
    if scope != CalendarScope::Favorites {
        for company in catalog.industries.iter().flat_map(|i| &i.companies) {
            result.insert(
                company.symbol.clone(),
                CalendarCompany {
                    symbol: company.symbol.clone(),
                    name: company.name.clone(),
                    personal: false,
                },
            );
        }
    }
    for entry in personal {
        if let Some(company) = result.get_mut(&entry.symbol) {
            company.personal = true;
        } else if scope != CalendarScope::Public {
            result.insert(
                entry.symbol.clone(),
                CalendarCompany {
                    symbol: entry.symbol.clone(),
                    name: entry.name.clone(),
                    personal: true,
                },
            );
        }
    }
    result.into_values().collect()
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEntry {
    #[serde(flatten)]
    pub company: CalendarCompany,
    pub plan: DividendPlan,
    pub fetched_at: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarCoverage {
    #[serde(flatten)]
    pub company: CalendarCompany,
    pub attempted_at: Option<String>,
    pub fetched_at: Option<String>,
    pub refresh_failed: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarBoard {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub as_of: NaiveDate,
    pub scope: CalendarScope,
    pub entries: Vec<CalendarEntry>,
    pub pending: Vec<CalendarEntry>,
    pub coverage: Vec<CalendarCoverage>,
}
impl CalendarBoard {
    pub fn build(
        start: NaiveDate,
        today: NaiveDate,
        scope: CalendarScope,
        snapshots: Vec<(CalendarCompany, PlanSnapshot)>,
    ) -> AppResult<Self> {
        let end = start
            .checked_add_days(Days::new(29))
            .ok_or_else(|| AppError::Validation("日历日期超出范围".into()))?;
        let pending_since = today.checked_sub_days(Days::new(365)).unwrap_or(today);
        let mut result = Self {
            start,
            end,
            as_of: today,
            scope,
            entries: vec![],
            pending: vec![],
            coverage: vec![],
        };
        for (company, snapshot) in snapshots {
            result.coverage.push(CalendarCoverage {
                company: company.clone(),
                attempted_at: snapshot.attempted_at,
                fetched_at: snapshot.fetched_at.clone(),
                refresh_failed: snapshot.refresh_failed,
            });
            for plan in snapshot.plans {
                if plan.announced_on > today
                    || matches!(plan.stage, PlanStage::Cancelled | PlanStage::NoDistribution)
                {
                    continue;
                }
                let scheduled = plan.stage == PlanStage::Scheduled;
                let dates = [plan.record_date, plan.ex_date, plan.payment_date];
                let in_range = dates.iter().flatten().any(|d| *d >= start && *d <= end);
                let entry = CalendarEntry {
                    company: company.clone(),
                    plan,
                    fetched_at: snapshot.fetched_at.clone(),
                };
                if scheduled && in_range {
                    result.entries.push(entry);
                } else if entry.plan.announced_on >= pending_since
                    && (!scheduled || dates.iter().all(Option::is_none))
                {
                    result.pending.push(entry);
                }
            }
        }
        result.entries.sort_by_key(|row| {
            (
                [
                    row.plan.record_date,
                    row.plan.ex_date,
                    row.plan.payment_date,
                ]
                .into_iter()
                .flatten()
                .filter(|d| *d >= start && *d <= end)
                .min(),
                row.company.symbol.clone(),
                row.plan.id.clone(),
            )
        });
        result.pending.sort_by_key(|row| {
            (
                std::cmp::Reverse(row.plan.announced_on),
                row.company.symbol.clone(),
                row.plan.id.clone(),
            )
        });
        Ok(result)
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn plan(symbol: &str, stage: PlanStage) -> DividendPlan {
        DividendPlan {
            id: format!("{symbol}:2026-06-30"),
            symbol: symbol.into(),
            report_date: "2026-06-30".parse().unwrap(),
            announced_on: "2026-11-06".parse().unwrap(),
            stage,
            source_progress: "实施分配".into(),
            profile: "10派1元（含税）".into(),
            cash_per_ten: Some("1".into()),
            bonus_per_ten: None,
            transfer_per_ten: None,
            record_date: Some("2026-11-10".parse().unwrap()),
            ex_date: Some("2026-11-11".parse().unwrap()),
            payment_date: None,
            issues: vec![],
            sources: vec!["https://example.test/dividend-plan".into()],
        }
    }
    #[test]
    fn calendar_scopes_only_combine_public_companies_and_the_current_accounts_entries() {
        let catalog = super::super::catalog::Catalog::bundled().unwrap();
        let personal = vec![
            WatchInput {
                symbol: "600000.SS".into(),
                name: "仅此账号".into(),
                thesis: "私有理由".into(),
            },
            WatchInput {
                symbol: "600036.SS".into(),
                name: "私有别名".into(),
                thesis: "私有理由".into(),
            },
        ];
        let public = companies(&catalog, &personal, CalendarScope::Public);
        let public_count = catalog
            .industries
            .iter()
            .map(|i| i.companies.len())
            .sum::<usize>();
        assert_eq!(public.len(), public_count);
        assert!(!public.iter().any(|c| c.symbol == "600000.SS"));
        let merged = companies(&catalog, &personal, CalendarScope::All);
        assert_eq!(merged.len(), public_count + 1);
        let shared = merged.iter().find(|c| c.symbol == "600036.SS").unwrap();
        assert!(shared.personal);
        assert_eq!(shared.name, "招商银行");
        assert!(companies(&catalog, &[], CalendarScope::Favorites).is_empty());
        assert_eq!(
            companies(&catalog, &personal, CalendarScope::Favorites).len(),
            2
        );
    }

    #[test]
    fn undated_proposals_stay_pending_and_past_cancelled_or_future_notices_do_not_become_events() {
        let today = "2026-11-07".parse().unwrap();
        let mut proposal = plan("600000.SS", PlanStage::Proposed);
        proposal.record_date = None;
        proposal.ex_date = None;
        proposal.cash_per_ten = None;
        let mut past = plan("600001.SS", PlanStage::Scheduled);
        past.record_date = Some("2026-11-05".parse().unwrap());
        past.ex_date = Some("2026-11-06".parse().unwrap());
        let cancelled = plan("600002.SS", PlanStage::Cancelled);
        let mut future = plan("600003.SS", PlanStage::Scheduled);
        future.announced_on = "2026-11-08".parse().unwrap();
        let board = CalendarBoard::build(
            today,
            today,
            CalendarScope::All,
            vec![(
                CalendarCompany {
                    symbol: "600000.SS".into(),
                    name: "公司".into(),
                    personal: true,
                },
                PlanSnapshot {
                    plans: vec![
                        proposal,
                        past,
                        cancelled,
                        future,
                        plan("600004.SS", PlanStage::Scheduled),
                    ],
                    ..Default::default()
                },
            )],
        )
        .unwrap();
        assert_eq!(board.entries.len(), 1);
        assert_eq!(board.pending.len(), 1);
        assert!(board.pending[0].plan.cash_per_ten.is_none());
        assert!(board.entries[0].plan.payment_date.is_none());
        assert_eq!(board.end.to_string(), "2026-12-06");
    }
}
