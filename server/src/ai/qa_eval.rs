//! Live evaluation only: fictional inputs, no account database or product changes.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    time::Instant,
};

use serde::{Deserialize, Serialize};

use super::{InvestmentOrchestrator, NativeModelProvider};
use crate::{
    context::{BuiltContext, ContextBuilder, ContextSources},
    models::{
        accounting::AccountingHolding, AnalysisRequest, AnalysisResult, AnalysisSettings,
        ContextSelection, Holding, HoldingTrade, ResearchEvidence, Snapshot,
    },
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Scenario {
    #[serde(rename = "watch-add")]
    Add,
    #[serde(rename = "watch-no-personal")]
    NoPersonal,
    #[serde(rename = "watch-weekly")]
    Weekly,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    id: String,
    question: String,
    focus: String,
    previous_case_id: Option<String>,
    scenario: Option<Scenario>,
}

fn fixtures() -> (Snapshot, Vec<Case>) {
    (
        serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/snapshot.json")).unwrap(),
        serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/questions.json")).unwrap(),
    )
}

fn select_cases(mut cases: Vec<Case>, filter: Option<&str>) -> Result<Vec<Case>, String> {
    let available: HashSet<_> = cases.iter().map(|case| case.id.as_str()).collect();
    if available.len() != cases.len() {
        return Err("duplicate evaluation case ID".into());
    }
    if let Some(filter) = filter {
        let ids: Vec<_> = filter.split(',').map(str::trim).collect();
        let unique: HashSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() || ids.iter().any(|id| !available.contains(id)) {
            return Err("evaluation subset contains empty, duplicate or unknown case IDs".into());
        }
        cases.retain(|case| unique.contains(case.id.as_str()));
    }
    let mut preceding = HashSet::new();
    for case in &cases {
        if let Some(parent) = &case.previous_case_id {
            if !preceding.contains(parent.as_str()) {
                return Err(format!(
                    "{} requires earlier selected case {parent}; include its dependency explicitly",
                    case.id
                ));
            }
        }
        preceding.insert(case.id.as_str());
    }
    Ok(cases)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatchFixture {
    holdings: Vec<Holding>,
    accounting_holdings: BTreeMap<String, AccountingHolding>,
    holding_trades: Vec<HoldingTrade>,
    evidence: Vec<ResearchEvidence>,
}
fn watch_fixture() -> WatchFixture {
    serde_json::from_str(include_str!("../../tests/fixtures/qa_eval/watch.json")).unwrap()
}
fn case_context(case: &Case, base: &Snapshot, request: &mut AnalysisRequest) -> BuiltContext {
    let Some(scenario) = case.scenario else {
        return ContextBuilder::build(request, base, &ContextSources::default());
    };
    request.context_selection = ContextSelection {
        include_profile: !matches!(scenario, Scenario::NoPersonal),
        include_holdings: !matches!(scenario, Scenario::NoPersonal),
        include_evidence: true,
    };
    let fixture = watch_fixture();
    let mut snapshot = base.clone();
    snapshot.holdings.retain(|holding| holding.id != "fund-a");
    snapshot.holdings.extend(fixture.holdings);
    snapshot.holding_trades = fixture.holding_trades;
    let accounting = snapshot.accounting.as_mut().unwrap();
    accounting.holdings.remove("fund-a");
    accounting.holdings.extend(fixture.accounting_holdings);
    // The two buy-only positions have zero settled profit; bond/USD still have unknown history.
    accounting.investments.realized.value = Some("0".into());
    accounting.investments.realized.missing = 2;
    snapshot.valuation_status.valuation_dates = vec!["2026-10-20".into(), "2026-11-06".into()];
    snapshot.valuation_status.aligned_valuation_date = None;
    snapshot.updated_at = "2026-11-07T00:00:00Z".into();
    let ids: &[&str] = match scenario {
        Scenario::Add => &["qa-watch-current", "qa-watch-support", "qa-watch-counter"],
        Scenario::NoPersonal => &["qa-watch-current", "qa-watch-counter"],
        Scenario::Weekly => &[
            "qa-watch-weekly-current",
            "qa-watch-support",
            "qa-watch-counter",
            "qa-watch-weekly-history",
        ],
    };
    let evidence: Vec<_> = fixture
        .evidence
        .into_iter()
        .filter(|item| ids.contains(&item.id.as_str()))
        .collect();
    ContextBuilder::build(
        request,
        &snapshot,
        &ContextSources {
            evidence_candidates: &evidence,
            ..ContextSources::default()
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseResult {
    id: String,
    question: String,
    focus: String,
    previous_case_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario: Option<Scenario>,
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
    // Validate the explicit paid subset before creating a provider or a report file.
    let (snapshot, cases) = fixtures();
    let filter = std::env::var_os("MARIO_EVAL_CASE_IDS").map(|value| {
        value
            .into_string()
            .expect("MARIO_EVAL_CASE_IDS must be Unicode")
    });
    let cases =
        select_cases(cases, filter.as_deref()).expect("invalid evaluation subset/dependencies");
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
            scenario: case.scenario,
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

                web_search: settings.web_search
                    && provider_kind != crate::models::ModelProviderKind::OpenaiChat,
                context_selection: settings.context_selection.clone(),
                preview_revision: None,
            };
            let context = case_context(case, &snapshot, &mut request);
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
    // Ordinary fixture validation never reads the live-run subset environment variable.
    let (snapshot, cases) = fixtures();
    assert_eq!(cases.len(), 15);
    assert_eq!(
        cases.iter().filter(|case| case.scenario.is_none()).count(),
        12
    );
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

fn fixture_request(case: &Case) -> AnalysisRequest {
    let settings = AnalysisSettings::default();
    AnalysisRequest {
        question: case.question.clone(),
        previous_turn: None,
        workflow: settings.workflow,
        web_search: settings.web_search,
        context_selection: settings.context_selection,
        preview_revision: None,
    }
}

#[test]
fn subset_preflight_requires_explicit_dependencies_and_never_adds_paid_cases() {
    for filter in ["followup", "missing-case", "reduce,reduce", "", "reduce,"] {
        assert!(
            select_cases(fixtures().1, Some(filter)).is_err(),
            "invalid subset accepted: {filter}"
        );
    }
    let error = select_cases(fixtures().1, Some("followup")).unwrap_err();
    assert!(error.contains("reduce") && error.contains("explicitly"));
    let pair = select_cases(fixtures().1, Some("followup, reduce")).unwrap();
    assert_eq!(
        pair.iter().map(|case| case.id.as_str()).collect::<Vec<_>>(),
        ["reduce", "followup"]
    );
    let one = select_cases(fixtures().1, Some("loss")).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].id, "loss");
    assert_eq!(select_cases(fixtures().1, None).unwrap().len(), 15);
}

#[test]
fn original_twelve_cases_keep_the_same_authorized_inputs() {
    let (snapshot, cases) = fixtures();
    for case in cases.iter().filter(|case| case.scenario.is_none()) {
        let mut request = fixture_request(case);
        let expected = ContextBuilder::build(&request, &snapshot, &ContextSources::default());
        let actual = case_context(case, &snapshot, &mut request);
        assert_eq!(actual.payload, expected.payload);
        assert_eq!(actual.revision, expected.revision);
        assert!(actual.evidence_catalog.is_empty());
    }
}

#[test]
fn watch_fixture_positions_match_original_trades_and_exact_accounting() {
    use rust_decimal::Decimal;
    let number = |value: &str| Decimal::from_str_exact(value).unwrap();
    let fixture = watch_fixture();
    assert_eq!(fixture.holdings.len(), 2);
    assert_eq!(fixture.holding_trades.len(), 2);
    assert_ne!(
        fixture.holding_trades[0].account,
        fixture.holding_trades[1].account
    );
    let (mut total_quantity, mut total_cost, mut total_value) =
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    for trade in &fixture.holding_trades {
        let holding = fixture
            .holdings
            .iter()
            .find(|holding| holding.id == trade.holding_id)
            .unwrap();
        let exact = &fixture.accounting_holdings[&trade.holding_id];
        let quantity = number(&trade.quantity);
        let cost = number(&trade.gross_amount);
        let value = quantity * number(exact.amounts.current_price.as_deref().unwrap());
        assert_eq!(trade.kind, crate::models::TradeKind::Buy);
        assert_eq!(number(&trade.fees) + number(&trade.taxes), Decimal::ZERO);
        assert_eq!(quantity * number(&trade.price), cost);
        assert_eq!(number(&exact.amounts.cost_basis), cost);
        assert_eq!(
            number(exact.amounts.cost_price.as_deref().unwrap()),
            number(&trade.price)
        );
        assert_eq!(number(&exact.amounts.market_value), value);
        assert_eq!(
            number(exact.unrealized_profit.as_deref().unwrap()),
            value - cost
        );
        assert_eq!(
            number(exact.settled_profit.as_deref().unwrap()),
            Decimal::ZERO
        );
        assert_eq!(number(&holding.quantity.unwrap().to_string()), quantity);
        assert_eq!(number(exact.amounts.quantity.as_deref().unwrap()), quantity);
        assert_eq!(number(&holding.market_value.to_string()), value);
        assert_eq!(number(&holding.cost_basis.to_string()), cost);
        assert_eq!(holding.symbol, "QA-WATCH");
        assert_eq!(holding.currency, trade.currency);
        assert!(trade.occurred_on <= holding.valuation_date);
        total_quantity += quantity;
        total_cost += cost;
        total_value += value;
    }
    assert_eq!(total_quantity, number("32000"));
    assert_eq!(total_cost, number("1000000"));
    assert_eq!(total_value, number("800000"));
    for item in &fixture.evidence {
        assert!(item.source_url.starts_with("https://fictional.example/"));
        assert!(item.claim.chars().count() <= 2000);
        let public = serde_json::to_string(item).unwrap();
        for private in [
            "watch-a",
            "watch-b",
            "qa-buy-a",
            "qa-buy-b",
            "虚构证券账户甲",
            "虚构证券账户乙",
            "16000",
            "480000",
            "520000",
            "800000",
            "1000000",
        ] {
            assert!(
                !public.contains(private),
                "public evidence contains private fixture data"
            );
        }
    }
}

#[test]
fn watch_scenarios_freeze_real_evidence_and_original_trade_units_with_authorization() {
    let (snapshot, cases) = fixtures();
    for case in cases.iter().filter(|case| case.scenario.is_some()) {
        let mut request = fixture_request(case);
        let context = case_context(case, &snapshot, &mut request);
        let sources = context.payload["researchEvidence"].as_array().unwrap();
        let expected_count = match case.id.as_str() {
            "watch-add" => 3,
            "watch-no-personal" => 2,
            "watch-weekly" => 4,
            _ => panic!("unexpected fixed scenario"),
        };
        assert_eq!(sources.len(), expected_count);
        assert_eq!(sources.len(), context.evidence_catalog.len());
        assert!(sources.len() <= crate::evidence::ANALYSIS_EVIDENCE_LIMIT);
        let source = |id: &str| {
            sources.iter().find(|item| item["id"] == id).unwrap()["claim"]
                .as_str()
                .unwrap()
        };
        assert!(source("qa-watch-counter").contains("未承诺重复"));
        match case.scenario.unwrap() {
            Scenario::NoPersonal => {
                assert!(
                    !request.context_selection.include_profile
                        && !request.context_selection.include_holdings
                );
                assert_eq!(context.payload["facts"], serde_json::json!({"summary": {}}));
                assert!(context.valuation_evidence.is_empty());
                assert_eq!(sources.len(), 2);
                assert!(source("qa-watch-current").contains("TTM股息率为6%"));
                let serialized = context.payload.to_string();
                for private in [
                    "watch-a",
                    "watch-b",
                    "qa-buy-a",
                    "qa-buy-b",
                    "虚构证券账户甲",
                    "虚构证券账户乙",
                    "16000",
                    "480000",
                    "520000",
                    "1000000",
                    "800000",
                    "monthlySurplus",
                    "emergencyFund",
                ] {
                    assert!(
                        !serialized.contains(private),
                        "unauthorized data reached the model payload"
                    );
                }
            }
            Scenario::Add | Scenario::Weekly => {
                assert!(
                    request.context_selection.include_profile
                        && request.context_selection.include_holdings
                );
                assert!(source("qa-watch-support").contains("经营活动现金流保持为正"));
                let facts = &context.payload["facts"];
                assert_eq!(facts["summary"]["monthlySurplus"], "8000");
                assert!(facts["summary"]["portfolioValue"].is_null());
                assert!(facts["summary"]["valuationDate"].is_null());
                assert!(facts["holdings"].get("fund-a").is_none());
                for (id, trade, price, cost, account) in [
                    ("watch-a", "qa-buy-a", "30", "480000", "虚构证券账户甲"),
                    ("watch-b", "qa-buy-b", "32.5", "520000", "虚构证券账户乙"),
                ] {
                    let holding = &facts["holdings"][id];
                    assert_eq!(holding["marketValue"], "400000");
                    assert_eq!(holding["costBasis"], cost);
                    assert_eq!(holding["priceDate"], "2026-11-06");
                    assert_eq!(holding["recordedTradeCount"], 1);
                    let execution = &holding["recentTrades"][trade];
                    assert_eq!(execution["originalQuantity"], "16000");
                    assert_eq!(execution["originalPrice"], price);
                    assert_eq!(execution["grossAmount"], cost);
                    assert_eq!(execution["account"], account);
                }
                if matches!(case.scenario, Some(Scenario::Weekly)) {
                    assert_eq!(sources.len(), 4);
                    assert!(source("qa-watch-weekly-current").contains("6.5%"));
                    assert!(source("qa-watch-weekly-history").contains("周二没有保存行情观察"));
                    assert!(source("qa-watch-weekly-history")
                        .contains("配置变更重算视图但未生成行情进出提醒"));
                } else {
                    assert_eq!(sources.len(), 3);
                    assert!(source("qa-watch-current").contains("TTM至少5%"));
                }
            }
        }
    }
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
