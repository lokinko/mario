//! Compact, stage-aware Markdown rendering for LLM prompts.
//! Internal JSON remains available for citations, aliases, and audit only.
use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use crate::models::analysis::AnalysisClaim;
use crate::models::council::ModuleTrace;

const ALIAS_THRESHOLD: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStage {
    Research,
    Module { has_research_brief: bool },
    Synthesizer { has_research_brief: bool },
    Ballot,
    Direct,
}

/// Deterministic short aliases (`h1`, `e1`, ...) for long UUID / observation IDs
/// inside one analysis run. Models read and emit compact IDs; the orchestrator
/// canonicalizes every citation back to real IDs before validation and storage.
#[derive(Debug, Default, Clone)]
pub struct ContextAliases {
    holding_to_alias: HashMap<String, String>,
    alias_to_holding: HashMap<String, String>,
    evidence_to_alias: HashMap<String, String>,
    alias_to_evidence: HashMap<String, String>,
}

impl ContextAliases {
    pub fn from_payload(payload: &Value) -> Self {
        let mut aliases = Self::default();
        if let Some(holdings) = payload
            .get("facts")
            .and_then(|f| f.get("holdings"))
            .and_then(Value::as_object)
        {
            let existing: HashSet<&str> = holdings.keys().map(String::as_str).collect();
            let mut next = 1usize;
            let ordered: BTreeMap<&str, ()> = holdings.keys().map(|k| (k.as_str(), ())).collect();
            for id in ordered.into_keys() {
                if id.len() > ALIAS_THRESHOLD {
                    let alias = loop {
                        let candidate = format!("h{next}");
                        next += 1;
                        if !existing.contains(candidate.as_str()) {
                            break candidate;
                        }
                    };
                    aliases
                        .holding_to_alias
                        .insert(id.to_owned(), alias.clone());
                    aliases.alias_to_holding.insert(alias, id.to_owned());
                }
            }
        }
        aliases.register_evidence_from_payload(payload);
        aliases
    }

    pub fn register_evidence_from_payload(&mut self, payload: &Value) {
        let mut all_ids = Vec::new();
        for key in ["researchEvidence", "nativeWebEvidence"] {
            if let Some(items) = payload.get(key).and_then(Value::as_array) {
                for item in items {
                    if let Some(id) = item.get("id").and_then(Value::as_str) {
                        all_ids.push(id.to_owned());
                    }
                }
            }
        }
        self.register_evidence_ids(&all_ids);
    }

    pub fn register_evidence_ids(&mut self, ids: &[String]) {
        let mut used: HashSet<String> = ids.iter().cloned().collect();
        used.extend(self.alias_to_evidence.keys().cloned());
        let mut next = self.alias_to_evidence.len() + 1;
        for id in ids {
            if id.len() <= ALIAS_THRESHOLD || self.evidence_to_alias.contains_key(id) {
                continue;
            }
            let alias = loop {
                let candidate = format!("e{next}");
                next += 1;
                if !used.contains(&candidate) {
                    used.insert(candidate.clone());
                    break candidate;
                }
            };
            self.evidence_to_alias.insert(id.clone(), alias.clone());
            self.alias_to_evidence.insert(alias, id.clone());
        }
    }

    pub fn holding_display<'a>(&'a self, id: &'a str) -> &'a str {
        self.holding_to_alias
            .get(id)
            .map(String::as_str)
            .unwrap_or(id)
    }

    pub fn evidence_display<'a>(&'a self, id: &'a str) -> &'a str {
        self.evidence_to_alias
            .get(id)
            .map(String::as_str)
            .unwrap_or(id)
    }

    pub fn display_evidence_ids(&self, ids: &[String]) -> Vec<String> {
        ids.iter()
            .map(|id| self.evidence_display(id).to_owned())
            .collect()
    }

