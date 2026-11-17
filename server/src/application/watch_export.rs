//! User reference-price CSV projection over a public archive. No account storage or source I/O.
use crate::{
    models::{
        market::{dividend_archive::DividendArchive, dividends::trailing_dividends},
        watch::MIN_YIELD_PCT,
    },
    AppError, AppResult,
};
use rust_decimal::Decimal;
pub(crate) fn render(archive: &DividendArchive) -> AppResult<Vec<u8>> {
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
        for stock in &archive.stocks {
            let total = stock
                .history
                .as_ref()
                .and_then(|h| trailing_dividends(&h.events, h.basis.as_of));
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
                stock.symbol.clone(),
                stock.name.clone(),
                status.into(),
                stock
                    .history
                    .as_ref()
                    .map(|h| h.basis.as_of.to_string())
                    .unwrap_or_else(|| archive.as_of.to_string()),
                total.map(|v| v.normalize().to_string()).unwrap_or_default(),
                threshold
                    .map(|v| v.normalize().to_string())
                    .unwrap_or_default(),
                stock
                    .history
                    .as_ref()
                    .map(|h| h.events.len().to_string())
                    .unwrap_or_default(),
                stock.issue.clone().unwrap_or_default(),
                stock
                    .history
                    .as_ref()
                    .map(|h| h.sources.join("\n"))
                    .unwrap_or_default(),
            ])
            .map_err(csv_error)?;
        }
        csv.flush()?;
    }
    Ok(bytes)
}
