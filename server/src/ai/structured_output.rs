use std::collections::HashSet;

use crate::models::{AnalysisClaim, StructuredAnalysis};

pub const STRUCTURED_ANALYSIS_CONTRACT: &str = r#"
只返回一个 JSON 对象，不要使用 Markdown 代码块或添加前后说明。结构必须严格为：
{
  "verdict": "当前最重要判断",
  "facts": [{"statement":"已知事实","basis":"user_data 或 research_evidence","evidenceIds":["证据 ID"]}],
  "inferences": [{"statement":"基于事实的推断","basis":"user_data 或 research_evidence","evidenceIds":[]}],
  "unknowns": ["仍需核实的信息"],
  "options": [{"name":"方案名称","suitableWhen":"适用条件","tradeoffs":["机会成本或取舍"],"risks":["主要风险"]}],
  "actions": [{"action":"下一步行动","rationale":"为什么","reversible":true,"reviewTrigger":"何时复盘或停止"}],
  "reviewTriggers": ["未来复盘或证伪条件"]
}
facts、unknowns、options、actions、reviewTriggers 均至少一项。basis 只能是 user_data 或 research_evidence；basis 为 research_evidence 时必须引用本次上下文里的证据 ID，basis 为 user_data 时 evidenceIds 必须为空。不得编造证据 ID。inferences 可以为空。所有字段都必须存在。"#;

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

pub fn render_report(report: &StructuredAnalysis) -> String {
    let mut sections = vec![format!("当前判断\n{}", report.verdict)];
    sections.push(format_claims("已知事实", &report.facts));
    if !report.inferences.is_empty() {
        sections.push(format_claims("合理推断", &report.inferences));
    }
    sections.push(format!("仍待核实\n{}", bullet_list(&report.unknowns)));

    let options = report
        .options
        .iter()
        .map(|option| {
            format!(
                "- {}：适用于 {}；取舍：{}；风险：{}",
                option.name,
                option.suitable_when,
                option.tradeoffs.join("、"),
                option.risks.join("、")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    sections.push(format!("方案比较\n{options}"));

    let actions = report
        .actions
        .iter()
        .map(|action| {
            let reversibility = if action.reversible {
                "可逆"
            } else {
                "需单独确认"
            };
            format!(
                "- {}（{}）：{}；复盘：{}",
                action.action, reversibility, action.rationale, action.review_trigger
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    sections.push(format!("下一步行动\n{actions}"));
    sections.push(format!(
        "复盘与证伪条件\n{}",
        bullet_list(&report.review_triggers)
    ));
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
    check_text("verdict", &report.verdict, 2_000)?;
    check_count("facts", report.facts.len(), 1, 12)?;
    check_count("inferences", report.inferences.len(), 0, 12)?;
    check_count("unknowns", report.unknowns.len(), 1, 12)?;
    check_count("options", report.options.len(), 1, 4)?;
    check_count("actions", report.actions.len(), 1, 8)?;
    check_count("reviewTriggers", report.review_triggers.len(), 1, 8)?;

    for claim in report.facts.iter().chain(&report.inferences) {
        validate_claim(claim, allowed_evidence_ids)?;
    }
    validate_text_list("unknowns", &report.unknowns, 800)?;
    validate_text_list("reviewTriggers", &report.review_triggers, 800)?;

    for option in &report.options {
        check_text("options.name", &option.name, 200)?;
        check_text("options.suitableWhen", &option.suitable_when, 800)?;
        check_count("options.tradeoffs", option.tradeoffs.len(), 1, 6)?;
        check_count("options.risks", option.risks.len(), 1, 6)?;
        validate_text_list("options.tradeoffs", &option.tradeoffs, 600)?;
        validate_text_list("options.risks", &option.risks, 600)?;
    }
    for action in &report.actions {
        check_text("actions.action", &action.action, 800)?;
        check_text("actions.rationale", &action.rationale, 800)?;
        check_text("actions.reviewTrigger", &action.review_trigger, 800)?;
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

fn validate_text_list(name: &str, values: &[String], max: usize) -> Result<(), String> {
    for value in values {
        check_text(name, value, max)?;
    }
    Ok(())
}

fn format_claims(title: &str, claims: &[AnalysisClaim]) -> String {
    let items = claims
        .iter()
        .map(|claim| {
            if claim.evidence_ids.is_empty() {
                format!("- {}", claim.statement)
            } else {
                format!(
                    "- {}（证据：{}）",
                    claim.statement,
                    claim.evidence_ids.join("、")
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{title}\n{items}")
}

fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> String {
        serde_json::json!({
            "verdict": "先降低集中风险",
            "facts": [{"statement":"集中度为 60%","basis":"user_data","evidenceIds":[]}],
            "inferences": [{"statement":"回撤可能超出预算","basis":"research_evidence","evidenceIds":["e1"]}],
            "unknowns": ["未来现金流是否稳定"],
            "options": [{"name":"分批调整","suitableWhen":"无需立即用钱","tradeoffs":["可能错过上涨"],"risks":["执行拖延"]}],
            "actions": [{"action":"先核对目标权重","rationale":"减少错误交易","reversible":true,"reviewTrigger":"一周后复核"}],
            "reviewTriggers": ["集中度降至目标范围"]
        })
        .to_string()
    }

    #[test]
    fn accepts_a_valid_report_and_renders_it() {
        let report =
            parse_structured_analysis(&valid_json(), &HashSet::from(["e1".into()])).unwrap();
        assert!(render_report(&report).contains("先降低集中风险"));
        assert!(render_report(&report).contains("证据：e1"));
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
