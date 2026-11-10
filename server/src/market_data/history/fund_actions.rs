//! The fund archive is distinct from stock dividends; a missing table is not "no splits".
use super::{bad, decimal, AppResult, BTreeMap, Decimal, NaiveDate, SplitEvent};
use chrono::Datelike;
use quick_xml::{events::Event, Reader};

pub(super) fn parse(html: &str, code: &str, today: NaiveDate) -> AppResult<Vec<SplitEvent>> {
    let title = html
        .split_once("<title>")
        .and_then(|(_, s)| s.split_once("</title>"))
        .map(|(title, _)| title)
        .ok_or_else(bad)?;
    if !title.contains(&format!("({code})基金分红送配")) {
        return Err(bad());
    }
    let (_, section) = html.split_once("拆分详情</label>").ok_or_else(bad)?;
    let start = section.find("<table").ok_or_else(bad)?;
    let table = &section[start..];
    let end = table.find("</table>").ok_or_else(bad)? + "</table>".len();
    let mut reader = Reader::from_str(&table[..end]);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell: Option<String> = None;
    loop {
        match reader.read_event().map_err(|_| bad())? {
            Event::Start(e) => match e.name().as_ref() {
                b"table" | b"thead" | b"tbody" | b"tr" => {}
                b"td" | b"th" if cell.is_none() => cell = Some(String::new()),
                _ => return Err(bad()),
            },
            Event::Text(e) => {
                let text = e.decode().map_err(|_| bad())?;
                if let Some(cell) = &mut cell {
                    cell.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(bad());
                }
            }
            Event::End(e) => match e.name().as_ref() {
                b"td" | b"th" => row.push(cell.take().ok_or_else(bad)?.trim().to_owned()),
                b"tr" => rows.push(std::mem::take(&mut row)),
                b"table" | b"thead" | b"tbody" => {}
                _ => return Err(bad()),
            },
            Event::Eof => break,
            _ => return Err(bad()),
        }
    }
    if rows
        .first()
        .map(|r| r.iter().map(String::as_str).collect::<Vec<_>>())
        != Some(vec!["年份", "拆分折算日", "拆分类型", "拆分折算比例"])
        || rows.len() < 2
    {
        return Err(bad());
    }
    if rows.len() == 2
        && rows[1].len() == 1
        && matches!(rows[1][0].as_str(), "暂无拆分信息!" | "暂无拆分信息！")
    {
        return Ok(Vec::new());
    }
    let mut splits = BTreeMap::new();
    for row in rows.into_iter().skip(1) {
        if row.len() != 4 || row[2].is_empty() {
            return Err(bad());
        }
        let day = row[1].parse::<NaiveDate>().map_err(|_| bad())?;
        if row[0] != format!("{}年", day.year()) || row[1] != day.to_string() {
            return Err(bad());
        }
        let (before, after) = row[3].split_once(':').ok_or_else(bad)?;
        let (before, after) = (decimal(before)?, decimal(after)?);
        if before <= Decimal::ZERO || after <= Decimal::ZERO {
            return Err(bad());
        }
        let split = SplitEvent {
            effective_on: day.to_string(),
            numerator: after.normalize().to_string(),
            denominator: before.normalize().to_string(),
        };
        if splits.insert(day, split).is_some() {
            return Err(bad());
        }
    }
    Ok(splits
        .into_iter()
        .filter(|(day, _)| *day <= today)
        .map(|(_, split)| split)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(rows: &str) -> String {
        format!("<title>黄金ETF华安(518880)基金分红送配 _ 基金档案</title><label>拆分详情</label><table><thead><tr><th>年份</th><th>拆分折算日</th><th>拆分类型</th><th>拆分折算比例</th></tr></thead><tbody>{rows}</tbody></table>")
    }
    #[test]
    fn archive_keeps_pre_window_splits_and_requires_explicit_empty_evidence() {
        let today = "2026-11-10".parse().unwrap();
        let row = "<tr><td>2013年</td><td>2013-07-24</td><td>份额折算</td><td>1:0.3776</td></tr>";
        let future = "<tr><td>2027年</td><td>2027-01-01</td><td>份额折算</td><td>1:2</td></tr>";
        let splits = parse(&page(&format!("{future}{row}")), "518880", today).unwrap();
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0].effective_on, "2013-07-24");
        assert_eq!(splits[0].numerator, "0.3776");
        assert_eq!(splits[0].denominator, "1");
        assert!(parse(
            &page("<tr><td colspan='5'>暂无拆分信息!</td></tr>"),
            "518880",
            today
        )
        .unwrap()
        .is_empty());
        for html in [
            page(""),
            page("<tr><td>数据加载中</td></tr>"),
            page(&format!("{row}{row}")),
            page(row).replace("1:0.3776", "1:0"),
            page(row).replace("2013年", "2014年"),
            page(row).replace("1:0.3776", "unknown"),
            page(row).replace("518880", "588000"),
            page(row).replace("拆分详情", "分红详情"),
            page(row).replace("</tbody>", ""),
            page(row).replace("<td>2013年", "<td><script>2013年"),
        ] {
            assert!(parse(&html, "518880", today).is_err(), "{html}");
        }
    }
}
