//! Live evaluation only: fictional inputs, no account database or product changes.
use std::{collections::HashMap, io::Write, time::Instant};

use serde::{Deserialize, Serialize};

use super::{InvestmentOrchestrator, NativeModelProvider};
use crate::{
    context::{BuiltContext, ContextBuilder, ContextSources},
    models::{AnalysisRequest, AnalysisResult, AnalysisSettings, Snapshot},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    id: String,
    question: String,
    focus: String,
    previous_case_id: Option<String>,
}

fn fixtures() -> (Snapshot, Vec<Case>) {
    (
        serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/snapshot.json")).unwrap(),
        serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/questions.json")).unwrap(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseResult {
    id: String,
    question: String,
    focus: String,
    previous_case_id: Option<String>,
    status: &'static str,
    elapsed_ms: u128,
    request: Option<AnalysisRequest>,
    context: Option<BuiltContext>,
    result: Option<AnalysisResult>,
    error: Option<String>,
}

fn previous_turn(question: &str, result: &AnalysisResult) -> crate::models::PreviousTurn {
    let (answer, suggestions) = match result.workflow_trace.structured_report.as_ref().unwrap() {
        crate::models::StoredAnalysisReport::Current(report) => (
            &report.answer,
            report
                .suggestions
                .iter()
                .map(|s| s.action.chars().take(100).collect())
                .collect(),
        ),
        crate::models::StoredAnalysisReport::Legacy(_) => {
            panic!("live evaluation must produce current reports")
        }
    };
    crate::models::PreviousTurn {
        question: question.chars().take(200).collect(),
        answer: answer.chars().take(300).collect(),
        suggestions,
    }
}

pub(super) async fn run_live() {
    let required = |name| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let path = required("MARIO_EVAL_REPORT_PATH");
    let base_url = required("MARIO_EVAL_BASE_URL");
    let model = required("MARIO_EVAL_MODEL");
    // Never serialize credentials, endpoints, provider errors containing credentials, or env vars.
    let api_key = required("MARIO_EVAL_API_KEY");
    assert!(!api_key.is_empty(), "MARIO_EVAL_API_KEY must not be empty");
    let provider_kind = std::env::var("MARIO_EVAL_PROVIDER")
        .unwrap_or("openai-responses".into())
        .parse()
        .expect("invalid MARIO_EVAL_PROVIDER");
    let provider =
        NativeModelProvider::new(provider_kind, base_url, model.clone(), api_key.clone())
            .expect("invalid evaluation model configuration");
    // Refuse overwriting a baseline or a partly completed paid run.
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("report path must be new and its parent directory must exist");
    let (snapshot, cases) = fixtures();
    let settings = AnalysisSettings::default();
    let mut results = Vec::<CaseResult>::new();
    let mut previous = HashMap::<String, (String, AnalysisResult)>::new();
    let generated_at = chrono::Utc::now().to_rfc3339();
    // A checkpoint is written before calls and after every question, including failures.
    let checkpoint = |output: &mut std::fs::File, results: &[CaseResult]| {
        use std::io::{Seek, SeekFrom};
        let report = serde_json::json!({
            "schemaVersion": 1,
            "generatedAt": generated_at,
            "workflowVersion": super::workflow::INVESTMENT_WORKFLOW_VERSION,
            "provider": provider_kind,
            "model": model,
            "settings": settings,
            "expectedCases": cases.len(),
            "cases": results,
        });
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        output.seek(SeekFrom::Start(0)).unwrap();
        output.write_all(&bytes).unwrap();
        output.set_len(bytes.len() as u64).unwrap();
        output.sync_all().unwrap();
    };
    checkpoint(&mut output, &results);
    for case in &cases {
        let mut record = CaseResult {
            id: case.id.clone(),
            question: case.question.clone(),
            focus: case.focus.clone(),
            previous_case_id: case.previous_case_id.clone(),
            status: "failed",
            elapsed_ms: 0,
            request: None,
            context: None,
            result: None,
            error: None,
        };
        let parent = case.previous_case_id.as_ref().map(|id| previous.get(id));
        if matches!(parent, Some(None)) {
            record.status = "skipped";
            record.error = Some("上一轮执行失败，不能伪造追问上下文".into());
        } else {
            let previous_turn = parent
                .flatten()
                .map(|(question, result)| previous_turn(question, result));
            let mut request = AnalysisRequest {
                question: case.question.clone(),
                previous_turn,

                workflow: settings.workflow.clone(),
                reflect: settings.reflect,
                explore_alternatives: settings.explore_alternatives,
                web_search: settings.web_search
                    && provider_kind != crate::models::ModelProviderKind::OpenaiChat,
                context_selection: settings.context_selection.clone(),
                preview_revision: None,
            };
            let context = ContextBuilder::build(&request, &snapshot, &ContextSources::default());
            request.preview_revision = Some(context.revision.clone());
            let started = Instant::now();
            match InvestmentOrchestrator::new(&provider)
                .run(&request, &context)
                .await
            {
                Ok(result) => {
                    record.status = "completed";
                    previous.insert(case.id.clone(), (case.question.clone(), result.clone()));
                    record.result = Some(result);
                }
                Err(error) => {
                    record.error = Some(error.to_string().replace(&api_key, "[REDACTED]"))
                }
            }
            record.elapsed_ms = started.elapsed().as_millis();
            record.request = Some(request);
            record.context = Some(context);
        }
        eprintln!("qa_eval {}: {}", case.id, record.status);
        results.push(record);
        checkpoint(&mut output, &results);
    }
    assert!(
        results.iter().all(|item| item.status == "completed"),
        "evaluation has failed/skipped cases; inspect the saved report"
    );
}

#[test]
fn fixture_preserves_exact_amounts_and_issue_scope() {
    let (snapshot, cases) = fixtures();
    assert!((10..=15).contains(&cases.len()));
    let mut seen = std::collections::HashSet::new();
    for case in cases {
        if let Some(parent) = &case.previous_case_id {
            assert!(
                seen.contains(parent),
                "follow-up must reference an earlier case"
            );
        }
        assert!(seen.insert(case.id));
    }
    let status =
        crate::valuation::issues("CNY", &snapshot.holdings, &[], &[], &snapshot.cash_balances);
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        serde_json::to_value(&snapshot.valuation_status).unwrap()
    );
    assert!(snapshot.emergency_months < 1.0);
    let accounting = snapshot.accounting.as_ref().unwrap();
    assert!(accounting.total_value.is_none());
    assert_eq!(accounting.holdings["fund-a"].amounts.market_value, "800000");
    assert_eq!(accounting.cash.value.value.as_deref(), Some("10090"));
    assert_eq!(accounting.cash.value.missing, 0);
    assert!(accounting.holdings["usd-c"].fx_rate.is_none());
    assert_eq!(snapshot.plan.exact_monthly_surplus.as_deref(), Some("8000"));
}

#[test]
fn prior_summary_is_bounded_and_is_not_a_factual_source() {
    let previous = crate::models::PreviousTurn {
        question: "问题".into(),
        answer: "😀".repeat(798),
        suggestions: vec![],
    };
    previous.validate().unwrap();
    let mut invalid = previous;
    invalid.answer.push('字');
    assert!(invalid.validate().is_err());
}
