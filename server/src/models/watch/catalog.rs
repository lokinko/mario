//! Public catalogue contracts; no account metadata is stored in public snapshots.
use super::{dividend_prices, rules::WatchRules, DividendPrice, Observation, WatchInput};
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
        if !matches!(catalog.version, 1..=4)
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

    /// Apply each explicitly authorized catalogue release once, preserving later removals.
    pub fn upgrade(&mut self, seed: &Catalog) -> AppResult<bool> {
        if self.version == seed.version {
            return Ok(false);
        }
        if self.version == 1 {
            self.upgrade_v1(seed)?;
        }
        if self.version == 2 && seed.version >= 3 {
            let requested: BTreeSet<String> = serde_json::from_str(include_str!(
                "../../../data/watch-catalog-screened-additions-20261107.json"
            ))?;
            let mut existing: BTreeSet<String> = self
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .map(|c| c.symbol.clone())
                .collect();
            for template in &seed.industries {
                for company in &template.companies {
                    if requested.contains(&company.symbol)
                        && existing.insert(company.symbol.clone())
                    {
                        let target = self
                            .industries
                            .iter_mut()
                            .find(|i| i.id == template.id)
                            .ok_or_else(|| {
                                AppError::Validation("旧目录领域不完整，请保留原数据核对".into())
                            })?;
                        target.companies.push(company.clone());
                    }
                }
            }
            self.version = 3;
        }
        if self.version == 3 && seed.version == 4 {
            let requested: BTreeSet<String> = serde_json::from_str(include_str!(
                "../../../data/watch-catalog-broker-additions-20261107.json"
            ))?;
            let mut existing: BTreeSet<String> = self
                .industries
                .iter()
                .flat_map(|i| &i.companies)
                .map(|c| c.symbol.clone())
                .collect();
            let template = seed.industry("brokers")?;
            if !self.industries.iter().any(|i| i.id == template.id) {
                self.industries.push(Industry {
                    id: template.id.clone(),
                    name: template.name.clone(),
                    companies: Vec::new(),
                });
            }
            let target = self
                .industries
                .iter_mut()
                .find(|i| i.id == template.id)
                .unwrap();
            for company in &template.companies {
                if requested.contains(&company.symbol) && existing.insert(company.symbol.clone()) {
                    target.companies.push(company.clone());
                }
            }
            self.version = 4;
        }
        if self.version != seed.version {
            return Err(AppError::Validation("目录升级版本不受支持".into()));
        }
        self.verified_on.clone_from(&seed.verified_on);
        self.validate()?;
        Ok(true)
    }

    /// Minimal adapter for pre-domain-reorganization data and whole-site backups.
    fn upgrade_v1(&mut self, seed: &Catalog) -> AppResult<()> {
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
        self.version = 2;
        self.validate()?;
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
    pub fn apply_rules(&mut self, rules: &WatchRules) {
        if let Some(observation) = &mut self.snapshot.latest {
            observation.apply_rules(rules);
        }
        self.dividend_prices = dividend_prices(self.snapshot.latest.as_ref());
    }

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
    pub rules: WatchRules,
}

impl CatalogBoard {
    /// Project the shared market facts using this authenticated account's preferences.
    pub fn apply_rules(&mut self, rules: WatchRules) {
        self.rules = rules;
        for row in &mut self.rows {
            row.apply_rules(&rules);
        }
    }

