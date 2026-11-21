//! Public statutory A-share announcement source backed by CNINFO (巨潮资讯网).
//! Never reads accounts, holdings or private rules.
use crate::{
    models::market::{
        announcements::{classify_announcement, OfficialAnnouncement},
        canonical_symbol, is_a_share_stock,
    },
    ports::market::AnnouncementProvider,
    AppError, AppResult,
};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};
use tokio::sync::{Mutex, Semaphore};

const CNINFO_SEARCH_URL: &str = "https://www.cninfo.com.cn/new/information/topSearch/query";
const CNINFO_QUERY_URL: &str = "https://www.cninfo.com.cn/new/hisAnnouncement/query";
const CNINFO_STATIC_BASE: &str = "https://static.cninfo.com.cn/";
const CNINFO_SOURCE_NAME: &str = "巨潮资讯网";

pub(crate) struct CninfoAnnouncementProvider {
    client: Client,
    slots: Semaphore,
    org_cache: Mutex<BTreeMap<String, String>>,
    search_url: String,
    query_url: String,
}

impl CninfoAnnouncementProvider {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("Mozilla/5.0 (compatible; mario/market-announcements)")
                .build()?,
            slots: Semaphore::new(2),
            org_cache: Mutex::new(BTreeMap::new()),
            search_url: CNINFO_SEARCH_URL.into(),
            query_url: CNINFO_QUERY_URL.into(),
        })
    }

    #[cfg(test)]
    fn with_urls(search_url: String, query_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            slots: Semaphore::new(2),
            org_cache: Mutex::new(BTreeMap::new()),
            search_url,
            query_url,
        }
    }

    async fn resolve_org_id(&self, code: &str) -> AppResult<Option<String>> {
        if let Some(cached) = self.org_cache.lock().await.get(code).cloned() {
            return Ok(Some(cached));
        }
        let response = self
            .client
            .post(&self.search_url)
            .header(
                "Referer",
                "https://www.cninfo.com.cn/new/commonUrl?url=disclosure/list/notice",
            )
            .form(&[("keyWord", code), ("maxNum", "10")])
            .send()
            .await?
            .error_for_status()?;
        let payload = super::read_public_json(response).await?;
        let org_id = payload.as_array().and_then(|rows| {
            rows.iter().find_map(|row| {
                let row_code = row.get("code")?.as_str()?;
                let org = row.get("orgId")?.as_str()?.trim();
                (row_code == code && !org.is_empty()).then(|| org.to_string())
            })
        });
        if let Some(ref found) = org_id {
            self.org_cache
                .lock()
                .await
                .insert(code.to_string(), found.clone());
        }
        Ok(org_id)
    }
}

