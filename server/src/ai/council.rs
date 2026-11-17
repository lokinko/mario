//! Machine contracts and parsers for the research agent and ballots.
use std::collections::HashSet;

use super::structured_output::{
    citation_rules, extract_json, validate_claims, MAX_STRUCTURED_OUTPUT_CHARS,
};
use crate::models::{
    analysis::StructuredAnalysis,
    council::{
        validate_ballot, Ballot, ResearchBrief, RESEARCH_FINDINGS_MAX, RESEARCH_NOTES_MAX,
        RESEARCH_NOTE_CHARS,
    },
};

pub const RESEARCH_BRIEF_CONTRACT: &str = concat!(
    r#"只返回JSON，不加代码块或说明：
{"findings":[{"statement":"与本题直接相关的输入事实或来源要点","basis":"user_data或research_evidence","sourceRefs":[],"evidenceIds":[]}],"counterpoints":["与用户倾向或常见结论相反、且有资料支撑或值得核查的观点，最多300字"],"unknowns":["影响判断但本轮资料缺失的信息，最多300字"],"checks":[{"index":0,"status":"covered或not_applicable或missing_source","note":"该核查项的结论依据或缺口，最多300字"}]}
"#,
    "findings为0–10条，user_data的sourceRefs须非空且每条最多12项。",
    citation_rules!(),
    r#"counterpoints与unknowns各0–6条。checks对discussionGuide.researchChecks的每一项恰好给一条，index从0开始：授权资料已直接回答该项才写covered；资料缺失写missing_source；与本题无关写not_applicable。不写建议、结论或行动。webLeads是未核实的网页摘要线索，不是证据，只能写进unknowns作为待核实方向。"#
);

pub const BALLOT_CONTRACT: &str = r#"只返回JSON，不加代码块或说明：
{"preferred":"你会采纳的方案ID","assessments":[{"proposal":"方案ID","scores":{"facts":0,"citations":0,"reasoning":0,"counterpoints":0,"actionability":0},"veto":null,"objection":"对该方案最重要的一条具体质疑，最多300字"}]}
对展示的每个方案恰好评价一次。每项0–2分：facts 事实与冻结资料一致、不编造；citations 引用可追溯，没有把网页线索、记忆或模型旧回答当证据；reasoning 推理和计算可复核，结论与自身事实不矛盾；counterpoints 正视最强反方和不确定性；actionability 下一步紧扣本题、条件清楚可执行。veto只用于严重缺陷：编造或错引事实与来源、结论与自身事实矛盾、迎合用户倾向而回避相反证据、越过账本自行生成交易或仓位规则；有则写明不超过200字的理由，否则为null。2分表示该维度找不到可指出的缺陷；objection指向的维度不给2分。preferred必填，在展示的方案中选一个你会原样采纳给用户的方案，分数相同时也须按哪份更能帮用户做出本题决定选出一个；不能选你否决的方案，只有全部否决时为null。按证据和推理打分与选择，不按立场、语气或是否与你的方案一致。"#;

/// Research findings may cite exactly what reports may cite; checks must cover the frozen list.
pub fn parse_research_brief(
    content: &str,
    allowed_evidence_ids: &HashSet<String>,
    context: &serde_json::Value,
) -> Result<ResearchBrief, String> {
    if content.chars().count() > MAX_STRUCTURED_OUTPUT_CHARS {
        return Err(format!("研究简报超过 {MAX_STRUCTURED_OUTPUT_CHARS} 个字符"));
    }
    let brief: ResearchBrief = serde_json::from_str(extract_json(content)?)
        .map_err(|error| format!("JSON 结构无效：{error}"))?;
    if brief.findings.len() > RESEARCH_FINDINGS_MAX {
        return Err(format!("findings 最多 {RESEARCH_FINDINGS_MAX} 条"));
    }
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

pub fn parse_ballot(content: &str, shown: &[&str]) -> Result<Ballot, String> {
    if content.chars().count() > MAX_STRUCTURED_OUTPUT_CHARS {
        return Err(format!("投票超过 {MAX_STRUCTURED_OUTPUT_CHARS} 个字符"));
    }
    let ballot: Ballot = serde_json::from_str(extract_json(content)?)
        .map_err(|error| format!("JSON 结构无效：{error}"))?;
    validate_ballot(&ballot, shown)?;
    Ok(ballot)
}

/// Proposals shown to a voter carry only their ID, so lens names cannot sway the score.
pub fn ballot_material(proposals: &[(&str, &StructuredAnalysis)]) -> serde_json::Value {
    serde_json::Value::Array(
        proposals
            .iter()
            .map(|(id, report)| serde_json::json!({"id": id, "report": report}))
            .collect(),
    )
}

fn check_note(field: &str, note: &str) -> Result<(), String> {
    let length = note.trim().chars().count();
    if length == 0 || length > RESEARCH_NOTE_CHARS {
        return Err(format!("{field} 须非空且不超过 {RESEARCH_NOTE_CHARS} 字"));
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
    fn ballots_must_score_exactly_the_shown_proposals() {
        let ballot = serde_json::json!({"preferred":"P2","assessments":[{"proposal":"P2","scores":{"facts":2,"citations":1,"reasoning":2,"counterpoints":1,"actionability":2},"veto":null,"objection":"未说明等待成本"}]});
        parse_ballot(&ballot.to_string(), &["P2"]).unwrap();
        assert!(parse_ballot(&ballot.to_string(), &["P2", "P3"]).is_err());
        assert!(parse_ballot("我投 P2", &["P2"]).is_err());
        let mut no_preference = ballot.clone();
        no_preference.as_object_mut().unwrap().remove("preferred");
        assert!(parse_ballot(&no_preference.to_string(), &["P2"]).is_err());
    }
}
