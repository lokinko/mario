//! Public catalogue contracts; no account metadata is stored in public snapshots.
use super::{
    dividend_prices, DividendPrice, Observation, WatchInput, BAND_DISTANCE_PCT, MIN_YIELD_PCT,
};
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub verified_on: String,
    pub industries: Vec<Industry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Industry {
    pub id: String,
    pub name: String,
    pub companies: Vec<Company>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Company {
    pub symbol: String,
    pub name: String,
    pub sources: Vec<String>,
}

impl Catalog {
    pub fn bundled() -> AppResult<Self> {
        Self::parse(include_str!("../../../data/watch-catalog.json"))
    }

    pub fn parse(json: &str) -> AppResult<Self> {
        let catalog: Self = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> AppResult<()> {
        let catalog = self;
        let mut industries = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        if catalog.version != 1
            || chrono::NaiveDate::parse_from_str(&catalog.verified_on, "%Y-%m-%d").is_err()
            || catalog.industries.is_empty()
        {
            return Err(AppError::Validation("行业目录版本或核查日期无效".into()));
        }
        for industry in &catalog.industries {
            if industry.id.is_empty()
                || !industry
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c == b'-')
                || !industries.insert(&industry.id)
                || industry.name.trim().is_empty()
            {
                return Err(AppError::Validation("行业目录分组无效或重复".into()));
            }
            for company in &industry.companies {
                let input = WatchInput {
                    symbol: company.symbol.clone(),
                    name: company.name.clone(),
                    thesis: String::new(),
                }
                .normalize()?;
                if input.symbol != company.symbol
                    || input.name != company.name
                    || !symbols.insert(&company.symbol)
                    || company.sources.len() < 2
                    || company.sources.iter().any(|s| {
                        !s.starts_with("https://")
                            || s.len() <= 8
                            || s.chars().any(char::is_whitespace)
                    })
                {
                    return Err(AppError::Validation(
                        "行业目录公司身份、来源或唯一性无效".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn industry(&self, id: &str) -> AppResult<&Industry> {
        self.industries
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| AppError::Validation("行业不存在".into()))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub latest: Option<Observation>,
    pub checked_at: Option<String>,
    pub checked_for: Option<String>,
    pub refresh_failed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRow {
    pub company: Company,
    pub industry_id: String,
    #[serde(flatten)]
    pub snapshot: CatalogSnapshot,
    pub dividend_prices: Vec<DividendPrice>,
}

impl CatalogRow {
    pub fn new(company: Company, industry_id: String, snapshot: CatalogSnapshot) -> Self {
        let dividend_prices = dividend_prices(snapshot.latest.as_ref());
        Self {
            company,
            industry_id,
            snapshot,
            dividend_prices,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBoard {
    pub revision: i64,
    pub can_edit: bool,
    pub catalog: Catalog,
    pub rows: Vec<CatalogRow>,
    pub min_yield_pct: f64,
    pub band_distance_pct: f64,
}

impl CatalogBoard {
    pub fn new(catalog: Catalog, rows: Vec<CatalogRow>, revision: i64) -> Self {
        Self {
            revision,
            can_edit: false,
            catalog,
            rows,
            min_yield_pct: MIN_YIELD_PCT,
            band_distance_pct: BAND_DISTANCE_PCT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_unique_and_contains_the_agreed_industries_and_companies() {
        let catalog = Catalog::bundled().unwrap();
        assert_eq!(
            catalog
                .industries
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            [
                "银行",
                "煤炭",
                "石油石化",
                "电力",
                "通信运营",
                "高速公路",
                "白酒",
                "家电"
            ]
        );
        assert_eq!(
            catalog
                .industries
                .iter()
                .map(|i| i.companies.len())
                .sum::<usize>(),
            21
        );
        assert!(catalog.industry("unknown").is_err());
        let mut value = serde_json::to_value(&catalog).unwrap();
        value["industries"][1]["companies"][0] = value["industries"][0]["companies"][0].clone();
        assert!(Catalog::parse(&value.to_string()).is_err());
        value = serde_json::to_value(&catalog).unwrap();
        value["industries"][0]["companies"][0]["symbol"] = "600036.SH".into();
        assert!(Catalog::parse(&value.to_string()).is_err());
        value = serde_json::to_value(&catalog).unwrap();
        value["industries"][0]["companies"][0]["sources"] = serde_json::json!([]);
        assert!(Catalog::parse(&value.to_string()).is_err());
    }
}
