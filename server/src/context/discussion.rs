//! Bounded research guidance derived only from frozen, authorized inputs.
//! It is a research path, never a company assessment or a new fact source.
use serde_json::{json, Value};

use crate::models::{analysis::AnalysisRequest, research::ResearchEvidence};

/// Most recent user input wins. Model answers and suggested actions are never queries.
pub fn retrieval_questions(request: &AnalysisRequest) -> Vec<&str> {
    let mut questions = vec![request.question.as_str()];
    if let Some(previous) = &request.previous_turn {
        questions.push(previous.question.as_str());
        questions.extend(previous.earlier_questions.iter().rev().map(String::as_str));
    }
    questions
}

pub fn mentions_company(text: &str, name: &str, symbol: &str) -> bool {
    let name = name.trim();
    if !name.is_empty() && !name.bytes().all(|byte| byte.is_ascii_digit()) && text.contains(name) {
        return true;
    }
    if let Some(symbol) = ashare_symbol(symbol) {
        return explicit_ashare_symbols(text).contains(&symbol);
    }
    !symbol.trim().is_empty() && text.contains(symbol)
}

/// Recognize only bounded six-digit Shanghai/Shenzhen A-share identifiers.
/// A current explicit code blocks falling back to a different historical company.
pub fn explicit_ashare_symbols(text: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    for (start, _) in text.match_indices(|ch: char| ch.is_ascii_digit()) {
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }
        let tail = &text[start..];
        let Some(code) = tail.get(..6) else { continue };
        if !code.bytes().all(|byte| byte.is_ascii_digit())
            || tail.as_bytes().get(6).is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        let exchange = if code.starts_with("60") || code.starts_with("68") {
            "SS"
        } else if code.starts_with("00") || code.starts_with("30") {
            "SZ"
        } else {
            continue;
        };
        let symbol = if tail[6..].starts_with('.') {
            tail[6..]
                .get(..3)
                .and_then(|suffix| ashare_symbol(&format!("{code}{suffix}")))
        } else {
            let after = tail[6..].trim_start();
            let before = text[..start].trim_end_matches(|ch: char| {
                ch.is_whitespace() || ['：', ':', '=', '为', '是'].contains(&ch)
            });
            let amount_unit = [
                "元",
                "万",
                "亿",
                "份",
                "手",
                "%",
                "％",
                "人民币",
                "美元",
                "港币",
                "港元",
                "CNY",
                "USD",
                "HKD",
            ]
            .iter()
            .any(|unit| after.starts_with(unit));
            let share_quantity = after.starts_with('股')
                && !["股息", "股价", "股票", "股份"]
                    .iter()
                    .any(|term| after.starts_with(term));
            let amount_label = [
                "现金", "余额", "金额", "收入", "支出", "市值", "成本", "负债", "数量",
            ]
            .iter()
            .any(|label| before.ends_with(label));
            if amount_unit || share_quantity || amount_label {
                continue;
            }
            Some(format!("{code}.{exchange}"))
        };
        if let Some(symbol) = symbol {
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
        }
    }
    symbols
}

fn ashare_symbol(symbol: &str) -> Option<String> {
    let normalized = symbol.to_ascii_uppercase().replace(".SH", ".SS");
    let (code, exchange) = normalized.split_once('.')?;
    (code.len() == 6
        && code.bytes().all(|byte| byte.is_ascii_digit())
        && ((exchange == "SS" && (code.starts_with("60") || code.starts_with("68")))
            || (exchange == "SZ" && (code.starts_with("00") || code.starts_with("30")))))
    .then_some(normalized)
}

fn evidence_company(item: &ResearchEvidence) -> Option<(String, String)> {
    let watch_symbol = item
        .id
        .strip_prefix("watch-")
        .and_then(|suffix| suffix.get(..9))
        .and_then(ashare_symbol);
    let symbol = watch_symbol.or_else(|| {
        item.asset_name
            .split(|ch: char| ch.is_whitespace() || ['（', '）', '(', ')'].contains(&ch))
            .find_map(ashare_symbol)
    })?;
    let name = item.asset_name.replace(&symbol, "").trim().to_owned();
    let name = if name.is_empty() {
        symbol.clone()
    } else {
        name
    };
    Some((name, symbol))
}

