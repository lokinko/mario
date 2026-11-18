//! Public collection scope and monitoring projections, independent of accounts and I/O.
use crate::{AppError, AppResult};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const UNIVERSE_URL: &str = "https://oss-ch.csindex.com.cn/static/html/csindex/public/uploads/file/autofile/cons/000300cons.xls";
pub const EASTMONEY_UNIVERSE_URL: &str = "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_INDEX_TS_COMPONENT&columns=SECUCODE,SECURITY_CODE,TYPE,SECURITY_NAME_ABBR,MAXTRADEDATE&filter=(TYPE%3D%221%22)&pageSize=500&pageNumber=1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceMember {
    pub symbol: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceUniverse {
    pub as_of: NaiveDate,
    pub fetched_at: i64,
    pub source_url: String,
    pub members: Vec<PriceMember>,
}
impl PriceUniverse {
    pub fn validate(&self) -> AppResult<()> {
        let mut seen = std::collections::BTreeSet::new();
        if self.members.len() != 300
            || ![UNIVERSE_URL, EASTMONEY_UNIVERSE_URL].contains(&self.source_url.as_str())
            || self.as_of > super::today().parse::<NaiveDate>().unwrap()
            || self.members.iter().any(|m| {
                !super::is_a_share_stock(&m.symbol)
                    || m.name.trim().is_empty()
                    || !seen.insert(&m.symbol)
            })
        {
            return Err(AppError::MarketData(
                "沪深300成分表的日期、证券身份、数量或来源无效，保留原范围".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCollectionDay {
    pub date: String,
    pub expected: usize,
    pub successful: usize,
    pub failed: usize,
    pub pending: usize,
    pub lagging: usize,
    pub updated_at: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCollectionRow {
    pub symbol: String,
    pub name: String,
    /// `csi300` member, `reference` trading-calendar index, or `other` registered symbol.
    pub scope: String,
    /// `cn`, `hk`, `us`, or `other`, derived from the canonical symbol.
    pub market: String,
    pub instrument_type: String,
    pub currency: String,
    /// Inactive rows keep their stored prices but are no longer refreshed or counted.
    pub tracked: bool,
    pub status: String,
    pub issue_codes: Vec<String>,
    pub error: String,
    pub attempted_at: Option<i64>,
    pub succeeded_at: Option<i64>,
    pub first_on: Option<NaiveDate>,
    pub latest_on: Option<NaiveDate>,
    pub bars: usize,
    pub provider: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCollectionDashboard {
    pub generated_at: i64,
    pub years: u32,
    pub target_through: NaiveDate,
    pub universe: Option<PriceUniverse>,
    pub universe_attempted_at: Option<i64>,
    pub universe_error: String,
    pub summary: PriceCollectionDay,
    pub rows: Vec<PriceCollectionRow>,
    pub daily: Vec<PriceCollectionDay>,
}
