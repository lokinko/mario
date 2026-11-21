//! Bounded research guidance, domain routing, and adaptive execution-mode selection
//! derived only from frozen, authorized inputs.
//! It is a research path, never a company assessment or a new fact source.
use serde_json::{json, Value};

use crate::models::{
    analysis::AnalysisRequest, council::ExecutionMode, market::canonical_symbol,
    research::ResearchEvidence,
};

/// Professional investment analysis domains supported by the advisor router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvestmentDomain {
    DividendDiscussion,
    EquityValuation,
    ExitOrRebalance,
    PortfolioAllocation,
    CashflowAndDebt,
    OptionsOrHedging,
    MacroOrSector,
    GeneralFinance,
}

impl InvestmentDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DividendDiscussion => "dividend_discussion",
            Self::EquityValuation => "equity_valuation",
            Self::ExitOrRebalance => "exit_or_rebalance",
            Self::PortfolioAllocation => "portfolio_allocation",
            Self::CashflowAndDebt => "cashflow_and_debt",
            Self::OptionsOrHedging => "options_or_hedging",
            Self::MacroOrSector => "macro_or_sector",
            Self::GeneralFinance => "general_finance",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DividendDiscussion => "红利与分红可持续性",
            Self::EquityValuation => "个股估值与基本面",
            Self::ExitOrRebalance => "减仓与换仓决策",
            Self::PortfolioAllocation => "资产配置与组合风控",
            Self::CashflowAndDebt => "现金流与负债规划",
            Self::OptionsOrHedging => "期权与对冲策略",
            Self::MacroOrSector => "宏观与行业事件评估",
            Self::GeneralFinance => "通用财务与资料核查",
        }
    }

    pub fn framework_focus(self) -> &'static str {
        match self {
            Self::DividendDiscussion => {
                "重点核查分红可持续性：严格区分已实施TTM与未来分红情景、普通分红与特别分红、公司每股派息与个人实际到账；结合同期盈利、经营现金流、资本开支或金融业监管资本评估覆盖能力，并正面处理长期需求放缓等最强反方。"
            }
            Self::EquityValuation => {
                "重点核查基本面质量与估值安全边际：拆解商业模式与盈利驱动（区分周期波动与结构性趋势），写清估值指标口径与隐含增长假设，核对资产负债表、自由现金流与资本开支约束，明确最能推翻多头逻辑的证伪指标。"
            }
            Self::ExitOrRebalance => {
                "重点评估卖出、止损与换仓的前瞻性风险收益比：严格剥离沉没成本与账面浮亏情绪，核查初始买入逻辑是否失效；按可用估值与权重测算集中度及资金去向（留存组合内与转出需区分分母重算），并比较分批执行与等待的交易及机会成本。"
            }
            Self::PortfolioAllocation => {
                "重点审视组合级风险预算与资产配置：优先对齐授权的期限、流动性安全垫、负债与最大回撤容忍度；识别单一标的、资产类别及币种集中度敞口，评估尾部回撤情景下的承压边界与增量资金/再平衡触发条件。"
            }
            Self::CashflowAndDebt => {
                "重点平衡流动性安全垫、刚性用款期限与负债成本：明确净结余口径（不重复扣除已计入支出的正常还款），比较确定性负债利率成本与风险资产不确定回报，先守住短期刚性现金需求再讨论投资配置。"
            }
            Self::OptionsOrHedging => {
                "重点拆解期权与对冲的非线性损益与尾部风险：按授权合约要素核对名义敞口、行权价、到期日与保证金/履约现金要求；对比上涨踏空、下跌接盘、时间价值与展期摩擦成本，并比较直接调仓或现金缓冲等更简单替代方案。"
            }
            Self::MacroOrSector => {
                "重点核查宏观政策或行业周期向资产价格与基本面的具体传导路径：区分官方数据与市场预期，避免泛泛宏大叙事；分别评估基准与反向情景对当前授权持仓盈利、折现率或汇率敞口的实际敏感度及可跟踪指标。"
            }
            Self::GeneralFinance => {
                "直接围绕本轮问题核对授权事实、口径或金融概念，清晰说明已知与未知边界，不强行展开无关的资产配置或公司研究。"
            }
        }
    }

    pub fn search_guidance(self) -> &'static str {
        match self {
            Self::DividendDiscussion => {
                "优先检索公司官方最新分红公告与派息政策、普通与特别分红拆分、同期盈利与经营现金流及资本开支覆盖数据、长期需求与行业反方风险。"
            }
            Self::EquityValuation => {
                "优先检索公司最新年报/季报核心营收与盈利质量、现金流与资产负债表、主流估值口径及历史区间、行业竞争与看空反方证据。"
            }
            Self::ExitOrRebalance => {
                "优先检索相关标的或指数最新基本面变化、估值与风险披露、交易/税务公开规则及反方观点，不涉及任何个人交易信息。"
            }
            Self::PortfolioAllocation => {
                "优先检索相关资产类别或公开标的的风险特征、历史回撤/相关性特征与费用结构等公开披露资料。"
            }
            Self::CashflowAndDebt => {
                "优先检索与本主题相关的公开利率、监管/税务或金融产品一般规则说明，不夹带任何个人收支或负债数据。"
            }
            Self::OptionsOrHedging => {
                "优先检索标的除息/分红日程、交易所期权合约规则、保证金与行权机制等公开资料。"
            }
            Self::MacroOrSector => {
                "优先检索央行、统计局或监管机构最新官方数据与政策原文、行业景气指标及正反两派核心分析依据。"
            }
            Self::GeneralFinance => {
                "只针对给定主题查找权威定义、监管或官方公开说明；无具体公司时不补猜公司。"
            }
        }
    }

    pub fn research_checks(self) -> Vec<&'static str> {
        match self {
            Self::DividendDiscussion => vec![
                "先按已保存规则与实际观察日解释市场条件；技术位置只产生关注状态，不证明低估。历史记录按当时规则，缺记录日期不补造。",
                "核对已实施TTM分红与未来承诺、普通与特别分红及其来源；没有公告区分时保持未知。普通分红情景必须标明假设，不替换观察使用的实际TTM。",
                "用同期间公司披露核查盈利、经营现金流和派息，结合资本开支、债务或监管资本需要判断覆盖；银行等金融企业不能机械套用非金融公司的自由现金流标准。",
                "列明长期需求和分红持续性的支持、最强反方及尚缺原文；引用模型摘要不能升级为已核查公司原文。指出哪项新证据会改变结论。",
                "区分当前价格的股息率、持仓成本口径、未来分红情景与个人实际到账；股息率不是总回报。情景引用当前精确股数与参考价及其日期、复权口径，不能从最近20笔交易反推完整持仓，不能直接混用前复权参考价和公司每股现金分红。",
                "个人适用性只用本轮授权的当前持仓与现金状态；未授权或缺数时说明对应判断边界，不猜买入数量、仓位上限或交易动机。给与本题有关的核查或等待触发条件，不要求完成所有研究才能继续观察。",
            ],
            Self::EquityValuation => vec![
                "核对标的商业模式、核心盈利驱动与盈利质量，区分周期波动与结构性趋势；缺少同期财报原文时保持未知。",
                "核对当前估值口径（如PE/PB/EV或自由现金流收益率）及其隐含增长与利润率假设，不把价格下跌或历史低位直接等同于低估。",
                "核查资产负债表韧性、资本开支、债务到期与现金流转化能力，识别稀释、商誉或再融资等关键脆弱点。",
                "列明最强反方（看空情景与关键证伪指标），并结合本轮授权持仓与风险边界给出明确的核查或等待条件。",
            ],
            Self::ExitOrRebalance => vec![
                "核查初始投资逻辑、基本面或估值是否已变化，明确区分沉没成本/账面盈亏与前瞻风险收益比；不把等待回本当作独立持有理由。",
                "按授权当前持仓与可用权重评估单一标的集中度、组合风险敞口与流动性约束；缺总值或汇率时仅计算可用口径。",
                "写清执行路径的交易摩擦、潜在税负与资金去向（卖款留在组合内与转出提款/还债须按变化后分母与现金缺口分别核算）。",
                "给出分批或条件执行的可复核触发条件、复核节点与放弃前提；减仓或换仓幅度须说明依据，依据不足时仅给条件公式或非默认算例。",
            ],
            Self::PortfolioAllocation => vec![
                "优先核对授权财务档案中的投资期限、应急金安全垫、负债压力与最大回撤容忍度；未提供的目标或偏好标为假设。",
                "核对当前资产类别、单一标的及币种集中度敞口，明确标出缺汇率、未估值或待核对项目的判断边界。",
                "评估核心尾部风险与压力情景（大幅回撤、利率/汇率变动或现金需求提前）下的组合承压能力及最强反方。",
                "给出配置调整或增量资金分配的优先级、再平衡触发阈值及可复核的复盘条件。",
            ],
            Self::CashflowAndDebt => vec![
                "核对授权的月收入、月支出、月结余、应急金与负债原值；已计入支出的正常还款不重复扣除，仅另计尚未包含的现金需求。",
                "评估应急金安全垫与短期刚性用款期限错配，避免将短期必须使用的资金暴露于高波动资产。",
                "比较确定性负债利率成本与风险资产预期回报及不确定性，兼顾流动性期权价值与机会成本，说明不同前提下的取舍。",
            ],
            Self::OptionsOrHedging => vec![
                "核对授权持仓与期权合约要素（方向、认购/认沽、行权价、到期日、合约乘数）及名义敞口与保证金/履约现金需求，不补造缺失参数。",
                "拆解非线性损益结构与极端尾部风险（上涨踏空、下跌接盘、保证金追加、隐含波动率与时间价值损耗）。",
                "评估到期日、流动性与展期/平仓交易摩擦成本，指出最可能推翻策略预期的市场情景。",
                "与直接持有、分批减仓或现金缓冲等简单替代方案对比，给出清晰的行权应对、平仓或放弃条件。",
            ],
            Self::MacroOrSector => vec![
                "区分官方已发布数据/政策原文与市场预期叙事，核对关键口径与发布时间，缺失一手来源时保持未知。",
                "理清宏观或行业变化向目标资产盈利、现金流、估值折现率或汇率的具体传导链条，避免空泛宏观推演。",
                "同时给出基准情景、反向情景与可观察的先行指标，并落回到本轮授权持仓与流动性的实际敏感边界。",
            ],
            Self::GeneralFinance => vec![
                "直接回答当前问题，只用授权事实和可复核计算。缺口只限制对应判断；不要将一般理财问题展开为无关配置或公司研究。",
            ],
        }
    }

    pub fn public_topics(self, question: &str) -> Vec<&'static str> {
        match self {
            Self::DividendDiscussion => vec![
                "dividend_composition_and_commitment",
                "cash_coverage_and_capital_needs",
                "long_term_demand_and_counterevidence",
                "price_and_yield_basis",
            ],
            Self::EquityValuation => vec![
                "business_model_and_earnings_quality",
                "valuation_and_margin_of_safety",
                "balance_sheet_and_cash_flow",
                "key_risks_and_bear_case",
            ],
            Self::ExitOrRebalance => vec![
                "portfolio_concentration_and_rebalancing",
                "exit_criteria_and_opportunity_cost",
                "transaction_and_tax_friction",
            ],
            Self::PortfolioAllocation => vec![
                "asset_allocation_and_risk_budget",
                "diversification_and_drawdown_control",
                "currency_and_liquidity_exposure",
            ],
            Self::CashflowAndDebt => vec![cashflow_topic(question)],
            Self::OptionsOrHedging => vec![
                "option_payoff_and_tail_risk",
                "hedging_cost_and_margin_requirements",
                "alternative_risk_management",
            ],
            Self::MacroOrSector => vec![
                "macro_policy_and_transmission",
                "sector_cycle_and_fundamentals",
                "portfolio_sensitivity_and_counterevidence",
            ],
            Self::GeneralFinance => vec!["general_finance"],
        }
    }
}

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
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return false;
    }
    if let Some(ashare) = ashare_symbol(symbol) {
        return explicit_ashare_symbols(text).contains(&ashare);
    }
    if let Some(hk) = hk_symbol(symbol) {
        return explicit_hk_symbols(text).contains(&hk);
    }
    if symbol.is_ascii() {
        return contains_ascii_token_case_insensitive(text, symbol);
    }
    text.contains(symbol)
}