    pub fn new(catalog: Catalog, rows: Vec<CatalogRow>, revision: i64) -> Self {
        Self {
            revision,
            can_edit: false,
            catalog,
            rows,
            rules: WatchRules::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::watch::{BAND_DISTANCE_PCT, MIN_YIELD_PCT};

    #[test]
    fn broker_screening_reproduces_implemented_ttm_and_strict_three_percent_selection() {
        use crate::models::watch::{trailing_dividends, DividendEvent};
        use rust_decimal::Decimal;
        let audit: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/audits/watch-catalog-brokers-20261107.json"
        ))
        .unwrap();
        let requested: BTreeSet<String> = serde_json::from_str(include_str!(
            "../../../data/watch-catalog-broker-additions-20261107.json"
        ))
        .unwrap();
        let day = chrono::NaiveDate::parse_from_str(audit["priceOn"].as_str().unwrap(), "%Y-%m-%d")
            .unwrap();
        let mut selected = BTreeSet::new();
        for row in audit["selected"]
            .as_array()
            .unwrap()
            .iter()
            .chain(audit["excluded"].as_array().unwrap())
        {
            let events: Vec<DividendEvent> =
                serde_json::from_value(row["implementedEvents"].clone()).unwrap();
            let cash = trailing_dividends(&events, day).unwrap();
            assert_eq!(
                cash,
                Decimal::from_str_exact(row["ttmDividendPerShare"].as_str().unwrap()).unwrap()
            );
            let price = Decimal::from_str_exact(row["price"].as_str().unwrap()).unwrap();
            let passes = cash * Decimal::ONE_HUNDRED > price * Decimal::from(3);
            assert_eq!(passes, row["passesThreePct"].as_bool().unwrap());
            assert_eq!(
                (cash * Decimal::ONE_HUNDRED / price).round_dp(4),
                Decimal::from_str_exact(row["yieldPct"].as_str().unwrap())
                    .unwrap()
                    .round_dp(4)
            );
            if passes {
                selected.insert(row["symbol"].as_str().unwrap().to_string());
            }
        }
        assert_eq!(selected, requested);
        let catalog = Catalog::bundled().unwrap();
        assert_eq!(
            catalog
                .industry("brokers")
                .unwrap()
                .companies
                .iter()
                .map(|c| c.symbol.clone())
                .collect::<BTreeSet<_>>(),
            requested
        );
        assert_eq!(MIN_YIELD_PCT, 5.0);
        assert_eq!(BAND_DISTANCE_PCT, 2.0);
    }

    #[test]
    fn screened_additions_reproduce_the_dated_ttm_above_four_without_changing_watch_rules() {
        use crate::models::watch::{trailing_dividends, DividendEvent};
        use rust_decimal::Decimal;
        let audit: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/audits/watch-catalog-screening-20261107.json"
        ))
        .unwrap();
        let selected = audit["selected"].as_array().unwrap();
        let requested: BTreeSet<String> = serde_json::from_str(include_str!(
            "../../../data/watch-catalog-screened-additions-20261107.json"
        ))
        .unwrap();
        assert_eq!(selected.len(), 8);
        let mut found = BTreeSet::new();
        let day = chrono::NaiveDate::parse_from_str(audit["priceOn"].as_str().unwrap(), "%Y-%m-%d")
            .unwrap();
        for row in selected {
            let events: Vec<DividendEvent> =
                serde_json::from_value(row["implementedEventsSince2023"].clone()).unwrap();
            let cash = trailing_dividends(&events, day).unwrap();
            assert_eq!(
                cash,
                Decimal::from_str_exact(row["ttmDividendPerShare"].as_str().unwrap()).unwrap()
            );
            let price = Decimal::from_str_exact(row["price"].as_str().unwrap()).unwrap();
            let yield_pct = cash * Decimal::ONE_HUNDRED / price;
            assert!(yield_pct > Decimal::from(4));
            assert_eq!(
                yield_pct.round_dp(4),
                Decimal::from_str_exact(row["yieldPct"].as_str().unwrap())
                    .unwrap()
                    .round_dp(4)
            );
            found.insert(row["symbol"].as_str().unwrap().to_string());
        }
        assert_eq!(found, requested);
        assert_eq!(MIN_YIELD_PCT, 5.0);
        assert_eq!(BAND_DISTANCE_PCT, 2.0);
    }

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
                "消费",
                "券商"
            ]
        );
        assert_eq!(
            catalog
                .industries
                .iter()
                .map(|i| i.companies.len())
                .sum::<usize>(),
            61
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
