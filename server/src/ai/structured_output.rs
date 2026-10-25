use std::collections::HashSet;

use crate::models::{AnalysisClaim, StructuredAnalysis};

pub const STRUCTURED_ANALYSIS_CONTRACT: &str = r#"只返回JSON，不加代码块或说明：
{"answer":"直接回答，最多300字","suggestions":[{"action":"对象、方向、幅度、时间，最多120字","why":"数字依据","factIndices":[0],"unlessWhen":"不适用的前提，可为空"}],"facts":[{"statement":"引用的事实或数字","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}],"question":null}
suggestions为1–3条，facts为1–8条，factIndices须非空、无重复且不越界。user_data只引用授权个人原值JSON Pointer，evidenceIds为空；research_evidence只引用本次授权证据ID，sourceRefs为空。金额引用精确字符串，不能引用浮点显示值。追问最多一个，没有则null。"#;

pub fn parse_structured_analysis(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
) -> Result<StructuredAnalysis, String> {
    if content.chars().count() > 30_000 {
        return Err("结构化输出超过 30000 个字符".into());
    }
    let payload = extract_json(content)?;
    let report: StructuredAnalysis =
        serde_json::from_str(payload).map_err(|error| format!("JSON 结构无效：{error}"))?;
    validate_report(&report, allowed_evidence_ids)?;
    Ok(report)
}

pub fn validate_source_types(
    report: &StructuredAnalysis,
    _sources: &[crate::models::AnalysisEvidenceReference],
    context: &serde_json::Value,
) -> Result<(), String> {
    for claim in &report.facts {
        if claim.source_refs.len() > 12
            || (claim.basis != "user_data" && !claim.source_refs.is_empty())
        {
            return Err("个人原始资料引用数量或类别无效".into());
        }
        let mut seen = HashSet::new();
        for path in &claim.source_refs {
            let field = path.rsplit('/').next().unwrap_or_default();
            if context
                .pointer("/financialProfile/profile/exactAmounts")
                .is_some_and(|v| v.is_object())
                && path.starts_with("/financialProfile/profile/")
                && !path.starts_with("/financialProfile/profile/exactAmounts/")
                && [
                    "monthlyIncome",
                    "monthlyExpense",
                    "emergencyFund",
                    "liabilities",
                    "investableAssets",
                ]
                .contains(&field)
            {
                return Err(format!("财务金额必须引用 exactAmounts 原值：{path}"));
            }
            if path == "/deterministicPlan/monthlySurplus"
                && context
                    .pointer("/deterministicPlan/exactMonthlySurplus")
                    .is_some_and(|v| v.is_string())
            {
                return Err("每月结余必须引用 exactMonthlySurplus 原值".into());
            }
            if context
                .pointer("/portfolio/accounting")
                .is_some_and(|v| v.is_object())
                && (path == "/portfolio/totalValue"
                    || (path.starts_with("/portfolio/holdings/")
                        && [
                            "marketValue",
                            "costBasis",
                            "quantity",
                            "costPrice",
                            "currentPrice",
                            "fxRateToBase",
                        ]
                        .contains(&field)))
            {
                return Err(format!(
                    "金额或数量必须引用精确会计原值，不能引用浮点显示投影：{path}"
                ));
            }
            let parts = path.split('/').collect::<Vec<_>>();
            if parts.len() >= 6 && parts[1..4] == ["portfolio", "accounting", "holdings"] {
                let parent = format!("/portfolio/accounting/holdings/{}", parts[4]);
                if context
                    .pointer(&format!("{parent}/usable"))
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
                    && field != "usable"
                    && field != "priceStatus"
                {
                    return Err(format!("待核对会计数据不能作为已确认事实原值：{path}"));
                }
            }
            let group = path.split('/').nth(1).unwrap_or_default();
            if !path.starts_with('/')
                || !seen.insert(path)
                || ![
                    "financialProfile",
                    "portfolio",
                    "currentUserMessage",
                    "dailyAssetHistory",
                    "deterministicPlan",
                    "riskFindings",
                    "personalInvestmentRules",
                    "periodicSystemReviews",
                ]
                .contains(&group)
                || !context.pointer(path).is_some_and(|value| {
                    !value.is_null() && !value.is_object() && !value.is_array()
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
    check_text("answer", &report.answer, 300)?;
    check_count("facts", report.facts.len(), 1, 8)?;
    check_count("suggestions", report.suggestions.len(), 1, 3)?;
    for claim in &report.facts {
        validate_claim(claim, allowed_evidence_ids)?;
    }
    if let Some(question) = &report.question {
        check_text("question", question, 200)?;
    }
    for suggestion in &report.suggestions {
        check_text("suggestions.action", &suggestion.action, 120)?;
        check_text("suggestions.why", &suggestion.why, 300)?;
        if suggestion.unless_when.chars().count() > 200 {
            return Err("suggestions.unlessWhen 不能超过200字符".into());
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
            "facts":[{"statement":"集中度为60%","basis":"user_data","sourceRefs":[],"evidenceIds":[]},{"statement":"有外部资料","basis":"research_evidence","sourceRefs":[],"evidenceIds":["e1"]}],
            "suggestions":[{"action":"本月分批降低集中持仓比例","why":"集中度60%","factIndices":[0],"unlessWhen":"近期有资金需求时优先留现金"}],
            "question":null
        }).to_string()
    }

    #[test]
    fn data_issues_allow_normal_actions_but_exact_amount_refs_are_required() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({"portfolio":{"dataIssues":[{"code":"quarantinedTrades","holdingIds":["asset"],"message":"交易链待修复"}],"rebalancingBlockedBy":["quarantinedTrades"],"holdings":[{"marketValue":1.0}],"accounting":{"holdings":{"asset":{"marketValue":"1.01","usable":false}}}}});
        assert!(validate_source_types(&report, &[], &context).is_ok());
        report.facts[0].source_refs = vec!["/portfolio/holdings/0/marketValue".into()];
        assert!(validate_source_types(&report, &[], &context)
            .unwrap_err()
            .contains("精确会计"));
        report.facts[0].source_refs =
            vec!["/portfolio/accounting/holdings/asset/marketValue".into()];
        assert!(validate_source_types(&report, &[], &context)
            .unwrap_err()
            .contains("待核对"));
        let mut ready = context;
        ready["portfolio"]["accounting"]["holdings"]["asset"]["usable"] = true.into();
        assert!(validate_source_types(&report, &[], &ready).is_ok());
    }

    #[test]
    fn accepts_a_valid_report_and_renders_it() {
        let report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        assert!(render_report(&report).contains("先降低集中风险"));
        assert!(!render_report(&report).contains("已知事实"));
    }

    #[test]
    fn validates_personal_references_against_frozen_scalars_and_source_groups() {
        let mut report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        let context = serde_json::json!({"portfolio":{"holdings":[{"marketValue":120000}]},"question":"旧模型回答"});
        report.facts[0].source_refs = vec!["/portfolio/holdings/0/marketValue".into()];
        assert!(validate_source_types(&report, &[], &context).is_ok());
        for path in [
            "/question",
            "/portfolio/holdings",
            "/portfolio/holdings/1/marketValue",
            "/researchEvidence/0/claim",
        ] {
            report.facts[0].source_refs = vec![path.into()];
            assert!(validate_source_types(&report, &[], &context).is_err());
        }
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
    fn enforces_concise_limits_and_rejects_retired_fields() {
        for (field, value) in [
            ("answer", serde_json::json!("字".repeat(301))),
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
