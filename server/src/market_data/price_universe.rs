//! Official CSI 300 constituent workbook; never accesses storage or private watch lists.
use crate::{
    models::market::{
        canonical_symbol,
        monitor::{PriceMember, PriceUniverse, UNIVERSE_URL},
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
                .timeout(Duration::from_secs(20))
                .user_agent("mario/public-prices")
                .build()?,
        })
    }
}
fn bad() -> AppError {
    AppError::MarketData("中证沪深300成分表格式、日期或证券身份不完整".into())
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
        let mut response = self
            .client
            .get(UNIVERSE_URL)
            .send()
            .await?
            .error_for_status()?;
        parse(super::read_public_bytes(&mut response).await?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
            "CSI300 as of {}: {} securities",
            scope.as_of,
            scope.members.len()
        );
    }
}