    pub fn display_source_ref(&self, path: &str) -> String {
        if let Some(rest) = path.strip_prefix("/facts/holdings/") {
            if let Some((raw_id, tail)) = rest.split_once('/') {
                let unescaped = unsegment(raw_id);
                if let Some(alias) = self.holding_to_alias.get(&unescaped) {
                    return format!("/facts/holdings/{}/{tail}", segment(alias));
                }
            }
        }
        path.to_owned()
    }

    pub fn canonicalize_claims(&self, claims: &mut [AnalysisClaim]) {
        for claim in claims {
            for path in &mut claim.source_refs {
                if let Some(rest) = path.strip_prefix("/facts/holdings/") {
                    if let Some((raw_id, tail)) = rest.split_once('/') {
                        let unescaped = unsegment(raw_id);
                        if let Some(real) = self.alias_to_holding.get(&unescaped) {
                            *path = format!("/facts/holdings/{}/{tail}", segment(real));
                        }
                    }
                }
            }
            for id in &mut claim.evidence_ids {
                if let Some(real) = self.alias_to_evidence.get(id.as_str()) {
                    *id = real.clone();
                }
            }
        }
    }
}

fn cell(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(s) if !s.trim().is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(v) => v.to_string(),
        _ => return None,
    };
    Some(
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('|', "&#124;")
            .replace(['\r', '\n'], "<br>"),
    )
}

fn inline_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().replace(['\r', '\n'], " ")),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        _ => None,
    }
}

fn segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn unsegment(value: &str) -> String {
    value.replace("~1", "/").replace("~0", "~")
}

fn needs_fence(text: &str) -> bool {
    text.len() > 160 || text.contains(['&', '<', '>', '|', '\n', '\r', '`'])
}

fn write_fenced_text(out: &mut String, text: &str) {
    let fence = "`".repeat(
        text.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .max(2)
            + 1,
    );
    out.push_str(&format!("{fence}text\n{text}\n{fence}\n"));
}

fn is_analysis_payload(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.contains_key("question") || map.contains_key("facts"))
}

pub fn render_for_model(value: &Value) -> String {
    if is_analysis_payload(value) {
        let aliases = ContextAliases::from_payload(value);
        render_for_stage(value, RenderStage::Research, &aliases)
    } else {
        let mut out = String::new();
        render_generic(value, "", &mut out);
        out
    }
}

pub fn render_public_research(public_research: &Value) -> String {
    let domain_label = public_research
        .get("domainLabel")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let companies = public_research
        .get("companies")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("symbol").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let topics = public_research
        .get("topics")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let guidance = public_research
        .get("guidance")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut lines = Vec::new();
    if let Some(label) = domain_label {
        lines.push(format!("研究领域: {label}"));
    }
    if !companies.is_empty() {
        lines.push(format!("公司: {}", companies.join(", ")));
    }
    if !topics.is_empty() {
        lines.push(format!("主题: {}", topics.join(", ")));
    }
    if let Some(guidance) = guidance {
        lines.push(format!("检索重点: {guidance}"));
    }
    lines.join("\n")
}

