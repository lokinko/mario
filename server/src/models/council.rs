//! Multi-agent council contracts and deterministic vote counting.
//! Model agents produce briefs, proposals and ballots; only this module decides the winner.
use crate::models::analysis::{AnalysisClaim, OutputValidationTrace, StructuredAnalysis};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::cmp::Ordering;

pub const RESEARCH_FINDINGS_MAX: usize = 10;
pub const RESEARCH_NOTES_MAX: usize = 6;
pub const RESEARCH_NOTE_CHARS: usize = 300;
#[cfg(test)]
pub const BALLOT_OBJECTION_CHARS: usize = 300;
#[cfg(test)]
pub const BALLOT_VETO_CHARS: usize = 200;
#[cfg(test)]
pub const SCORE_MAX: u8 = 2;
pub const MODULE_STANCE_CHARS: usize = 120;
pub const MODULE_SUMMARY_CHARS: usize = 600;
pub const MODULE_ITEM_CHARS: usize = 240;
pub const MODULE_GAP_CHARS: usize = 180;
pub const MODULE_FACTS_MAX: usize = 5;

/// Adaptive workflow execution mode selected from the frozen inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Full multi-module investment analysis: research → specialist modules → chief strategist synthesis.
    #[default]
    Council,
    /// Single direct structured advisor call for lightweight concept, data-status or lookup queries.
    Fast,
}

/// Deterministic domain and execution-mode routing recorded in the workflow trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteTrace {
    pub domain: String,
    pub domain_label: String,
    pub execution_mode: ExecutionMode,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companies: Vec<String>,
}

