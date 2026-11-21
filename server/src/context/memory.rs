//! Choose which long-term memory notes enter one analysis.
use std::collections::HashSet;

use serde_json::{json, Value};

use crate::models::{
    analysis::AnalysisRequest,
    memory::{MemoryKind, MemoryNote, MemoryOrigin},
};

/// User-stated preferences always fit; the rest of the budget goes to relevant notes.
const PREFERENCES_MAX: usize = 4;
const NOTES_MAX: usize = 10;

/// Viewpoints and context can carry details from any financial group, so a run that
/// narrows authorization keeps only preferences the user stated in their own words.
pub fn select<'a>(request: &AnalysisRequest, notes: &'a [MemoryNote]) -> Vec<&'a MemoryNote> {
    let selection = &request.context_selection;
    if !selection.include_memory {
        return Vec::new();
    }
    let mut active = notes.iter().filter(|note| note.active).collect::<Vec<_>>();
    active.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    let mut chosen = active
        .iter()
        .filter(|note| {
            note.kind == MemoryKind::Preference && note.origin == MemoryOrigin::UserStated
        })
        .take(PREFERENCES_MAX)
        .copied()
        .collect::<Vec<_>>();
    if !selection.all_financial_groups() {
        return chosen;
    }
    let query = bigrams(
        &std::iter::once(request.question.as_str())
            .chain(request.previous_turn.iter().flat_map(|turn| {
                std::iter::once(turn.question.as_str())
                    .chain(turn.earlier_questions.iter().map(String::as_str))
            }))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut relevant = active
        .iter()
        .filter(|note| !chosen.iter().any(|chosen| chosen.id == note.id))
        .filter_map(|note| {
            let overlap = bigrams(&note.statement).intersection(&query).count();
            (overlap > 0).then_some((overlap, *note))
        })
        .collect::<Vec<_>>();
    // Stable sort keeps the recency order among equally relevant notes.
    relevant.sort_by(|a, b| b.0.cmp(&a.0));
    chosen.extend(
        relevant
            .into_iter()
            .take(NOTES_MAX - chosen.len())
            .map(|(_, note)| note),
    );
    chosen
}

pub fn payload(notes: &[&MemoryNote]) -> Value {
    json!({
        "notes": notes.iter().map(|note| {
            let mut item = serde_json::Map::new();
            item.insert("kind".into(), json!(note.kind));
            item.insert("origin".into(), json!(note.origin));
            item.insert("statement".into(), json!(note.statement));
            if let Some(date) = note.updated_at.get(..10).filter(|d| !d.trim().is_empty()) {
                item.insert("date".into(), json!(date));
            }
            Value::Object(item)
        }).collect::<Vec<_>>()
    })
}

fn bigrams(text: &str) -> HashSet<(char, char)> {
    let chars = text
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<Vec<_>>();
    chars.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, kind: MemoryKind, origin: MemoryOrigin, statement: &str) -> MemoryNote {
        MemoryNote {
            id: id.into(),
            kind,
            origin,
            statement: statement.into(),
            rationale: "来源分析中的依据".into(),
            source_analysis_ids: vec!["a1".into()],
            quote: "原文".into(),
            active: true,
            created_at: "2026-11-16T00:00:00+00:00".into(),
            updated_at: format!("2026-11-16T00:00:{:02}+00:00", id.len()),
        }
    }

    fn request(question: &str) -> AnalysisRequest {
        serde_json::from_value(json!({"question": question, "workflow": "council"})).unwrap()
    }

    #[test]
    fn preferences_always_fit_and_other_notes_need_relevance() {
        let notes = vec![
            note(
                "pref",
                MemoryKind::Preference,
                MemoryOrigin::UserStated,
                "先讲结论再讲依据",
            ),
            note(
                "bank",
                MemoryKind::Viewpoint,
                MemoryOrigin::ModelInferred,
                "银行股分红依赖拨备释放",
            ),
            note(
                "cash",
                MemoryKind::Context,
                MemoryOrigin::UserStated,
                "两年后需要一笔购房首付",
            ),
            MemoryNote {
                active: false,
                ..note(
                    "old",
                    MemoryKind::Preference,
                    MemoryOrigin::UserStated,
                    "旧偏好",
                )
            },
        ];
        let chosen = select(&request("这家银行的分红还能维持吗？"), &notes);
        let ids = chosen
            .iter()
            .map(|note| note.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["pref", "bank"]);
        assert_eq!(payload(&chosen)["notes"][1]["origin"], "modelInferred");
    }

    #[test]
    fn narrowed_authorization_or_opt_out_keeps_no_financial_memory() {
        let notes = vec![
            note(
                "pref",
                MemoryKind::Preference,
                MemoryOrigin::UserStated,
                "先讲结论",
            ),
            note(
                "cash",
                MemoryKind::Context,
                MemoryOrigin::UserStated,
                "两年后需要购房首付",
            ),
        ];
        let mut narrowed = request("购房首付怎么准备");
        narrowed.context_selection.include_holdings = false;
        let ids = select(&narrowed, &notes)
            .iter()
            .map(|note| note.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["pref"]);
        let mut opted_out = request("购房首付怎么准备");
        opted_out.context_selection.include_memory = false;
        assert!(select(&opted_out, &notes).is_empty());
    }
}
