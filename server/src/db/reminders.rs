use super::{
    AppError, AppResult, Database, Digest, NaiveDate, ReminderSettings, ReviewReminderSummary,
    Sha256,
};

impl Database {
    pub fn reminder_settings(&self) -> AppResult<ReminderSettings> {
        Ok(ReminderSettings {
            enabled: self.setting("reminders.enabled")?.as_deref() == Some("true"),
        })
    }

    pub fn save_reminder_settings(&self, enabled: bool) -> AppResult<ReminderSettings> {
        self.set_setting("reminders.enabled", if enabled { "true" } else { "false" })?;
        Ok(ReminderSettings { enabled })
    }

    pub fn review_reminder_summary(&self, today: NaiveDate) -> AppResult<ReviewReminderSummary> {
        let enabled = self.reminder_settings()?.enabled;
        let mut due_ids = self
            .decisions()?
            .into_iter()
            .filter(|decision| {
                decision.review.is_none()
                    && NaiveDate::parse_from_str(&decision.review_date, "%Y-%m-%d")
                        .is_ok_and(|date| date <= today)
            })
            .map(|decision| decision.id)
            .collect::<Vec<_>>();
        due_ids.sort();

        let latest_review = self.system_reviews()?.into_iter().next();
        let periodic_review_due = latest_review.as_ref().is_none_or(|review| {
            NaiveDate::parse_from_str(&review.next_review_date, "%Y-%m-%d")
                .map(|date| date <= today)
                .unwrap_or(true)
        });
        let periodic_marker = latest_review
            .as_ref()
            .map(|review| format!("{}:{}", review.id, review.next_review_date))
            .unwrap_or_else(|| "never-reviewed".into());
        let fingerprint_source = format!(
            "decisions={};periodic={periodic_review_due}:{periodic_marker}",
            due_ids.join(",")
        );
        let fingerprint = format!("{:x}", Sha256::digest(fingerprint_source.as_bytes()));
        let checked_on = today.format("%Y-%m-%d").to_string();
        let already_notified = self.setting("reminders.last_notified_on")?.as_deref()
            == Some(checked_on.as_str())
            && self
                .setting("reminders.last_notified_fingerprint")?
                .as_deref()
                == Some(fingerprint.as_str());
        let has_due_work = !due_ids.is_empty() || periodic_review_due;

        Ok(ReviewReminderSummary {
            enabled,
            due_decision_count: due_ids.len(),
            periodic_review_due,
            fingerprint,
            should_notify: enabled && has_due_work && !already_notified,
            checked_on,
        })
    }

    pub fn acknowledge_review_reminder(
        &self,
        today: NaiveDate,
        fingerprint: &str,
    ) -> AppResult<ReviewReminderSummary> {
        let current = self.review_reminder_summary(today)?;
        if fingerprint.trim().is_empty() || fingerprint != current.fingerprint {
            return Err(AppError::Validation(
                "复盘提醒状态已经变化，请刷新后重试".into(),
            ));
        }
        self.set_setting("reminders.last_notified_on", &current.checked_on)?;
        self.set_setting("reminders.last_notified_fingerprint", &current.fingerprint)?;
        self.review_reminder_summary(today)
    }
}