/// Recognize bounded six-digit Shanghai/Shenzhen A-share stock identifiers.
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

/// Recognize multi-market public security symbols explicitly written in user text:
/// A-share stocks, suffixed mainland ETFs/LOFs, Hong Kong `.HK` codes, and `$TICKER` notations.
pub fn explicit_market_symbols(text: &str) -> Vec<String> {
    let mut symbols = explicit_ashare_symbols(text);
    for symbol in explicit_suffixed_mainland_funds(text)
        .into_iter()
        .chain(explicit_hk_symbols(text))
        .chain(explicit_cashtag_symbols(text))
    {
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    symbols
}

fn explicit_suffixed_mainland_funds(text: &str) -> Vec<String> {
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
        if !code.bytes().all(|byte| byte.is_ascii_digit()) || !tail[6..].starts_with('.') {
            continue;
        }
        let Some(suffix) = tail[6..].get(..3) else {
            continue;
        };
        if tail
            .as_bytes()
            .get(9)
            .is_some_and(|b| b.is_ascii_alphanumeric())
        {
            continue;
        }
        if let Ok(normalized) = canonical_symbol(&format!("{code}{suffix}")) {
            if crate::models::market::is_mainland_fund(&normalized)
                && !symbols.contains(&normalized)
            {
                symbols.push(normalized);
            }
        }
    }
    symbols
}