pub fn render_for_stage(value: &Value, stage: RenderStage, aliases: &ContextAliases) -> String {
    let Some(map) = value.as_object() else {
        let mut out = String::new();
        render_generic(value, "", &mut out);
        return out;
    };
    let mut out = String::new();

    if let Some(question) = map.get("question").and_then(Value::as_str) {
        if !question.trim().is_empty() {
            out.push_str("\n## 本轮问题（引用：/question）\n");
            if needs_fence(question) {
                write_fenced_text(&mut out, question);
            } else {
                out.push_str(question);
                out.push('\n');
            }
        }
    }

    if let Some(previous) = map.get("previousTurn").and_then(Value::as_object) {
        render_previous_turn(previous, &mut out);
    }

    if let Some(facts) = map.get("facts").and_then(Value::as_object) {
        render_facts(facts, aliases, &mut out);
    }

    if let Some(items) = map.get("researchEvidence").and_then(Value::as_array) {
        render_evidence_section("研究证据（引用 evidenceIds）", items, aliases, &mut out);
    }

    if let Some(items) = map.get("nativeWebEvidence").and_then(Value::as_array) {
        render_evidence_section("网页原文证据（引用 evidenceIds）", items, aliases, &mut out);
    }

    if stage != RenderStage::Ballot {
        if let Some(leads) = map.get("webLeads").and_then(Value::as_array) {
            if !leads.is_empty() {
                out.push_str("\n## 网页检索线索（公开参考线索：正文分析应充分利用并注明“据网页线索，待公告复核”；非一手原文，禁止写入 facts.evidenceIds）\n");
                for lead in leads {
                    let title = lead
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim();
                    let summary = lead
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim();
                    if !title.is_empty() && !summary.is_empty() {
                        out.push_str(&format!("- {title}：{summary}\n"));
                    } else if !summary.is_empty() {
                        out.push_str(&format!("- {summary}\n"));
                    } else if !title.is_empty() {
                        out.push_str(&format!("- {title}\n"));
                    }
                }
            }
        }
    }

    if let Some(status) = map.get("webSearchStatus").and_then(Value::as_object) {
        let state = status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let warnings = status
            .get("warnings")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        if state != "completed" || !warnings.is_empty() {
            out.push_str(&format!("\n## 网页检索状态：{state}\n"));
            for warning in warnings {
                out.push_str(&format!("- {warning}\n"));
            }
        }
    }

    if let Some(memory) = map
        .get("memory")
        .and_then(|m| m.get("notes"))
        .and_then(Value::as_array)
    {
        if !memory.is_empty() {
            out.push_str("\n## 长期记忆（仅参考偏好与背景，禁止引用为事实）\n");
            for note in memory {
                let kind = note.get("kind").and_then(Value::as_str).unwrap_or_default();
                let origin = note
                    .get("origin")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let date = note
                    .get("date")
                    .or_else(|| note.get("updatedAt"))
                    .and_then(Value::as_str)
                    .and_then(|d| d.get(..10))
                    .unwrap_or_default();
                let statement = note
                    .get("statement")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                if statement.is_empty() {
                    continue;
                }
                if date.is_empty() {
                    out.push_str(&format!("- [{kind}/{origin}] {statement}\n"));
                } else {
                    out.push_str(&format!("- [{kind}/{origin}|{date}] {statement}\n"));
                }
            }
        }
    }

    let include_checks = match stage {
        RenderStage::Research | RenderStage::Direct => true,
        RenderStage::Module { has_research_brief }
        | RenderStage::Synthesizer { has_research_brief } => !has_research_brief,
        RenderStage::Ballot => false,
    };
    if include_checks {
        if let Some(guide) = map.get("discussionGuide").and_then(Value::as_object) {
            if let Some(checks) = guide.get("researchChecks").and_then(Value::as_array) {
                if !checks.is_empty() {
                    let kind = guide
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let label = guide
                        .get("domainLabel")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    match (label.is_empty(), kind.is_empty()) {
                        (false, false) => {
                            out.push_str(&format!("\n## 核查清单（{label}｜{kind}）\n"))
                        }
                        (false, true) => out.push_str(&format!("\n## 核查清单（{label}）\n")),
                        (true, false) => out.push_str(&format!("\n## 核查清单（{kind}）\n")),
                        (true, true) => out.push_str("\n## 核查清单\n"),
                    }
                    for (idx, check) in checks.iter().enumerate() {
                        if let Some(text) = check.as_str() {
                            out.push_str(&format!("{idx}. {text}\n"));
                        }
                    }
                }
            }
        }
    }

    if let Some(brief) = map.get("researchBrief").and_then(Value::as_object) {
        render_research_brief(brief, aliases, &mut out);
    }

    // Any unknown extra top-level keys (excluding intentionally omitted metadata) fall back cleanly.
    const KNOWN_TOP_KEYS: [&str; 11] = [
        "investmentMethodology",
        "webSearch",
        "question",
        "previousTurn",
        "facts",
        "researchEvidence",
        "nativeWebEvidence",
        "webLeads",
        "webSearchStatus",
        "memory",
        "discussionGuide",
    ];
    for (key, val) in map {
        if key == "researchBrief" || KNOWN_TOP_KEYS.contains(&key.as_str()) {
            continue;
        }
        render_generic(val, &format!("/{}", segment(key)), &mut out);
    }

    out
}

