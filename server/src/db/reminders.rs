use super::{AppResult, Database, NaiveDate, ReviewReminderSummary};

impl Database {
    pub fn review_reminder_summary(&self, today: NaiveDate) -> AppResult<ReviewReminderSummary> {
        let periodic_review_due = self.system_reviews()?.first().is_none_or(|review| {
            !review.next_review_date.is_empty()
                && NaiveDate::parse_from_str(&review.next_review_date, "%Y-%m-%d")
                    .map(|date| date <= today)
                    .unwrap_or(true)
        });
        Ok(ReviewReminderSummary {
            periodic_review_due,
            checked_on: today.format("%Y-%m-%d").to_string(),
        })
    }
}