fn companies(
    request: &AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> Vec<(String, String)> {
    let mut companies = Vec::new();
    if request.context_selection.include_evidence {
        companies.extend(candidates.iter().filter_map(evidence_company));
    }
    if request.context_selection.include_holdings {
        for holding in facts["holdings"]
            .as_object()
            .into_iter()
            .flat_map(|v| v.values())
        {
            if let (Some(name), Some(symbol)) = (
                holding["name"].as_str(),
                holding["symbol"].as_str().and_then(ashare_symbol),
            ) {
                companies.push((name.to_owned(), symbol));
            }
        }
    }
    companies
}

fn has_dividend_topic(question: &str) -> bool {
    ["股息", "分红", "红利", "派息", "买点", "BOLL"]
        .iter()
        .any(|term| question.to_uppercase().contains(term))
}

fn general_topic(question: &str) -> &'static str {
    if ["负债", "贷款", "还贷", "提前还"]
        .iter()
        .any(|term| question.contains(term))
    {
        "debt_interest_and_repayment_costs"
    } else if ["应急", "结余", "收入", "支出", "备用金"]
        .iter()
        .any(|term| question.contains(term))
    {
        "emergency_liquidity_and_cash_flow"
    } else if ["期限", "年后", "用钱", "用款"]
        .iter()
        .any(|term| question.contains(term))
    {
        "investment_horizon_and_liquidity"
    } else if ["加仓", "减仓", "减持", "集中", "亏损"]
        .iter()
        .any(|term| question.contains(term))
    {
        "portfolio_concentration_and_transaction_costs"
    } else {
        "general_finance"
    }
}

pub fn is_general_finance_question(question: &str) -> bool {
    !has_dividend_topic(question) && general_topic(question) != "general_finance"
}

pub fn subject_question<'a>(
    request: &'a AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> Option<&'a str> {
    let companies = companies(request, candidates, facts);
    if !explicit_ashare_symbols(&request.question).is_empty()
        || companies
            .iter()
            .any(|(name, symbol)| mentions_company(&request.question, name, symbol))
        || is_general_finance_question(&request.question)
    {
        return Some(&request.question);
    }
    retrieval_questions(request)
        .into_iter()
        .skip(1)
        .find(|question| {
            !explicit_ashare_symbols(question).is_empty()
                || companies
                    .iter()
                    .any(|(name, symbol)| mentions_company(question, name, symbol))
        })
}

