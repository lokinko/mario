//! Complete CSI 300 scope from Eastmoney, with the official workbook as fallback.
use crate::{
    models::market::{
        canonical_symbol,
        monitor::{PriceMember, PriceUniverse, EASTMONEY_UNIVERSE_URL, UNIVERSE_URL},
    },
    ports::market::PriceUniverseProvider,
    AppError, AppResult,
};
use calamine::{Data, Reader, Xls};
use std::{io::Cursor, time::Duration};

pub(crate) struct Csi300Source {
    client: reqwest::Client,
}
impl Csi300Source {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(8))
                .user_agent("mario/public-prices")
                .build()?,
        })
    }
}
fn parse_eastmoney(payload: &serde_json::Value) -> AppResult<PriceUniverse> {
    let result = &payload["result"];
    let rows = result["data"].as_array().ok_or_else(bad)?;
    if payload["success"] != true
        || payload["code"] != 0
        || result["pages"] != 1
        || result["count"] != 300
        || rows.len() != 300
    {
        return Err(bad());
    }
    let mut as_of = None;
    let mut members = Vec::new();
    for row in rows {
        let date = row["MAXTRADEDATE"].as_str().ok_or_else(bad)?;
        let day = chrono::NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S")
            .map_err(|_| bad())?
            .date();
        if row["TYPE"] != "1" || as_of.is_some_and(|old| old != day) {
            return Err(bad());
        }
        as_of = Some(day);
        let code = row["SECURITY_CODE"].as_str().ok_or_else(bad)?;
        let secucode = row["SECUCODE"].as_str().ok_or_else(bad)?;
        if code.len() != 6
            || !code.bytes().all(|b| b.is_ascii_digit())
            || ![format!("{code}.SH"), format!("{code}.SZ")].contains(&secucode.to_owned())
        {
            return Err(bad());
        }
        members.push(PriceMember {
            symbol: canonical_symbol(secucode)?,
            name: row["SECURITY_NAME_ABBR"]
                .as_str()
                .ok_or_else(bad)?
                .trim()
                .into(),
        });
    }
    members.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    let scope = PriceUniverse {
        as_of: as_of.ok_or_else(bad)?,
        fetched_at: chrono::Utc::now().timestamp(),
        source_url: EASTMONEY_UNIVERSE_URL.into(),
        members,
    };
    scope.validate()?;
    Ok(scope)
}
fn bad() -> AppError {
    AppError::MarketData("沪深300成分表格式、日期或证券身份不完整".into())
}
fn parse_rows(rows: &[Vec<String>]) -> AppResult<PriceUniverse> {
    let header = rows.first().ok_or_else(bad)?;
    let column = |label: &str| header.iter().position(|s| s == label).ok_or_else(bad);
    let date = column("日期Date")?;
    let index = column("指数代码 Index Code")?;
    let code = column("成份券代码Constituent Code")?;
    let name = column("成份券名称Constituent Name")?;
    let exchange = column("交易所Exchange")?;
    let mut as_of = None;
    let mut members = Vec::new();
    for row in &rows[1..] {
        let get = |i| row.get(i).map(String::as_str).ok_or_else(bad);
        if get(index)? != "000300" {
            return Err(bad());
        }
        let day = chrono::NaiveDate::parse_from_str(get(date)?, "%Y%m%d").map_err(|_| bad())?;
        if as_of.is_some_and(|old| old != day) {
            return Err(bad());
        }
        as_of = Some(day);
        let digits = get(code)?;
        if digits.len() != 6 || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        let suffix = match get(exchange)? {
            "上海证券交易所" => "SS",
            "深圳证券交易所" => "SZ",
            _ => return Err(bad()),
        };
        members.push(PriceMember {
            symbol: canonical_symbol(&format!("{digits}.{suffix}"))?,
            name: get(name)?.into(),
        });
    }
    members.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    let universe = PriceUniverse {
        as_of: as_of.ok_or_else(bad)?,
        fetched_at: chrono::Utc::now().timestamp(),
        source_url: UNIVERSE_URL.into(),
        members,
    };
    universe.validate()?;
    Ok(universe)
}
fn parse(bytes: Vec<u8>) -> AppResult<PriceUniverse> {
    let mut workbook = Xls::new(Cursor::new(bytes)).map_err(|_| bad())?;
    let sheet = workbook
        .worksheet_range_at(0)
        .ok_or_else(bad)?
        .map_err(|_| bad())?;
    let rows = sheet
        .rows()
        .map(|r| {
            r.iter()
                .map(|v| match v {
                    Data::String(s) => s.trim().to_owned(),
                    _ => v.to_string(),
                })
                .collect()
        })
        .collect::<Vec<_>>();
    parse_rows(&rows)
}
#[async_trait::async_trait]
impl PriceUniverseProvider for Csi300Source {
    async fn constituents(&self) -> AppResult<PriceUniverse> {
        let primary = async {
            let response = self
                .client
                .get(EASTMONEY_UNIVERSE_URL)
                .header("Referer", "https://data.eastmoney.com/other/index/")
                .send()
                .await?
                .error_for_status()?;
            parse_eastmoney(&super::read_public_json(response).await?)
        }
        .await;
        let error = match primary {
            Ok(scope) => return Ok(scope),
            Err(error) => {
                tracing::warn!(error=?error, "Eastmoney CSI300 scope failed; trying official workbook");
                error
            }
        };
        let fallback = async {
            let mut response = self
                .client
                .get(UNIVERSE_URL)
                .send()
                .await?
                .error_for_status()?;
            parse(super::read_public_bytes(&mut response).await?)
        }
        .await;
        fallback.map_err(|fallback| {
            AppError::MarketData(format!("东方财富成分表：{error}；中证成分表：{fallback}"))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eastmoney_scope_requires_one_complete_consistent_index_snapshot() {
        let rows = (1..=300).map(|i| serde_json::json!({
            "SECUCODE":format!("{i:06}.SZ"), "SECURITY_CODE":format!("{i:06}"),
            "SECURITY_NAME_ABBR":format!("公司{i}"), "TYPE":"1", "MAXTRADEDATE":"2026-11-11 00:00:00"
        })).collect::<Vec<_>>();
        let payload = serde_json::json!({"success":true,"code":0,"result":{"pages":1,"count":300,"data":rows}});
        let scope = parse_eastmoney(&payload).unwrap();
        assert_eq!(scope.members.len(), 300);
        assert_eq!(scope.source_url, EASTMONEY_UNIVERSE_URL);
        for (path, value) in [
            ("TYPE", serde_json::json!("2")),
            ("SECUCODE", serde_json::json!("000002.SH")),
            ("SECURITY_CODE", serde_json::json!("000003")),
            ("MAXTRADEDATE", serde_json::json!("2026-11-10 00:00:00")),
            ("SECURITY_NAME_ABBR", serde_json::json!("")),
        ] {
            let mut invalid = payload.clone();
            invalid["result"]["data"][1][path] = value;
            assert!(parse_eastmoney(&invalid).is_err(), "{path}");
        }
        let mut invalid = payload.clone();
        invalid["result"]["data"][1] = invalid["result"]["data"][0].clone();
        assert!(parse_eastmoney(&invalid).is_err());
        for field in ["pages", "count"] {
            let mut invalid = payload.clone();
            invalid["result"][field] = serde_json::json!(2);
            assert!(parse_eastmoney(&invalid).is_err());
        }
        let mut invalid = payload;
        invalid["result"]["data"].as_array_mut().unwrap().pop();
        assert!(parse_eastmoney(&invalid).is_err());
    }
    #[test]
    fn validates_entire_scope_before_adopting_it() {
        let mut rows = vec![vec![
            "日期Date",
            "指数代码 Index Code",
            "成份券代码Constituent Code",
            "成份券名称Constituent Name",
            "交易所Exchange",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()];
        for i in 1..=300 {
            rows.push(vec![
                "20261111".into(),
                "000300".into(),
                format!("{i:06}"),
                format!("公司{i}"),
                "深圳证券交易所".into(),
            ]);
        }
        assert_eq!(parse_rows(&rows).unwrap().members.len(), 300);
        let mut invalid = rows.clone();
        invalid.pop();
        assert!(parse_rows(&invalid).is_err());
        invalid = rows.clone();
        invalid[2] = invalid[1].clone();
        assert!(parse_rows(&invalid).is_err());
        invalid = rows.clone();
        invalid[2][1] = "000905".into();
        assert!(parse_rows(&invalid).is_err());
        invalid = rows.clone();
        invalid[2][4] = "上海证券交易所".into();
        invalid[2][2] = "300001".into();
        assert!(parse_rows(&invalid).is_err());
        invalid = rows.clone();
        invalid[2][0] = "20261112".into();
        assert!(parse_rows(&invalid).is_err());
        assert!(parse(b"not an Excel workbook".to_vec()).is_err());
    }
    #[tokio::test]
    #[ignore = "requires the official public CSI workbook endpoint"]
    async fn live_csi300_constituents() {
        let scope = Csi300Source::new().unwrap().constituents().await.unwrap();
        println!(
            "CSI300 as of {}: {} securities; source {}",
            scope.as_of,
            scope.members.len(),
            scope.source_url
        );
    }
}