fn render_previous_turn(previous: &serde_json::Map<String, Value>, out: &mut String) {
    let mut lines = Vec::new();
    if let Some(q) = previous.get("question").and_then(Value::as_str) {
        if !q.trim().is_empty() {
            lines.push(format!("- 上轮提问(/previousTurn/question): {q}"));
        }
    }
    if let Some(earlier) = previous.get("earlierQuestions").and_then(Value::as_array) {
        for (idx, item) in earlier.iter().enumerate() {
            if let Some(q) = item.as_str() {
                if !q.trim().is_empty() {
                    lines.push(format!(
                        "- 更早提问(/previousTurn/earlierQuestions/{idx}): {q}"
                    ));
                }
            }
        }
    }
    if let Some(a) = previous.get("answer").and_then(Value::as_str) {
        if !a.trim().is_empty() {
            lines.push(format!("- 上轮回答(非事实，仅供参考): {a}"));
        }
    }
    if let Some(suggestions) = previous.get("suggestions").and_then(Value::as_array) {
        let items: Vec<&str> = suggestions.iter().filter_map(Value::as_str).collect();
        if !items.is_empty() {
            lines.push(format!("- 上轮建议(非事实): {}", items.join("；")));
        }
    }
    if let Some(pending) = previous.get("pendingQuestion").and_then(Value::as_str) {
        if !pending.trim().is_empty() {
            lines.push(format!("- 上轮追问(非事实): {pending}"));
        }
    }
    if !lines.is_empty() {
        out.push_str("\n## 上一轮对话\n");
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
}

fn render_facts(
    facts: &serde_json::Map<String, Value>,
    aliases: &ContextAliases,
    out: &mut String,
) {
    if let Some(summary) = facts.get("summary").and_then(Value::as_object) {
        let pairs: Vec<(&String, String)> = summary
            .iter()
            .filter_map(|(k, v)| inline_text(v).map(|val| (k, val)))
            .collect();
        if !pairs.is_empty() {
            out.push_str("\n## 个人摘要（引用格式：/facts/summary/{键}）\n");
            for (k, v) in pairs {
                out.push_str(&format!("- {}: {v}\n", segment(k)));
            }
        }
    }
    if let Some(holdings) = facts.get("holdings").and_then(Value::as_object) {
        if !holdings.is_empty() {
            let fields = [
                "name",
                "symbol",
                "currency",
                "currentQuantity",
                "quantityUnit",
                "contract",
                "costBasis",
                "marketValue",
                "unrealizedProfit",
                "weightPct",
            ];
            let columns = fields
                .into_iter()
                .filter(|key| holdings.values().any(|v| cell(&v[*key]).is_some()))
                .collect::<Vec<_>>();
            out.push_str("\n## 当前持仓（引用格式：/facts/holdings/{行ID}/{列名}）\n| 行ID |");
            for key in &columns {
                out.push_str(&format!(" {key} |"));
            }
            out.push_str("\n| --- |");
            for _ in &columns {
                out.push_str(" --- |");
            }
            out.push('\n');
            for (id, row) in holdings {
                let display_id = aliases.holding_display(id);
                out.push_str(&format!(
                    "| {} |",
                    cell(&Value::String(segment(display_id))).unwrap_or_default()
                ));
                for key in &columns {
                    out.push_str(&format!(" {} |", cell(&row[*key]).unwrap_or_default()));
                }
                out.push('\n');
            }
        }
    }
}

fn render_evidence_section(
    heading: &str,
    items: &[Value],
    aliases: &ContextAliases,
    out: &mut String,
) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {heading}\n"));
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let raw_id = obj.get("id").and_then(Value::as_str).unwrap_or_default();
        let display_id = aliases.evidence_display(raw_id);
        let title = obj
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let mut meta = Vec::new();
        for key in ["assetName", "asOfDate", "sourceTier", "publisher", "stance"] {
            if let Some(val) = obj.get(key).and_then(Value::as_str).map(str::trim) {
                if !(val.is_empty() || key == "stance" && val == "neutral") {
                    meta.push(val);
                }
            }
        }
        let header = match (title.is_empty(), meta.is_empty()) {
            (false, false) => format!("[{display_id}] {title}（{}）", meta.join("｜")),
            (false, true) => format!("[{display_id}] {title}"),
            (true, false) => format!("[{display_id}] {}", meta.join("｜")),
            (true, true) => format!("[{display_id}]"),
        };
        out.push_str(&format!("\n### {header}\n"));
        if let Some(notes) = obj
            .get("notes")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            out.push_str(&format!("说明：{notes}\n"));
        }
        if let Some(claim) = obj.get("claim").and_then(Value::as_str) {
            if !claim.trim().is_empty() {
                if needs_fence(claim) && !claim.starts_with("标的: ") {
                    write_fenced_text(out, claim);
                } else {
                    out.push_str(claim);
                    out.push('\n');
                }
            }
        }
    }
}