#[async_trait]
impl AnnouncementProvider for CninfoAnnouncementProvider {
    async fn announcements(
        &self,
        symbol: &str,
        since: NaiveDate,
        through: NaiveDate,
    ) -> AppResult<Vec<OfficialAnnouncement>> {
        let normalized = canonical_symbol(symbol)?;
        if !is_a_share_stock(&normalized) {
            return Ok(Vec::new());
        }
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| AppError::MarketData("公告采集通道已关闭".into()))?;
        let (code, exchange) = normalized
            .split_once('.')
            .ok_or_else(|| AppError::Validation("无效 A 股代码".into()))?;
        let column = if exchange == "SS" { "sse" } else { "szse" };
        let org_id = self.resolve_org_id(code).await?;
        let stock_param = match org_id {
            Some(org) => format!("{code},{org}"),
            None => code.to_string(),
        };
        let se_date = format!(
            "{}~{}",
            since.format("%Y-%m-%d"),
            through.format("%Y-%m-%d")
        );
        let response = self
            .client
            .post(&self.query_url)
            .header(
                "Referer",
                "https://www.cninfo.com.cn/new/commonUrl?url=disclosure/list/notice",
            )
            .header("X-Requested-With", "XMLHttpRequest")
            .form(&[
                ("pageNum", "1"),
                ("pageSize", "30"),
                ("column", column),
                ("tabName", "fulltext"),
                ("stock", stock_param.as_str()),
                ("seDate", se_date.as_str()),
                ("isHLtitle", "true"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let payload = super::read_public_json(response).await?;
        parse_cninfo_payload(&payload, &normalized, since, through)
    }
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

pub(crate) fn parse_cninfo_payload(
    payload: &Value,
    symbol: &str,
    since: NaiveDate,
    through: NaiveDate,
) -> AppResult<Vec<OfficialAnnouncement>> {
    let normalized = canonical_symbol(symbol)?;
    let expected_code = normalized
        .split_once('.')
        .map(|(code, _)| code)
        .unwrap_or(&normalized);
    let Some(rows) = payload.get("announcements") else {
        return Err(AppError::MarketData("巨潮资讯响应缺少公告列表".into()));
    };
    if rows.is_null() {
        return Ok(Vec::new());
    }
    let Some(items) = rows.as_array() else {
        return Err(AppError::MarketData("巨潮资讯公告字段格式无效".into()));
    };
    let mut announcements = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for row in items {
        let sec_code = row
            .get("secCode")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !sec_code.is_empty() && sec_code != expected_code {
            continue;
        }
        let id = row
            .get("announcementId")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        let raw_title = row
            .get("announcementTitle")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = strip_html_tags(raw_title);
        if id.is_empty() || title.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        // Skip English duplicates to avoid noisy double entries in the Chinese review feed.
        if title.contains("（英文版）") || title.contains("(英文版)") {
            continue;
        }
        let millis = row
            .get("announcementTime")
            .and_then(Value::as_i64)
            .ok_or_else(|| AppError::MarketData("巨潮资讯公告缺少发布时间".into()))?;
        let published_dt: DateTime<Utc> = chrono_tz::Asia::Shanghai
            .timestamp_millis_opt(millis)
            .single()
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or_else(|| AppError::MarketData("巨潮资讯公告发布时间无效".into()))?;
        let published_day = published_dt
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        if published_day < since || published_day > through {
            continue;
        }
        let stock_name = row
            .get("secName")
            .and_then(Value::as_str)
            .map(strip_html_tags)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| normalized.clone());
        let adjunct = row
            .get("adjunctUrl")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let source_url = if adjunct.starts_with("http://") || adjunct.starts_with("https://") {
            adjunct.to_string()
        } else if !adjunct.is_empty() {
            format!("{CNINFO_STATIC_BASE}{}", adjunct.trim_start_matches('/'))
        } else {
            format!(
                "https://www.cninfo.com.cn/new/disclosure/detail?stockCode={expected_code}&announcementId={id}"
            )
        };
        let (category, importance) = classify_announcement(&title);
        announcements.push(OfficialAnnouncement {
            id: format!("cninfo:{id}"),
            symbol: normalized.clone(),
            stock_name,
            title,
            category: category.into(),
            published_on: published_day.to_string(),
            published_at: published_dt.to_rfc3339(),
            source_name: CNINFO_SOURCE_NAME.into(),
            source_url,
            importance,
        });
    }
    announcements.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(announcements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::announcements::AnnouncementImportance;
    use axum::{routing::post, Json, Router};
    use chrono::TimeZone;

    #[test]
    fn parses_cninfo_payload_filters_english_duplicates_and_classifies() {
        let ts = chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2026, 11, 20, 18, 30, 0)
            .unwrap()
            .timestamp_millis();
        let payload = serde_json::json!({
            "announcements": [
                {
                    "secCode": "600519",
                    "secName": "贵州茅台",
                    "announcementId": "1220001",
                    "announcementTitle": "关于2026年中期<em>利润分配</em>方案的公告",
                    "announcementTime": ts,
                    "adjunctUrl": "finalpage/2026-11-20/1220001.PDF"
                },
                {
                    "secCode": "600519",
                    "secName": "贵州茅台",
                    "announcementId": "1220002",
                    "announcementTitle": "2026年中期报告（英文版）",
                    "announcementTime": ts,
                    "adjunctUrl": "finalpage/2026-11-20/1220002.PDF"
                },
                {
                    "secCode": "000001",
                    "secName": "平安银行",
                    "announcementId": "1220003",
                    "announcementTitle": "无关股票公告",
                    "announcementTime": ts,
                    "adjunctUrl": "finalpage/2026-11-20/1220003.PDF"
                }
            ]
        });
        let since = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let through = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let items = parse_cninfo_payload(&payload, "600519.SS", since, through).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "cninfo:1220001");
        assert_eq!(items[0].title, "关于2026年中期利润分配方案的公告");
        assert_eq!(items[0].category, "分红派息");
        assert_eq!(items[0].importance, AnnouncementImportance::High);
        assert_eq!(
            items[0].source_url,
            "https://static.cninfo.com.cn/finalpage/2026-11-20/1220001.PDF"
        );
    }

    #[tokio::test]
    async fn fetches_announcements_from_cninfo_endpoints_and_caches_org_id() {
        let ts = chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2026, 11, 20, 17, 0, 0)
            .unwrap()
            .timestamp_millis();
        let app = Router::new()
            .route(
                "/search",
                post(|| async {
                    Json(serde_json::json!([
                        { "code": "600036", "orgId": "gssh0600036" }
                    ]))
                }),
            )
            .route(
                "/query",
                post(move |body: String| async move {
                    assert!(
                        body.contains("600036%2Cgssh0600036")
                            || body.contains("600036,gssh0600036")
                    );
                    Json(serde_json::json!({
                        "announcements": [
                            {
                                "secCode": "600036",
                                "secName": "招商银行",
                                "announcementId": "9001",
                                "announcementTitle": "2026年第三季度报告",
                                "announcementTime": ts,
                                "adjunctUrl": "finalpage/2026-11-20/9001.PDF"
                            }
                        ]
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = CninfoAnnouncementProvider::with_urls(
            format!("http://{addr}/search"),
            format!("http://{addr}/query"),
        );
        let since = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let through = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let rows = provider
            .announcements("600036.SS", since, through)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].stock_name, "招商银行");
        assert_eq!(rows[0].importance, AnnouncementImportance::High);
        // Non-A-share symbols return empty without hitting CNINFO.
        assert!(provider
            .announcements("AAPL", since, through)
            .await
            .unwrap()
            .is_empty());
        server.abort();
    }
}
