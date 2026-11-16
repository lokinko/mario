//! Field-preserving observation excerpts; full source JSON remains in the audit catalog.
use crate::models::AnalysisEvidenceReference;
use serde_json::{json, Value};

pub fn observation_excerpt(source: &AnalysisEvidenceReference) -> Option<String> {
    if source.evidence_type != "watch_observation" {
        return None;
    }
    let full: Value = serde_json::from_str(&source.claim).ok()?;
    if full["kind"] != "mario_watch_observation_v1" {
        return None;
    }
    let mut facts = full.get("modelFacts")?.as_object()?.clone();
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
    // Validate the mandatory domain values before recognizing the special shape.
    // Their bounded typed representation must survive even an oversized source.
    let rules: crate::models::watch::rules::WatchRules =
        serde_json::from_value(facts.get("rules")?.clone()).ok()?;
    let daily: Option<crate::models::watch::Band> =
        serde_json::from_value(facts.get("daily")?.clone()).ok()?;
    let weekly: Option<crate::models::watch::Band> =
        serde_json::from_value(facts.get("weekly")?.clone()).ok()?;
    let _: crate::models::watch::WatchStatus =
        serde_json::from_value(facts.get("status")?.clone()).ok()?;
    let _: chrono::NaiveDate = date.parse().ok()?;
    if symbol.len() != 9 || !symbol.get(..6)?.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    facts.insert("rules".into(), json!(rules));
    facts.insert("daily".into(), json!(daily));
    facts.insert("weekly".into(), json!(weekly));
    let mut events = facts.remove("dividendEvents")?.as_array()?.clone();
    let total = events.len();
    let week = facts.get_mut("thisWeek")?.as_object_mut()?;
    let record_count = week.get("rows")?.as_array()?.len();
    week.retain(|key, _| ["rows", "rules"].contains(&key.as_str()));
    week.insert(
        "columns".into(),
        json!([
            "date",
            "status",
            "price",
            "yieldPct",
            "dailyDistancePct",
            "weeklyDistancePct",
            "ruleIndex"
        ]),
    );
    week.insert(
        "ruleColumns".into(),
        json!([
            "minYieldBps",
            "dailyReference",
            "dailyMode",
            "dailyOffsetBps",
            "weeklyReference",
            "weeklyMode",
            "weeklyOffsetBps"
        ]),
    );
    if record_count == 0 {
        week.insert("rules".into(), json!([]));
    }
    week.insert("recordCount".into(), json!(record_count));
    week.insert("omittedCount".into(), json!(0));
    facts.insert("dividendEventCount".into(), json!(total));
    // Remove whole dividend records, never characters or observation fields.
    // Normal excerpts fit 2,000 characters; a detailed week may use 4,000.
    loop {
        facts.insert("dividendEventsOmitted".into(), json!(total - events.len()));
        facts.insert("dividendEvents".into(), json!(events));
        let excerpt = serde_json::to_string(&facts).ok()?;
        if excerpt.chars().count() <= 2_000 {
            return Some(excerpt);
        }
        if events.is_empty() {
            break;
        }
        events.pop();
    }
    let mut omitted_fields = Vec::new();
    // A malformed optional metadata field must not displace useful saved days.
    for key in ["dividendBasis", "issues", "dividendPerShare"] {
        if facts
            .get(key)
            .is_some_and(|value| value.to_string().chars().count() > 1_000)
        {
            facts.remove(key);
            omitted_fields.push(key);
        }
    }
    if !omitted_fields.is_empty() {
        facts.insert("omittedFields".into(), json!(omitted_fields));
    }
    // Retain the full saved week whenever it fits. For exceptional oversized
    // records, omit whole oldest days and state the exact resulting coverage.
    while serde_json::to_string(&facts).ok()?.chars().count() > 4_000 {
        let week = facts.get_mut("thisWeek")?.as_object_mut()?;
        let rows = week.get_mut("rows")?.as_array_mut()?;
        if rows.is_empty() {
            break;
        }
        rows.remove(0);
        let omitted = record_count - rows.len();
        week.insert("omittedCount".into(), json!(omitted));
        if omitted == record_count {
            week.insert("rules".into(), json!([]));
        }
    }
    for key in [
        "dividendBasis",
        "issues",
        "dividendPerShare",
        "yieldPct",
        "yieldMatchesRule",
        "price",
    ] {
        if serde_json::to_string(&facts).ok()?.chars().count() <= 4_000 {
            break;
        }
        if facts.remove(key).is_some() {
            omitted_fields.push(key);
            facts.insert("omittedFields".into(), json!(omitted_fields));
        }
    }
    // Unknown surplus fields are not generated by the application. Keep only
    // the declared projection if such data appears in a restored source.
    if serde_json::to_string(&facts).ok()?.chars().count() > 4_000 {
        facts.retain(|key, _| {
            [
                "symbol",
                "observedOn",
                "rules",
                "daily",
                "weekly",
                "status",
                "thisWeek",
                "dividendEventCount",
                "dividendEventsOmitted",
                "dividendEvents",
                "omittedFields",
            ]
            .contains(&key.as_str())
        });
        facts.insert("surplusFieldsOmitted".into(), json!(true));
    }
    serde_json::to_string(&facts).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_week_retains_every_day_and_saved_rule_within_the_observation_budget() {
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
        assert!(excerpt.chars().count() <= 4_000);
        let actual: Value = serde_json::from_str(&excerpt).unwrap();
        assert_eq!(actual["thisWeek"]["rows"], facts["thisWeek"]["rows"]);
        assert_eq!(actual["thisWeek"]["rules"], facts["thisWeek"]["rules"]);
        assert_eq!(actual["daily"], facts["daily"]);
        assert_eq!(actual["rules"], facts["rules"]);

        let mut oversized: Value = serde_json::from_str(&source.claim).unwrap();
        oversized["modelFacts"]["thisWeek"]["rows"][0][1] = json!("异常历史元信息".repeat(1_000));
        oversized["modelFacts"]["dividendBasis"]["fetchedAt"] = json!("异常来源日期".repeat(1_000));
        oversized["modelFacts"]["issues"] = json!(["异常来源说明".repeat(1_000)]);
        source.claim = oversized.to_string();
        let excerpt = observation_excerpt(&source).unwrap();
        assert!(excerpt.chars().count() <= 4_000);
        let actual: Value = serde_json::from_str(&excerpt).unwrap();
        assert_eq!(actual["thisWeek"]["recordCount"], 7);
        assert_eq!(actual["thisWeek"]["omittedCount"], 1);
        assert_eq!(actual["thisWeek"]["rows"].as_array().unwrap().len(), 6);
        assert_eq!(actual["daily"], facts["daily"]);
        assert_eq!(actual["weekly"], facts["weekly"]);
        assert_eq!(actual["rules"], facts["rules"]);
        assert_eq!(actual["status"], facts["status"]);
        assert_eq!(actual["omittedFields"], json!(["dividendBasis", "issues"]));
    }

    #[test]
    fn only_application_observations_use_the_field_preserving_projection() {
        let mut source: AnalysisEvidenceReference = serde_json::from_value(json!({
            "id":"watch-600036.SS-2026-11-12","assetName":"公司","title":"观察",
            "publisher":"计算","sourceUrl":"https://example.com","sourceTier":"二手来源",
            "evidenceType":"watch_observation","stance":"neutral","asOfDate":"2026-11-12",
            "capturedAt":"2026-11-12T12:00:00Z","notes":"","attachments":[],
            "claim":json!({"kind":"mario_watch_observation_v1",
                "modelFacts":{"symbol":"600036.SS","observedOn":"2026-11-12","rules":crate::models::watch::rules::WatchRules::default(),
                    "daily":null,"weekly":null,"status":"attention",
                    "thisWeek":{"rows":[["2026-11-12","attention"]]},
                    "dividendEvents":[{"exDate":"2026-01-01","cashPerTen":"1"}]},
                "originalFacts":{"latest":{"symbol":"600036.SS","observedOn":"2026-11-12"}}
            }).to_string()
        })).unwrap();
        let excerpt: Value = serde_json::from_str(&observation_excerpt(&source).unwrap()).unwrap();
        assert_eq!(excerpt["thisWeek"]["rows"][0][0], "2026-11-12");
        source.id = "manual-evidence".into();
        assert!(observation_excerpt(&source).is_none());
        source.evidence_type = "market_data".into();
        assert!(observation_excerpt(&source).is_none());
    }
}