/// Output of one specialist analysis module (fundamentals/valuation, portfolio/execution, risk/bear).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleAnalysis {
    pub stance: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub data_gaps: Vec<String>,
    #[serde(default)]
    pub facts: Vec<AnalysisClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleTrace {
    pub id: String,
    pub module: String,
    pub label: String,
    #[serde(default)]
    pub analysis: Option<ModuleAnalysis>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisTrace {
    pub status: String,
    pub modules_used: usize,
    pub modules_total: usize,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
}

/// Research agent output: cited findings shared by every proposal agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchBrief {
    pub findings: Vec<AnalysisClaim>,
    pub counterpoints: Vec<String>,
    pub unknowns: Vec<String>,
    pub checks: Vec<ResearchCheckStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchCheckStatus {
    pub index: usize,
    pub status: ResearchCheckOutcome,
    pub note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCheckOutcome {
    Covered,
    NotApplicable,
    MissingSource,
}

/// A failed brief degrades the run: proposals then work from the frozen context alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTrace {
    #[serde(default)]
    pub brief: Option<ResearchBrief>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalTrace {
    pub id: String,
    pub lens: String,
    pub label: String,
    #[serde(default)]
    pub report: Option<StructuredAnalysis>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

/// One agent's assessment of the other proposals; its own proposal is never shown to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ballot {
    /// Forced choice among the shown proposals, so equal scores still express a preference.
    /// Null only when the voter vetoed every proposal it was shown.
    pub preferred: Option<String>,
    pub assessments: Vec<BallotAssessment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BallotAssessment {
    pub proposal: String,
    pub scores: BallotScores,
    pub veto: Option<String>,
    pub objection: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BallotScores {
    pub facts: u8,
    pub citations: u8,
    pub reasoning: u8,
    pub counterpoints: u8,
    pub actionability: u8,
}

#[cfg(test)]
impl BallotScores {
    pub fn total(&self) -> u32 {
        [
            self.facts,
            self.citations,
            self.reasoning,
            self.counterpoints,
            self.actionability,
        ]
        .iter()
        .map(|score| u32::from(*score))
        .sum()
    }

    fn in_range(&self) -> bool {
        [
            self.facts,
            self.citations,
            self.reasoning,
            self.counterpoints,
            self.actionability,
        ]
        .iter()
        .all(|score| *score <= SCORE_MAX)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BallotTrace {
    pub voter: String,
    pub lens: String,
    #[serde(default)]
    pub ballot: Option<Ballot>,
    #[serde(default)]
    pub validation: Option<OutputValidationTrace>,
    #[serde(default)]
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoteStatus {
    /// The winner was assessed by at least one other agent and not eliminated.
    Voted,
    /// The winner tied the runner-up on preferences, mean score and vetoes; repairs or lens
    /// order decided, so the vote expressed no preference between them.
    Tied,
    /// Every assessed proposal was eliminated, or the winner was never assessed.
    Contested,
    /// Only one proposal passed validation; no vote took place.
    Single,
    /// No ballot passed validation; the first valid proposal in lens order was used.
    Unvoted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TallyRow {
    pub proposal: String,
    pub voters: usize,
    pub preferences: usize,
    pub vetoes: usize,
    pub score_total: u32,
    pub eliminated: bool,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteTrace {
    pub status: VoteStatus,
    pub winner: String,
    pub tally: Vec<TallyRow>,
    pub ballots: Vec<BallotTrace>,
}

/// A valid proposal entering the vote, in fixed lens order.
#[cfg(test)]
pub struct Candidate<'a> {
    pub id: &'a str,
    pub repairs: usize,
}

/// Validate one ballot against the exact proposals its voter was shown.
#[cfg(test)]
pub fn validate_ballot(ballot: &Ballot, shown: &[&str]) -> Result<(), String> {
    if ballot.assessments.len() != shown.len() {
        return Err(format!(
            "assessments 必须恰好评价 {} 个方案，当前 {} 个",
            shown.len(),
            ballot.assessments.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for assessment in &ballot.assessments {
        if !shown.contains(&assessment.proposal.as_str()) {
            return Err(format!("评价了未展示的方案：{}", assessment.proposal));
        }
        if !seen.insert(assessment.proposal.as_str()) {
            return Err(format!("重复评价方案：{}", assessment.proposal));
        }
        if !assessment.scores.in_range() {
            return Err(format!(
                "{} 的评分必须在 0–{SCORE_MAX}",
                assessment.proposal
            ));
        }
        let objection = assessment.objection.trim().chars().count();
        if objection == 0 || objection > BALLOT_OBJECTION_CHARS {
            return Err(format!(
                "{} 的 objection 须非空且不超过 {BALLOT_OBJECTION_CHARS} 字",
                assessment.proposal
            ));
        }
        if let Some(veto) = &assessment.veto {
            let length = veto.trim().chars().count();
            if length == 0 || length > BALLOT_VETO_CHARS {
                return Err(format!(
                    "{} 的 veto 须为 null，或写明不超过 {BALLOT_VETO_CHARS} 字的否决理由",
                    assessment.proposal
                ));
            }
        }
    }
    let all_vetoed = ballot.assessments.iter().all(|a| a.veto.is_some());
    match &ballot.preferred {
        None if all_vetoed => Ok(()),
        None => Err("preferred 只能在否决全部展示方案时为 null".into()),
        Some(preferred) => match ballot
            .assessments
            .iter()
            .find(|assessment| &assessment.proposal == preferred)
        {
            None => Err(format!("preferred 必须是展示的方案之一：{preferred}")),
            Some(assessment) if assessment.veto.is_some() => {
                Err(format!("preferred 不能是本票否决的方案：{preferred}"))
            }
            Some(_) => Ok(()),
        },
    }
}

/// Count validated ballots. A proposal is eliminated when vetoes reach half of its voters.
/// Ranking: not eliminated, assessed, more preferences, higher mean score, fewer vetoes,
/// fewer repairs, lens order.
#[cfg(test)]
pub fn tally(
    candidates: &[Candidate<'_>],
    ballots: &[(&str, &Ballot)],
) -> (VoteStatus, Vec<TallyRow>) {
    let mut rows = candidates
        .iter()
        .enumerate()
        .map(|(order, candidate)| {
            let assessments = ballots
                .iter()
                .filter(|(voter, _)| *voter != candidate.id)
                .flat_map(|(_, ballot)| ballot.assessments.iter())
                .filter(|assessment| assessment.proposal == candidate.id)
                .collect::<Vec<_>>();
            let voters = assessments.len();
            let vetoes = assessments.iter().filter(|a| a.veto.is_some()).count();
            let preferences = ballots
                .iter()
                .filter(|(voter, ballot)| {
                    *voter != candidate.id && ballot.preferred.as_deref() == Some(candidate.id)
                })
                .count();
            (
                order,
                candidate.repairs,
                TallyRow {
                    proposal: candidate.id.to_owned(),
                    voters,
                    preferences,
                    vetoes,
                    score_total: assessments.iter().map(|a| a.scores.total()).sum(),
                    eliminated: voters > 0 && vetoes * 2 >= voters,
                    rank: 0,
                },
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|(order_a, repairs_a, a), (order_b, repairs_b, b)| {
        a.eliminated
            .cmp(&b.eliminated)
            .then((a.voters == 0).cmp(&(b.voters == 0)))
            .then(b.preferences.cmp(&a.preferences))
            .then_with(|| compare_mean(b, a))
            .then(a.vetoes.cmp(&b.vetoes))
            .then(repairs_a.cmp(repairs_b))
            .then(order_a.cmp(order_b))
    });
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(rank, (_, _, mut row))| {
            row.rank = rank + 1;
            row
        })
        .collect::<Vec<_>>();
    let status = match (rows.first(), rows.get(1)) {
        _ if candidates.len() == 1 => VoteStatus::Single,
        _ if rows.iter().all(|row| row.voters == 0) => VoteStatus::Unvoted,
        (Some(winner), _) if winner.voters == 0 || winner.eliminated => VoteStatus::Contested,
        (Some(winner), Some(next)) if !next.eliminated && next.voters > 0 && tied(winner, next) => {
            VoteStatus::Tied
        }
        _ => VoteStatus::Voted,
    };
    (status, rows)
}

#[cfg(test)]
fn tied(a: &TallyRow, b: &TallyRow) -> bool {
    a.preferences == b.preferences && compare_mean(a, b).is_eq() && a.vetoes == b.vetoes
}

#[cfg(test)]
fn compare_mean(a: &TallyRow, b: &TallyRow) -> Ordering {
    // Exact comparison of score_total / voters without floating point.
    let left = u64::from(a.score_total) * b.voters.max(1) as u64;
    let right = u64::from(b.score_total) * a.voters.max(1) as u64;
    left.cmp(&right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(value: u8) -> BallotScores {
        BallotScores {
            facts: value,
            citations: value,
            reasoning: value,
            counterpoints: value,
            actionability: value,
        }
    }

    fn assess(proposal: &str, value: u8, veto: bool) -> BallotAssessment {
        BallotAssessment {
            proposal: proposal.into(),
            scores: scores(value),
            veto: veto.then(|| "编造来源".into()),
            objection: "理由".into(),
        }
    }

    fn candidates() -> Vec<Candidate<'static>> {
        ["P1", "P2", "P3"]
            .into_iter()
            .map(|id| Candidate { id, repairs: 0 })
            .collect()
    }

    fn ballot(preferred: Option<&str>, assessments: Vec<BallotAssessment>) -> Ballot {
        Ballot {
            preferred: preferred.map(str::to_owned),
            assessments,
        }
    }

    #[test]
    fn preferences_decide_before_mean_score_and_own_votes_never_count() {
        // P1 rates itself and prefers itself: neither counts.
        let p1 = ballot(
            Some("P1"),
            vec![
                assess("P2", 2, false),
                assess("P3", 1, false),
                assess("P1", 2, false),
            ],
        );
        let p2 = ballot(
            Some("P3"),
            vec![assess("P1", 1, false), assess("P3", 1, false)],
        );
        let p3 = ballot(
            Some("P2"),
            vec![assess("P1", 1, false), assess("P2", 2, false)],
        );
        let (status, rows) = tally(&candidates(), &[("P1", &p1), ("P2", &p2), ("P3", &p3)]);
        assert_eq!(status, VoteStatus::Voted);
        assert_eq!(rows[0].proposal, "P2");
        let p1_row = rows.iter().find(|r| r.proposal == "P1").unwrap();
        assert_eq!((p1_row.voters, p1_row.preferences), (2, 0));

        // A lower-scoring proposal that more voters would adopt outranks a higher mean.
        let p2 = ballot(
            Some("P3"),
            vec![assess("P1", 2, false), assess("P3", 1, false)],
        );
        let p1 = ballot(
            Some("P3"),
            vec![assess("P2", 2, false), assess("P3", 1, false)],
        );
        let (status, rows) = tally(&candidates(), &[("P1", &p1), ("P2", &p2), ("P3", &p3)]);
        assert_eq!(status, VoteStatus::Voted);
        assert_eq!(rows[0].proposal, "P3");
    }

    #[test]
    fn saturated_scores_and_split_preferences_are_reported_as_a_tie() {
        let p1 = ballot(
            Some("P2"),
            vec![assess("P2", 2, false), assess("P3", 2, false)],
        );
        let p2 = ballot(
            Some("P3"),
            vec![assess("P1", 2, false), assess("P3", 2, false)],
        );
        let p3 = ballot(
            Some("P1"),
            vec![assess("P1", 2, false), assess("P2", 2, false)],
        );
        let (status, rows) = tally(&candidates(), &[("P1", &p1), ("P2", &p2), ("P3", &p3)]);
        assert_eq!(status, VoteStatus::Tied);
        // Lens order still picks a report, but the status does not claim a preference.
        assert_eq!(rows[0].proposal, "P1");
    }

    #[test]
    fn half_vetoes_eliminate_even_a_higher_score() {
        let p1 = ballot(
            Some("P3"),
            vec![assess("P2", 2, true), assess("P3", 1, false)],
        );
        let p2 = ballot(
            Some("P1"),
            vec![assess("P1", 1, false), assess("P3", 1, false)],
        );
        let p3 = ballot(
            Some("P2"),
            vec![assess("P1", 1, false), assess("P2", 2, false)],
        );
        let (status, rows) = tally(&candidates(), &[("P1", &p1), ("P2", &p2), ("P3", &p3)]);
        let p2_row = rows.iter().find(|r| r.proposal == "P2").unwrap();
        assert!(p2_row.eliminated);
        assert_eq!(rows.last().unwrap().proposal, "P2");
        // P1 and P3 tie on preferences, mean and vetoes; the eliminated P2 is not a rival.
        assert_eq!(rows[0].proposal, "P1");
        assert_eq!(status, VoteStatus::Tied);
    }

    #[test]
    fn degraded_votes_are_labelled_instead_of_presented_as_consensus() {
        let only = [Candidate {
            id: "P2",
            repairs: 1,
        }];
        assert_eq!(tally(&only, &[]).0, VoteStatus::Single);
        let (status, rows) = tally(&candidates(), &[]);
        assert_eq!(status, VoteStatus::Unvoted);
        assert_eq!(rows[0].proposal, "P1");
        let all_veto = ballot(None, vec![assess("P2", 2, true), assess("P3", 2, true)]);
        let (status, rows) = tally(&candidates()[..], &[("P1", &all_veto)]);
        // P1 was never assessed and P2/P3 were vetoed: the unreviewed proposal is not a consensus.
        assert_eq!(rows[0].proposal, "P1");
        assert_eq!(status, VoteStatus::Contested);
    }

    #[test]
    fn ballots_must_assess_exactly_the_shown_proposals_within_bounds() {
        let shown = ["P2", "P3"];
        let valid = ballot(
            Some("P2"),
            vec![assess("P2", 2, false), assess("P3", 0, true)],
        );
        validate_ballot(&valid, &shown).unwrap();
        validate_ballot(
            &ballot(None, vec![assess("P2", 0, true), assess("P3", 0, true)]),
            &shown,
        )
        .unwrap();
        for invalid in [
            ballot(Some("P2"), vec![assess("P2", 2, false)]),
            ballot(
                Some("P2"),
                vec![assess("P2", 2, false), assess("P2", 1, false)],
            ),
            ballot(
                Some("P3"),
                vec![assess("P1", 2, false), assess("P3", 1, false)],
            ),
            ballot(
                Some("P2"),
                vec![assess("P2", 3, false), assess("P3", 1, false)],
            ),
            // Preference must be a shown, unvetoed proposal, and null only if all are vetoed.
            ballot(
                Some("P1"),
                vec![assess("P2", 2, false), assess("P3", 1, false)],
            ),
            ballot(
                Some("P3"),
                vec![assess("P2", 2, false), assess("P3", 0, true)],
            ),
            ballot(None, vec![assess("P2", 2, false), assess("P3", 0, true)]),
        ] {
            assert!(validate_ballot(&invalid, &shown).is_err());
        }
        let mut empty_objection = valid.clone();
        empty_objection.assessments[0].objection = " ".into();
        assert!(validate_ballot(&empty_objection, &shown).is_err());
    }
}