fn render_research_brief(
    brief: &serde_json::Map<String, Value>,
    aliases: &ContextAliases,
    out: &mut String,
) {
    out.push_str("\n## 研究简报（线索汇总，引用仍须核对原始资料）\n");
    if let Some(findings) = brief.get("findings").and_then(Value::as_array) {
        if !findings.is_empty() {
            out.push_str("### 已核对发现\n");
            for item in findings {
                let statement = item
                    .get("statement")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let basis = item
                    .get("basis")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let mut refs = Vec::new();
                if let Some(srefs) = item.get("sourceRefs").and_then(Value::as_array) {
                    refs.extend(
                        srefs
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|p| aliases.display_source_ref(p)),
                    );
                }
                if let Some(eids) = item.get("evidenceIds").and_then(Value::as_array) {
                    refs.extend(
                        eids.iter()
                            .filter_map(Value::as_str)
                            .map(|id| aliases.evidence_display(id).to_owned()),
                    );
                }
                out.push_str(&format!("- [{basis}|{}] {statement}\n", refs.join(", ")));
            }
        }
    }
    if let Some(counterpoints) = brief.get("counterpoints").and_then(Value::as_array) {
        if !counterpoints.is_empty() {
            out.push_str("### 反方观点\n");
            for item in counterpoints {
                if let Some(text) = item.as_str() {
                    out.push_str(&format!("- {text}\n"));
                }
            }
        }
    }
    if let Some(unknowns) = brief.get("unknowns").and_then(Value::as_array) {
        if !unknowns.is_empty() {
            out.push_str("### 缺失与未知\n");
            for item in unknowns {
                if let Some(text) = item.as_str() {
                    out.push_str(&format!("- {text}\n"));
                }
            }
        }
    }
    if let Some(checks) = brief.get("checks").and_then(Value::as_array) {
        if !checks.is_empty() {
            out.push_str("### 核查项结论\n");
            for item in checks {
                let idx = item.get("index").and_then(Value::as_u64).unwrap_or(0);
                let status = item
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let note = item.get("note").and_then(Value::as_str).unwrap_or_default();
                out.push_str(&format!("- #{idx} [{status}] {note}\n"));
            }
        }
    }
}

