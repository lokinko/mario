//! Parse disclosed plans without feeding proposals or future dates into implemented TTM history.
use crate::{
    models::watch::{
        calendar::{DividendPlan, PlanIssue, PlanStage},
        WatchInput,
    },
    AppResult,
};
use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeSet;

fn date(value: &Value, issues: &mut Vec<PlanIssue>) -> Option<NaiveDate> {
    if value.is_null() {
        return None;
    }
    match value
        .as_str()
        .and_then(|s| s.get(..10))
        .and_then(|s| s.parse().ok())
    {
        Some(date) => Some(date),
        None => {
            if !issues.contains(&PlanIssue::InvalidDates) {
                issues.push(PlanIssue::InvalidDates);
            }
            None
        }
    }
}
fn amount(value: &Value, issues: &mut Vec<PlanIssue>) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    match Decimal::from_str_exact(&raw)
        .ok()
        .filter(|v| *v >= Decimal::ZERO)
    {
        Some(value) => Some(value.normalize().to_string()),
        None => {
            if !issues.contains(&PlanIssue::InvalidAmount) {
                issues.push(PlanIssue::InvalidAmount);
            }
            None
        }
    }
}
pub(super) fn parse(
    payload: &Value,
    symbol: &str,
    today: NaiveDate,
    sources: &[String],
) -> AppResult<Vec<DividendPlan>> {
    WatchInput {
        symbol: symbol.into(),
        name: "分红日历".into(),
        thesis: String::new(),
    }
    .normalize()?;
    let rows = payload["result"]["data"]
        .as_array()
        .ok_or_else(super::bad)?;
    if payload["success"] != true
        || payload["result"]["pages"] != 1
        || payload["result"]["count"].as_u64() != Some(rows.len() as u64)
    {
        return Err(super::bad());
    }
    let mut plans = Vec::new();
    let mut seen = BTreeSet::new();
    let since = today.checked_sub_days(Days::new(730)).unwrap_or(today);
    for row in rows {
        if row["SECUCODE"] != symbol.replace(".SS", ".SH") || row["SECURITY_CODE"] != symbol[..6] {
            return Err(super::bad());
        }
        let report_date: NaiveDate = row["REPORT_DATE"]
            .as_str()
            .and_then(|s| s.get(..10))
            .ok_or_else(super::bad)?
            .parse()
            .map_err(|_| super::bad())?;
        let mut issues = Vec::new();
        let initial = date(&row["PLAN_NOTICE_DATE"], &mut issues);
        let announced_on = date(&row["NOTICE_DATE"], &mut issues)
            .or(initial)
            .ok_or_else(super::bad)?;
        if announced_on > today {
            continue;
        }
        let progress = row["ASSIGN_PROGRESS"].as_str().unwrap_or_default();
        let stage = match progress {
            "预披露" => PlanStage::Preannounced,
            "董事会决议通过" | "董事会预案" | "预案" => PlanStage::Proposed,
            "股东大会决议通过" | "股东大会通过" => PlanStage::Approved,
            "实施分配" => PlanStage::Scheduled,
            "取消分配" | "方案取消" | "股东大会否决" => PlanStage::Cancelled,
            "不分配" | "不分红" => PlanStage::NoDistribution,
            _ => {
                issues.push(PlanIssue::UnknownProgress);
                PlanStage::Unknown
            }
        };
        let mut record_date = date(&row["EQUITY_RECORD_DATE"], &mut issues);
        let mut ex_date = date(&row["EX_DIVIDEND_DATE"], &mut issues);
        if record_date
            .zip(ex_date)
            .is_some_and(|(record, ex)| record > ex)
        {
            record_date = None;
            ex_date = None;
            if !issues.contains(&PlanIssue::InvalidDates) {
                issues.push(PlanIssue::InvalidDates);
            }
        }
        // A plan can pay an older reporting period years later. Filter by disclosure
        // and actual schedule, never by the report-period label alone.
        if announced_on < since
            && [record_date, ex_date]
                .iter()
                .flatten()
                .all(|date| *date < today)
        {
            continue;
        }
        let id = format!("{symbol}:{report_date}:{}", initial.unwrap_or(announced_on));
        if !seen.insert(id.clone()) {
            return Err(super::bad());
        }
        plans.push(DividendPlan {
            id,
            symbol: symbol.into(),
            report_date,
            announced_on,
            stage,
            source_progress: progress.into(),
            profile: row["IMPL_PLAN_PROFILE"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(2000)
                .collect(),
            cash_per_ten: amount(&row["PRETAX_BONUS_RMB"], &mut issues),
            bonus_per_ten: amount(&row["BONUS_RATIO"], &mut issues),
            transfer_per_ten: amount(&row["IT_RATIO"], &mut issues),
            record_date,
            ex_date,
            // This source does not expose a reliable cash-payment date; never copy ex-date into it.
            payment_date: None,
            issues,
            sources: std::iter::once(format!(
                "https://data.eastmoney.com/yjfp/detail/{}.html",
                &symbol[..6]
            ))
            .chain(sources.iter().cloned())
            .collect(),
        });
    }
    Ok(plans)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Value {
        serde_json::json!({"SECUCODE":"600000.SH","SECURITY_CODE":"600000","REPORT_DATE":"2026-06-30 00:00:00","PLAN_NOTICE_DATE":"2026-09-01 00:00:00","NOTICE_DATE":"2026-11-06 00:00:00","ASSIGN_PROGRESS":"实施分配","PRETAX_BONUS_RMB":1.25,"EQUITY_RECORD_DATE":"2026-11-10 00:00:00","EX_DIVIDEND_DATE":"2026-11-11 00:00:00"})
    }
    fn payload(row: Value) -> Value {
        serde_json::json!({"success":true,"result":{"pages":1,"count":1,"data":[row]}})
    }
    #[test]
    fn future_scheduled_dates_are_kept_while_unknown_amount_and_cash_date_are_not_invented() {
        let day = "2026-11-07".parse().unwrap();
        let parsed = parse(&payload(row()), "600000.SS", day, &[]).unwrap();
        assert_eq!(parsed[0].stage, PlanStage::Scheduled);
        assert_eq!(parsed[0].cash_per_ten.as_deref(), Some("1.25"));
        assert_eq!(parsed[0].ex_date.unwrap().to_string(), "2026-11-11");
        assert!(parsed[0].payment_date.is_none());
        let mut proposal = row();
        proposal["ASSIGN_PROGRESS"] = "预披露".into();
        proposal["PRETAX_BONUS_RMB"] = Value::Null;
        proposal["EX_DIVIDEND_DATE"] = Value::Null;
        proposal["EQUITY_RECORD_DATE"] = Value::Null;
        let parsed = parse(&payload(proposal), "600000.SS", day, &[]).unwrap();
        assert!(parsed[0].cash_per_ten.is_none());
        assert!(parsed[0].ex_date.is_none());
    }
    #[test]
    fn old_report_periods_can_still_have_real_upcoming_implementation_dates() {
        let mut value = row();
        value["REPORT_DATE"] = "2022-12-31".into();
        let parsed = parse(
            &payload(value),
            "600000.SS",
            "2026-11-07".parse().unwrap(),
            &[],
        )
        .unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].report_date.to_string(), "2022-12-31");
        assert_eq!(parsed[0].ex_date.unwrap().to_string(), "2026-11-11");
    }

    #[test]
    fn bad_parts_stay_unknown_and_wrong_identity_or_incomplete_pages_reject_the_snapshot() {
        let day = "2026-11-07".parse().unwrap();
        let mut invalid = row();
        invalid["PRETAX_BONUS_RMB"] = (-1).into();
        invalid["EX_DIVIDEND_DATE"] = "2026-11-01".into();
        let parsed = parse(&payload(invalid), "600000.SS", day, &[]).unwrap();
        assert!(parsed[0].cash_per_ten.is_none());
        assert!(parsed[0].record_date.is_none());
        assert!(parsed[0].issues.contains(&PlanIssue::InvalidAmount));
        let mut wrong = row();
        wrong["SECURITY_CODE"] = "600001".into();
        assert!(parse(&payload(wrong), "600000.SS", day, &[]).is_err());
        let mut incomplete = payload(row());
        incomplete["result"]["pages"] = 2.into();
        assert!(parse(&incomplete, "600000.SS", day, &[]).is_err());
    }
}
