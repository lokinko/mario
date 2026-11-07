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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Company {
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources_checked_on: Option<String>,
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
        if !matches!(catalog.version, 1 | 2)
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
                    || (company.sources_checked_on.is_some() && company.sources.len() < 2)
                    || company.sources_checked_on.as_ref().is_some_and(|date| {
                        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
                    })
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

    /// One-time migration for the explicitly requested November company additions.
    /// Existing custom entries and prior removals outside this request remain intact.
    pub fn upgrade(&mut self, seed: &Catalog) -> AppResult<bool> {
        if self.version == seed.version {
            return Ok(false);
        }
        let requested: BTreeSet<String> = serde_json::from_str(include_str!(
            "../../../data/watch-catalog-additions-20261107.json"
        ))?;
        let mut groups = std::mem::take(&mut self.industries);
        for group in &mut groups {
            match group.id.as_str() {
                "coal" | "oil" => {
                    group.id = "energy".into();
                    group.name = "能源".into();
                }
                "highways" => {
                    group.id = "transport".into();
                    group.name = "运输".into();
                }
                "appliances" => group.name = "白色家电".into(),
                _ => {}
            }
        }
        for template in &seed.industries {
            let mut companies = Vec::new();
            for group in &mut groups {
                let mut retained = Vec::new();
                for company in group.companies.drain(..) {
                    let target = seed
                        .industries
                        .iter()
                        .find(|i| i.companies.iter().any(|c| c.symbol == company.symbol));
                    let belongs = if requested.contains(&company.symbol) {
                        target.is_some_and(|i| i.id == template.id)
                    } else {
                        group.id == template.id
                    };
                    if belongs {
                        companies.push(company);
                    } else {
                        retained.push(company);
                    }
                }
                group.companies = retained;
            }
            for company in &template.companies {
                if requested.contains(&company.symbol)
                    && !companies.iter().any(|c| c.symbol == company.symbol)
                {
                    companies.push(company.clone());
                }
            }
            self.industries.push(Industry {
                id: template.id.clone(),
                name: template.name.clone(),
                companies,
            });
        }
        for group in groups {
            if !group.companies.is_empty() {
                return Err(AppError::Validation(
                    "旧公共目录包含无法迁移的领域，请保留原数据核对".into(),
                ));
            }
        }
        self.version = seed.version;
        self.verified_on.clone_from(&seed.verified_on);
        self.validate()?;
        Ok(true)
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
                "能源",
                "电力",
                "通信运营",
                "运输",
                "白酒",
                "白色家电",
                "保险",
                "中药",
                "消费"
            ]
        );
        assert_eq!(
            catalog
                .industries
                .iter()
                .map(|i| i.companies.len())
                .sum::<usize>(),
            43
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
        value["industries"][0]["companies"][0]
            .as_object_mut()
            .unwrap()
            .remove("sourcesCheckedOn");
        assert!(Catalog::parse(&value.to_string()).is_ok());
        value["industries"][0]["companies"][0]["sourcesCheckedOn"] = "invalid".into();
        assert!(Catalog::parse(&value.to_string()).is_err());
    }
}
