//! Public, versioned bootstrap data. No research CLI or account storage at runtime.
use crate::{
    models::market::{
        dividend_archive::DividendArchive, dividends::DividendHistory,
        securities::normalize_a_share_identity,
    },
    AppError, AppResult,
};

pub(crate) fn load() -> AppResult<DividendArchive> {
    let mut archive = parse(include_str!("../../data/watch-dividends-20261104.json"))?;
    let update = parse(include_str!("../../data/watch-dividends-20261107.json"))?;
    for stock in update.stocks {
        let existing = archive
            .stocks
            .iter_mut()
            .find(|s| s.symbol == stock.symbol)
            .ok_or_else(|| AppError::Validation("分红补充包包含范围外证券".into()))?;
        if stock.history.as_ref().is_some_and(|new| {
            existing
                .history
                .as_ref()
                .is_none_or(|old| new.is_newer_than(old))
        }) {
            *existing = stock;
        }
    }
    Ok(archive)
}

pub(crate) fn initial_histories() -> AppResult<Vec<(String, DividendHistory)>> {
    Ok(load()?
        .stocks
        .into_iter()
        .filter_map(|s| s.history.map(|h| (s.symbol, h)))
        .collect())
}

fn parse(contents: &str) -> AppResult<DividendArchive> {
    let archive: DividendArchive = serde_json::from_str(contents)?;
    let invalid = || AppError::Validation("内置分红资料包无效".into());
    if archive.format_version != 1 || !hash(&archive.source_manifest_sha256) {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for stock in &archive.stocks {
        let (canonical_symbol, _) = normalize_a_share_identity(&stock.symbol, &stock.name)?;
        if canonical_symbol != stock.symbol
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::market::dividends::trailing_dividends;
    use chrono::NaiveDate;
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
        let update = parse(include_str!("../../data/watch-dividends-20261107.json")).unwrap();
        assert_eq!(update.stocks.len(), 21);
        let start = NaiveDate::from_ymd_opt(2023, 11, 7).unwrap();
        assert_eq!(
            update
                .stocks
                .iter()
                .map(|s| s
                    .history
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .filter(|e| e.ex_date >= start)
                    .count())
                .sum::<usize>(),
            105
        );
        let bank = archive
            .stocks
            .iter()
            .find(|s| s.symbol == "600036.SS")
            .unwrap()
            .history
            .as_ref()
            .unwrap();
        assert_eq!(bank.basis.as_of.to_string(), "2026-11-07");
        assert_eq!(
            archive
                .stocks
                .iter()
                .find(|s| s.symbol == "600000.SS")
                .unwrap()
                .history
                .as_ref()
                .unwrap()
                .basis
                .as_of
                .to_string(),
            "2026-11-04"
        );
        assert_eq!(
            trailing_dividends(&bank.events, bank.basis.as_of)
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
}
