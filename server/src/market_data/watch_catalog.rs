//! Official CNINFO identity and annual-report lookup; no account or storage access.
use crate::{
    models::watch::{catalog::Company, WatchInput},
    ports::market::CatalogSourceProvider,
    AppError, AppResult,
};
use chrono::{Datelike, Utc};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

const STOCKS: &str = "https://www.cninfo.com.cn/new/data/szse_stock.json";
const REPORTS: &str = "https://www.cninfo.com.cn/new/hisAnnouncement/query";
type DirectoryCache = Option<(Instant, Result<Value, String>)>;

pub(crate) struct PublicCatalogSources {
    client: reqwest::Client,
    directory: Mutex<DirectoryCache>,
    slots: Semaphore,
}
impl PublicCatalogSources {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("Mozilla/5.0 mario/catalog")
                .build()?,
            directory: Mutex::new(None),
            slots: Semaphore::new(2),
        })
    }
    async fn json(response: reqwest::Response) -> AppResult<Value> {
        let response = response.error_for_status()?;
        if response
            .content_length()
            .is_some_and(|size| size > 4 * 1024 * 1024)
        {
            return Err(AppError::MarketData("公司资料响应超过上限".into()));
        }
        let bytes = response.bytes().await?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(AppError::MarketData("公司资料响应超过上限".into()));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn stocks(&self) -> AppResult<Value> {
        let mut cache = self.directory.lock().await;
        if let Some((time, result)) = cache.as_ref() {
            let ttl = if result.is_ok() { 86400 } else { 60 };
            if time.elapsed() < Duration::from_secs(ttl) {
                return result.clone().map_err(AppError::MarketData);
            }
        }
        let result = async {
            let value = Self::json(
                self.client
                    .get(STOCKS)
                    .header("Referer", "https://www.cninfo.com.cn/")
                    .send()
                    .await?,
            )
            .await?;
            if value["stockList"]
                .as_array()
                .is_none_or(|rows| rows.is_empty())
            {
                return Err(AppError::MarketData("公开证券身份目录暂不可用".into()));
            }
            Ok(value)
        }
        .await
        .map_err(|error: AppError| error.to_string());
        *cache = Some((Instant::now(), result.clone()));
        result.map_err(AppError::MarketData)
    }
}

fn identity(directory: &Value, query: &str) -> AppResult<(Company, String)> {
    let query = query.trim().to_uppercase().replace(".SH", ".SS");
    if query.is_empty() || query.len() > 120 {
        return Err(AppError::Validation("请输入公司简称或沪深 A 股代码".into()));
    }
    let code = query.split('.').next().unwrap_or_default();
    let rows = directory["stockList"]
        .as_array()
        .ok_or_else(|| AppError::MarketData("证券身份目录格式无效".into()))?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| row["category"] == "A股" && (row["code"] == code || row["zwjc"] == query))
        .collect();
    if matches.len() != 1 {
        return Err(AppError::Validation(
            "未找到唯一的沪深 A 股公司，请核对名称或输入六位证券代码".into(),
        ));
    }
    let row = matches[0];
    let code = row["code"].as_str().unwrap_or_default();
    let symbol = format!("{code}.{}", if code.starts_with('6') { "SS" } else { "SZ" });
    let input = WatchInput {
        symbol,
        name: row["zwjc"].as_str().unwrap_or_default().into(),
        thesis: String::new(),
    }
    .normalize()?;
    if query.contains('.') && query != input.symbol {
        return Err(AppError::Validation("证券代码与交易所不一致".into()));
    }
    let org = row["orgId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::MarketData("公司组织标识缺失".into()))?;
    let profile = if code.starts_with('6') {
        format!("https://www.sse.com.cn/assortment/stock/list/info/company/index.shtml?COMPANY_CODE={code}")
    } else {
        format!("https://www.szse.cn/certificate/individual/index.html?code={code}")
    };
    Ok((
        Company {
            symbol: input.symbol,
            name: input.name,
            sources: vec![profile],
            sources_checked_on: None,
        },
        org.into(),
    ))
}