/// Compact Markdown rendering of specialist module analyses for chief strategist synthesis.
pub fn render_modules_for_synthesis(modules: &[ModuleTrace], aliases: &ContextAliases) -> String {
    let mut out = String::new();
    for item in modules {
        out.push_str(&format!("\n## 模块 {} · {}\n", item.id, item.label));
        let Some(analysis) = item.analysis.as_ref() else {
            let reason = item
                .failure
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("该模块本轮未产出有效结果");
            out.push_str(&format!("- 状态：未完成（{reason}）\n"));
            continue;
        };
        out.push_str(&format!("- 核心立场：{}\n", analysis.stance));
        out.push_str(&format!("- 模块分析：{}\n", analysis.summary));
        if !analysis.key_points.is_empty() {
            out.push_str("- 关键论点：\n");
            for point in &analysis.key_points {
                out.push_str(&format!("  - {point}\n"));
            }
        }
        if !analysis.triggers.is_empty() {
            out.push_str("- 触发/证伪条件：\n");
            for trigger in &analysis.triggers {
                out.push_str(&format!("  - {trigger}\n"));
            }
        }
        if !analysis.data_gaps.is_empty() {
            out.push_str("- 资料缺口：\n");
            for gap in &analysis.data_gaps {
                out.push_str(&format!("  - {gap}\n"));
            }
        }
        if !analysis.facts.is_empty() {
            out.push_str("- 引用授权事实：\n");
            for fact in &analysis.facts {
                let mut refs: Vec<String> = fact
                    .source_refs
                    .iter()
                    .map(|p| aliases.display_source_ref(p))
                    .collect();
                refs.extend(
                    fact.evidence_ids
                        .iter()
                        .map(|eid| aliases.evidence_display(eid).to_owned()),
                );
                out.push_str(&format!(
                    "  - ({}|{}) {}\n",
                    fact.basis,
                    refs.join(", "),
                    fact.statement
                ));
            }
        }
    }
    out
}

