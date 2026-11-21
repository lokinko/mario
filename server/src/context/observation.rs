//! Compact Markdown observation excerpts; full source JSON remains in the audit catalog.
use crate::models::{
    analysis::AnalysisEvidenceReference,
    watch::{rules::WatchRules, Band, WatchStatus},
};
use serde_json::Value;

const NORMAL_BUDGET: usize = 2_000;
const EXTENDED_BUDGET: usize = 4_000;

fn fmt_f64(value: f64) -> String {
    if !value.is_finite() {
        return "-".into();
    }
    let rounded = format!("{value:.4}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" || trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_owned()
    }
}

fn fmt_val(value: Option<&Value>) -> String {
    match value {
        Some(Value::Number(n)) => n.as_f64().map(fmt_f64).unwrap_or_else(|| n.to_string()),
        Some(Value::String(s)) if !s.trim().is_empty() => {
            s.trim().replace('|', "/").replace(['\r', '\n'], " ")
        }
        Some(Value::Bool(b)) => b.to_string(),
        _ => "-".into(),
    }
}

fn fmt_bps(bps: i64) -> String {
    let pct = (bps as f64) / 100.0;
    format!("{}%", fmt_f64(pct))
}

fn fmt_rules(rules: &WatchRules) -> String {
    let min_yield = fmt_bps(i64::from(rules.min_yield_bps));
    let daily_ref = serde_json::to_value(rules.daily.reference)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let daily_mode = serde_json::to_value(rules.daily.mode)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let weekly_ref = serde_json::to_value(rules.weekly.reference)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let weekly_mode = serde_json::to_value(rules.weekly.mode)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    format!(
        "股息>={min_yield}; 日:{daily_mode}/{daily_ref}({}); 周:{weekly_mode}/{weekly_ref}({})",
        fmt_bps(i64::from(rules.daily.offset_bps)),
        fmt_bps(i64::from(rules.weekly.offset_bps))
    )
}

fn fmt_rule_row(row: &[Value]) -> String {
    if row.len() < 7 {
        return row
            .iter()
            .map(|v| fmt_val(Some(v)))
            .collect::<Vec<_>>()
            .join("/");
    }
    let min_y = row[0]
        .as_i64()
        .map(fmt_bps)
        .unwrap_or_else(|| fmt_val(Some(&row[0])));
    let d_off = row[3]
        .as_i64()
        .map(fmt_bps)
        .unwrap_or_else(|| fmt_val(Some(&row[3])));
    let w_off = row[6]
        .as_i64()
        .map(fmt_bps)
        .unwrap_or_else(|| fmt_val(Some(&row[6])));
    format!(
        "股息>={min_y}, 日:{}/{}({d_off}), 周:{}/{}({w_off})",
        fmt_val(Some(&row[2])),
        fmt_val(Some(&row[1])),
        fmt_val(Some(&row[5])),
        fmt_val(Some(&row[4])),
    )
}

fn fmt_band(label: &str, band: &Option<Band>) -> String {
    let Some(band) = band else {
        return format!("{label}: 无");
    };
    let reference = serde_json::to_value(band.reference)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let matched = if band.matches_rule {
        "达标"
    } else {
        "未达标"
    };
    let lower = fmt_f64(band.lower);
    let middle = fmt_f64(band.middle);
    let upper = fmt_f64(band.upper);
    let dist = band
        .distance_pct
        .map(|d| format!("{}%", fmt_f64(d)))
        .unwrap_or_else(|| "-".into());
    format!("{label}({reference}, {matched}): 下{lower} / 中{middle} / 上{upper} / 距{dist}")
}

struct ObservationDraft<'a> {
    symbol: &'a str,
    date: &'a str,
    status: String,
    rules: WatchRules,
    daily: Option<Band>,
    weekly: Option<Band>,
    price: Option<String>,
    yield_pct: Option<String>,
    yield_matches_rule: Option<bool>,
    dividend_per_share: Option<String>,
    dividend_basis: Option<String>,
    issues: Vec<String>,
    week_rows: Vec<Vec<Value>>,
    week_total: usize,
    week_rules: Vec<Vec<Value>>,
    dividend_events: Vec<(String, String)>,
    dividend_total: usize,
    omitted_fields: Vec<&'static str>,
}

