use std::collections::HashSet;

use crate::models::ResearchEvidence;

pub trait EvidenceRetriever: Send + Sync {
    fn search(
        &self,
        query: &str,
        items: &[ResearchEvidence],
        limit: usize,
    ) -> Vec<ResearchEvidence>;
}

pub struct LexicalEvidenceRetriever;

impl EvidenceRetriever for LexicalEvidenceRetriever {
    fn search(
        &self,
        query: &str,
        items: &[ResearchEvidence],
        limit: usize,
    ) -> Vec<ResearchEvidence> {
        let query_tokens = tokens(query);
        let normalized_query = query.to_lowercase();
        let mut scored = items
            .iter()
            .filter(|item| item.active)
            .map(|item| {
                let identity =
                    format!("{} {} {}", item.asset_name, item.title, item.publisher).to_lowercase();
                let details = format!("{} {}", item.claim, item.notes).to_lowercase();
                let identity_score = query_tokens
                    .iter()
                    .filter(|token| identity.contains(token.as_str()))
                    .count()
                    * 3;
                let detail_score = query_tokens
                    .iter()
                    .filter(|token| details.contains(token.as_str()))
                    .count();
                let exact_asset_bonus = usize::from(
                    !item.asset_name.trim().is_empty()
                        && normalized_query.contains(&item.asset_name.to_lowercase()),
                ) * 5;
                (identity_score + detail_score + exact_asset_bonus, item)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.as_of_date.cmp(&a.1.as_of_date))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        scored
            .into_iter()
            .filter(|(score, _)| *score > 0)
            .take(limit)
            .map(|(_, item)| item.clone())
            .collect()
    }
}

fn tokens(input: &str) -> HashSet<String> {
    let normalized = input.to_lowercase();
    let separators = "，。！？；：,.!?;:()（）/、-_";
    let mut result = normalized
        .split(|character: char| character.is_whitespace() || separators.contains(character))
        .filter(|part| part.chars().count() >= 2)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let characters = normalized
        .chars()
        .filter(|character| !character.is_whitespace() && !separators.contains(*character))
        .collect::<Vec<_>>();
    for pair in characters.windows(2).take(160) {
        result.insert(pair.iter().collect());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(id: &str, asset: &str, claim: &str, active: bool) -> ResearchEvidence {
        ResearchEvidence {
            id: id.into(),
            asset_name: asset.into(),
            title: format!("{asset} 年报"),
            publisher: "公司官网".into(),
            source_url: "https://example.com/report".into(),
            source_tier: "一手来源".into(),
            evidence_type: "公司披露".into(),
            stance: "背景".into(),
            as_of_date: "2026-06-30".into(),
            claim: claim.into(),
            notes: String::new(),
            active,
            captured_at: "2026-09-05".into(),
        }
    }

    #[test]
    fn retrieves_relevant_active_evidence_and_excludes_archived_items() {
        let items = vec![
            evidence("1", "全球指数", "费用率保持稳定", true),
            evidence("2", "黄金", "储备需求变化", true),
            evidence("3", "全球指数", "旧版已归档", false),
        ];
        let result = LexicalEvidenceRetriever.search("检查全球指数费用", &items, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "1");
    }
}