fn annual_report(value: &Value, code: &str, org: &str, end: i64) -> Option<String> {
    let mut rows: Vec<_> = value["announcements"]
        .as_array()?
        .iter()
        .filter(|row| {
            let title = row["announcementTitle"].as_str().unwrap_or_default();
            row["secCode"] == code
                && row["orgId"] == org
                && row["announcementTime"]
                    .as_i64()
                    .is_some_and(|time| time < end)
                && title.contains("年度报告")
                && !["摘要", "英文", "取消", "说明", "意见", "审计", "更正公告"]
                    .iter()
                    .any(|word| title.contains(word))
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row["announcementTime"].as_i64()));
    rows.into_iter().find_map(|row| {
        let path = row["adjunctUrl"].as_str()?;
        (path.starts_with("finalpage/")
            && path.ends_with(".PDF")
            && !path.contains("..")
            && path
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/-_.".contains(&c)))
        .then(|| format!("https://static.cninfo.com.cn/{path}"))
    })
}

#[async_trait::async_trait]
impl CatalogSourceProvider for PublicCatalogSources {
    async fn resolve(&self, query: &str) -> AppResult<Company> {
        let (mut company, org) = identity(&self.stocks().await?, query)?;
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| AppError::Busy("公司资料查询已关闭".into()))?;
        let today = Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive();
        let code = &company.symbol[..6];
        let reports = async {
            let response = self
                .client
                .post(REPORTS)
                .header("Referer", "https://www.cninfo.com.cn/")
                .form(&[
                    ("pageNum", "1".to_string()),
                    ("pageSize", "100".into()),
                    ("column", "szse".into()),
                    ("tabName", "fulltext".into()),
                    ("stock", format!("{code},{org}")),
                    ("seDate", format!("{}-01-01~{today}", today.year() - 1)),
                    ("sortName", "time".into()),
                    ("sortType", "desc".into()),
                    ("category", "category_ndbg_szsh".into()),
                    ("isHLtitle", "false".into()),
                ])
                .send()
                .await?;
            Self::json(response).await
        }
        .await;
        match reports {
            Ok(value) => {
                let end = today
                    .succ_opt()
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp_millis()
                    - 8 * 3600 * 1000;
                if let Some(report) = annual_report(&value, code, &org, end) {
                    company.sources.push(report);
                    company.sources_checked_on = Some(today.to_string());
                }
            }
            Err(error) => {
                tracing::warn!(symbol=%company.symbol, %error, "company report source pending; verified identity retained")
            }
        }
        Ok(company)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_requires_exact_unique_a_share_identity_and_correct_exchange() {
        let directory = serde_json::json!({"stockList":[
            {"code":"000651","zwjc":"格力电器","orgId":"gssz0000651","category":"A股"},
            {"code":"000651","zwjc":"其他市场","orgId":"other","category":"港股"}
        ]});
        for query in ["格力电器", "000651", "000651.SZ"] {
            let (company, org) = identity(&directory, query).unwrap();
            assert_eq!(company.symbol, "000651.SZ");
            assert_eq!(org, "gssz0000651");
            assert!(company.sources_checked_on.is_none());
        }
        for query in ["隔离电器", "000651.SH", "格力", ""] {
            assert!(identity(&directory, query).is_err());
        }
    }
    #[test]
    fn report_lookup_rejects_summaries_wrong_identity_future_and_untrusted_paths() {
        let row = serde_json::json!({"secCode":"600795", "orgId":"org", "announcementTime":100,
            "announcementTitle":"2025年年度报告", "adjunctUrl":"finalpage/2026-04-15/1225103091.PDF"});
        let mut wrong = row.clone();
        wrong["secCode"] = "600000".into();
        let mut summary = row.clone();
        summary["announcementTitle"] = "年度报告摘要".into();
        let mut future = row.clone();
        future["announcementTime"] = 200.into();
        let mut unsafe_path = row.clone();
        unsafe_path["adjunctUrl"] = "https://evil.test/file.PDF".into();
        let value = serde_json::json!({"announcements":[wrong, summary, future, unsafe_path]});
        assert!(annual_report(&value, "600795", "org", 200).is_none());
        let value = serde_json::json!({"announcements":[row]});
        assert_eq!(
            annual_report(&value, "600795", "org", 200).unwrap(),
            "https://static.cninfo.com.cn/finalpage/2026-04-15/1225103091.PDF"
        );
    }
}
