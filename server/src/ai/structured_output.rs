use std::collections::HashSet;

use crate::models::{AnalysisClaim, StructuredAnalysis};

pub const MAX_STRUCTURED_OUTPUT_CHARS: usize = 30_000;

pub const STRUCTURED_ANALYSIS_CONTRACT: &str = r#"只返回JSON，不加代码块或说明：
{"answer":"先直接回答本题，再解释关键依据、推理和不确定性，最多1200字","suggestions":[{"action":"本题的具体下一步，写明对象、做法和时点或触发条件；依赖目标或假设时先写条件再给金额或动作，最多240字","why":"相关事实如何支持判断，说明计算与假设；比较本题可选路线的紧迫性、等待和交易成本，最多800字","factIndices":[0],"unlessWhen":"不适用或推翻判断的条件，最多400字，可为空"}],"facts":[{"statement":"引用的输入事实或来源数字","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}],"question":null}
suggestions为1–3条，facts为1–8条，factIndices非空、无重复且不越界。建议紧扣本题，不凑无关行动或金额；资料不足可给本题的核查、等待条件或公式。仅在问题需要且资料支持时量化，金额保留精确原值、币种与可复核计算；建议目标或假设须明确标识并解释，不能冒充facts。无需填满字数。
user_data的sourceRefs须非空且每条fact最多12项，仅用授权原值JSON Pointer，evidenceIds为空；不同账户或交易分别组织为独立事实，只选支持该事实必要的原值，facts总数仍为1–8条。research_evidence只用授权证据ID，sourceRefs为空。原值路径如/facts/summary/monthlySurplus或/facts/holdings/持仓ID/marketValue。只引非空标量，空值改引问题码，不以其他金额替代缺失值。最多一个追问，不超过200字，否则null。"#;

pub fn parse_structured_analysis(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
) -> Result<StructuredAnalysis, String> {
    if content.chars().count() > MAX_STRUCTURED_OUTPUT_CHARS {
        return Err(format!(
            "结构化输出超过 {MAX_STRUCTURED_OUTPUT_CHARS} 个字符"
        ));
    }
    let payload = extract_json(content)?;
    let report: StructuredAnalysis =
        serde_json::from_str(payload).map_err(|error| format!("JSON 结构无效：{error}"))?;
    validate_report(&report, allowed_evidence_ids)?;
    Ok(report)
}

pub fn validate_source_types(
    report: &StructuredAnalysis,
    context: &serde_json::Value,
) -> Result<(), String> {
    for (index, claim) in report.facts.iter().enumerate() {
        if claim.basis != "user_data" && !claim.source_refs.is_empty() {
            return Err(format!(
                "facts[{index}] 的 basis={}，sourceRefs 必须为空；研究事实请仅用本次授权的 evidenceIds 引用",
                claim.basis
            ));
        }
        if claim.source_refs.len() > 12 {
            return Err(format!(
                "facts[{index}].sourceRefs 最多12项，当前{}项；请只保留必要原值，或将不同账户、交易拆分为独立事实（facts 总数仍为1–8条）",
                claim.source_refs.len()
            ));
        }
        let mut seen = HashSet::new();
        for path in &claim.source_refs {
            let group = path.split('/').nth(1).unwrap_or_default();
            let fact_group = path.split('/').nth(2).unwrap_or_default();
            let allowed = path == "/question"
                || (group == "facts"
                    && ["summary", "holdings", "dataIssues", "options"].contains(&fact_group));
            if !path.starts_with('/')
                || !seen.insert(path)
                || !allowed
                || !context.pointer(path).is_some_and(|value| match value {
                    serde_json::Value::String(text) => !text.trim().is_empty(),
                    serde_json::Value::Number(_) | serde_json::Value::Bool(_) => true,
                    _ => false,
                })
            {
                return Err(format!("个人原始资料引用路径无效或未授权：{path}"));
            }
        }
    }
    Ok(())
}

pub fn render_report(report: &StructuredAnalysis) -> String {
    let mut sections = vec![report.answer.clone()];
    sections.push(
        report
            .suggestions
            .iter()
            .map(|suggestion| {
                let premise = if suggestion.unless_when.is_empty() {
                    String::new()
                } else {
                    format!("；前提：{}", suggestion.unless_when)
                };
                format!("- {}：{}{}", suggestion.action, suggestion.why, premise)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if let Some(question) = &report.question {
        sections.push(format!("追问：{question}"));
    }
    sections.join("\n\n")
}

fn extract_json(content: &str) -> Result<&str, String> {
    let trimmed = content.trim();
    if !trimmed.starts_with("```") {
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return Ok(trimmed);
        }
        return Err("输出必须只包含一个 JSON 对象".into());
    }

    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .ok_or_else(|| "JSON 代码块起始标记无效".to_string())?;
    let inner = without_open
        .strip_suffix("```")
        .ok_or_else(|| "JSON 代码块缺少结束标记".to_string())?
        .trim();
    if inner.starts_with('{') && inner.ends_with('}') {
        Ok(inner)
    } else {
        Err("代码块中必须只有一个 JSON 对象".into())
    }
}

fn validate_report(
    report: &StructuredAnalysis,
    allowed_evidence_ids: &HashSet<String>,
) -> Result<(), String> {
    check_text("answer", &report.answer, 1_200)?;
    check_count("facts", report.facts.len(), 1, 8)?;
    check_count("suggestions", report.suggestions.len(), 1, 3)?;
    for claim in &report.facts {
        validate_claim(claim, allowed_evidence_ids)?;
    }
    if let Some(question) = &report.question {
        check_text("question", question, 200)?;
    }
    for suggestion in &report.suggestions {
        check_text("suggestions.action", &suggestion.action, 240)?;
        check_text("suggestions.why", &suggestion.why, 800)?;
        if suggestion.unless_when.chars().count() > 400 {
            return Err("suggestions.unlessWhen 不能超过400字符".into());
        }
        check_count(
            "suggestions.factIndices",
            suggestion.fact_indices.len(),
            1,
            8,
        )?;
        let mut seen = HashSet::new();
        for index in &suggestion.fact_indices {
            if *index >= report.facts.len() || !seen.insert(index) {
                return Err("suggestions.factIndices 不能越界或重复".into());
            }
        }
    }
    Ok(())
}

fn validate_claim(
    claim: &AnalysisClaim,
    allowed_evidence_ids: &HashSet<String>,
) -> Result<(), String> {
    check_text("claim.statement", &claim.statement, 1_000)?;
    match claim.basis.as_str() {
        "user_data" if !claim.evidence_ids.is_empty() => {
            return Err("basis=user_data 时 evidenceIds 必须为空".into());
        }
        "user_data" if claim.source_refs.is_empty() => {
            return Err("basis=user_data 时 sourceRefs 必须非空".into());
        }
        "user_data" => {}
        "research_evidence" if claim.evidence_ids.is_empty() => {
            return Err("basis=research_evidence 时必须提供 evidenceIds".into());
        }
        "research_evidence" => {}
        _ => return Err("claim.basis 只能是 user_data 或 research_evidence".into()),
    }

    let mut seen = HashSet::new();
    for id in &claim.evidence_ids {
        if !allowed_evidence_ids.contains(id) {
            return Err(format!("引用了本次未授权的证据 ID：{id}"));
        }
        if !seen.insert(id) {
            return Err(format!("同一主张重复引用证据 ID：{id}"));
        }
    }
    Ok(())
}

fn check_count(name: &str, count: usize, min: usize, max: usize) -> Result<(), String> {
    if (min..=max).contains(&count) {
        Ok(())
    } else {
        Err(format!("{name} 数量必须在 {min}—{max} 之间"))
    }
}

fn check_text(name: &str, value: &str, max: usize) -> Result<(), String> {
    let length = value.trim().chars().count();
    if length == 0 {
        Err(format!("{name} 不能为空"))
    } else if length > max {
        Err(format!("{name} 不能超过 {max} 个字符"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> String {
        serde_json::json!({
            "answer":"先降低集中风险",
            "facts":[{"statement":"集中度为60%","basis":"user_data","sourceRefs":["/facts/holdings/fund/weightPct"],"evidenceIds":[]},{"statement":"有外部资料","basis":"research_evidence","sourceRefs":[],"evidenceIds":["e1"]}],
            "suggestions":[{"action":"本月分批降低集中持仓比例","why":"集中度60%","factIndices":[0],"unlessWhen":"近期有资金需求时优先留现金"}],
            "question":null
        }).to_string()
    }

    #[test]
    fn invalid_or_hidden_amounts_cannot_be_cited_but_other_holdings_remain_available() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({"facts":{"summary":{"portfolioValue":null},"holdings":{"ok":{"marketValue":"800000.01"},"blocked":{"marketValue":null}}}});
        report.facts[0].source_refs = vec!["/facts/holdings/ok/marketValue".into()];
        assert!(validate_source_types(&report, &context).is_ok());
        for path in [
            "/facts/holdings/blocked/marketValue",
            "/facts/summary/portfolioValue",
            "/portfolio/holdings/0/marketValue",
            "/facts/holdings/ok",
            "/facts/holdings/other/marketValue",
        ] {
            report.facts[0].source_refs = vec![path.into()];
            assert!(validate_source_types(&report, &context).is_err());
        }
    }

    #[test]
    fn trade_refs_over_budget_are_rejected_but_can_be_split_without_losing_sources() {
        let context = serde_json::json!({
            "facts": {"holdings": {
                "first": {"recentTrades": {"buy-a": {
                    "kind": "buy", "occurredOn": "2026-10-12", "sequence": 1,
                    "originalQuantity": "100", "originalPrice": "12.34",
                    "grossAmount": "1234", "account": "账户甲"
                }}},
                "second": {"recentTrades": {"buy-b": {
                    "kind": "buy", "occurredOn": "2026-10-16", "sequence": 1,
                    "originalQuantity": "200", "originalPrice": "13.5",
                    "grossAmount": "2700", "account": "账户乙"
                }}}
            }}
        });
        let fields = [
            "kind",
            "occurredOn",
            "sequence",
            "originalQuantity",
            "originalPrice",
            "grossAmount",
            "account",
        ];
        let mut refs: Vec<_> = fields
            .iter()
            .map(|field| format!("/facts/holdings/first/recentTrades/buy-a/{field}"))
            .collect();
        refs.extend(
            fields
                .iter()
                .skip(1)
                .map(|field| format!("/facts/holdings/second/recentTrades/buy-b/{field}")),
        );
        assert_eq!(refs.len(), 13);
        assert_eq!(refs.iter().collect::<HashSet<_>>().len(), 13);
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        report.facts.truncate(1);
        report.answer = "先核对两账户原始成交，不由买入价格倒推投资论据".into();
        report.suggestions[0].action = "比较已记录的原始成交，补充当时的投资论据".into();
        report.suggestions[0].why = "记录提供成交时点与单位，但不提供完整业绩或买入动机".into();
        report.suggestions[0].unless_when.clear();
        report.facts[0].statement = "已保存的原始成交记录".into();
        for path in &refs {
            report.facts[0].source_refs = vec![path.clone()];
            validate_source_types(&report, &context).unwrap();
        }
        report.facts[0].source_refs = refs[..12].to_vec();
        validate_source_types(&report, &context).unwrap();
        report.facts[0].source_refs = refs.clone();
        let error = validate_source_types(&report, &context).unwrap_err();
        assert!(error.contains("facts[0].sourceRefs") && error.contains("最多12项"));
        assert!(error.contains("当前13项") && error.contains("拆分"));

        report.facts[0].statement = "账户甲的原始买入成交".into();
        report.facts[0].source_refs = refs[..7].to_vec();
        let mut second = report.facts[0].clone();
        second.statement = "账户乙的原始买入成交".into();
        second.source_refs = refs[7..].to_vec();
        report.facts.push(second);
        report.suggestions[0].fact_indices = vec![0, 1];
        let split =
            parse_structured_analysis(&serde_json::to_string(&report).unwrap(), &HashSet::new())
                .unwrap();
        assert_eq!(
            split
                .facts
                .iter()
                .flat_map(|fact| &fact.source_refs)
                .collect::<HashSet<_>>(),
            refs.iter().collect::<HashSet<_>>()
        );
        validate_source_types(&split, &context).unwrap();
    }

    #[test]
    fn research_refs_get_a_category_error_instead_of_a_personal_budget_error() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({"facts": {"holdings": {"fund": {"weightPct": "60"}}}});
        report.facts[1].source_refs = report.facts[0].source_refs.clone();
        let error = validate_source_types(&report, &context).unwrap_err();
        assert!(error.contains("facts[1]") && error.contains("basis=research_evidence"));
        assert!(error.contains("sourceRefs 必须为空") && error.contains("evidenceIds"));
        assert!(!error.contains("最多12项"));
        report.facts[1].source_refs.clear();
        validate_source_types(&report, &context).unwrap();
    }

    #[test]
    fn rejects_blank_personal_source_values_in_all_authorized_groups() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        for blank in ["", " \t\r\n", "\u{3000}\u{a0}"] {
            let context = serde_json::json!({
                "question": blank,
                "facts": {
                    "summary": {"monthlySurplus": blank},
                    "holdings": {"fund": {"name": blank}},
                    "dataIssues": [{"code": blank}],
                    "options": {"option": {"account": blank}}
                }
            });
            for path in [
                "/question",
                "/facts/summary/monthlySurplus",
                "/facts/holdings/fund/name",
                "/facts/dataIssues/0/code",
                "/facts/options/option/account",
            ] {
                report.facts[0].source_refs = vec![path.into()];
                let error = validate_source_types(&report, &context).unwrap_err();
                assert!(error.contains(path), "{error}");
            }
        }
    }

    #[test]
    fn accepts_nonblank_personal_source_values_including_zero_and_false() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        report.facts[0].source_refs = vec!["/facts/summary/value".into()];
        for value in [
            serde_json::json!("有效原值"),
            serde_json::json!(" \t有效原值\u{3000}"),
            serde_json::json!("0"),
            serde_json::json!(0),
            serde_json::json!(-1.25),
            serde_json::json!(false),
            serde_json::json!(true),
        ] {
            let context = serde_json::json!({"facts": {"summary": {"value": value}}});
            assert!(validate_source_types(&report, &context).is_ok());
        }
    }

    #[test]
    fn accepts_a_valid_report_and_renders_it() {
        let report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        assert!(render_report(&report).contains("先降低集中风险"));
        assert!(!render_report(&report).contains("已知事实"));
    }

    #[test]
    fn refuses_previous_answers_and_unauthorized_source_groups() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({"facts":{"summary":{"monthlySurplus":"8000"}},"question":"问题", "previousTurn":{"answer":"模型结论"},"researchEvidence":[{"claim":"来源"}]});
        for path in [
            "/previousTurn/answer",
            "/researchEvidence/0/claim",
            "/facts/summary",
            "/facts/metadata/other",
        ] {
            report.facts[0].source_refs = vec![path.into()];
            assert!(validate_source_types(&report, &context).is_err());
        }
        report.facts[0].source_refs = vec!["/facts/summary/monthlySurplus".into()];
        assert!(validate_source_types(&report, &context).is_ok());
    }

    #[test]
    fn rejects_advice_without_grounding_or_with_invalid_fact_indices() {
        for indices in [
            serde_json::json!([]),
            serde_json::json!([9]),
            serde_json::json!([0, 0]),
        ] {
            let mut value: serde_json::Value = serde_json::from_str(&valid_json()).unwrap();
            value["suggestions"][0]["factIndices"] = indices;
            let error =
                parse_structured_analysis(&value.to_string(), &HashSet::from(["e1".into()]))
                    .unwrap_err();
            assert!(error.contains("factIndices"));
        }
    }

    #[test]
    fn rejects_personal_facts_with_omitted_or_empty_source_refs() {
        for omitted in [false, true] {
            let mut value: serde_json::Value = serde_json::from_str(&valid_json()).unwrap();
            if omitted {
                value["facts"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("sourceRefs");
            } else {
                value["facts"][0]["sourceRefs"] = serde_json::json!([]);
            }
            let error =
                parse_structured_analysis(&value.to_string(), &HashSet::from(["e1".into()]))
                    .unwrap_err();
            assert!(error.contains("sourceRefs 必须非空"));
        }
    }

    #[test]
    fn enforces_detailed_limits_and_rejects_retired_fields() {
        for (field, value) in [
            ("answer", serde_json::json!("字".repeat(1_201))),
            ("suggestions", serde_json::json!([])),
            ("verdict", serde_json::json!("旧字段")),
        ] {
            let mut report: serde_json::Value = serde_json::from_str(&valid_json()).unwrap();
            report[field] = value;
            assert!(
                parse_structured_analysis(&report.to_string(), &HashSet::from(["e1".into()]))
                    .is_err()
            );
        }
    }

    #[test]
    fn accepts_a_plain_json_code_fence_for_compatible_models() {
        let content = format!("```json\n{}\n```", valid_json());
        assert!(parse_structured_analysis(&content, &HashSet::from(["e1".into()])).is_ok());
    }

    #[test]
    fn detailed_fields_preserve_unicode_at_their_limits_and_reject_overflow() {
        let allowed = HashSet::from(["e1".into()]);
        for (path, limit) in [
            ("/answer", 1_200),
            ("/suggestions/0/action", 240),
            ("/suggestions/0/why", 800),
            ("/suggestions/0/unlessWhen", 400),
            ("/question", 200),
        ] {
            let mut value: serde_json::Value = serde_json::from_str(&valid_json()).unwrap();
            let text = "🙂".repeat(limit);
            *value.pointer_mut(path).unwrap() = serde_json::json!(&text);
            let report = parse_structured_analysis(&value.to_string(), &allowed).unwrap();
            assert!(
                render_report(&report).contains(&text),
                "{path} was truncated"
            );
            *value.pointer_mut(path).unwrap() = serde_json::json!(format!("{text}字"));
            assert!(
                parse_structured_analysis(&value.to_string(), &allowed).is_err(),
                "{path} exceeded its character limit"
            );
        }
    }

    #[test]
    fn the_global_output_budget_still_applies_before_json_whitespace_is_trimmed() {
        let allowed = HashSet::from(["e1".into()]);
        let valid = valid_json();
        let padding = " ".repeat(MAX_STRUCTURED_OUTPUT_CHARS - valid.chars().count());
        let at_limit = format!("{valid}{padding}");
        assert_eq!(at_limit.chars().count(), MAX_STRUCTURED_OUTPUT_CHARS);
        parse_structured_analysis(&at_limit, &allowed).unwrap();
        let error = parse_structured_analysis(&format!("{at_limit} "), &allowed).unwrap_err();
        assert!(error.contains("结构化输出超过 30000"));
    }

    #[test]
    fn rejects_unapproved_or_missing_evidence_references() {
        let unknown = parse_structured_analysis(&valid_json(), &HashSet::new()).unwrap_err();
        assert!(unknown.contains("未授权"));

        let missing = valid_json().replace(
            r#""basis":"research_evidence","evidenceIds":["e1"]"#,
            r#""basis":"research_evidence","evidenceIds":[]"#,
        );
        assert!(
            parse_structured_analysis(&missing, &HashSet::from(["e1".into()]))
                .unwrap_err()
                .contains("必须提供")
        );
    }

    #[test]
    fn rejects_prose_wrapped_around_json() {
        let content = format!("这是结果：{}", valid_json());
        assert!(
            parse_structured_analysis(&content, &HashSet::from(["e1".into()]))
                .unwrap_err()
                .contains("只包含")
        );
    }
}
