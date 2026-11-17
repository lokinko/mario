//! Public listed-security identities and official source metadata, without user selection.
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecuritySources {
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources_checked_on: Option<String>,
}

/// Scope used by A-share company lookup and implemented-distribution sources.
pub(crate) fn normalize_a_share_identity(symbol: &str, name: &str) -> AppResult<(String, String)> {
    let symbol = symbol.trim().to_uppercase().replace(".SH", ".SS");
    let name = name.trim().to_owned();
    let valid = symbol.len() == 9
        && symbol.as_bytes()[..6].iter().all(|c| c.is_ascii_digit())
        && ((symbol.ends_with(".SS") && (symbol.starts_with("60") || symbol.starts_with("68")))
            || (symbol.ends_with(".SZ") && (symbol.starts_with("00") || symbol.starts_with("30"))));
    if !valid || name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::Validation(
            "请填写沪深 A 股代码（如 600036.SS）和公司名称".into(),
        ));
    }
    Ok((symbol, name))
}
