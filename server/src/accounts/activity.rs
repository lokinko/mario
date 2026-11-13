//! Site usage metadata lives with account authority, never in a tenant ledger.
use super::Accounts;
use crate::{
    storage::{params, Connection},
    AppError, AppResult,
};
use serde::{Deserialize, Serialize};

pub(super) fn initialize(conn: &Connection) -> AppResult<()> {
    conn.execute_batch("BEGIN;
        CREATE TABLE IF NOT EXISTS activity_metadata(id INTEGER PRIMARY KEY CHECK(id=1), started_at BIGINT NOT NULL);
        INSERT INTO activity_metadata VALUES(1,EXTRACT(EPOCH FROM NOW())::BIGINT) ON CONFLICT DO NOTHING;
        CREATE TABLE IF NOT EXISTS activity_minutes(user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, minute BIGINT NOT NULL, page TEXT NOT NULL, PRIMARY KEY(user_id,minute,page));
        CREATE INDEX IF NOT EXISTS activity_minute_time ON activity_minutes(minute);
        COMMIT;")?;
    Ok(())
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Page {
    Watch,
    Calendar,
    Facts,
    Evidence,
    Review,
    Advisor,
    Methodology,
    Settings,
}
impl Page {
    fn name(self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::Calendar => "calendar",
            Self::Facts => "facts",
            Self::Evidence => "evidence",
            Self::Review => "review",
            Self::Advisor => "advisor",
            Self::Methodology => "methodology",
            Self::Settings => "settings",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageDashboard {
    generated_at: i64,
    tracking_since: i64,
    days: i64,
    total_users: i64,
    daily_active: i64,
    weekly_active: i64,
    monthly_active: i64,
    daily: Vec<DailyUsage>,
    users: Vec<UserUsage>,
    pages: Vec<PageUsage>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyUsage {
    date: String,
    active_users: i64,
    active_minutes: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserUsage {
    user_id: String,
    email: String,
    last_seen: Option<i64>,
    active_days: i64,
    active_minutes: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageUsage {
    page: String,
    active_users: i64,
    active_minutes: i64,
}

// Fixed UTC+8 calendar boundaries, independent of the host/DB timezone.
fn day_start(now: i64) -> i64 {
    (now + 8 * 3600).div_euclid(86400) * 86400 - 8 * 3600
}

impl Accounts {
    pub(crate) fn record_activity(&self, user: &str, page: Page) -> AppResult<()> {
        self.record_activity_at(user, page, chrono::Utc::now().timestamp())
    }
    fn record_activity_at(&self, user: &str, page: Page, now: i64) -> AppResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM activity_minutes WHERE minute<$1",
            [(day_start(now) - 89 * 86400) / 60],
        )?;
        tx.execute("INSERT INTO activity_minutes(user_id,minute,page) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",params![user,now/60,page.name()])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn usage_dashboard(&self, user: &str, days: i64) -> AppResult<UsageDashboard> {
        self.usage_dashboard_at(user, days, chrono::Utc::now().timestamp())
    }
    fn usage_dashboard_at(&self, user: &str, days: i64, now: i64) -> AppResult<UsageDashboard> {
        if !matches!(days, 7 | 30 | 90) {
            return Err(AppError::Validation("统计范围须为 7、30 或 90 天".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute_batch("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")?;
        let allowed: bool =
            tx.query_row("SELECT is_superuser FROM users WHERE id=$1", [user], |r| {
                r.get(0)
            })?;
        if !allowed {
            return Err(AppError::Forbidden("仅超级账户可查看全站活跃度".into()));
        }
        let today = day_start(now) / 60;
        let start = today - (days - 1) * 1440;
        let summary = tx.query_row("SELECT COUNT(DISTINCT user_id) FILTER(WHERE minute >= $1), COUNT(DISTINCT user_id) FILTER(WHERE minute >= $2), COUNT(DISTINCT user_id) FILTER(WHERE minute >= $3) FROM activity_minutes WHERE minute <= $4",params![today,today-6*1440,today-29*1440,now/60],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let tracking_since = tx.query_row(
            "SELECT started_at FROM activity_metadata WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let mut statement=tx.prepare("SELECT ((minute+480)/1440)*1440-480 AS day, COUNT(DISTINCT user_id), COUNT(DISTINCT (user_id,minute)) FROM activity_minutes WHERE minute >= $1 AND minute <= $2 GROUP BY day ORDER BY day DESC")?;
        let rows = statement
            .query_map(params![start, now / 60], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let daily = (0..days)
            .map(|offset| {
                let minute = today - offset * 1440;
                let (_, active_users, active_minutes) = rows
                    .iter()
                    .find(|r| r.0 == minute)
                    .copied()
                    .unwrap_or((minute, 0, 0));
                DailyUsage {
                    date: chrono::DateTime::from_timestamp(minute * 60 + 8 * 3600, 0)
                        .unwrap()
                        .format("%Y-%m-%d")
                        .to_string(),
                    active_users,
                    active_minutes,
                }
            })
            .collect();
        let mut statement=tx.prepare("SELECT u.id,u.email,MAX(a.minute)*60,COUNT(DISTINCT ((a.minute+480)/1440)),COUNT(DISTINCT a.minute) FROM users u LEFT JOIN activity_minutes a ON a.user_id=u.id AND a.minute >= $1 AND a.minute <= $2 GROUP BY u.id,u.email ORDER BY MAX(a.minute) DESC NULLS LAST,u.email")?;
        let users = statement
            .query_map(params![start, now / 60], |r| {
                Ok(UserUsage {
                    user_id: r.get(0)?,
                    email: r.get(1)?,
                    last_seen: r.get(2)?,
                    active_days: r.get(3)?,
                    active_minutes: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let total_users = users.len() as i64;
        let mut statement=tx.prepare("SELECT page,COUNT(DISTINCT user_id),COUNT(*) FROM activity_minutes WHERE minute >= $1 AND minute <= $2 GROUP BY page ORDER BY COUNT(*) DESC,page")?;
        let pages = statement
            .query_map(params![start, now / 60], |r| {
                Ok(PageUsage {
                    page: r.get(0)?,
                    active_users: r.get(1)?,
                    active_minutes: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok(UsageDashboard {
            generated_at: now,
            tracking_since,
            days,
            total_users,
            daily_active: summary.0,
            weekly_active: summary.1,
            monthly_active: summary.2,
            daily,
            users,
            pages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_deduplicate_across_tabs_and_pages_and_respect_shanghai_days() {
        let accounts = super::super::tests::accounts();
        let conn = accounts.conn().unwrap();
        conn.execute_batch("INSERT INTO users(id,email,password_hash,is_superuser) VALUES('admin','admin@example.test','unused',TRUE),('a','a@example.test','unused',FALSE),('b','b@example.test','unused',FALSE)").unwrap();
        drop(conn);
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-13T00:00:10+08:00")
            .unwrap()
            .timestamp();
        for page in [Page::Watch, Page::Watch, Page::Facts] {
            accounts.record_activity_at("a", page, now).unwrap();
        }
        accounts
            .record_activity_at("b", Page::Watch, now - 60)
            .unwrap();
        accounts
            .record_activity_at("a", Page::Watch, now - 7 * 86400)
            .unwrap();
        let result = accounts.usage_dashboard_at("admin", 7, now).unwrap();
        assert_eq!(
            (
                result.total_users,
                result.daily_active,
                result.weekly_active,
                result.monthly_active
            ),
            (3, 1, 2, 2)
        );
        assert_eq!(
            (
                result.daily[0].date.as_str(),
                result.daily[0].active_minutes
            ),
            ("2026-11-13", 1)
        );
        assert_eq!(result.daily[1].active_users, 1);
        assert_eq!(
            result
                .users
                .iter()
                .find(|u| u.user_id == "a")
                .unwrap()
                .active_minutes,
            1
        );
        assert_eq!(
            result
                .pages
                .iter()
                .find(|p| p.page == "watch")
                .unwrap()
                .active_minutes,
            2
        );
        assert!(matches!(
            accounts.usage_dashboard_at("a", 7, now),
            Err(AppError::Forbidden(_))
        ));
        assert!(accounts.usage_dashboard_at("admin", 8, now).is_err());
        accounts
            .record_activity_at("b", Page::Watch, now - 90 * 86400)
            .unwrap();
        accounts.record_activity_at("a", Page::Watch, now).unwrap();
        assert_eq!(
            accounts
                .conn()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM activity_minutes WHERE minute<$1",
                    [(day_start(now) - 89 * 86400) / 60],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        // Startup upgrade is idempotent; it preserves collected data.
        initialize(&accounts.conn().unwrap()).unwrap();
        assert_eq!(
            accounts.usage_dashboard_at("admin", 7, now).unwrap().daily[0].active_minutes,
            1
        );
    }
}