/// Keep the current wording and at most one relevant user question for excerpt selection.
pub fn retrieval_query(
    request: &AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> String {
    match subject_question(request, candidates, facts) {
        Some(subject) if subject != request.question => format!("{}\n{subject}", request.question),
        _ => request.question.clone(),
    }
}

/// Both report generation and native search consume this frozen public task pack.
/// Only canonical securities codes and fixed topics cross the search boundary.
pub fn build_discussion(
    request: &AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> Value {
    let subject = subject_question(request, candidates, facts).unwrap_or(&request.question);
    let mut selected = explicit_ashare_symbols(subject);
    if selected.is_empty() {
        selected.extend(
            companies(request, candidates, facts)
                .into_iter()
                .filter_map(|(name, symbol)| {
                    mentions_company(subject, &name, &symbol).then_some(symbol)
                }),
        );
    }
    selected.sort();
    selected.dedup();
    let watch_company = request.context_selection.include_evidence
        && selected.iter().any(|symbol| {
            candidates
                .iter()
                .any(|item| item.id.starts_with(&format!("watch-{symbol}-")))
        });
    let dividend =
        has_dividend_topic(&request.question) || has_dividend_topic(subject) || watch_company;
    let topics = if dividend {
        vec![
            "dividend_composition_and_commitment",
            "cash_coverage_and_capital_needs",
            "long_term_demand_and_counterevidence",
            "price_and_yield_basis",
        ]
    } else {
        vec![general_topic(&request.question)]
    };
    let checks = if dividend {
        vec![
            "先按已保存规则与实际观察日解释市场条件；技术位置只产生关注状态，不证明低估。历史记录按当时规则，缺记录日期不补造。",
            "核对已实施TTM分红与未来承诺、普通与特别分红及其来源；没有公告区分时保持未知。普通分红情景必须标明假设，不替换观察使用的实际TTM。",
            "用同期间公司披露核查盈利、经营现金流和派息，结合资本开支、债务或监管资本需要判断覆盖；银行等金融企业不能机械套用非金融公司的自由现金流标准。",
            "列明长期需求和分红持续性的支持、最强反方及尚缺原文；引用模型摘要不能升级为已核查公司原文。指出哪项新证据会改变结论。",
            "区分当前价格的股息率、持仓成本口径、未来分红情景与个人实际到账；股息率不是总回报。情景引用当前精确股数与参考价及其日期、复权口径，不能从最近20笔交易反推完整持仓，不能直接混用前复权参考价和公司每股现金分红。",
            "个人适用性只用本轮授权的当前持仓与现金状态；未授权或缺数时说明对应判断边界，不猜买入数量、仓位上限或交易动机。给与本题有关的核查或等待触发条件，不要求完成所有研究才能继续观察。",
        ]
    } else {
        vec!["直接回答当前问题，只用授权事实和可复核计算。缺口只限制对应判断；不要将一般理财问题展开为公司红利研究。"]
    };
    json!({
        "kind": if dividend { "dividend_discussion" } else { "general_finance" },
        "publicResearch": {
            "companies": selected.into_iter().map(|symbol| json!({"symbol":symbol})).collect::<Vec<_>>(),
            "topics": topics,
        },
        "researchChecks": checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(question: &str) -> AnalysisRequest {
        serde_json::from_value(json!({"question":question,"workflow":"council"})).unwrap()
    }

    fn facts() -> Value {
        json!({"holdings": {
            "one":{"name":"虚构红利甲","symbol":"600036.SS","marketValue":"PRIVATE_AMOUNT"},
            "two":{"name":"虚构红利乙","symbol":"000001.SZ","account":"PRIVATE_ACCOUNT"}
        }})
    }

    #[test]
    fn holding_horizon_alone_is_not_a_dividend_question() {
        let guide = build_discussion(
            &request("指数基金准备长期持有，要不要设止损？"),
            &[],
            &facts(),
        );
        assert_eq!(guide["kind"], "general_finance");
        assert_eq!(guide["researchChecks"].as_array().unwrap().len(), 1);
        let guide = build_discussion(&request("准备长期持有，分红能持续吗？"), &[], &facts());
        assert_eq!(guide["kind"], "dividend_discussion");
    }

    #[test]
    fn company_matching_accepts_exchange_aliases_but_not_embedded_numeric_ids() {
        for text in ["600036", "600036.SH的分红", "600036.SS", "虚构公司好吗"] {
            assert!(mentions_company(text, "虚构公司", "600036.SS"));
        }
        assert!(!mentions_company("订单16000360", "虚构公司", "600036.SS"));
        assert!(!mentions_company("问公司", "", ""));
    }

    #[test]
    fn personal_amounts_that_look_like_codes_never_become_search_companies() {
        for question in [
            "我的现金600036元，该怎么留应急金？",
            "金额：300000元怎么分配？",
            "我有现金 600036，结余怎么留？",
            "持有600036股，还需要留多少应急金？",
            "收入600036万元，支出怎么办？",
        ] {
            assert!(explicit_ashare_symbols(question).is_empty(), "{question}");
            assert!(!mentions_company(question, "虚构红利甲", "600036.SS"));
            let guide = build_discussion(&request(question), &[], &facts());
            assert_eq!(
                guide["publicResearch"]["companies"],
                json!([]),
                "{question}"
            );
            let public = guide["publicResearch"].to_string();
            assert!(!public.contains("600036") && !public.contains("300000"));
        }
        for question in [
            "600036股息能持续吗",
            "600036股票值得看吗",
            "600036分红",
            "600036",
            "600036.SH",
        ] {
            assert_eq!(
                explicit_ashare_symbols(question),
                vec!["600036.SS"],
                "{question}"
            );
        }
    }

    #[test]
    fn followup_uses_only_user_questions_and_current_company_wins() {
        let mut request = request("它的普通分红能覆盖吗？私人数字SECRET_123");
        request.previous_turn = Some(serde_json::from_value(json!({
            "question":"还有什么风险？", "answer":"模型声称研究000001.SZ MODEL_SECRET", "suggestions":["买乙"],
            "earlierQuestions":["600036.SH值得研究吗？"]
        })).unwrap());
        assert_eq!(retrieval_questions(&request).len(), 3);
        let guide = build_discussion(&request, &[], &facts());
        assert_eq!(
            guide["publicResearch"]["companies"],
            json!([{"symbol":"600036.SS"}])
        );
        let public = guide["publicResearch"].to_string();
        for private in [
            "PRIVATE_AMOUNT",
            "PRIVATE_ACCOUNT",
            "SECRET_123",
            "MODEL_SECRET",
            "000001",
        ] {
            assert!(!public.contains(private));
        }
        request.question = "000001.SZ的股息呢？".into();
        assert_eq!(
            build_discussion(&request, &[], &facts())["publicResearch"]["companies"][0]["symbol"],
            "000001.SZ"
        );
    }

    #[test]
    fn general_finance_has_no_dividend_template_or_implicit_portfolio_topics() {
        let guide = build_discussion(&request("应急金怎么补？"), &[], &facts());
        assert_eq!(guide["kind"], "general_finance");
        assert_eq!(guide["publicResearch"]["companies"], json!([]));
        assert_eq!(
            guide["publicResearch"]["topics"],
            json!(["emergency_liquidity_and_cash_flow"])
        );
    }

    #[test]
    fn explicit_new_code_blocks_old_company_and_excerpt_query_omits_old_topics() {
        let mut request = request("600519.SH股息呢？");
        request.previous_turn = Some(
            serde_json::from_value(json!({
                "question":"600036.SS分红如何？", "answer":"模型结论", "suggestions":[]
            }))
            .unwrap(),
        );
        assert_eq!(
            explicit_ashare_symbols(&request.question),
            vec!["600519.SS"]
        );
        let guide = build_discussion(&request, &[], &facts());
        assert_eq!(
            guide["publicResearch"]["companies"],
            json!([{"symbol":"600519.SS"}])
        );
        assert_eq!(retrieval_query(&request, &[], &facts()), request.question);
        assert!(explicit_ashare_symbols("订单16000360 600036.SZ 900000").is_empty());
        assert_eq!(
            explicit_ashare_symbols("600036，000001.sz和600036.SH"),
            vec!["600036.SS", "000001.SZ"]
        );
    }

    #[test]
    fn explicit_finance_question_replaces_prior_dividend_subject() {
        let mut request = request("应急金怎么补？");
        request.previous_turn = Some(
            serde_json::from_value(json!({
                "question":"600036.SS股息和分红如何？", "answer":"模型结论", "suggestions":[]
            }))
            .unwrap(),
        );
        let guide = build_discussion(&request, &[], &facts());
        assert_eq!(guide["kind"], "general_finance");
        assert_eq!(guide["publicResearch"]["companies"], json!([]));
        assert_eq!(retrieval_query(&request, &[], &facts()), request.question);
        let mut names = facts();
        names["holdings"]["one"]["name"] = json!("PRIVATE_NAME_9000000");
        request.question = "600036分红如何？".into();
        assert!(!build_discussion(&request, &[], &names)["publicResearch"]
            .to_string()
            .contains("PRIVATE_NAME"));
    }

    #[test]
    fn authorized_watch_company_resolves_name_without_exporting_free_form_metadata() {
        let evidence: ResearchEvidence = serde_json::from_value(json!({
            "id":"watch-600036.SS-2026-11-06", "assetName":"虚构公司甲", "title":"收盘观察", "publisher":"虚构来源",
            "sourceUrl":"https://fictional.example/watch", "sourceTier":"二手来源", "evidenceType":"watch_observation", "stance":"neutral",
            "asOfDate":"2026-11-06", "claim":"PRIVATE_SOURCE_TEXT", "notes":"PRIVATE_NOTE", "active":true,"capturedAt":"2026-11-06T12:00:00Z"
        })).unwrap();
        let mut request = request("虚构公司甲值得进一步研究吗？");
        request.context_selection.include_holdings = false;
        let guide = build_discussion(&request, std::slice::from_ref(&evidence), &json!({}));
        assert_eq!(guide["kind"], "dividend_discussion");
        assert_eq!(
            guide["publicResearch"]["companies"],
            json!([{"symbol":"600036.SS"}])
        );
        assert!(!guide.to_string().contains("PRIVATE_"));
        request.context_selection.include_evidence = false;
        let hidden = build_discussion(&request, &[evidence], &json!({}));
        assert_eq!(hidden["publicResearch"]["companies"], json!([]));
        assert_eq!(hidden["kind"], "general_finance");
    }

    #[test]
    fn holdings_authorization_and_unresolved_identity_do_not_leak_free_text() {
        let mut request = request("它分红如何；私人数据PRIVATE_SENTINEL");
        request.context_selection.include_holdings = false;
        let guide = build_discussion(&request, &[], &facts());
        assert_eq!(guide["kind"], "dividend_discussion");
        assert_eq!(guide["publicResearch"]["companies"], json!([]));
        assert!(!guide.to_string().contains("PRIVATE_SENTINEL"));
        assert!(guide["researchChecks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("保持未知")));
    }
}
