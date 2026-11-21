//! Machine contracts and parsers for the research agent and specialist modules.
use std::collections::HashSet;

use super::structured_output::{
    citation_rules, extract_json, validate_claims, MAX_STRUCTURED_OUTPUT_CHARS,
};
use crate::models::council::{
    ModuleAnalysis, ResearchBrief, MODULE_FACTS_MAX, MODULE_GAP_CHARS, MODULE_ITEM_CHARS,
    MODULE_STANCE_CHARS, MODULE_SUMMARY_CHARS, RESEARCH_FINDINGS_MAX, RESEARCH_NOTES_MAX,
    RESEARCH_NOTE_CHARS,
};

pub const RESEARCH_BRIEF_CONTRACT: &str = concat!(
    r#"只返回JSON，不加代码块或说明：
{"findings":[{"statement":"与本题直接相关的输入事实或来源要点","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}],"counterpoints":["与用户倾向或常见结论相反、且有资料支撑或值得核查的观点，最多300字"],"unknowns":["影响判断但本轮资料缺失的信息，最多300字"],"checks":[{"index":0,"status":"covered或not_applicable或missing_source","note":"该核查项的结论依据或缺口，最多300字"}]}
"#,
    "findings为0–10条，user_data的sourceRefs须非空且每条最多12项。",
    citation_rules!(),
    r#"counterpoints与unknowns各0–6条。checks对discussionGuide.researchChecks的每一项恰好给一条，index从0开始：授权资料已直接回答该项才写covered；资料缺失写missing_source；与本题无关写not_applicable。不写建议、结论或行动。webLeads是公开检索线索，禁止写入findings.evidenceIds，但可将其中的关键行业/财报/估值线索提炼进counterpoints或unknowns并注明“据网页线索，待公告复核”。"#
);

pub const MODULE_ANALYSIS_CONTRACT: &str = concat!(
    r#"只返回JSON，不加代码块或说明：
{"stance":"本模块的一句话核心立场与判断，最多120字","summary":"本模块的高密度专业分析，直接给逻辑、盈利/估值/仓位/风险拆解，严禁官僚套话和把查财报踢回给用户，最多600字","keyPoints":["支撑本模块判断的核心论点，1–4条，每条最多240字"],"triggers":["明确的可执行触发条件、边界条件或证伪锚，1–3条，每条最多240字"],"dataGaps":["真正卡住决策的1–3个关键信息缺口，可为空数组，每条最多180字"],"facts":[{"statement":"引用的授权事实","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}]}
"#,
    "facts为0–5条（有授权资料支撑时尽量列出关键事实），user_data的sourceRefs须非空且每条最多12项；webLeads可写入summary/keyPoints并注明“据网页线索，待公告复核”，但严禁写入facts.evidenceIds。",
    citation_rules!()
);

/// Research findings may cite exactly what reports may cite; checks must cover the frozen list.
#[cfg(test)]
pub fn parse_research_brief(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
) -> Result<ResearchBrief, String> {
    parse_research_brief_with_aliases(
        content,
        allowed_evidence_ids,
        context,
        &crate::context::ContextAliases::default(),
    )
}

pub fn parse_research_brief_with_aliases(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
    aliases: &crate::context::ContextAliases,
) -> Result<ResearchBrief, String> {
    if content.chars().count() > MAX_STRUCTURED_OUTPUT_CHARS {
        return Err(format!("研究简报超过 {MAX_STRUCTURED_OUTPUT_CHARS} 个字符"));
    }
    let mut brief: ResearchBrief = serde_json::from_str(extract_json(content)?)
        .map_err(|error| format!("JSON 结构无效：{error}"))?;
    if brief.findings.len() > RESEARCH_FINDINGS_MAX {
        return Err(format!("findings 最多 {RESEARCH_FINDINGS_MAX} 条"));
    }
    aliases.canonicalize_claims(&mut brief.findings);
    validate_claims("findings", &brief.findings, allowed_evidence_ids, context)?;
    for (field, notes) in [
        ("counterpoints", &brief.counterpoints),
        ("unknowns", &brief.unknowns),
    ] {
        if notes.len() > RESEARCH_NOTES_MAX {
            return Err(format!("{field} 最多 {RESEARCH_NOTES_MAX} 条"));
        }
        for note in notes {
            check_note(field, note)?;
        }
    }
    let expected = context["discussionGuide"]["researchChecks"]
        .as_array()
        .map_or(0, Vec::len);
    let mut indices = brief
        .checks
        .iter()
        .map(|check| check.index)
        .collect::<Vec<_>>();
    indices.sort_unstable();
    if indices != (0..expected).collect::<Vec<_>>() {
        return Err(format!(
            "checks 必须对 researchChecks 的 {expected} 项各给一条，index 从0开始且不重复"
        ));
    }
    for check in &brief.checks {
        check_note("checks.note", &check.note)?;
    }
    Ok(brief)
}

#[cfg(test)]
pub fn parse_module_analysis(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
) -> Result<ModuleAnalysis, String> {
    parse_module_analysis_with_aliases(
        content,
        allowed_evidence_ids,
        context,
        &crate::context::ContextAliases::default(),
    )
}

pub fn parse_module_analysis_with_aliases(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
    aliases: &crate::context::ContextAliases,
) -> Result<ModuleAnalysis, String> {
    if content.chars().count() > MAX_STRUCTURED_OUTPUT_CHARS {
        return Err(format!("模块分析超过 {MAX_STRUCTURED_OUTPUT_CHARS} 个字符"));
    }
    let mut analysis: ModuleAnalysis = serde_json::from_str(extract_json(content)?)
        .map_err(|error| format!("JSON 结构无效：{error}"))?;
    check_text_len("stance", &analysis.stance, MODULE_STANCE_CHARS)?;
    check_text_len("summary", &analysis.summary, MODULE_SUMMARY_CHARS)?;
    if !(1..=4).contains(&analysis.key_points.len()) {
        return Err("keyPoints 数量必须在 1—4 之间".into());
    }
    for point in &analysis.key_points {
        check_text_len("keyPoints", point, MODULE_ITEM_CHARS)?;
    }
    if !(1..=3).contains(&analysis.triggers.len()) {
        return Err("triggers 数量必须在 1—3 之间".into());
    }
    for trigger in &analysis.triggers {
        check_text_len("triggers", trigger, MODULE_ITEM_CHARS)?;
    }
    if analysis.data_gaps.len() > 3 {
        return Err("dataGaps 最多 3 条".into());
    }
    for gap in &analysis.data_gaps {
        check_text_len("dataGaps", gap, MODULE_GAP_CHARS)?;
    }
    if analysis.facts.len() > MODULE_FACTS_MAX {
        return Err(format!("facts 最多 {MODULE_FACTS_MAX} 条"));
    }
    aliases.canonicalize_claims(&mut analysis.facts);
    validate_claims("facts", &analysis.facts, allowed_evidence_ids, context)?;
    Ok(analysis)
}

fn check_note(field: &str, note: &str) -> Result<(), String> {
    check_text_len(field, note, RESEARCH_NOTE_CHARS)
}

fn check_text_len(field: &str, text: &str, max: usize) -> Result<(), String> {
    let length = text.trim().chars().count();
    if length == 0 || length > max {
        return Err(format!("{field} 须非空且不超过 {max} 字"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> serde_json::Value {
        serde_json::json!({
            "question": "分红能持续吗？",
            "facts": {"summary": {"monthlySurplus": "8000"}},
            "previousTurn": {"answer": "模型旧结论"},
            "memory": {"notes": [{"statement": "偏好先看现金流"}]},
            "discussionGuide": {"researchChecks": ["核对分红", "核对现金流"]}
        })
    }

    fn brief() -> serde_json::Value {
        serde_json::json!({
            "findings": [
                {"statement":"月结余8000","basis":"user_data","sourceRefs":["/facts/summary/monthlySurplus"],"evidenceIds":[]},
                {"statement":"公告列明分红","basis":"research_evidence","sourceRefs":[],"evidenceIds":["e1"]}
            ],
            "counterpoints": ["盈利下滑时分红可能下调"],
            "unknowns": ["未来资本开支"],
            "checks": [
                {"index":1,"status":"missing_source","note":"缺同期现金流量表"},
                {"index":0,"status":"covered","note":"公告列明分红"}
            ]
        })
    }

    #[test]
    fn research_briefs_follow_report_citation_rules_and_cover_every_check() {
        let allowed = HashSet::from(["e1".to_string()]);
        parse_research_brief(&brief().to_string(), &allowed, &context()).unwrap();

        for (pointer, value) in [
            (
                "/findings/0/sourceRefs",
                serde_json::json!(["/previousTurn/answer"]),
            ),
            (
                "/findings/0/sourceRefs",
                serde_json::json!(["/memory/notes/0/statement"]),
            ),
            (
                "/findings/1/evidenceIds",
                serde_json::json!(["web-summary"]),
            ),
            ("/checks/0/index", serde_json::json!(0)),
            ("/checks/0/note", serde_json::json!(" ")),
            ("/unknowns/0", serde_json::json!("字".repeat(301))),
        ] {
            let mut invalid = brief();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(
                parse_research_brief(&invalid.to_string(), &allowed, &context()).is_err(),
                "{pointer}"
            );
        }
        let mut missing = brief();
        missing["checks"].as_array_mut().unwrap().pop();
        assert!(parse_research_brief(&missing.to_string(), &allowed, &context()).is_err());
        let mut advice = brief();
        advice["actions"] = serde_json::json!(["买入"]);
        assert!(parse_research_brief(&advice.to_string(), &allowed, &context()).is_err());
    }

    #[test]
    fn module_analysis_enforces_schema_lengths_and_citations() {
        let allowed = HashSet::from(["e1".to_string()]);
        let valid = serde_json::json!({
            "stance": "周期中段位置未确认，暂不急于加仓",
            "summary": "云铝具备水电铝成本优势，但电解铝盈利高度依赖吨铝价差与氧化铝成本。",
            "keyPoints": ["水电铝电价优势显著", "需以周期中枢盈利而非盈利高点低PE定价"],
            "triggers": ["铝价回落至行业高成本线且PB回落至历史低分位时分批左侧布局"],
            "dataGaps": ["最新季度电解铝产量与吨铝完全成本"],
            "facts": [{"statement": "月结余8000", "basis": "user_data", "sourceRefs": ["/facts/summary/monthlySurplus"], "evidenceIds": []}]
        });
        parse_module_analysis(&valid.to_string(), &allowed, &context()).unwrap();

        let mut bad_fact = valid.clone();
        bad_fact["facts"][0]["sourceRefs"] = serde_json::json!(["/previousTurn/answer"]);
        assert!(parse_module_analysis(&bad_fact.to_string(), &allowed, &context()).is_err());

        let mut empty_points = valid.clone();
        empty_points["keyPoints"] = serde_json::json!([]);
        assert!(parse_module_analysis(&empty_points.to_string(), &allowed, &context()).is_err());

        let mut overflow_stance = valid;
        overflow_stance["stance"] = serde_json::json!("字".repeat(MODULE_STANCE_CHARS + 1));
        assert!(parse_module_analysis(&overflow_stance.to_string(), &allowed, &context()).is_err());
    }
}
