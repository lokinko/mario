//! Long-term discussion memory and the dreaming consolidation contract.
//! Notes are guidance distilled from completed analyses, never financial facts.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MEMORY_ACTIVE_LIMIT: usize = 60;
pub const MEMORY_STATEMENT_CHARS: usize = 200;
pub const MEMORY_RATIONALE_CHARS: usize = 300;
pub const MEMORY_QUOTE_CHARS: usize = 200;
pub const MEMORY_SOURCES_MAX: usize = 3;
pub const DREAM_OPERATIONS_MAX: usize = 8;
/// Completed analyses consumed by one dreaming run.
pub const DREAM_BATCH: usize = 12;
/// Automatic dreaming waits for this many unconsolidated analyses.
pub const DREAM_DUE_AFTER: usize = 3;
pub const DREAM_ANSWER_CHARS: usize = 1_500;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum MemoryKind {
    /// How the user wants analysis framed or communicated, in their own words.
    Preference,
    /// A standing view from earlier discussion, with the condition that would change it.
    Viewpoint,
    /// A circumstance the user stated in conversation; it does not rewrite the profile.
    Context,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Viewpoint => "viewpoint",
            Self::Context => "context",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "preference" => Some(Self::Preference),
            "viewpoint" => Some(Self::Viewpoint),
            "context" => Some(Self::Context),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryOrigin {
    /// Quoted from a user question.
    UserStated,
    /// Summarised from a model answer; carries the model's uncertainty.
    ModelInferred,
}

impl MemoryOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserStated => "userStated",
            Self::ModelInferred => "modelInferred",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "userStated" => Some(Self::UserStated),
            "modelInferred" => Some(Self::ModelInferred),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNote {
    pub id: String,
    pub kind: MemoryKind,
    pub origin: MemoryOrigin,
    pub statement: String,
    pub rationale: String,
    pub source_analysis_ids: Vec<String>,
    pub quote: String,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DreamState {
    /// `created_at` of the newest analysis already consolidated.
    #[serde(default)]
    pub dreamed_through: Option<String>,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_result: Option<DreamSummary>,
}

impl DreamState {
    pub fn validate(&self) -> crate::AppResult<()> {
        for value in [&self.dreamed_through, &self.last_run_at]
            .into_iter()
            .flatten()
        {
            chrono::DateTime::parse_from_rfc3339(value)
                .map_err(|_| crate::AppError::Validation("记忆整理时间格式无效".into()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DreamSummary {
    pub analyses: usize,
    pub added: usize,
    pub updated: usize,
    pub archived: usize,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryOverview {
    pub items: Vec<MemoryNote>,
    pub dream: DreamState,
    pub pending_analyses: usize,
    pub dream_due: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatusInput {
    pub active: bool,
}

/// A completed analysis as frozen for dreaming.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DreamSourceAnalysis {
    pub id: String,
    pub created_at: String,
    pub question: String,
    pub answer: String,
}

/// Everything the dreaming agent may see; frozen under the account lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamInput {
    pub analyses: Vec<DreamSourceAnalysis>,
    pub memories: Vec<MemoryNote>,
}

impl DreamInput {
    pub fn revision(&self) -> String {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        format!("{:x}", Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DreamProposal {
    pub operations: Vec<DreamOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum DreamOperation {
    #[serde(rename_all = "camelCase")]
    Add {
        kind: MemoryKind,
        origin: MemoryOrigin,
        statement: String,
        rationale: String,
        source_analysis_ids: Vec<String>,
        quote: String,
    },
    #[serde(rename_all = "camelCase")]
    Update {
        id: String,
        statement: String,
        rationale: String,
        source_analysis_ids: Vec<String>,
        quote: String,
    },
    #[serde(rename_all = "camelCase")]
    Archive { id: String, reason: String },
}

/// Model execution trace for one dreaming run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamOutcome {
    pub proposal: DreamProposal,
    pub calls: Vec<crate::models::analysis::ModelCallTrace>,
    pub validation: crate::models::analysis::OutputValidationTrace,
}

/// Operations accepted for storage after deterministic checks.
#[derive(Debug, Clone, PartialEq)]
pub enum AcceptedOperation {
    Add {
        kind: MemoryKind,
        origin: MemoryOrigin,
        statement: String,
        rationale: String,
        source_analysis_ids: Vec<String>,
        quote: String,
    },
    Update {
        id: String,
        statement: String,
        rationale: String,
        source_analysis_ids: Vec<String>,
        quote: String,
    },
    Archive {
        id: String,
    },
}

/// Accept each valid operation and explain every rejection; one bad operation does not
/// discard the rest. Quotes must be verbatim from the cited analyses: user-stated notes
/// may only quote user questions, and preferences or context are never model-inferred.
pub fn screen_operations(
    input: &DreamInput,
    proposal: &DreamProposal,
) -> (Vec<AcceptedOperation>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut touched = HashSet::new();
    let mut statements = input
        .memories
        .iter()
        .filter(|note| note.active)
        .map(|note| normalize(&note.statement))
        .collect::<HashSet<_>>();
    let mut active = input.memories.iter().filter(|note| note.active).count();
    for (index, operation) in proposal.operations.iter().enumerate() {
        let label = format!("操作{}", index + 1);
        if index >= DREAM_OPERATIONS_MAX {
            rejected.push(format!("{label}：单次整理最多 {DREAM_OPERATIONS_MAX} 项"));
            continue;
        }
        let checked = match operation {
            DreamOperation::Add {
                kind,
                origin,
                statement,
                rationale,
                source_analysis_ids,
                quote,
            } => check_note(
                input,
                *origin,
                statement,
                rationale,
                source_analysis_ids,
                quote,
            )
            .and_then(|()| {
                if *origin == MemoryOrigin::ModelInferred && *kind != MemoryKind::Viewpoint {
                    return Err("偏好与背景只能来自用户原话，不能由模型推断".into());
                }
                if !statements.insert(normalize(statement)) {
                    return Err("与已有记忆重复".into());
                }
                if active >= MEMORY_ACTIVE_LIMIT {
                    return Err(format!("有效记忆已达 {MEMORY_ACTIVE_LIMIT} 条上限"));
                }
                active += 1;
                Ok(AcceptedOperation::Add {
                    kind: *kind,
                    origin: *origin,
                    statement: statement.trim().into(),
                    rationale: rationale.trim().into(),
                    source_analysis_ids: source_analysis_ids.clone(),
                    quote: quote.trim().into(),
                })
            }),
            DreamOperation::Update {
                id,
                statement,
                rationale,
                source_analysis_ids,
                quote,
            } => existing(input, id, &mut touched).and_then(|note| {
                check_note(
                    input,
                    note.origin,
                    statement,
                    rationale,
                    source_analysis_ids,
                    quote,
                )?;
                if normalize(statement) != normalize(&note.statement)
                    && !statements.insert(normalize(statement))
                {
                    return Err("与已有记忆重复".into());
                }
                Ok(AcceptedOperation::Update {
                    id: id.clone(),
                    statement: statement.trim().into(),
                    rationale: rationale.trim().into(),
                    source_analysis_ids: source_analysis_ids.clone(),
                    quote: quote.trim().into(),
                })
            }),
            DreamOperation::Archive { id, reason } => {
                existing(input, id, &mut touched).and_then(|_| {
                    let length = reason.trim().chars().count();
                    if length == 0 || length > MEMORY_RATIONALE_CHARS {
                        return Err(format!(
                            "归档理由须非空且不超过 {MEMORY_RATIONALE_CHARS} 字"
                        ));
                    }
                    active = active.saturating_sub(1);
                    Ok(AcceptedOperation::Archive { id: id.clone() })
                })
            }
        };
        match checked {
            Ok(operation) => accepted.push(operation),
            Err(reason) => rejected.push(format!("{label}：{reason}")),
        }
    }
    (accepted, rejected)
}

fn existing<'a>(
    input: &'a DreamInput,
    id: &str,
    touched: &mut HashSet<String>,
) -> Result<&'a MemoryNote, String> {
    let note = input
        .memories
        .iter()
        .find(|note| note.id == id && note.active)
        .ok_or_else(|| format!("记忆 {id} 不存在或已归档"))?;
    if !touched.insert(id.to_owned()) {
        return Err(format!("同一次整理重复修改记忆 {id}"));
    }
    Ok(note)
}

fn check_note(
    input: &DreamInput,
    origin: MemoryOrigin,
    statement: &str,
    rationale: &str,
    sources: &[String],
    quote: &str,
) -> Result<(), String> {
    bounded("statement", statement, 1, MEMORY_STATEMENT_CHARS)?;
    bounded("rationale", rationale, 1, MEMORY_RATIONALE_CHARS)?;
    bounded("quote", quote, 1, MEMORY_QUOTE_CHARS)?;
    if sources.is_empty() || sources.len() > MEMORY_SOURCES_MAX {
        return Err(format!("须引用 1–{MEMORY_SOURCES_MAX} 条本次分析"));
    }
    let mut cited = Vec::new();
    for id in sources {
        let analysis = input
            .analyses
            .iter()
            .find(|analysis| &analysis.id == id)
            .ok_or_else(|| format!("引用了本次未提供的分析 {id}"))?;
        if cited
            .iter()
            .any(|seen: &&DreamSourceAnalysis| seen.id == *id)
        {
            return Err(format!("重复引用分析 {id}"));
        }
        cited.push(analysis);
    }
    let quote = quote.trim();
    let found = cited.iter().any(|analysis| match origin {
        MemoryOrigin::UserStated => analysis.question.contains(quote),
        MemoryOrigin::ModelInferred => {
            analysis.question.contains(quote) || analysis.answer.contains(quote)
        }
    });
    if !found {
        return Err(match origin {
            MemoryOrigin::UserStated => "quote 不是所引分析中用户问题的原文".into(),
            MemoryOrigin::ModelInferred => "quote 不是所引分析问题或回答的原文".into(),
        });
    }
    Ok(())
}

fn bounded(name: &str, value: &str, min: usize, max: usize) -> Result<(), String> {
    let length = value.trim().chars().count();
    if length < min || length > max {
        return Err(format!("{name} 须为 {min}–{max} 字"));
    }
    Ok(())
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            !ch.is_whitespace() && !ch.is_ascii_punctuation() && !"，。；：、！？".contains(*ch)
        })
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> DreamInput {
        DreamInput {
            analyses: vec![DreamSourceAnalysis {
                id: "a1".into(),
                created_at: "2026-11-16T00:00:00Z".into(),
                question: "我更看重分红持续性，先别跟我谈技术位置".into(),
                answer: "若经营现金流连续两年不足以覆盖派息，应下调分红可持续性的判断。".into(),
            }],
            memories: vec![MemoryNote {
                id: "m1".into(),
                kind: MemoryKind::Viewpoint,
                origin: MemoryOrigin::ModelInferred,
                statement: "旧观点".into(),
                rationale: "旧理由".into(),
                source_analysis_ids: vec!["old".into()],
                quote: "旧".into(),
                active: true,
                created_at: "2026-11-01T00:00:00Z".into(),
                updated_at: "2026-11-01T00:00:00Z".into(),
            }],
        }
    }

    fn add(kind: MemoryKind, origin: MemoryOrigin, quote: &str) -> DreamOperation {
        DreamOperation::Add {
            kind,
            origin,
            statement: format!("新记忆{quote}"),
            rationale: "用户明确表述".into(),
            source_analysis_ids: vec!["a1".into()],
            quote: quote.into(),
        }
    }

    #[test]
    fn verbatim_user_quotes_become_preferences_but_model_text_cannot() {
        let proposal = DreamProposal {
            operations: vec![
                add(
                    MemoryKind::Preference,
                    MemoryOrigin::UserStated,
                    "我更看重分红持续性",
                ),
                add(
                    MemoryKind::Preference,
                    MemoryOrigin::UserStated,
                    "应下调分红可持续性",
                ),
                add(
                    MemoryKind::Preference,
                    MemoryOrigin::ModelInferred,
                    "应下调分红可持续性",
                ),
                add(
                    MemoryKind::Viewpoint,
                    MemoryOrigin::ModelInferred,
                    "经营现金流连续两年不足以覆盖派息",
                ),
                add(
                    MemoryKind::Viewpoint,
                    MemoryOrigin::UserStated,
                    "用户喜欢被肯定",
                ),
            ],
        };
        let (accepted, rejected) = screen_operations(&input(), &proposal);
        assert_eq!(accepted.len(), 2);
        assert_eq!(rejected.len(), 3);
        assert!(rejected[0].contains("用户问题的原文"));
        assert!(rejected[1].contains("不能由模型推断"));
    }

    #[test]
    fn updates_and_archives_need_existing_notes_once_and_sources_from_this_batch() {
        let proposal = DreamProposal {
            operations: vec![
                DreamOperation::Archive {
                    id: "m1".into(),
                    reason: "已被新讨论取代".into(),
                },
                DreamOperation::Archive {
                    id: "m1".into(),
                    reason: "重复".into(),
                },
                DreamOperation::Archive {
                    id: "missing".into(),
                    reason: "不存在".into(),
                },
                DreamOperation::Update {
                    id: "m1".into(),
                    statement: "新".into(),
                    rationale: "理由".into(),
                    source_analysis_ids: vec!["a1".into()],
                    quote: "我更看重".into(),
                },
                DreamOperation::Add {
                    kind: MemoryKind::Context,
                    origin: MemoryOrigin::UserStated,
                    statement: "引用不存在的分析".into(),
                    rationale: "理由".into(),
                    source_analysis_ids: vec!["elsewhere".into()],
                    quote: "我更看重".into(),
                },
            ],
        };
        let (accepted, rejected) = screen_operations(&input(), &proposal);
        assert_eq!(
            accepted,
            vec![AcceptedOperation::Archive { id: "m1".into() }]
        );
        assert_eq!(rejected.len(), 4);
        assert!(rejected[3].contains("未提供的分析"));
    }

    #[test]
    fn duplicates_and_the_operation_and_active_limits_are_rejected_individually() {
        let mut full = input();
        full.memories = (0..MEMORY_ACTIVE_LIMIT)
            .map(|index| MemoryNote {
                id: format!("m{index}"),
                statement: format!("记忆{index}"),
                ..input().memories[0].clone()
            })
            .collect();
        let (accepted, rejected) = screen_operations(
            &full,
            &DreamProposal {
                operations: vec![add(
                    MemoryKind::Preference,
                    MemoryOrigin::UserStated,
                    "我更看重分红持续性",
                )],
            },
        );
        assert!(accepted.is_empty());
        assert!(rejected[0].contains("上限"));
        let operations = (0..DREAM_OPERATIONS_MAX + 1)
            .map(|_| {
                add(
                    MemoryKind::Preference,
                    MemoryOrigin::UserStated,
                    "我更看重分红持续性",
                )
            })
            .collect();
        let (accepted, rejected) = screen_operations(&input(), &DreamProposal { operations });
        assert_eq!(accepted.len(), 1);
        assert!(rejected.iter().filter(|r| r.contains("重复")).count() == DREAM_OPERATIONS_MAX - 1);
        assert!(rejected.last().unwrap().contains("最多"));
    }
}