fn explicit_hk_symbols(text: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    let upper = text.to_ascii_uppercase();
    let mut offset = 0;
    while let Some(pos) = upper[offset..].find(".HK") {
        let dot = offset + pos;
        let after_idx = dot + 3;
        offset = after_idx;
        if upper
            .as_bytes()
            .get(after_idx)
            .is_some_and(|b| b.is_ascii_alphanumeric())
        {
            continue;
        }
        let prefix = &upper[..dot];
        let digit_start = prefix
            .rfind(|ch: char| !ch.is_ascii_digit())
            .map_or(0, |idx| idx + 1);
        let digits = &prefix[digit_start..];
        if (1..=5).contains(&digits.len())
            && digits.bytes().all(|b| b.is_ascii_digit())
            && !prefix[..digit_start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric())
        {
            if let Some(normalized) = hk_symbol(&format!("{digits}.HK")) {
                if !symbols.contains(&normalized) {
                    symbols.push(normalized);
                }
            }
        }
    }
    symbols
}

fn explicit_cashtag_symbols(text: &str) -> Vec<String> {
    const NON_TICKER_ACRONYMS: &[&str] = &[
        "USD", "CNY", "HKD", "EUR", "JPY", "GBP", "ETF", "LOF", "REIT", "PE", "PB", "PS", "ROE",
        "ROA", "ROIC", "TTM", "FCF", "EPS", "BPS", "DCF", "IPO", "BOLL", "MACD", "RSI", "API",
        "AI", "JSON", "CSV", "HTTP", "PUT", "CALL", "IV", "NAV", "AUM", "CAGR", "CPI", "GDP",
    ];
    let mut symbols = Vec::new();
    let bytes = text.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'$' {
            continue;
        }
        if idx > 0 && bytes[idx - 1].is_ascii_alphanumeric() {
            continue;
        }
        let tail = &text[idx + 1..];
        let len = tail
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b'.')
            .count();
        if !(1..=10).contains(&len) {
            continue;
        }
        let raw = &tail[..len];
        if !raw.bytes().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let upper = raw.to_ascii_uppercase();
        if NON_TICKER_ACRONYMS.contains(&upper.as_str()) {
            continue;
        }
        if let Ok(normalized) = canonical_symbol(&upper) {
            if !symbols.contains(&normalized) {
                symbols.push(normalized);
            }
        }
    }
    symbols
}

