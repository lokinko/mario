use super::{
    AppError, AppResult, HoldingInput, InvestmentRuleInput, Local, NaiveDate,
    ResearchEvidenceInput, SecurityPriceQuote, SystemReviewInput, Utc,
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
        .map_err(|_| AppError::Validation("估值日期必须使用 YYYY-MM-DD".into()))?;
    let observed_on = if observed_on.trim().is_empty() {
        effective_on.trim()
    } else {
        observed_on.trim()
    };
    let observed_date = NaiveDate::parse_from_str(observed_on, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("汇率观察日期必须使用 YYYY-MM-DD".into()))?;
    if observed_date > effective_date {
        return Err(AppError::Validation(
            "汇率观察日期不能晚于对应的估值日期".into(),
        ));
    }
    if observed_date
        > Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive()
    {
        return Err(AppError::Validation("汇率观察日期不能晚于今天".into()));
    }
    Ok((source.into(), observed_on.into()))
}

pub(super) fn normalized_holding_fx_provenance(
    currency: &str,
    base_currency: &str,
    rate: Option<f64>,
    source: &str,
    observed: &str,
    effective: &str,
) -> AppResult<(String, String)> {
    if !currency.eq_ignore_ascii_case(base_currency)
        && rate.is_none()
        && source.trim().is_empty()
        && observed.trim().is_empty()
    {
        return Ok((String::new(), String::new()));
    }
    normalized_fx_provenance(
        currency,
        base_currency,
        rate,
        source,
        observed,
        if matches!(source, "central_daily" | "central_manual") {
            observed
        } else {
            effective
        },
    )
}

pub(super) fn validate_holding(input: &HoldingInput, base_currency: &str) -> AppResult<()> {
    if input.name.trim().is_empty() || input.market_value < 0.0 || input.market_value > 1e15 {
        return Err(AppError::Validation(
            "资产名称和有效的非负市值为必填项".into(),
        ));
    }
    validate_non_negative(&[input.market_value, input.cost_basis, input.target_pct])?;
    validate_percentage(input.target_pct, "目标权重")?;
    validate_currency(&input.currency)?;
    let valuation_date = NaiveDate::parse_from_str(input.valuation_date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::Validation("持仓估值日期必须使用 YYYY-MM-DD".into()))?;
    if valuation_date > Local::now().date_naive() {
        return Err(AppError::Validation("持仓估值日期不能晚于今天".into()));
    }
    if input.currency.eq_ignore_ascii_case(base_currency)
        && input
            .fx_rate_to_base
            .is_some_and(|rate| !rate.is_finite() || rate <= 0.0)
    {
        return Err(AppError::Validation("汇率必须是有效正数".into()));
    }
    normalized_holding_fx_provenance(
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
    if let Some(exact) = &quote.exact_close {
        use rust_decimal::prelude::ToPrimitive;
        let price = rust_decimal::Decimal::from_str_exact(exact)
            .map_err(|_| AppError::Validation("行情原始价格无效".into()))?;
        if price <= rust_decimal::Decimal::ZERO
            || price > rust_decimal::Decimal::from(1_000_000_000_000_000_i64)
            || !price.to_f64().is_some_and(|value| {
                (value - quote.close).abs() <= 4.0 * f64::EPSILON * value.abs().max(1.0)
            })
        {
            return Err(AppError::Validation("行情价格与精确原值不一致".into()));
        }
    }
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
    if requested_on
        > Utc::now()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive()
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
    let twelve_is_canonical = source_url.scheme() == "https"
        && source_url.host_str() == Some("api.twelvedata.com")
        && source_url.path() == "/time_series"
        && source_url.username().is_empty()
        && source_url.password().is_none()
        && source_symbol.as_deref() == Some(quote.symbol.as_str())
        && !source_url
            .query_pairs()
            .any(|(key, _)| key.eq_ignore_ascii_case("apikey"));
    let yahoo_is_canonical = source_url.scheme() == "https"
        && matches!(
            source_url.host_str(),
            Some("query1.finance.yahoo.com" | "query2.finance.yahoo.com")
        )
        && source_url.username().is_empty()
        && source_url.password().is_none()
        && crate::models::market::canonical_symbol(&quote.symbol)
            .is_ok_and(|symbol| source_url.path() == format!("/v8/finance/chart/{symbol}"))
        && source_url
            .query_pairs()
            .any(|(key, value)| key == "interval" && value == "1d");
    let supported = (twelve_is_canonical
        && quote.provider_code == "twelve_data_raw_close"
        && quote.provider_name == "Twelve Data"
        && quote.price_basis == "unadjusted_daily_close"
        && quote.methodology_url == "https://twelvedata.com/docs/market-data/time-series")
        || (yahoo_is_canonical
            && quote.provider_code == "yahoo_daily_close"
            && quote.provider_name == "Yahoo Finance"
            && quote.price_basis == "split_adjusted_daily_close"
            && quote.methodology_url == crate::models::market::METHODOLOGY_URL);
    if quote.symbol.trim().is_empty()
        || quote.symbol.chars().count() > 80
        || !supported
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

fn validate_percentage(value: f64, label: &str) -> AppResult<()> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(AppError::Validation(format!("{label}必须在 0—100% 之间")));
    }
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
    if input.content.trim().is_empty() {
        return Err(AppError::Validation("复盘内容不能为空".into()));
    }
    Ok(())
}

pub(super) fn validate_research_evidence(input: &ResearchEvidenceInput) -> AppResult<()> {
    crate::attachments::validate(&input.attachments)?;
    if input.content.trim().is_empty() && input.attachments.is_empty() {
        return Err(AppError::Validation("请填写原始资料内容".into()));
    }
    if input.content.chars().count() > 20_000 {
        return Err(AppError::Validation("原始资料不能超过 20000 个字符".into()));
    }
    Ok(())
}
