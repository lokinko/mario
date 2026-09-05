use std::{cmp::Ordering, collections::HashSet};

use chrono::{NaiveDate, Utc};

use crate::models::{MemoryItem, MemoryRetrieval};

pub trait MemoryRetriever: Send + Sync {
    fn search(&self, query: &str, items: &[MemoryItem], limit: usize) -> Vec<MemoryItem>;
}

#[derive(Default)]
pub struct HybridMemoryRetriever {
    as_of: Option<NaiveDate>,
}

impl HybridMemoryRetriever {
    #[cfg(test)]
    fn at(as_of: NaiveDate) -> Self {
        Self { as_of: Some(as_of) }
    }
}

impl MemoryRetriever for HybridMemoryRetriever {
    fn search(&self, query: &str, items: &[MemoryItem], limit: usize) -> Vec<MemoryItem> {
        let query_tokens = tokens(query);
        let normalized_query = normalize(query);
        let as_of = self.as_of.unwrap_or_else(|| Utc::now().date_naive());
        let mut scored = items
            .iter()
            .filter_map(|item| score_item(item, &normalized_query, &query_tokens, as_of))
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .0
                .partial_cmp(&left.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right.1.occurred_at.cmp(&left.1.occurred_at))
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
        scored
            .into_iter()
            .take(limit)
            .map(|(_, item)| item)
            .collect()
    }
}

fn score_item(
    item: &MemoryItem,
    normalized_query: &str,
    query_tokens: &HashSet<String>,
    as_of: NaiveDate,
) -> Option<(f64, MemoryItem)> {
    if !item.selected {
        return None;
    }
    let identity = normalize(&format!(
        "{} {} {} {}",
        item.title,
        item.summary,
        item.status,
        item.tags.join(" ")
    ));
    let details = normalize(&item.content.to_string());
    let identity_matches = query_tokens
        .iter()
        .filter(|token| identity.contains(token.as_str()))
        .count();
    let detail_matches = query_tokens
        .iter()
        .filter(|token| details.contains(token.as_str()))
        .count();
    let exact_title =
        !item.title.trim().is_empty() && normalized_query.contains(normalize(&item.title).as_str());
    let concept_matches = shared_concepts(normalized_query, &format!("{identity} {details}"));
    let asks_for_review = contains_any(
        normalized_query,
        &["复盘", "结果", "教训", "经验", "校准", "过去", "历史"],
    );
    let asks_for_challenge = contains_any(
        normalized_query,
        &["风险", "反方", "错误", "失效", "证伪", "冲突", "遗漏"],
    );
    let asks_for_analysis = contains_any(
        normalized_query,
        &["分析", "方案", "建议", "判断", "如何", "比较"],
    );
    let intent_score = usize::from(asks_for_review && item.reviewed) * 5
        + usize::from(asks_for_challenge && item.contradiction) * 4
        + usize::from(asks_for_analysis && item.kind == "analysis") * 2;
    let relevance = identity_matches * 4
        + detail_matches
        + usize::from(exact_title) * 8
        + concept_matches * 3
        + intent_score;
    if relevance == 0 {
        return None;
    }

    let age_days = memory_age_days(item, as_of);
    let decay = match age_days {
        0..=90 => 1.0,
        91..=365 => 0.9,
        366..=1095 => 0.8,
        _ => 0.7,
    };
    let source_weight = if item.reviewed {
        3.0
    } else if item.kind == "decision" {
        1.5
    } else {
        0.5
    };
    let score = ((relevance as f64 * decay + source_weight) * 100.0).round() / 100.0;
    let mut reasons = Vec::new();
    if exact_title || identity_matches > 0 {
        reasons.push("主题、资产或结构化标签匹配".into());
    }
    if detail_matches > 0 {
        reasons.push("原始判断或分析内容匹配".into());
    }
    if concept_matches > 0 {
        reasons.push("投资概念关联".into());
    }
    if item.reviewed {
        reasons.push("已完成结果复盘，证据权重更高".into());
    }
    if item.contradiction {
        reasons.push("复盘结果削弱或反驳原始判断".into());
    }
    if item.kind == "analysis" {
        reasons.push("历史 AI 分析未经结果验证，仅作为线索".into());
    }
    reasons.push(match age_days {
        0..=90 => "90 天内记忆".into(),
        91..=365 => "一年内记忆".into(),
        366..=1095 => "较早记忆，已降低时效权重".into(),
        _ => "长期旧记忆，仅保留核心相关性".into(),
    });

    let mut matched = item.clone();
    matched.retrieval = Some(MemoryRetrieval {
        score,
        age_days,
        reasons,
        passes: Vec::new(),
    });
    Some((score, matched))
}