fn contains_ascii_token_case_insensitive(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let hay = text.to_ascii_uppercase();
    let needle = token.to_ascii_uppercase();
    let bytes = hay.as_bytes();
    let mut offset = 0;
    while let Some(pos) = hay[offset..].find(&needle) {
        let start = offset + pos;
        let end = start + needle.len();
        let left_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if left_ok && right_ok {
            return true;
        }
        offset = start + 1;
    }
    false
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

fn hk_symbol(symbol: &str) -> Option<String> {
    let normalized = symbol.trim().to_ascii_uppercase();
    let digits = normalized.strip_suffix(".HK")?;
    ((1..=5).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| format!("{:04}.HK", digits.parse::<u32>().unwrap_or_default()))
}

fn normalize_public_symbol(symbol: &str) -> Option<String> {
    if let Some(ashare) = ashare_symbol(symbol) {
        return Some(ashare);
    }
    if let Some(hk) = hk_symbol(symbol) {
        return Some(hk);
    }
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return None;
    }
    let canonical = canonical_symbol(trimmed).ok()?;
    if crate::models::market::is_mainland_fund(&canonical) {
        return Some(canonical);
    }
    // Accept concise standard ticker symbols (e.g. AAPL, SPY, BRK-B) without leaking long free-form strings.
    let is_ticker = (1..=8).contains(&canonical.len())
        && canonical
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic())
        && canonical
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    is_ticker.then_some(canonical)
}

