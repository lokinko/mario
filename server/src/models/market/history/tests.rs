use super::*;

#[test]
fn overseas_eastmoney_sources_bind_market_currency_and_the_exact_symbol() {
    let day = "2026-11-06".parse().unwrap();
    for (symbol, exchange, currency) in [
        ("0700.HK", "HKG", "HKD"),
        ("QQQ", "NASDAQ", "USD"),
        ("PDD", "NASDAQ", "USD"),
        ("VOO", "AMEX", "USD"),
        ("KO", "NYSE", "USD"),
    ] {
        let security = super::super::eastmoney::security(symbol, exchange).unwrap();
        let mut history = fixture(symbol, day);
        history.provider_code = "eastmoney".into();
        history.provider_name = "东方财富".into();
        history.exchange = exchange.into();
        history.currency = currency.into();
        history.raw_price_basis = "unadjusted_daily_close".into();
        history.raw_source_url = format!(
            "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid={}&klt=101&fqt=0",
            security.secid()
        );
        history.adjusted_source_url = history.raw_source_url.replace("fqt=0", "fqt=1");
        history.methodology_url = super::super::eastmoney_methodology_url(symbol);
        history.validate().unwrap();
        for case in ["market", "symbol", "currency", "duplicate", "host"] {
            let mut wrong = history.clone();
            match case {
                "market" => {
                    wrong.raw_source_url =
                        wrong.raw_source_url.replace(&security.secid(), "106.PDD")
                }
                "symbol" => wrong.symbol = "AAPL".into(),
                "currency" => wrong.currency = "JPY".into(),
                "duplicate" => wrong.raw_source_url.push_str("&secid=105.QQQ"),
                _ => {
                    wrong.raw_source_url = wrong
                        .raw_source_url
                        .replace("push2his.eastmoney.com", "attacker.test")
                }
            }
            assert!(wrong.validate().is_err(), "{symbol}: {case}");
        }
    }
}

pub(crate) fn fixture(symbol: &str, through: NaiveDate) -> PriceHistory {
    let (data, end) = crate::models::watch::tests::market_fixture();
    let source = format!("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d");
    PriceHistory {
        symbol: symbol.into(),
        currency: if symbol.ends_with(".SS") || symbol.ends_with(".SZ") {
            "CNY"
        } else {
            "USD"
        }
        .into(),
        window_start: history_start(through),
        completed_through: through,
        adjustment_anchor: through,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        provider_code: "yahoo".into(),
        provider_name: "Yahoo Finance".into(),
        raw_price_basis: "split_adjusted_daily_close".into(),
        raw_source_url: source.clone(),
        adjusted_source_url: source,
        action_source_urls: vec![],
        methodology_url: crate::models::market::METHODOLOGY_URL.into(),
        exchange: "TEST".into(),
        mic_code: String::new(),
        instrument_type: "EQUITY".into(),
        bars: data
            .bars
            .into_iter()
            .map(|b| DailyPrice {
                day: b.day + (through - end),
                raw_close: Some("100".into()),
                adjusted_close: Some("100".into()),
            })
            .collect(),
        splits: vec![],
    }
    .normalize()
    .unwrap()
}
#[test]
fn revision_keeps_forward_prices_raw_values_dates_and_sources_together() {
    let day = "2026-11-06".parse().unwrap();
    let mut h = fixture("600941.SS", day);
    h.bars[0].raw_close = Some("80".into());
    h.bars[0].adjusted_close = Some("40".into());
    h.bars.last_mut().unwrap().raw_close = Some("103.49".into());
    h.bars.last_mut().unwrap().adjusted_close = Some("50".into());
    let h = h.normalize().unwrap();
    assert_eq!(h.bars[0].adjusted_close.as_deref(), Some("82.792"));
    assert_eq!(h.bars[0].raw_close.as_deref(), Some("80"));
    let q = h.quote(day, day, "600941.SS").unwrap();
    assert_eq!(q.exact_close.as_deref(), Some("103.49"));
    assert_eq!(q.observed_on, "2026-11-06");
    assert_eq!(q.price_basis, "forward_adjusted_daily_close");
    let mut invalid = h.clone();
    invalid.adjustment_anchor -= chrono::Duration::days(1);
    assert!(invalid.validate().is_err());
    let mut invalid = h.clone();
    invalid.bars[0].day = h.window_start - chrono::Duration::days(1);
    assert!(invalid.validate().is_err());
    let mut invalid = h.clone();
    invalid.raw_source_url = invalid
        .raw_source_url
        .replace("query1.finance.yahoo.com", "attacker.test");
    assert!(invalid.validate().is_err());
    let mut invalid = h.clone();
    invalid.bars[0].day = h.bars[1].day;
    assert!(invalid.validate().is_err());
}
#[test]
fn five_year_window_uses_calendar_years_and_no_price_is_filled_for_missing_adjustments() {
    assert_eq!(
        history_start("2024-02-29".parse().unwrap()).to_string(),
        "2019-02-28"
    );
    let day = "2026-11-06".parse().unwrap();
    let mut h = fixture("600941.SS", day);
    h.bars.last_mut().unwrap().adjusted_close = None;
    h = h.normalize().unwrap();
    assert!(h.quote(day, day, "600941.SS").unwrap().observed_on < day.to_string());
    assert!(h.bars.last().unwrap().adjusted_close.is_none());
}

#[test]
fn content_revision_tracks_price_corrections_without_poll_timestamp_churn() {
    let mut h = fixture("600941.SS", "2026-11-06".parse().unwrap());
    let original = h.revision().unwrap();
    h.fetched_at = "2026-11-08T08:00:00+08:00".into();
    h.action_source_urls
        .push("https://example.test/volatile-request-timestamp".into());
    assert_eq!(h.revision().unwrap(), original);
    h.bars[0].adjusted_close = Some("12.34".into());
    assert_ne!(h.revision().unwrap(), original);
}
