use super::{
    AppError, AppResult, DecisionRuleCheck, GoalInput, HashSet, HoldingInput,
    HoldingValuationEvidence, InvestmentRule, InvestmentRuleInput, Local, NaiveDate,
    PortfolioEventInput, PortfolioEventType, ResearchEvidenceInput, SecurityPriceQuote,
    SystemReviewInput, Utc,
};

pub(super) fn validate_non_negative(values: &[f64]) -> AppResult<()> {
    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(AppError::Validation(
            "金额、比例和期限必须是有效的非负数".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_currency(value: &str) -> AppResult<()> {
    let currency = value.trim();
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(AppError::Validation(
            "币种必须使用三个英文字母，例如 CNY、USD 或 HKD".into(),
        ));
    }
    Ok(())
}

pub(super) fn normalized_fx_provenance(
    currency: &str,
    base_currency: &str,
    fx_rate_to_base: Option<f64>,
    source: &str,
    observed_on: &str,
    effective_on: &str,
) -> AppResult<(String, String)> {
    if currency.trim().eq_ignore_ascii_case(base_currency.trim()) {
        return Ok((String::new(), String::new()));
    }
    if fx_rate_to_base.is_none_or(|rate| !rate.is_finite() || rate <= 0.0 || rate > 1e9) {
        return Err(AppError::Validation("外币折算汇率必须是有效正数".into()));
    }
    let source = if source.trim().is_empty() {
        "user_declared"
    } else {
        source.trim()
    };
    if source.chars().count() > 120 {
        return Err(AppError::Validation("汇率来源不能超过 120 个字符".into()));
    }
    let effective_date = NaiveDate::parse_from_str(effective_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("估值或流水日期必须使用 YYYY-MM-DD".into()))?;
    let observed_on = if observed_on.trim().is_empty() {
        effective_on.trim()
    } else {
        observed_on.trim()
    };
    let observed_date = NaiveDate::parse_from_str(observed_on, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("汇率观察日期必须使用 YYYY-MM-DD".into()))?;
    if observed_date > effective_date {
        return Err(AppError::Validation(
            "汇率观察日期不能晚于对应的估值或流水日期".into(),
        ));
    }
    if observed_date > Local::now().date_naive() {
        return Err(AppError::Validation("汇率观察日期不能晚于今天".into()));
    }
    Ok((source.into(), observed_on.into()))
}

pub(super) fn validate_portfolio_event(
    input: &PortfolioEventInput,
    base_currency: &str,
) -> AppResult<()> {
    if !input.amount.is_finite() || input.amount <= 0.0 || input.amount > 1e15 {
        return Err(AppError::Validation(
            "流水金额必须是大于 0 的有效数字".into(),
        ));
    }
    let currency = input.currency.trim();
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(AppError::Validation("流水币种必须是 3 位字母代码".into()));
    }
    if !currency.eq_ignore_ascii_case(base_currency)
        && !input
            .fx_rate_to_base
            .is_some_and(|rate| rate.is_finite() && rate > 0.0 && rate <= 1e9)
    {
        return Err(AppError::Validation(
            "外币流水必须填写折算到当前基准币种的有效汇率".into(),
        ));
    }
    let occurred_on = NaiveDate::parse_from_str(input.occurred_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("流水日期必须使用 YYYY-MM-DD".into()))?;
    if occurred_on > Local::now().date_naive() {
        return Err(AppError::Validation("流水日期不能晚于今天".into()));
    }
    if input.note.trim().is_empty() {
        return Err(AppError::Validation(
            "流水说明为必填项，便于未来核对".into(),
        ));
    }
    let source = input.source.trim();
    let external_id = input.external_id.trim();
    if source.is_empty() && !external_id.is_empty() {
        return Err(AppError::Validation(
            "填写来源交易 ID 时必须同时填写流水来源".into(),
        ));
    }
    if matches!(
        input.event_type,
        PortfolioEventType::Buy
            | PortfolioEventType::Sell
            | PortfolioEventType::Dividend
            | PortfolioEventType::Interest
    ) && input.asset_name.trim().is_empty()
    {
        return Err(AppError::Validation(
            "买卖、分红或利息流水必须填写关联资产".into(),
        ));
    }
    if source.chars().count() > 120
        || external_id.chars().count() > 200
        || input.asset_name.chars().count() > 200
        || input.note.chars().count() > 2_000
    {
        return Err(AppError::Validation(
            "流水来源、交易 ID、资产名称或说明过长".into(),
        ));
    }
    normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.occurred_on,
    )?;
    Ok(())
}

