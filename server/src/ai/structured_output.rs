use std::collections::HashSet;

use crate::models::{AnalysisClaim, StructuredAnalysis};

pub const MAX_STRUCTURED_OUTPUT_CHARS: usize = 30_000;

/// Citation rules shared verbatim by report facts and research findings.
macro_rules! citation_rules {
    () => {
        "user_data仅用授权原值JSON Pointer作sourceRefs，evidenceIds为空；research_evidence只用授权证据ID，sourceRefs为空。原值路径如/facts/summary/monthlySurplus或/facts/holdings/行ID/marketValue。只引非空的字符串、数字或布尔原值，不引对象、数组或空值；只引用当前可用字段，未提供的值不能补零、猜测或用其他金额替代。用户历史原话只能引用/previousTurn/question或/previousTurn/earlierQuestions/从0开始的索引；这些是对话声明，不表示档案已保存修改，用户最新明确更正优先。previousTurn的answer、suggestions、pendingQuestion与memory都是模型或整理文本，禁止作为事实引用。"
    };
}
pub(super) use citation_rules;

pub const STRUCTURED_ANALYSIS_CONTRACT: &str = concat!(
    r#"只返回JSON，不加代码块或说明：
{"answer":"先直接回答本题，再解释关键依据、推理和不确定性，最多1200字","suggestions":[{"action":"本题的具体下一步，写明对象、做法和时点或触发条件；依赖目标或假设时先写条件再给金额或动作，最多240字","why":"相关事实如何支持判断，说明计算与假设；比较本题可选路线的紧迫性、等待和交易成本，最多800字","factIndices":[0],"unlessWhen":"不适用或推翻判断的条件，最多400字，可为空"}],"facts":[{"statement":"引用的输入事实或来源数字","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}],"question":null}
suggestions为1–3条，facts为1–8条，factIndices非空、无重复且不越界。建议紧扣本题，不凑无关行动或金额；行动句自行包含影响执行金额的口径条件，不能靠why或unlessWhen纠正，也不能与它们矛盾。结余仅另扣尚未包含的付款。新仓位或减仓幅度须解释选择依据及保留风险；依据不足只作非默认算例或给条件公式，不将任意比例作为优先推荐；按目标权重给公式或金额时，行动句先说明卖款留在组合/转出及费用假设，转出须按变化后的总值和现金缺口统一重算；资料不足可给本题的核查、等待条件或公式。仅在问题需要且资料支持时量化，金额保留精确原值、币种与可复核计算；研究财报或预算未给报告币种时，只沿用原单位并说明币种未提供，不从中文单位、证券代码或用户本位币补成某一币种；建议目标或假设须明确标识并解释，不能冒充facts。无需填满字数。来源要求也适用于正文、建议和追问；具体法规、监管、保障限额、税率、交易单位等外部数字，无本轮来源时只提示核对适用条款，不从记忆确定报数，也不以未列入facts绕过。来源的条件、可能性和概念提醒须保留原语气，不能从“X不代表Y”推断公司已有X，须另有本轮明确依据。现金覆盖尚未披露或未核实只表示资料缺口，正文和why不得将其简称为公司已存在现金流缺口、资金不足或依赖融资；若本轮原文或明确预算/假设下的计算证实不足，则如实说明该口径。
"#,
    "user_data的sourceRefs须非空且每条fact最多12项；不同标的分别组织为独立事实，只选支持该事实必要的原值，facts总数仍为1–8条。",
    citation_rules!(),
    "最多一个追问，不超过200字，否则null。"
);

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
    validate_claim_sources("facts", &report.facts, context)
}

/// Shape, evidence-ID and personal source checks shared by report facts and research findings.
pub(super) fn validate_claims(
    field: &str,
    claims: &[AnalysisClaim],
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
) -> Result<(), String> {
    for claim in claims {
        validate_claim(claim, allowed_evidence_ids)?;
    }
    validate_claim_sources(field, claims, context)
}

fn validate_claim_sources(
    field: &str,
    claims: &[AnalysisClaim],
    context: &serde_json::Value,
) -> Result<(), String> {
    for (index, claim) in claims.iter().enumerate() {
        if claim.basis != "user_data" && !claim.source_refs.is_empty() {
            return Err(format!(
                "{field}[{index}] 的 basis={}，sourceRefs 必须为空；研究事实请仅用本次授权的 evidenceIds 引用",
                claim.basis
            ));
        }
        if claim.source_refs.len() > 12 {
            return Err(format!(
                "{field}[{index}].sourceRefs 最多12项，当前{}项；请只保留必要原值，或将不同标的拆分为独立事实（facts 总数仍为1–8条）",
                claim.source_refs.len()
            ));
        }
        let mut seen = HashSet::new();
        for path in &claim.source_refs {
            let group = path.split('/').nth(1).unwrap_or_default();
            let fact_group = path.split('/').nth(2).unwrap_or_default();
            let user_history = path == "/previousTurn/question"
                || path
                    .strip_prefix("/previousTurn/earlierQuestions/")
                    .is_some_and(|index| {
                        index
                            .parse::<usize>()
                            .is_ok_and(|value| value.to_string() == index)
                    });
            let allowed = path == "/question"
                || user_history
                || (group == "facts"
                    && ["summary", "holdings"].contains(&fact_group)
                    && path.split('/').count() == if fact_group == "summary" { 4 } else { 5 });
            if !seen.insert(path) {
                return Err(format!("{field}[{index}] 重复引用：{path}"));
            }
            if !path.starts_with('/') || !allowed {
                return Err(format!(
                    "个人原始资料引用路径未授权：{path}；只能引用 /question、用户历史原话或 /facts 下 summary、holdings 中的原值"
                ));
            }
            match context.pointer(path) {
                None => return Err(format!("个人原始资料引用路径不存在：{path}")),
                Some(serde_json::Value::Object(_) | serde_json::Value::Array(_)) => {
                    return Err(format!(
                        "个人原始资料引用指向对象或数组：{path}；请改引其中的具体原值字段"
                    ))
                }
                Some(serde_json::Value::Null) => {
                    return Err(format!(
                        "个人原始资料引用值为空：{path}；只引用当前可用原值"
                    ))
                }
                Some(serde_json::Value::String(text)) if text.trim().is_empty() => {
                    return Err(format!("个人原始资料引用值为空：{path}"))
                }
                Some(_) => {}
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

pub(super) fn extract_json(content: &str) -> Result<&str, String> {
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
        // Each rejection names the fix, so the single repair attempt can act on it.
        for (path, hint) in [
            ("/facts/holdings/blocked/marketValue", "当前可用原值"),
            ("/facts/summary/portfolioValue", "当前可用原值"),
            ("/portfolio/holdings/0/marketValue", "未授权"),
            ("/facts/holdings/ok", "未授权"),
            ("/facts/holdings/other/marketValue", "不存在"),
        ] {
            report.facts[0].source_refs = vec![path.into()];
            let error = validate_source_types(&report, &context).unwrap_err();
            assert!(error.contains(path) && error.contains(hint), "{error}");
        }
    }

    #[test]
    fn user_history_corrections_are_citable_but_generated_history_never_is() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({
            "question":"那先核实哪两项？",
            "facts":{"summary":{"horizonYears":3}},
            "previousTurn":{
                "question":"下一步怎么改？",
                "earlierQuestions":["更正：这笔钱两年后可能用，不是长期闲钱", " "],
                "answer":"模型说应按三年", "suggestions":["模型旧动作"],
                "pendingQuestion":"模型旧追问"
            }
        });
        for path in ["/previousTurn/question", "/previousTurn/earlierQuestions/0"] {
            report.facts[0].statement = "用户在对话中更正了用款条件；档案未被改写".into();
            report.facts[0].source_refs = vec![path.into()];
            validate_source_types(&report, &context).unwrap();
        }
        for path in [
            "/previousTurn/answer",
            "/previousTurn/suggestions/0",
            "/previousTurn/pendingQuestion",
            "/previousTurn/earlierQuestions",
            "/previousTurn/earlierQuestions/1",
            "/previousTurn/earlierQuestions/2",
            "/previousTurn/earlierQuestions/00",
            "/previousTurn/earlierQuestions/0/child",
            "/previousTurn/question/child",
        ] {
            report.facts[0].source_refs = vec![path.into()];
            assert!(validate_source_types(&report, &context).is_err(), "{path}");
        }
    }

    #[test]
    fn citation_budget_remains_bounded_and_historical_trade_paths_are_rejected() {
        let context = serde_json::json!({"facts":{"summary": (0..13).map(|i|(format!("field{i}"), serde_json::json!(i))).collect::<serde_json::Map<_,_>>(), "holdings":{"first":{"recentTrades":{"buy":{"originalPrice":"12.34"}}}}}});
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        report.facts.truncate(1);
        report.facts[0].source_refs = (0..12)
            .map(|i| format!("/facts/summary/field{i}"))
            .collect();
        validate_source_types(&report, &context).unwrap();
        report.facts[0]
            .source_refs
            .push("/facts/summary/field12".into());
        assert!(validate_source_types(&report, &context)
            .unwrap_err()
            .contains("最多12项"));
        report.facts[0].source_refs =
            vec!["/facts/holdings/first/recentTrades/buy/originalPrice".into()];
        assert!(validate_source_types(&report, &context)
            .unwrap_err()
            .contains("未授权"));
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