impl ObservationDraft<'_> {
    fn render(&self) -> String {
        let mut lines = Vec::new();
        let mut head = vec![
            format!("标的: {}", self.symbol),
            format!("观察日: {}", self.date),
            format!("状态: {}", self.status),
        ];
        if let Some(price) = &self.price {
            head.push(format!("现价: {price}"));
        }
        if let Some(yield_pct) = &self.yield_pct {
            let tag = match self.yield_matches_rule {
                Some(true) => "(达标)",
                Some(false) => "(未达标)",
                None => "",
            };
            head.push(format!("股息率: {yield_pct}%{tag}"));
        }
        if let Some(dps) = &self.dividend_per_share {
            head.push(format!("每股TTM分红: {dps}"));
        }
        lines.push(head.join(" | "));
        lines.push(format!("当前规则: {}", fmt_rules(&self.rules)));
        lines.push(fmt_band("日线BOLL", &self.daily));
        lines.push(fmt_band("周线BOLL", &self.weekly));
        if let Some(basis) = &self.dividend_basis {
            lines.push(format!("分红基准: {basis}"));
        }
        if !self.issues.is_empty() {
            lines.push(format!("数据提示: {}", self.issues.join("；")));
        }

        let week_omitted = self.week_total.saturating_sub(self.week_rows.len());
        if !self.week_rows.is_empty() {
            let suffix = if week_omitted > 0 {
                format!(
                    "({}/{}天, 省略最早{}天)",
                    self.week_rows.len(),
                    self.week_total,
                    week_omitted
                )
            } else {
                format!("({}天)", self.week_rows.len())
            };
            lines.push(format!("本周观察{suffix}:"));
            lines.push("| 日期 | 状态 | 现价 | 股息率% | 日距% | 周距% | 规则 |".into());
            lines.push("| --- | --- | --- | --- | --- | --- | --- |".into());
            for row in &self.week_rows {
                let rule_cell = row
                    .get(6)
                    .and_then(Value::as_u64)
                    .map(|idx| format!("#{idx}"))
                    .unwrap_or_else(|| "-".into());
                lines.push(format!(
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    fmt_val(row.first()),
                    fmt_val(row.get(1)),
                    fmt_val(row.get(2)),
                    fmt_val(row.get(3)),
                    fmt_val(row.get(4)),
                    fmt_val(row.get(5)),
                    rule_cell,
                ));
            }
            if !self.week_rules.is_empty() {
                let rule_summary = self
                    .week_rules
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| format!("#{idx} {}", fmt_rule_row(row)))
                    .collect::<Vec<_>>()
                    .join("；");
                lines.push(format!("本周规则: {rule_summary}"));
            }
        } else if self.week_total > 0 {
            lines.push(format!(
                "本周观察(0/{}天, 省略全部{}天)",
                self.week_total, self.week_total
            ));
        }

        let div_omitted = self
            .dividend_total
            .saturating_sub(self.dividend_events.len());
        if !self.dividend_events.is_empty() {
            let suffix = if div_omitted > 0 {
                format!(
                    "(展示{}/{}条, 省略{}条)",
                    self.dividend_events.len(),
                    self.dividend_total,
                    div_omitted
                )
            } else {
                format!("({}条)", self.dividend_events.len())
            };
            lines.push(format!("分红记录{suffix}:"));
            lines.push("| 除权日 | 每10股派现 |".into());
            lines.push("| --- | --- |".into());
            for (ex_date, cash) in &self.dividend_events {
                lines.push(format!("| {ex_date} | {cash} |"));
            }
        } else if self.dividend_total > 0 {
            lines.push(format!(
                "分红记录(展示0/{}条, 省略{}条)",
                self.dividend_total, self.dividend_total
            ));
        }

        if !self.omitted_fields.is_empty() {
            lines.push(format!(
                "已省略超长字段: {}",
                self.omitted_fields.join(", ")
            ));
        }
        lines.join("\n")
    }
}