pub(super) fn validate_holding(input: &HoldingInput, base_currency: &str) -> AppResult<()> {
    if input.name.trim().is_empty() || input.market_value <= 0.0 {
        return Err(AppError::Validation("资产名称和正数市值为必填项".into()));
    }
    validate_non_negative(&[input.market_value, input.cost_basis, input.target_pct])?;
    validate_percentage(input.target_pct, "目标权重")?;
    validate_currency(&input.currency)?;
    let valuation_date = NaiveDate::parse_from_str(input.valuation_date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("持仓估值日期必须使用 YYYY-MM-DD".into()))?;
    if valuation_date > Local::now().date_naive() {
        return Err(AppError::Validation("持仓估值日期不能晚于今天".into()));
    }
    if input.currency.eq_ignore_ascii_case(base_currency) {
        if input
            .fx_rate_to_base
            .is_some_and(|rate| !rate.is_finite() || rate <= 0.0)
        {
            return Err(AppError::Validation("汇率必须是有效正数".into()));
        }
    } else if input
        .fx_rate_to_base
        .is_none_or(|rate| !rate.is_finite() || rate <= 0.0)
    {
        return Err(AppError::Validation(format!(
            "{} 持仓必须填写折算到基准币种 {} 的汇率",
            input.currency.trim().to_ascii_uppercase(),
            base_currency.trim().to_ascii_uppercase()
        )));
    }
    normalized_fx_provenance(
        &input.currency,
        base_currency,
        input.fx_rate_to_base,
        &input.fx_rate_source,
        &input.fx_rate_observed_on,
        &input.valuation_date,
    )?;
    Ok(())
}

