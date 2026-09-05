use std::collections::HashSet;

use crate::models::MemoryItem;

pub trait MemoryRetriever: Send + Sync {
    fn search(&self, query: &str, items: &[MemoryItem], limit: usize) -> Vec<MemoryItem>;
}

pub struct LexicalMemoryRetriever;

impl MemoryRetriever for LexicalMemoryRetriever {
    fn search(&self, query: &str, items: &[MemoryItem], limit: usize) -> Vec<MemoryItem> {
        let query_tokens = tokens(query);
        let mut scored = items
            .iter()
            .map(|item| {
                let haystack = format!("{} {}", item.title, item.content).to_lowercase();
                let score = query_tokens
                    .iter()
                    .filter(|token| haystack.contains(token.as_str()))
                    .count();
                (score, item)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.created_at.cmp(&a.1.created_at))
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
    let mut result = normalized
        .split(|c: char| c.is_whitespace() || "，。！？；：,.!?;:()（）/".contains(c))
        .filter(|part| part.chars().count() >= 2)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let chars = normalized
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<Vec<_>>();
    for pair in chars.windows(2).take(120) {
        result.insert(pair.iter().collect());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieves_relevant_decision() {
        let items = vec![
            MemoryItem {
                id: "1".into(),
                kind: "decision".into(),
                title: "宽基指数".into(),
                content: "长期配置".into(),
                created_at: "1".into(),
            },
            MemoryItem {
                id: "2".into(),
                kind: "decision".into(),
                title: "黄金".into(),
                content: "避险资产".into(),
                created_at: "2".into(),
            },
        ];
        let result = LexicalMemoryRetriever.search("复盘我的指数配置", &items, 3);
        assert_eq!(result[0].id, "1");
    }
}
