//! Versioned public dividend archive; user watch preferences are not part of this data.
use super::dividends::DividendHistory;
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DividendArchive {
    pub format_version: u32,
    pub as_of: NaiveDate,
    pub source_manifest_sha256: String,
    pub stocks: Vec<ArchivedDividendStock>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchivedDividendStock {
    pub symbol: String,
    pub name: String,
    pub status: String,
    pub history: Option<DividendHistory>,
    pub raw_files: BTreeMap<String, String>,
    pub issue: Option<String>,
}