pub fn observation_excerpt(source: &AnalysisEvidenceReference) -> Option<String> {
    if source.evidence_type != "watch_observation" {
        return None;
    }
    let full: Value = serde_json::from_str(&source.claim).ok()?;
    if full["kind"] != "mario_watch_observation_v1" {
        return None;
    }
    let facts = full.get("modelFacts")?.as_object()?;
    let symbol = facts.get("symbol")?.as_str()?;
    let date = facts.get("observedOn")?.as_str()?;
    if source.id != format!("watch-{symbol}-{date}")
        || source.as_of_date != date
        || full["originalFacts"]["latest"]["symbol"] != symbol
        || full["originalFacts"]["latest"]["observedOn"] != date
        || !["rules", "daily", "weekly", "status", "thisWeek"]
            .iter()
            .all(|key| facts.contains_key(*key))
    {
        return None;
    }
    // Validate mandatory typed fields before recognizing the structured observation shape.
    let rules: WatchRules = serde_json::from_value(facts.get("rules")?.clone()).ok()?;
    let daily: Option<Band> = serde_json::from_value(facts.get("daily")?.clone()).ok()?;
    let weekly: Option<Band> = serde_json::from_value(facts.get("weekly")?.clone()).ok()?;
    let status_val: WatchStatus = serde_json::from_value(facts.get("status")?.clone()).ok()?;
    let status = serde_json::to_value(status_val)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))?;
    let _: chrono::NaiveDate = date.parse().ok()?;
    if symbol.len() != 9 || !symbol.get(..6)?.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let dividend_raw = facts.get("dividendEvents")?.as_array()?;
    let dividend_total = dividend_raw.len();
    let dividend_events = dividend_raw
        .iter()
        .map(|ev| {
            let ex = fmt_val(ev.get("exDate"));
            let cash = fmt_val(ev.get("cashPerTen").or_else(|| ev.get("amount")));
            (ex, cash)
        })
        .collect::<Vec<_>>();

    let week = facts.get("thisWeek")?.as_object()?;
    let week_rows = week
        .get("rows")?
        .as_array()?
        .iter()
        .map(|row| row.as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let week_total = week_rows.len();
    let week_rules = if week_total == 0 {
        Vec::new()
    } else {
        week.get("rules")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|row| row.as_array().cloned().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default()
    };

    let dividend_basis = facts.get("dividendBasis").and_then(|val| match val {
        Value::Object(map) => {
            let mut parts = Vec::new();
            if let Some(as_of) = map.get("asOf").and_then(Value::as_str) {
                if !as_of.is_empty() {
                    parts.push(format!("截至{as_of}"));
                }
            }
            if let Some(fetched) = map.get("fetchedAt").and_then(Value::as_str) {
                if !fetched.is_empty() {
                    let short = fetched.get(..10).unwrap_or(fetched);
                    parts.push(format!("获取{short}"));
                }
            }
            (!parts.is_empty()).then(|| parts.join(" / "))
        }
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        _ => None,
    });
    let issues = facts
        .get("issues")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut draft = ObservationDraft {
        symbol,
        date,
        status,
        rules,
        daily,
        weekly,
        price: facts
            .get("price")
            .filter(|v| !v.is_null())
            .map(|v| fmt_val(Some(v))),
        yield_pct: facts
            .get("yieldPct")
            .filter(|v| !v.is_null())
            .map(|v| fmt_val(Some(v))),
        yield_matches_rule: facts.get("yieldMatchesRule").and_then(Value::as_bool),
        dividend_per_share: facts
            .get("dividendPerShare")
            .filter(|v| !v.is_null())
            .map(|v| fmt_val(Some(v))),
        dividend_basis,
        issues,
        week_rows,
        week_total,
        week_rules,
        dividend_events,
        dividend_total,
        omitted_fields: Vec::new(),
    };

    // Trim oldest dividend events first to fit the 2,000-char normal budget.
    loop {
        let rendered = draft.render();
        if rendered.chars().count() <= NORMAL_BUDGET {
            return Some(rendered);
        }
        if draft.dividend_events.is_empty() {
            break;
        }
        draft.dividend_events.pop();
    }

    // Drop malformed oversized optional metadata before dropping any saved week day.
    if draft
        .dividend_basis
        .as_ref()
        .is_some_and(|s| s.chars().count() > 200)
        || facts
            .get("dividendBasis")
            .is_some_and(|v| v.to_string().chars().count() > 1_000)
    {
        draft.dividend_basis = None;
        draft.omitted_fields.push("dividendBasis");
    }
    if facts
        .get("issues")
        .is_some_and(|v| v.to_string().chars().count() > 1_000)
    {
        draft.issues.clear();
        draft.omitted_fields.push("issues");
    }
    if draft
        .dividend_per_share
        .as_ref()
        .is_some_and(|s| s.chars().count() > 200)
    {
        draft.dividend_per_share = None;
        draft.omitted_fields.push("dividendPerShare");
    }

    // Retain the full saved week whenever it fits 4,000 chars; omit oldest days first if oversized.
    while draft.render().chars().count() > EXTENDED_BUDGET {
        if draft.week_rows.is_empty() {
            break;
        }
        draft.week_rows.remove(0);
        if draft.week_rows.is_empty() {
            draft.week_rules.clear();
        }
    }

    if draft.render().chars().count() > EXTENDED_BUDGET && draft.dividend_basis.take().is_some() {
        draft.omitted_fields.push("dividendBasis");
    }
    if draft.render().chars().count() > EXTENDED_BUDGET && !draft.issues.is_empty() {
        draft.issues.clear();
        draft.omitted_fields.push("issues");
    }
    if draft.render().chars().count() > EXTENDED_BUDGET && draft.dividend_per_share.take().is_some()
    {
        draft.omitted_fields.push("dividendPerShare");
    }
    if draft.render().chars().count() > EXTENDED_BUDGET && draft.yield_pct.take().is_some() {
        draft.yield_matches_rule = None;
        draft.omitted_fields.push("yieldPct");
    }
    if draft.render().chars().count() > EXTENDED_BUDGET && draft.price.take().is_some() {
        draft.omitted_fields.push("price");
    }

    Some(draft.render())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detailed_week_retains_every_day_and_saved_rule_as_compact_markdown() {
        let band = json!({"lower":123.45678901234567,"middle":234.56789012345678,
            "upper":345.678_901_234_567_9,"distancePct":-12.345678901234567,"reference":"middle","matchesRule":true});
        let rows = (0..7)
            .map(|i| {
                json!([
                    format!("2026-11-{:02}", 9 + i),
                    "attention",
                    123.45678901234567,
                    12.345678901234567,
                    -12.345678901234567,
                    -12.345678901234567,
                    i
                ])
            })
            .collect::<Vec<_>>();
        let rules = (0..7)
            .map(|i| {
                json!([
                    500 + i,
                    "lower",
                    "near",
                    200 + i,
                    "middle",
                    "at_or_below",
                    i
                ])
            })
            .collect::<Vec<_>>();
        let facts = json!({"symbol":"600036.SS","observedOn":"2026-11-15",
            "rules":{"minYieldBps":500,"daily":{"reference":"lower","mode":"near","offsetBps":200},
                "weekly":{"reference":"middle","mode":"at_or_below","offsetBps":0}},
            "daily":band,"weekly":band,"status":"attention","yieldPct":12.345678901234567,
            "yieldMatchesRule":true,"price":123.45678901234567,"dividendPerShare":"15.24157875323883",
            "dividendBasis":{"asOf":"2026-11-15","fetchedAt":"2026-11-15T12:00:00.123456789+08:00"},
            "issues":[],"dividendEvents":[],"thisWeek":{
                "columns":["date","status","price","yieldPct","dailyDistancePct","weeklyDistancePct","ruleIndex"],
                "rows":rows,"ruleColumns":["minYieldBps","dailyReference","dailyMode","dailyOffsetBps","weeklyReference","weeklyMode","weeklyOffsetBps"],
                "rules":rules}});
        let mut source: AnalysisEvidenceReference = serde_json::from_value(json!({
            "id":"watch-600036.SS-2026-11-15","title":"观察","publisher":"计算","sourceUrl":"https://example.com",
            "sourceTier":"二手来源","asOfDate":"2026-11-15","evidenceType":"watch_observation",
            "claim":json!({"kind":"mario_watch_observation_v1","modelFacts":facts,
                "originalFacts":{"latest":{"symbol":"600036.SS","observedOn":"2026-11-15"}}}).to_string()
        })).unwrap();
        let excerpt = observation_excerpt(&source).unwrap();
        assert!(excerpt.chars().count() <= NORMAL_BUDGET);
        assert!(!excerpt.contains("\"thisWeek\""));
        assert!(!excerpt.contains("123.45678901234567"));
        assert!(excerpt.contains("标的: 600036.SS | 观察日: 2026-11-15 | 状态: attention"));
        assert!(excerpt.contains(
            "日线BOLL(middle, 达标): 下123.4568 / 中234.5679 / 上345.6789 / 距-12.3457%"
        ));
        assert!(excerpt.contains("本周观察(7天):"));
        assert!(excerpt.contains("| 2026-11-09 |"));
        assert!(excerpt.contains("| 2026-11-15 |"));
        assert!(excerpt.contains("#6 "));

        let mut oversized: Value = serde_json::from_str(&source.claim).unwrap();
        oversized["modelFacts"]["thisWeek"]["rows"][0][1] = json!("异常历史元信息".repeat(1_000));
        oversized["modelFacts"]["dividendBasis"]["fetchedAt"] = json!("异常来源日期".repeat(1_000));
        oversized["modelFacts"]["issues"] = json!(["异常来源说明".repeat(1_000)]);
        source.claim = oversized.to_string();
        let excerpt = observation_excerpt(&source).unwrap();
        assert!(excerpt.chars().count() <= EXTENDED_BUDGET);
        assert!(excerpt.contains("本周观察(6/7天, 省略最早1天):"));
        assert!(excerpt.contains("已省略超长字段: dividendBasis, issues"));
        assert!(excerpt.contains("日线BOLL(middle, 达标)"));
        assert!(excerpt.contains("周线BOLL(middle, 达标)"));
    }

    #[test]
    fn only_application_observations_use_the_field_preserving_projection() {
        let mut source: AnalysisEvidenceReference = serde_json::from_value(json!({
            "id":"watch-600036.SS-2026-11-12","assetName":"公司","title":"观察",
            "publisher":"计算","sourceUrl":"https://example.com","sourceTier":"二手来源",
            "evidenceType":"watch_observation","stance":"neutral","asOfDate":"2026-11-12",
            "capturedAt":"2026-11-12T12:00:00Z","notes":"","attachments":[],
            "claim":json!({"kind":"mario_watch_observation_v1",
                "modelFacts":{"symbol":"600036.SS","observedOn":"2026-11-12","rules":WatchRules::default(),
                    "daily":null,"weekly":null,"status":"attention",
                    "thisWeek":{"rows":[["2026-11-12","attention"]]},
                    "dividendEvents":[{"exDate":"2026-01-01","cashPerTen":"1"}]},
                "originalFacts":{"latest":{"symbol":"600036.SS","observedOn":"2026-11-12"}}
            }).to_string()
        }))
        .unwrap();
        let excerpt = observation_excerpt(&source).unwrap();
        assert!(excerpt.contains("| 2026-01-01 | 1 |"));
        source.evidence_type = "user_note".into();
        assert!(observation_excerpt(&source).is_none());
    }
}