fn memory_age_days(item: &MemoryItem, as_of: NaiveDate) -> i64 {
    parse_date(&item.occurred_at)
        .or_else(|| parse_date(&item.created_at))
        .map_or(0, |date| (as_of - date).num_days().max(0))
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.date_naive())
        .ok()
        .or_else(|| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

fn shared_concepts(query: &str, memory: &str) -> usize {
    const CONCEPTS: &[&[&str]] = &[
        &["风险", "回撤", "下跌", "损失", "永久损失", "压力"],
        &["集中", "分散", "仓位", "配置", "权重"],
        &["目标", "期限", "达成", "投入", "现金流"],
        &["复盘", "结果", "教训", "校准", "过程"],
        &["逻辑", "假设", "证伪", "反方", "失效"],
        &["估值", "价格", "价值", "赔率", "预期收益"],
        &["流动性", "应急", "现金", "负债", "安全垫"],
    ];
    CONCEPTS
        .iter()
        .filter(|group| {
            group.iter().any(|term| query.contains(term))
                && group.iter().any(|term| memory.contains(term))
        })
        .count()
}

fn contains_any(value: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| value.contains(term))
}

fn normalize(value: &str) -> String {
    value.to_lowercase()
}

fn tokens(input: &str) -> HashSet<String> {
    let normalized = normalize(input);
    let separators = "，。！？；：,.!?;:()（）/、-_\"'{}[]";
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
    use serde_json::json;

    use super::*;

    fn memory(
        id: &str,
        title: &str,
        content: &str,
        occurred_at: &str,
        reviewed: bool,
        contradiction: bool,
    ) -> MemoryItem {
        MemoryItem {
            id: id.into(),
            kind: "decision".into(),
            title: title.into(),
            summary: content.into(),
            content: json!({ "thesis": content }),
            created_at: occurred_at.into(),
            occurred_at: occurred_at.into(),
            status: if reviewed { "已复盘" } else { "待复盘" }.into(),
            reviewed,
            contradiction,
            tags: vec![title.into()],
            selected: true,
            retrieval: None,
        }
    }

    fn retriever() -> HybridMemoryRetriever {
        HybridMemoryRetriever::at(NaiveDate::from_ymd_opt(2026, 9, 5).unwrap())
    }

    #[test]
    fn retrieves_relevant_decision_and_explains_the_match() {
        let items = vec![
            memory(
                "1",
                "宽基指数",
                "长期配置与集中度",
                "2026-08-01",
                false,
                false,
            ),
            memory("2", "黄金", "避险资产", "2026-08-01", false, false),
        ];
        let result = retriever().search("复盘我的指数配置", &items, 3);
        assert_eq!(result[0].id, "1");
        assert!(result[0].retrieval.as_ref().unwrap().score > 0.0);
        assert!(!result[0].retrieval.as_ref().unwrap().reasons.is_empty());
    }

    #[test]
    fn reviewed_memory_outranks_an_equivalent_unreviewed_opinion() {
        let items = vec![
            memory("raw", "指数", "集中风险", "2026-08-01", false, false),
            memory("reviewed", "指数", "集中风险", "2026-08-01", true, false),
        ];
        let result = retriever().search("复盘指数集中风险", &items, 3);
        assert_eq!(result[0].id, "reviewed");
    }

    #[test]
    fn contradiction_is_preserved_as_a_first_class_signal() {
        let items = vec![memory(
            "failed",
            "主动基金",
            "原始逻辑后来失效",
            "2026-07-01",
            true,
            true,
        )];
        let result = retriever().search("检查主动基金的反方风险", &items, 3);
        let retrieval = result[0].retrieval.as_ref().unwrap();
        assert!(retrieval
            .reasons
            .iter()
            .any(|reason| reason.contains("反驳")));
    }

    #[test]
    fn time_decay_reduces_but_does_not_erase_old_relevant_lessons() {
        let recent = memory("recent", "指数", "集中风险", "2026-08-01", true, false);
        let old = memory("old", "指数", "集中风险", "2020-01-01", true, false);
        let result = retriever().search("复盘指数集中风险", &[old, recent], 3);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "recent");
        assert!(result[1].retrieval.as_ref().unwrap().score > 0.0);
    }

    #[test]
    fn concept_expansion_connects_drawdown_and_loss_language() {
        let items = vec![memory(
            "risk",
            "组合记录",
            "压力情景出现较大回撤",
            "2026-08-01",
            true,
            false,
        )];
        assert_eq!(retriever().search("如何控制下跌损失", &items, 3).len(), 1);
    }

    #[test]
    fn never_returns_a_memory_the_user_excluded() {
        let mut excluded = memory("private", "指数", "集中风险", "2026-08-01", true, true);
        excluded.selected = false;
        assert!(retriever()
            .search("复盘指数集中风险", &[excluded], 3)
            .is_empty());
    }
}
