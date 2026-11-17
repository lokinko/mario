//! Public implemented distributions, dated coverage and share-adjusted TTM facts.
use crate::{AppError, AppResult};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividendEvent {
    // Absent only in observations saved before independent dividend storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_date: Option<NaiveDate>,
    pub ex_date: NaiveDate,
    pub cash_per_ten: String,
    pub bonus_per_ten: String,
    pub transfer_per_ten: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendBasis {
    pub as_of: NaiveDate,
    pub fetched_at: String,
}

/// One complete, replaceable source snapshot per security, including known-empty results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DividendHistory {
    #[serde(flatten)]
    pub basis: DividendBasis,
    pub sources: Vec<String>,
    pub events: Vec<DividendEvent>,
}

impl DividendHistory {
    /// Coverage wins over collection time; a delayed older cutoff cannot downgrade facts.
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.basis
            .as_of
            .cmp(&other.basis.as_of)
            .then_with(|| {
                chrono::DateTime::parse_from_rfc3339(&self.basis.fetched_at)
                    .ok()
                    .cmp(&chrono::DateTime::parse_from_rfc3339(&other.basis.fetched_at).ok())
            })
            .is_gt()
    }
    pub fn validate(&self) -> AppResult<()> {
        let fetched = chrono::DateTime::parse_from_rfc3339(&self.basis.fetched_at)
            .map_err(|_| AppError::Validation("分红采集时间无效".into()))?;
        let mut seen = std::collections::BTreeSet::new();
        if self.basis.as_of
            > fetched
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .date_naive()
            || self.sources.is_empty()
            || self.sources.iter().any(|s| !s.starts_with("https://"))
            || self.events.iter().any(|e| {
                e.ex_date > self.basis.as_of
                    || e.report_date.is_none()
                    || !seen.insert((e.ex_date, e.report_date))
                    || [&e.cash_per_ten, &e.bonus_per_ten, &e.transfer_per_ten]
                        .iter()
                        .any(|v| Decimal::from_str_exact(v).map_or(true, |v| v < Decimal::ZERO))
            })
        {
            return Err(AppError::Validation("分红事件或来源覆盖无效".into()));
        }
        Ok(())
    }
}

pub fn trailing_dividends(events: &[DividendEvent], day: NaiveDate) -> Option<Decimal> {
    let start = day - chrono::Duration::days(365);
    let mut total = Decimal::ZERO;
    for event in events
        .iter()
        .filter(|e| e.ex_date > start && e.ex_date <= day)
    {
        let mut cash = Decimal::from_str_exact(&event.cash_per_ten).ok()? / Decimal::TEN;
        if cash < Decimal::ZERO {
            return None;
        }
        // Normalize every past cash distribution to the current share unit, including
        // simultaneous bonus shares. The provider supplies per-ten original shares.
        for action in events
            .iter()
            .filter(|a| a.ex_date >= event.ex_date && a.ex_date <= day)
        {
            let bonus = Decimal::from_str_exact(&action.bonus_per_ten).ok()?;
            let transfer = Decimal::from_str_exact(&action.transfer_per_ten).ok()?;
            if bonus < Decimal::ZERO || transfer < Decimal::ZERO {
                return None;
            }
            let factor = Decimal::ONE.checked_add(bonus.checked_add(transfer)? / Decimal::TEN)?;
            cash = cash.checked_div(factor)?;
        }
        total = total.checked_add(cash)?;
    }
    Some(total)
}