pub(super) fn validate_holding_valuation_evidence(
    quantity: f64,
    quote: &SecurityPriceQuote,
) -> AppResult<()> {
    if !quantity.is_finite() || quantity <= 0.0 || quantity > 1e15 {
        return Err(AppError::Validation("持仓数量必须是有效正数".into()));
    }
    if !quote.close.is_finite() || quote.close <= 0.0 || quote.close > 1e15 {
        return Err(AppError::Validation("证券单位价格必须是有效正数".into()));
    }
    let requested_on = NaiveDate::parse_from_str(quote.requested_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("证券价格请求日期无效".into()))?;
    let observed_on = NaiveDate::parse_from_str(quote.observed_on.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("证券价格观察日期无效".into()))?;
    if requested_on > Local::now().date_naive()
        || observed_on > requested_on
        || (requested_on - observed_on).num_days() != quote.staleness_days
        || !(0..=10).contains(&quote.staleness_days)
    {
        return Err(AppError::Validation("证券价格日期或回退天数不一致".into()));
    }
    validate_currency(&quote.currency)?;
    let source_url = reqwest::Url::parse(&quote.source_url)
        .map_err(|_| AppError::Validation("证券价格来源地址无效".into()))?;
    let source_symbol = source_url
        .query_pairs()
        .find(|(key, _)| key == "symbol")
        .map(|(_, value)| value.into_owned());
    let source_is_canonical = source_url.scheme() == "https"
        && source_url.host_str() == Some("api.twelvedata.com")
        && source_url.path() == "/time_series"
        && source_url.username().is_empty()
        && source_url.password().is_none()
        && source_symbol.as_deref() == Some(quote.symbol.as_str())
        && !source_url
            .query_pairs()
            .any(|(key, _)| key.eq_ignore_ascii_case("apikey"));
    if quote.symbol.trim().is_empty()
        || quote.symbol.chars().count() > 80
        || quote.provider_code != "twelve_data_raw_close"
        || quote.provider_name != "Twelve Data"
        || quote.price_basis != "unadjusted_daily_close"
        || !source_is_canonical
        || quote.methodology_url != "https://twelvedata.com/docs/market-data/time-series"
        || quote.disclaimer.trim().is_empty()
        || quote.exchange.chars().count() > 120
        || quote.mic_code.chars().count() > 32
        || quote.instrument_type.chars().count() > 120
    {
        return Err(AppError::Validation(
            "证券价格来源或元数据不是受支持的规范形式".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_stored_holding_valuation(
    evidence: &HoldingValuationEvidence,
) -> AppResult<()> {
    let quote = SecurityPriceQuote {
        symbol: evidence.symbol.clone(),
        currency: evidence.currency.clone(),
        close: evidence.unit_price,
        requested_on: evidence.requested_on.clone(),
        observed_on: evidence.observed_on.clone(),
        staleness_days: evidence.staleness_days,
        provider_code: evidence.provider_code.clone(),
        provider_name: evidence.provider_name.clone(),
        exchange: evidence.exchange.clone(),
        mic_code: evidence.mic_code.clone(),
        instrument_type: evidence.instrument_type.clone(),
        price_basis: evidence.price_basis.clone(),
        source_url: evidence.source_url.clone(),
        methodology_url: evidence.methodology_url.clone(),
        disclaimer: evidence.disclaimer.clone(),
    };
    validate_holding_valuation_evidence(evidence.quantity, &quote)?;
    let expected = evidence.quantity * evidence.unit_price;
    if (evidence.market_value - expected).abs() > 0.005_f64.max(expected.abs() * 1e-10)
        || chrono::DateTime::parse_from_rfc3339(&evidence.captured_at).is_err()
    {
        return Err(AppError::Validation("持仓估值证据内容无效".into()));
    }
    Ok(())
}

pub(super) fn validate_and_canonicalize_rule_checks(
    checks: &[DecisionRuleCheck],
    active_rules: &[InvestmentRule],
) -> AppResult<Vec<DecisionRuleCheck>> {
    if checks.len() != active_rules.len() {
        return Err(AppError::Validation(
            "请逐条确认当前所有有效投资规则后再冻结决策".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut canonical = Vec::with_capacity(active_rules.len());
    for rule in active_rules {
        let check = checks
            .iter()
            .find(|check| check.rule_id == rule.id)
            .ok_or_else(|| AppError::Validation("缺少当前有效规则的确认结果".into()))?;
        if !seen.insert(check.rule_id.as_str()) {
            return Err(AppError::Validation("投资规则确认不能重复".into()));
        }
        if check.rule_revision != rule.revision {
            return Err(AppError::Validation(
                "投资规则已修订，请刷新并按最新版本重新确认".into(),
            ));
        }
        if !matches!(check.status.as_str(), "遵守" | "偏离" | "不适用") {
            return Err(AppError::Validation(
                "每条有效规则必须明确标记为遵守、偏离或不适用".into(),
            ));
        }
        if check.status == "偏离" && check.note.trim().is_empty() {
            return Err(AppError::Validation("偏离规则时必须记录原因".into()));
        }
        if check.note.chars().count() > 1_000 {
            return Err(AppError::Validation(
                "规则确认备注不能超过 1000 个字符".into(),
            ));
        }
        canonical.push(DecisionRuleCheck {
            rule_id: rule.id.clone(),
            rule_revision: rule.revision,
            category: rule.category.clone(),
            statement: rule.statement.clone(),
            trigger: rule.trigger.clone(),
            status: check.status.clone(),
            note: check.note.trim().into(),
        });
    }
    Ok(canonical)
}

fn validate_percentage(value: f64, label: &str) -> AppResult<()> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(AppError::Validation(format!("{label}必须在 0—100% 之间")));
    }
    Ok(())
}

pub(super) fn validate_goal(input: &GoalInput) -> AppResult<()> {
    if input.name.trim().is_empty()
        || input.target_amount <= 0.0
        || input.target_date.trim().is_empty()
    {
        return Err(AppError::Validation("目标名称、金额和日期为必填项".into()));
    }
    validate_non_negative(&[
        input.target_amount,
        input.current_amount,
        input.monthly_contribution,
    ])?;
    chrono::NaiveDate::parse_from_str(&input.target_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("目标日期格式无效".into()))?;
    Ok(())
}

pub(super) fn validate_investment_rule(input: &InvestmentRuleInput) -> AppResult<()> {
    if input.category.trim().is_empty()
        || input.statement.trim().is_empty()
        || input.trigger.trim().is_empty()
        || input.rationale.trim().is_empty()
    {
        return Err(AppError::Validation(
            "规则类别、内容、触发条件和依据均为必填项".into(),
        ));
    }
    if !matches!(
        input.category.as_str(),
        "资产配置" | "风险" | "研究" | "仓位" | "行为" | "复盘"
    ) {
        return Err(AppError::Validation("未知的投资规则类别".into()));
    }
    Ok(())
}

pub(super) fn validate_system_review(input: &SystemReviewInput) -> AppResult<()> {
    if input.period_label.trim().is_empty()
        || input.process_summary.trim().is_empty()
        || input.lessons.trim().is_empty()
        || input.next_actions.trim().is_empty()
        || input.next_review_date.trim().is_empty()
    {
        return Err(AppError::Validation(
            "复盘周期、过程事实、经验、下一步和下次复盘日均为必填项".into(),
        ));
    }
    if !(1..=5).contains(&input.adherence_score) {
        return Err(AppError::Validation("纪律执行评分必须在 1—5 之间".into()));
    }
    chrono::NaiveDate::parse_from_str(&input.next_review_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("下次复盘日期格式无效".into()))?;
    Ok(())
}

pub(super) fn validate_research_evidence(input: &ResearchEvidenceInput) -> AppResult<()> {
    if input.asset_name.trim().is_empty()
        || input.title.trim().is_empty()
        || input.publisher.trim().is_empty()
        || input.source_url.trim().is_empty()
        || input.as_of_date.trim().is_empty()
        || input.claim.trim().is_empty()
    {
        return Err(AppError::Validation(
            "资产、标题、发布方、来源链接、资料日期和证据摘要均为必填项".into(),
        ));
    }
    if !matches!(
        input.source_tier.as_str(),
        "一手来源" | "二手研究" | "媒体报道"
    ) {
        return Err(AppError::Validation("未知的来源层级".into()));
    }
    if !matches!(
        input.evidence_type.as_str(),
        "公司披露" | "监管文件" | "数据发布" | "研究报告" | "新闻" | "其他"
    ) {
        return Err(AppError::Validation("未知的证据类型".into()));
    }
    if !matches!(input.stance.as_str(), "支持" | "反驳" | "背景") {
        return Err(AppError::Validation("未知的证据立场".into()));
    }
    let as_of_date = chrono::NaiveDate::parse_from_str(&input.as_of_date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("资料日期格式无效".into()))?;
    if as_of_date > Utc::now().date_naive() {
        return Err(AppError::Validation("资料日期不能晚于今天".into()));
    }
    let source = reqwest::Url::parse(input.source_url.trim())
        .map_err(|_| AppError::Validation("来源链接格式无效".into()))?;
    if source.scheme() != "https"
        || source.host_str().is_none()
        || !source.username().is_empty()
        || source.password().is_some()
    {
        return Err(AppError::Validation(
            "研究证据必须使用有效的 HTTPS 来源链接".into(),
        ));
    }
    for (label, value, limit) in [
        ("资产或主题", input.asset_name.as_str(), 120),
        ("资料标题", input.title.as_str(), 300),
        ("发布方", input.publisher.as_str(), 200),
        ("来源链接", input.source_url.as_str(), 2_048),
        ("证据摘要", input.claim.as_str(), 4_000),
        ("限制与待核实项", input.notes.as_str(), 4_000),
    ] {
        if value.chars().count() > limit {
            return Err(AppError::Validation(format!(
                "{label}不能超过 {limit} 个字符"
            )));
        }
    }
    Ok(())
}
