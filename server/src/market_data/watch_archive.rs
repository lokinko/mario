//! Public, versioned bootstrap data. No research CLI or account storage at runtime.
use crate::{
    models::watch::{trailing_dividends, DividendHistory, WatchInput, MIN_YIELD_PCT},
    AppError, AppResult,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Archive {
    format_version: u32,
    pub as_of: NaiveDate,
    source_manifest_sha256: String,
    pub stocks: Vec<Stock>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Stock {
    pub symbol: String,
    name: String,
    status: String,
    pub history: Option<DividendHistory>,
    raw_files: BTreeMap<String, String>,
    issue: Option<String>,
}

pub(super) fn load() -> AppResult<Archive> {
    parse(include_str!("../../data/watch-dividends-20261104.json"))
}

fn parse(contents: &str) -> AppResult<Archive> {
    let archive: Archive = serde_json::from_str(contents)?;
    let invalid = || AppError::Validation("内置分红资料包无效".into());
    if archive.format_version != 1 || !hash(&archive.source_manifest_sha256) {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for stock in &archive.stocks {
        let canonical = WatchInput {
            symbol: stock.symbol.clone(),
            name: stock.name.clone(),
            thesis: String::new(),
        }
        .normalize()?;
        if canonical.symbol != stock.symbol
            || !seen.insert(&stock.symbol)
            || stock.raw_files.values().any(|v| !hash(v))
        {
            return Err(invalid());
        }
        match &stock.history {
            Some(history) => {
                history.validate()?;
                if stock.status != "ready"
                    || stock.issue.is_some()
                    || history.basis.as_of != archive.as_of
                    || stock.raw_files.is_empty()
                {
                    return Err(invalid());
                }
            }
            None if [
                "source_no_records",
                "source_unavailable",
                "source_incomplete",
            ]
            .contains(&stock.status.as_str())
                && stock.issue.is_some() => {}
            None => return Err(invalid()),
        }
    }
    Ok(archive)
}

fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

/// A public-data export only: never opens the account database or invents observations.
pub(crate) fn export(path: &Path) -> AppResult<()> {
    let archive = load()?;
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    {
        let mut csv = csv::Writer::from_writer(&mut bytes);
        let csv_error = |e: csv::Error| AppError::Validation(format!("分红清单导出失败：{e}"));
        csv.write_record([
            "证券代码",
            "公司",
            "资料状态",
            "分红截止日",
            "TTM每股税前分红（元）",
            "满足5%股息率的最高参考价（元）",
            "历史事件数",
            "待核查原因",
            "来源",
        ])
        .map_err(csv_error)?;
        for stock in archive.stocks {
            let total = stock
                .history
                .as_ref()
                .and_then(|h| trailing_dividends(&h.events, archive.as_of));
            let threshold = total.and_then(|v| {
                v.checked_mul(Decimal::ONE_HUNDRED)?
                    .checked_div(Decimal::from_str_exact(&MIN_YIELD_PCT.to_string()).ok()?)
            });
            let status = match stock.status.as_str() {
                "ready" => "可用",
                "source_no_records" => "来源无记录",
                "source_incomplete" => "来源待核查",
                _ => "来源不可用",
            };
            csv.write_record([
                stock.symbol,
                stock.name,
                status.into(),
                archive.as_of.to_string(),
                total.map(|v| v.normalize().to_string()).unwrap_or_default(),
                threshold
                    .map(|v| v.normalize().to_string())
                    .unwrap_or_default(),
                stock
                    .history
                    .as_ref()
                    .map(|h| h.events.len().to_string())
                    .unwrap_or_default(),
                stock.issue.unwrap_or_default(),
                stock
                    .history
                    .map(|h| h.sources.join("\n"))
                    .unwrap_or_default(),
            ])
            .map_err(csv_error)?;
        }
        csv.flush()?;
    }
    // Do not overwrite an existing hand-edited reference sheet.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_originals_are_valid_and_unknown_sources_never_become_zero() {
        let archive = load().unwrap();
        assert_eq!(archive.stocks.len(), 210);
        assert_eq!(
            archive
                .stocks
                .iter()
                .filter(|s| s.history.is_some())
                .count(),
            202
        );
        let bank = archive
            .stocks
            .iter()
            .find(|s| s.symbol == "600036.SS")
            .unwrap()
            .history
            .as_ref()
            .unwrap();
        assert_eq!(
            trailing_dividends(&bank.events, archive.as_of)
                .unwrap()
                .normalize()
                .to_string(),
            "2.016"
        );
        assert!(archive
            .stocks
            .iter()
            .find(|s| s.symbol == "002008.SZ")
            .unwrap()
            .history
            .is_none());
        assert_eq!(
            archive
                .stocks
                .iter()
                .filter(|s| s.status == "source_no_records" && s.history.is_none())
                .count(),
            7
        );
        // Old vendor report-period labels may end after the actual distribution;
        // the ex-date determines payment timing, never the report-period label.
        let glass = archive
            .stocks
            .iter()
            .find(|s| s.symbol == "600660.SS")
            .unwrap()
            .history
            .as_ref()
            .unwrap();
        assert!(glass
            .events
            .iter()
            .any(|e| e.report_date.is_some_and(|d| d > e.ex_date)));
    }
    #[test]
    fn export_keeps_unknowns_blank_and_refuses_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("watch.csv");
        export(&path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let mut reader = csv::Reader::from_reader(&data[3..]);
        let rows: Vec<_> = reader.records().collect::<Result<_, _>>().unwrap();
        assert_eq!(rows.len(), 210);
        let unknown = rows.iter().find(|r| &r[0] == "002008.SZ").unwrap();
        assert_eq!(&unknown[4], "");
        let bank = rows.iter().find(|r| &r[0] == "600036.SS").unwrap();
        assert_eq!(&bank[5], "40.32");
        assert!(export(&path).is_err());
    }
}