fn evidence_company(item: &ResearchEvidence) -> Option<(String, String)> {
    let watch_symbol = item
        .id
        .strip_prefix("watch-")
        .and_then(|suffix| suffix.split('-').next())
        .and_then(normalize_public_symbol);
    let symbol = watch_symbol.or_else(|| {
        item.asset_name
            .split(|ch: char| ch.is_whitespace() || ['（', '）', '(', ')'].contains(&ch))
            .find_map(normalize_public_symbol)
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
                holding["symbol"].as_str().and_then(normalize_public_symbol),
            ) {
                companies.push((name.to_owned(), symbol));
            }
        }
    }
    companies
}

fn has_dividend_topic(question: &str) -> bool {
    let upper = question.to_uppercase();
    ["股息", "分红", "红利", "派息", "买点", "BOLL"]
        .iter()
        .any(|term| upper.contains(term))
}

fn has_options_topic(question: &str) -> bool {
    [
        "期权",
        "备兑",
        "卖沽",
        "认购",
        "认沽",
        "行权价",
        "行权",
        "到期日",
        "对冲",
        "保证金",
        "滚仓",
        "看涨期权",
        "看跌期权",
    ]
    .iter()
    .any(|term| question.contains(term))
        || contains_ascii_token_case_insensitive(question, "put")
        || contains_ascii_token_case_insensitive(question, "call")
        || contains_ascii_token_case_insensitive(question, "covered")
}

