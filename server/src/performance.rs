use chrono::NaiveDate;

use crate::models::{PortfolioEventRecord, PortfolioEventSummary, PortfolioEventType};

pub fn summarize(events: &[PortfolioEventRecord]) -> PortfolioEventSummary {
    let mut summary = PortfolioEventSummary::default();
    for event in events {
        summary.event_ids.push(event.id.clone());
        match event.event_type {
            PortfolioEventType::Deposit => summary.external_cash_flow += event.base_amount,
            PortfolioEventType::Withdrawal => summary.external_cash_flow -= event.base_amount,
            PortfolioEventType::Dividend | PortfolioEventType::Interest => {
                summary.income += event.base_amount;
            }
            PortfolioEventType::Fee | PortfolioEventType::Tax => {
                summary.costs += event.base_amount;
            }
            PortfolioEventType::Buy | PortfolioEventType::Sell => {
                summary.turnover += event.base_amount;
            }
        }
    }
    summary
}

pub fn modified_dietz_return_pct(
    beginning_value: f64,
    ending_value: f64,
    period_start: NaiveDate,
    period_end: NaiveDate,
    events: &[PortfolioEventRecord],
) -> Option<f64> {
    let total_days = (period_end - period_start).num_days();
    if beginning_value <= 0.0 || total_days <= 0 {
        return None;
    }

    let mut external_cash_flow = 0.0;
    let mut weighted_cash_flow = 0.0;
    for event in events {
        let signed_flow = match event.event_type {
            PortfolioEventType::Deposit => event.base_amount,
            PortfolioEventType::Withdrawal => -event.base_amount,
            _ => continue,
        };
        let occurred_on = NaiveDate::parse_from_str(&event.occurred_on, "%Y-%m-%d").ok()?;
        let remaining_days = (period_end - occurred_on).num_days();
        let weight = remaining_days as f64 / total_days as f64;
        external_cash_flow += signed_flow;
        weighted_cash_flow += weight * signed_flow;
    }

    let denominator = beginning_value + weighted_cash_flow;
    if denominator <= 0.0 || !denominator.is_finite() {
        return None;
    }
    let result = (ending_value - beginning_value - external_cash_flow) / denominator * 100.0;
    result.is_finite().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        event_type: PortfolioEventType,
        amount: f64,
        occurred_on: &str,
    ) -> PortfolioEventRecord {
        PortfolioEventRecord {
            id: format!("{event_type:?}-{occurred_on}"),
            event_type,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            base_currency: "CNY".into(),
            base_amount: amount,
            occurred_on: occurred_on.into(),
            note: "test".into(),
            created_at: "2026-02-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn separates_external_flows_income_costs_and_turnover() {
        let events = vec![
            event(PortfolioEventType::Deposit, 10_000.0, "2026-01-15"),
            event(PortfolioEventType::Withdrawal, 2_000.0, "2026-01-20"),
            event(PortfolioEventType::Dividend, 500.0, "2026-01-21"),
            event(PortfolioEventType::Fee, 50.0, "2026-01-22"),
            event(PortfolioEventType::Buy, 4_000.0, "2026-01-23"),
        ];
        let summary = summarize(&events);
        assert_eq!(summary.external_cash_flow, 8_000.0);
        assert_eq!(summary.income, 500.0);
        assert_eq!(summary.costs, 50.0);
        assert_eq!(summary.turnover, 4_000.0);
        assert_eq!(summary.event_ids.len(), 5);
    }

    #[test]
    fn weights_external_flows_by_their_dates() {
        let events = vec![event(PortfolioEventType::Deposit, 10_000.0, "2026-01-16")];
        let result = modified_dietz_return_pct(
            100_000.0,
            121_000.0,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            &events,
        )
        .unwrap();
        assert!((result - 10.476_190_476).abs() < 1e-6);
    }
}