fn render_generic(value: &Value, path: &str, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut scalars = Vec::new();
            let mut nested = Vec::new();
            for (key, val) in map {
                if let Some(text) = val.as_str() {
                    if text.trim().is_empty() {
                        continue;
                    }
                    if needs_fence(text) {
                        nested.push((key, val));
                    } else {
                        scalars.push((key, text.to_owned()));
                    }
                } else if let Some(val_str) = inline_text(val) {
                    scalars.push((key, val_str));
                } else {
                    nested.push((key, val));
                }
            }
            if !scalars.is_empty() {
                if !path.is_empty() {
                    out.push_str(&format!("\n### {path}\n"));
                }
                for (k, v) in scalars {
                    out.push_str(&format!("- {}: {v}\n", segment(k)));
                }
            }
            for (key, val) in nested {
                render_generic(val, &format!("{path}/{}", segment(key)), out);
            }
        }
        Value::Array(values) => {
            for (index, val) in values.iter().enumerate() {
                render_generic(val, &format!("{path}/{index}"), out);
            }
        }
        Value::String(text) if !text.trim().is_empty() => {
            if !path.is_empty() {
                out.push_str(&format!("\n### {path}\n"));
            }
            if needs_fence(text) {
                write_fenced_text(out, text);
            } else {
                out.push_str(text);
                out.push('\n');
            }
        }
        _ => {
            if let Some(v) = inline_text(value) {
                out.push_str(&format!("- {path}: {v}\n"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_states_are_tables_and_long_originals_keep_exact_text_without_json() {
        let text = "原始问题含 A&B <条件> | 表格符号\n```原文围栏```";
        let payload = serde_json::json!({
            "investmentMethodology": "重复方法论不应出现在user正文",
            "webSearch": true,
            "question": text,
            "facts": {
                "summary": {"monthlySurplus": "0"},
                "holdings": {
                    "h": {"name": "测试标的", "currency": "CNY", "marketValue": "900719925474099.91"}
                }
            },
            "researchEvidence": [{"id": "e1", "title": "财报", "claim": "原文"}]
        });
        let rendered = render_for_model(&payload);
        assert!(rendered.contains(text));
        assert!(rendered.contains("````text"));
        assert!(rendered.contains("| 行ID | name | currency | marketValue |"));
        assert!(rendered.contains("900719925474099.91"));
        assert!(rendered.contains("- monthlySurplus: 0"));
        assert!(rendered.contains("### [e1] 财报"));
        assert!(!rendered.contains("重复方法论不应出现在user正文"));
        assert!(!rendered.contains("webSearch"));
        assert!(!rendered.contains("\"marketValue\""));
        assert!(!rendered.contains("null"));
    }

    #[test]
    fn long_uuids_use_short_aliases_and_canonicalize_back_to_real_ids() {
        let holding_uuid = "065d8511-1776-4bd4-b7ad-094daec2b1b4";
        let evidence_uuid = "9a4c8d12-2345-6789-abcd-ef0123456789";
        let payload = serde_json::json!({
            "question": "仓位是否过高？",
            "facts": {
                "summary": {},
                "holdings": {
                    holding_uuid: {"name": "核心红利", "currency": "CNY", "marketValue": "500000"}
                }
            },
            "researchEvidence": [
                {"id": evidence_uuid, "title": "年报", "claim": "派息率50%"}
            ]
        });
        let aliases = ContextAliases::from_payload(&payload);
        let rendered = render_for_stage(&payload, RenderStage::Research, &aliases);
        assert!(rendered.contains("| h1 | 核心红利 | CNY | 500000 |"));
        assert!(rendered.contains("### [e1] 年报"));
        assert!(!rendered.contains(holding_uuid));
        assert!(!rendered.contains(evidence_uuid));

        let mut claims = vec![
            AnalysisClaim {
                statement: "持仓50万".into(),
                basis: "user_data".into(),
                source_refs: vec!["/facts/holdings/h1/marketValue".into()],
                evidence_ids: vec![],
            },
            AnalysisClaim {
                statement: "派息率50%".into(),
                basis: "research_evidence".into(),
                source_refs: vec![],
                evidence_ids: vec!["e1".into()],
            },
        ];
        aliases.canonicalize_claims(&mut claims);
        assert_eq!(
            claims[0].source_refs,
            vec![format!("/facts/holdings/{holding_uuid}/marketValue")]
        );
        assert_eq!(claims[1].evidence_ids, vec![evidence_uuid.to_string()]);
    }

    #[test]
    fn stage_rendering_omits_resolved_checks_and_ballot_web_leads() {
        let payload = serde_json::json!({
            "question": "分红如何",
            "facts": {"summary": {}},
            "webLeads": [{"title": "新闻线索", "summary": "某媒体传闻"}],
            "discussionGuide": {
                "kind": "dividend_discussion",
                "publicResearch": {"companies": [{"symbol": "600036.SS"}], "topics": ["dividend_composition_and_commitment"]},
                "researchChecks": ["核查项零", "核查项一"]
            },
            "researchBrief": {
                "findings": [],
                "counterpoints": [],
                "unknowns": [],
                "checks": [{"index": 0, "status": "covered", "note": "已核查"}]
            }
        });
        let aliases = ContextAliases::from_payload(&payload);
        let research = render_for_stage(&payload, RenderStage::Research, &aliases);
        assert!(research.contains("0. 核查项零"));
        assert!(research.contains("新闻线索"));
        assert!(!research.contains("dividend_composition_and_commitment"));

        let module_view = render_for_stage(
            &payload,
            RenderStage::Module {
                has_research_brief: true,
            },
            &aliases,
        );
        assert!(!module_view.contains("0. 核查项零"));
        assert!(module_view.contains("## 研究简报"));
        assert!(module_view.contains("新闻线索"));

        let ballot = render_for_stage(&payload, RenderStage::Ballot, &aliases);
        assert!(!ballot.contains("0. 核查项零"));
        assert!(!ballot.contains("新闻线索"));
    }
}