fn has_exit_or_rebalance_topic(question: &str) -> bool {
    [
        "减仓",
        "减持",
        "卖出",
        "清仓",
        "止损",
        "止盈",
        "割肉",
        "换仓",
        "调仓",
        "加仓",
        "补仓",
        "套牢",
        "浮亏",
        "回本",
        "高抛",
        "再平衡",
        "亏损",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn has_cashflow_or_debt_topic(question: &str) -> bool {
    [
        "负债",
        "贷款",
        "还贷",
        "房贷",
        "提前还",
        "利息",
        "应急",
        "结余",
        "收入",
        "支出",
        "备用金",
        "期限",
        "年后",
        "用钱",
        "用款",
        "购房",
        "教育金",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn has_macro_or_sector_topic(question: &str) -> bool {
    [
        "宏观",
        "降息",
        "加息",
        "美联储",
        "央行",
        "通胀",
        "通缩",
        "国债收益率",
        "美债",
        "关税",
        "衰退",
        "行业周期",
        "周期拐点",
        "政策冲击",
        "汇率波动",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn has_portfolio_allocation_topic(question: &str) -> bool {
    [
        "资产配置",
        "组合",
        "配置比例",
        "集中度",
        "集中风险",
        "风险预算",
        "回撤",
        "风险摘要",
        "检查风险",
        "检查持仓",
        "持仓体检",
        "分散",
        "股债",
        "相关性",
        "汇率敞口",
        "仓位",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn has_valuation_topic(question: &str) -> bool {
    [
        "估值",
        "市盈率",
        "市净率",
        "护城河",
        "商业模式",
        "财报",
        "年报",
        "季报",
        "盈利质量",
        "毛利率",
        "自由现金流",
        "资本开支",
        "安全边际",
        "值不值得",
        "值得投资",
        "值得买",
        "值得研究",
        "值得进一步研究",
        "贵不贵",
        "基本面",
        "业绩",
    ]
    .iter()
    .any(|term| question.contains(term))
        || contains_ascii_token_case_insensitive(question, "pe")
        || contains_ascii_token_case_insensitive(question, "pb")
        || contains_ascii_token_case_insensitive(question, "roe")
        || contains_ascii_token_case_insensitive(question, "roic")
        || contains_ascii_token_case_insensitive(question, "fcf")
        || contains_ascii_token_case_insensitive(question, "dcf")
}

fn cashflow_topic(question: &str) -> &'static str {
    if ["负债", "贷款", "还贷", "房贷", "提前还", "利息"]
        .iter()
        .any(|term| question.contains(term))
    {
        "debt_interest_and_repayment_costs"
    } else if ["应急", "结余", "收入", "支出", "备用金"]
        .iter()
        .any(|term| question.contains(term))
    {
        "emergency_liquidity_and_cash_flow"
    } else {
        "investment_horizon_and_liquidity"
    }
}

fn has_decision_intent(question: &str) -> bool {
    [
        "要不要",
        "该不该",
        "是否",
        "怎么",
        "如何",
        "应该",
        "建议",
        "方案",
        "值不值得",
        "值得",
        "能不能",
        "可以买",
        "买不买",
        "卖不卖",
        "先还",
        "分配",
        "安排",
        "调整",
        "配置",
        "加仓",
        "减仓",
        "减持",
        "卖出",
        "买入",
        "止损",
        "止盈",
        "换仓",
        "调仓",
        "补仓",
        "对冲",
        "目标",
        "对比",
        "比较",
        "评估",
        "分析",
        "复盘",
        "核查",
        "检查风险",
        "风险摘要",
        "讨论",
        "持续",
        "维持",
        "贵不贵",
        "影响",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn is_pure_concept_question(question: &str) -> bool {
    let concept_pattern = [
        "什么是",
        "是什么意思",
        "有何区别",
        "有什么区别",
        "的区别",
        "怎么理解",
        "如何理解",
        "概念",
        "定义是什么",
        "怎么计算",
        "怎么算的",
        "计算公式是什么",
    ]
    .iter()
    .any(|term| question.contains(term));
    let action_decision = [
        "要不要",
        "该不该",
        "买",
        "卖",
        "加仓",
        "减仓",
        "止损",
        "换仓",
        "调仓",
        "还贷",
        "配置",
        "分配",
        "值得",
        "我应该",
        "我的",
        "组合",
        "持仓",
    ]
    .iter()
    .any(|term| question.contains(term));
    concept_pattern && !action_decision
}

fn is_pure_status_or_lookup_question(question: &str) -> bool {
    let lookup_pattern = [
        "够几个月",
        "是多少",
        "有多少",
        "占多少",
        "占比多少",
        "有哪些缺",
        "缺什么数据",
        "缺少哪些",
        "缺少汇率",
        "数据状态",
        "资料状态",
        "口径是什么",
        "统计口径",
        "查看当前",
        "列一下",
        "汇总一下",
        "总市值",
        "现金有多少",
        "含税吗",
        "算不算",
    ]
    .iter()
    .any(|term| question.contains(term));
    let action_decision = [
        "要不要",
        "该不该",
        "怎么补",
        "怎么配",
        "怎么分",
        "怎么调",
        "如何安排",
        "建议",
        "方案",
        "加仓",
        "减仓",
        "卖出",
        "买入",
        "止损",
        "换仓",
        "提前还",
        "值得",
        "持续",
    ]
    .iter()
    .any(|term| question.contains(term));
    lookup_pattern && !action_decision
}

pub fn is_general_finance_question(question: &str) -> bool {
    !has_dividend_topic(question)
        && (has_cashflow_or_debt_topic(question)
            || has_exit_or_rebalance_topic(question)
            || has_portfolio_allocation_topic(question))
}

pub fn subject_question<'a>(
    request: &'a AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> Option<&'a str> {
    let companies = companies(request, candidates, facts);
    if !explicit_market_symbols(&request.question).is_empty()
        || companies
            .iter()
            .any(|(name, symbol)| mentions_company(&request.question, name, symbol))
        || is_general_finance_question(&request.question)
        || is_pure_concept_question(&request.question)
        || is_pure_status_or_lookup_question(&request.question)
    {
        return Some(&request.question);
    }
    retrieval_questions(request)
        .into_iter()
        .skip(1)
        .find(|question| {
            !explicit_market_symbols(question).is_empty()
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

fn classify_domain(
    question: &str,
    subject: &str,
    selected_companies: &[String],
    watch_company: bool,
) -> InvestmentDomain {
    if is_pure_concept_question(question) || is_pure_status_or_lookup_question(question) {
        return InvestmentDomain::GeneralFinance;
    }
    // Macro/policy impact questions without a named company belong to macro_or_sector
    // even if they mention asset categories like "高股息" or "红利资产".
    if has_macro_or_sector_topic(question) && selected_companies.is_empty() && !watch_company {
        return InvestmentDomain::MacroOrSector;
    }
    if has_dividend_topic(question) || has_dividend_topic(subject) || watch_company {
        return InvestmentDomain::DividendDiscussion;
    }
    if has_options_topic(question) {
        return InvestmentDomain::OptionsOrHedging;
    }
    if has_exit_or_rebalance_topic(question) {
        return InvestmentDomain::ExitOrRebalance;
    }
    if has_cashflow_or_debt_topic(question) {
        return InvestmentDomain::CashflowAndDebt;
    }
    if has_macro_or_sector_topic(question) {
        return InvestmentDomain::MacroOrSector;
    }
    if has_portfolio_allocation_topic(question) {
        return InvestmentDomain::PortfolioAllocation;
    }
    if has_valuation_topic(question) || !selected_companies.is_empty() {
        return InvestmentDomain::EquityValuation;
    }
    InvestmentDomain::GeneralFinance
}

fn decide_execution_mode(
    request: &AnalysisRequest,
    domain: InvestmentDomain,
    selected_companies: &[String],
    has_authorized_evidence: bool,
) -> (ExecutionMode, String) {
    let question = request.question.trim();
    let multi_part = question.matches(['？', '?']).count() > 1
        || question.matches(['；', ';']).count() > 1
        || question.chars().count() >= 60;

    if is_pure_concept_question(question) && !multi_part {
        return (
            ExecutionMode::Fast,
            "属于概念/术语解释类轻量问题，启用精简快速模式".into(),
        );
    }
    if is_pure_status_or_lookup_question(question) && !multi_part {
        return (
            ExecutionMode::Fast,
            "属于资料状态或口径核查类轻量问题，启用精简快速模式".into(),
        );
    }
    if domain != InvestmentDomain::GeneralFinance {
        return (
            ExecutionMode::Council,
            format!(
                "识别到「{}」专业投研场景，启用完整投委会评议",
                domain.label()
            ),
        );
    }
    if !selected_companies.is_empty() {
        return (
            ExecutionMode::Council,
            "涉及具体投资标的评估，启用完整投委会评议".into(),
        );
    }
    if has_authorized_evidence {
        return (
            ExecutionMode::Council,
            "包含授权研究资料需交叉印证，启用完整投委会评议".into(),
        );
    }
    if multi_part || has_decision_intent(question) || request.previous_turn.is_some() {
        return (
            ExecutionMode::Council,
            "涉及投资判断或多轮讨论，启用完整投委会评议".into(),
        );
    }
    (
        ExecutionMode::Fast,
        "属于轻量一般查询且无复杂决策权衡，启用精简快速模式".into(),
    )
}

/// Both report generation and native search consume this frozen public task pack.
/// Only canonical securities codes and fixed domain metadata cross the search boundary.
pub fn build_discussion(
    request: &AnalysisRequest,
    candidates: &[ResearchEvidence],
    facts: &Value,
) -> Value {
    let subject = subject_question(request, candidates, facts).unwrap_or(&request.question);
    let mut selected = if is_pure_concept_question(&request.question)
        || is_pure_status_or_lookup_question(&request.question)
    {
        Vec::new()
    } else {
        explicit_market_symbols(subject)
    };
    if selected.is_empty()
        && !is_pure_concept_question(&request.question)
        && !is_pure_status_or_lookup_question(&request.question)
    {
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
    let domain = classify_domain(&request.question, subject, &selected, watch_company);
    let has_authorized_evidence =
        request.context_selection.include_evidence && !candidates.is_empty();
    let (execution_mode, route_reason) =
        decide_execution_mode(request, domain, &selected, has_authorized_evidence);
    let topics = domain.public_topics(&request.question);
    let checks = domain.research_checks();

    json!({
        "kind": domain.as_str(),
        "domainLabel": domain.label(),
        "executionMode": execution_mode,
        "routeReason": route_reason,
        "frameworkFocus": domain.framework_focus(),
        "publicResearch": {
            "domain": domain.as_str(),
            "domainLabel": domain.label(),
            "companies": selected.into_iter().map(|symbol| json!({"symbol":symbol})).collect::<Vec<_>>(),
            "topics": topics,
            "guidance": domain.search_guidance(),
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
            "two":{"name":"虚构红利乙","symbol":"000001.SZ","account":"PRIVATE_ACCOUNT"},
            "three":{"name":"虚构港股","symbol":"0700.HK","marketValue":"PRIVATE_HK"},
            "four":{"name":"虚构美股","symbol":"AAPL","marketValue":"PRIVATE_US"}
        }})
    }

    #[test]
    fn holding_horizon_and_stop_loss_routes_to_exit_or_rebalance_not_dividend() {
        let guide = build_discussion(
            &request("指数基金准备长期持有，要不要设止损？"),
            &[],
            &facts(),
        );
        assert_eq!(guide["kind"], "exit_or_rebalance");
        assert_eq!(guide["executionMode"], "council");
        assert_eq!(guide["researchChecks"].as_array().unwrap().len(), 4);
        let guide = build_discussion(&request("准备长期持有，分红能持续吗？"), &[], &facts());
        assert_eq!(guide["kind"], "dividend_discussion");
        assert_eq!(guide["executionMode"], "council");
        assert_eq!(guide["researchChecks"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn routes_all_specialized_investment_domains_and_lightweight_fast_mode() {
        for (question, expected_kind, expected_mode) in [
            (
                "0700.HK 现在估值贵不贵，护城河稳吗？",
                "equity_valuation",
                "council",
            ),
            ("$NVDA 最新财报现金流如何？", "equity_valuation", "council"),
            (
                "我的组合集中度太高，怎么调整资产配置控制回撤？",
                "portfolio_allocation",
                "council",
            ),
            (
                "每月结余8000，先补应急金还是提前还房贷？",
                "cashflow_and_debt",
                "council",
            ),
            (
                "持仓想卖 covered call 备兑期权可行吗？",
                "options_or_hedging",
                "council",
            ),
            (
                "美联储降息周期对高股息和美债资产有何影响？",
                "macro_or_sector",
                "council",
            ),
            (
                "什么是自由现金流，和净利润有什么区别？",
                "general_finance",
                "fast",
            ),
            ("我现在应急金够几个月？", "general_finance", "fast"),
            ("当前有哪些持仓缺少汇率？", "general_finance", "fast"),
        ] {
            let guide = build_discussion(&request(question), &[], &facts());
            assert_eq!(guide["kind"], expected_kind, "{question}");
            assert_eq!(guide["executionMode"], expected_mode, "{question}");
            assert!(
                !guide["domainLabel"].as_str().unwrap().is_empty(),
                "{question}"
            );
            assert!(
                !guide["frameworkFocus"].as_str().unwrap().is_empty(),
                "{question}"
            );
            assert!(
                !guide["routeReason"].as_str().unwrap().is_empty(),
                "{question}"
            );
        }
    }

    #[test]
    fn multi_market_symbol_recognition_supports_hk_etf_and_cashtags_without_false_positives() {
        assert_eq!(
            explicit_market_symbols("700.hk 和 00700.HK 估值"),
            vec!["0700.HK"]
        );
        assert_eq!(
            explicit_market_symbols("510300.SH 和 159915.SZ"),
            vec!["510300.SS", "159915.SZ"]
        );
        assert_eq!(
            explicit_market_symbols("看下 $AAPL 和 $BRK-B 的估值"),
            vec!["AAPL", "BRK-B"]
        );
        assert!(explicit_market_symbols("对比 $USD 和 $PE 以及 $ETF").is_empty());

        let guide = build_discussion(&request("AAPL 估值和盈利质量如何？"), &[], &facts());
        assert_eq!(guide["kind"], "equity_valuation");
        assert_eq!(
            guide["publicResearch"]["companies"],
            json!([{"symbol": "AAPL"}])
        );
        assert!(!guide["publicResearch"].to_string().contains("PRIVATE_US"));
    }

    #[test]
    fn company_matching_accepts_exchange_aliases_but_not_embedded_numeric_ids() {
        for text in ["600036", "600036.SH的分红", "600036.SS", "虚构公司好吗"] {
            assert!(mentions_company(text, "虚构公司", "600036.SS"));
        }
        assert!(mentions_company("700.hk 财报", "虚构港股", "0700.HK"));
        assert!(mentions_company("aapl 估值", "虚构美股", "AAPL"));
        assert!(!mentions_company("MAAPLE 公司", "虚构美股", "AAPL"));
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
    fn cashflow_finance_has_no_dividend_template_or_implicit_portfolio_topics() {
        let guide = build_discussion(&request("应急金怎么补？"), &[], &facts());
        assert_eq!(guide["kind"], "cashflow_and_debt");
        assert_eq!(guide["executionMode"], "council");
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
        assert_eq!(guide["kind"], "cashflow_and_debt");
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
        assert_eq!(hidden["kind"], "equity_valuation");
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
