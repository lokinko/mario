use super::*;

pub(crate) fn fixture() -> Value {
    let stocks = json!([{"code":"600900","name":"测试电力","price":12,"shares":200,"costPrice":"8.04","transactions":[
        {"type":"buy","qty":200,"price":11,"ts":1767398400000_i64},
        {"type":"buy","qty":100,"price":10,"ts":1767225600000_i64},
        {"type":"dividend","qty":300,"price":0.9800000000000001_f64,"gross":1,"ts":1767311999999_i64},
        {"type":"sell","qty":100,"price":12,"ts":1767571200000_i64},
        {"type":"dividendTax","qty":100,"price":55.300000000000004_f64,"ts":1767571200000_i64}
    ]},{"code":"000333","name":"测试家电","price":25,"shares":0,"costPrice":"","transactions":[
        {"type":"buy","qty":100,"price":20,"ts":1767225600000_i64},
        {"type":"sell","qty":100,"price":25,"ts":1767312000000_i64}
    ]},{"code":"600036","name":"仅观察测试","shares":0,"transactions":[]}]);
    json!({"version":"1.0","exportedAt":"2026-01-06T01:00:00Z","data":{
        "watchlist":stocks,"accounts":[{"id":"default","name":"测试账户","watchlist":stocks,
        "feeConfig":{"enabled":false},"cashTrackingEnabled":false,"cashBalance":{"CNY":0}}],
        "discoveryStaticEdits":{"600900":{"price":999}},"discoveryManualStocks":[]}})
}

#[test]
fn external_import_replays_once_keeps_closed_positions_and_roundtrips() {
    let db = Database::test();
    let old_rules = db.watch_rules().unwrap();
    let mut rules = old_rules;
    rules.min_yield_bps = 600;
    db.save_watch_rules(rules, old_rules).unwrap();
    db.import_account_data(&fixture()).unwrap();
    assert_eq!(db.watch_rules().unwrap(), rules);
    let s = db.snapshot().unwrap();
    assert_eq!(db.watch_entries().unwrap().len(), 3);
    assert_eq!(s.holdings.len(), 2);
    assert_eq!(s.holding_trades.len(), 6);
    assert!(s.cash_balances.is_empty());
    let h = s.holdings.iter().find(|h| h.symbol == "600900.SS").unwrap();
    let summary = &s.holding_trade_summaries[&h.id];
    assert_eq!(summary.quantity, "200");
    assert_eq!(summary.total_taxes, "61.3");
    assert_eq!(summary.dividend_income, "294");
    assert_ne!(summary.average_cost, "8.04");
    let dividend = s
        .holding_trades
        .iter()
        .find(|t| t.kind == TradeKind::Dividend)
        .unwrap();
    assert_eq!(dividend.gross_amount, "300");
    assert_eq!(dividend.taxes, "6");
    assert_eq!(dividend.occurred_on, "2026-01-02");
    assert_eq!(dividend.quantity, "0");
    assert!(dividend.note.contains("1767311999999"));
    let copy = Database::test();
    let saved = db.export_sync_data().unwrap();
    copy.import_account_data(&serde_json::to_value(&saved).unwrap())
        .unwrap();
    assert_eq!(
        saved.content_hash().unwrap(),
        copy.export_sync_data().unwrap().content_hash().unwrap()
    );
    assert!(Database::test().watch_entries().unwrap().is_empty());
}

#[test]
fn external_import_selects_one_account_and_preserves_cash_without_resettling() {
    let mut data = fixture();
    let mut second = data["data"]["accounts"][0].clone();
    second["id"] = json!("second");
    second["name"] = json!("第二账户");
    second["cashTrackingEnabled"] = json!(true);
    second["cashBalance"] = json!({"CNY":"123.45","USD":0});
    data["data"]["accounts"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert!(convert(&data).is_err());
    data["selectedAccountId"] = json!("missing");
    assert!(convert(&data).is_err());
    data["selectedAccountId"] = json!("second");
    let db = Database::test();
    db.import_account_data(&data).unwrap();
    let s = db.snapshot().unwrap();
    assert!(s.holding_trades.iter().all(|t| t.account == "第二账户"));
    let cash = s
        .cash_balances
        .iter()
        .find(|c| c.currency == "CNY")
        .unwrap();
    assert_eq!(cash.amount, "123.45");
    assert!(cash.needs_reconciliation);
    assert_eq!(db.watch_entries().unwrap().len(), 3);
}

#[test]
fn external_import_rejects_bad_financial_data_without_changing_existing_account() {
    let db = Database::test();
    db.import_account_data(&fixture()).unwrap();
    let before = db.export_sync_data().unwrap().content_hash().unwrap();
    let cases = [
        ("/version", json!("3.0")),
        ("/exportedAt", json!("invalid")),
        ("/data/accounts/0/watchlist/0/shares", json!(201)),
        ("/data/accounts/0/watchlist/0/transactions/0/qty", json!(10)),
        (
            "/data/accounts/0/watchlist/0/transactions/0/type",
            json!("unknown"),
        ),
        (
            "/data/accounts/0/watchlist/0/transactions/0/price",
            json!("1.123456789"),
        ),
        (
            "/data/accounts/0/watchlist/0/transactions/4/ts",
            json!(1767571200001_i64),
        ),
        ("/data/accounts/0/watchlist/0/code", json!("AAPL")),
        ("/data/accounts/0/watchlist/0/price", json!(0)),
        ("/data/accounts/0/watchlist/0/price", Value::Null),
        (
            "/data/accounts/0/watchlist/0/transactions/0/ts",
            json!(4102444800000_i64),
        ),
    ];
    for (path, value) in cases {
        let mut data = fixture();
        *data.pointer_mut(path).unwrap() = value;
        assert!(db.import_account_data(&data).is_err(), "{path}");
        assert_eq!(
            before,
            db.export_sync_data().unwrap().content_hash().unwrap(),
            "{path}"
        );
    }
    let mut duplicate = fixture();
    let sale = duplicate["data"]["accounts"][0]["watchlist"][0]["transactions"][3].clone();
    duplicate["data"]["accounts"][0]["watchlist"][0]["transactions"]
        .as_array_mut()
        .unwrap()
        .push(sale);
    assert!(convert(&duplicate)
        .unwrap_err()
        .to_string()
        .contains("唯一匹配"));
    crate::db::fail_inserts(
        &db.conn().unwrap(),
        "holding_amounts",
        "synthetic import failure",
    );
    assert!(db.import_account_data(&fixture()).is_err());
    assert_eq!(
        before,
        db.export_sync_data().unwrap().content_hash().unwrap()
    );
}

#[test]
fn external_import_accepts_top_level_watchlist_and_manual_holdings() {
    let db = Database::test();
    let data = json!({"version":"1.0","exportedAt":"2026-01-06T01:00:00Z","data":{
        "watchlist":[{"code":"600900","name":"手工证券","shares":100,"costPrice":"10.12","price":11}]}});
    db.import_account_data(&data).unwrap();
    let s = db.snapshot().unwrap();
    assert!(s.holding_trades.is_empty());
    assert_eq!(s.holdings[0].cost_basis, 1012.0);
    assert_eq!(s.holdings[0].market_value, 1100.0);
    assert_eq!(
        s.accounting.unwrap().holdings[&s.holdings[0].id]
            .amounts
            .price_status,
        "unconfirmed"
    );
}

#[test]
fn external_v2_import_keeps_fund_ledgers_and_ignores_estimated_fees() {
    let mut data = fixture();
    data["version"] = json!("2.2");
    let account = &mut data["data"]["accounts"][0];
    account["feeConfig"] = json!({"enabled":true,"commissionRate":0.00025});
    let watchlist = account["watchlist"].as_array_mut().unwrap();
    watchlist.push(json!({"code":"588000","name":"测试科创ETF","price":1.375,"shares":7500,"isETF":false,"transactions":[
        {"type":"buy","qty":7500,"price":1.4,"ts":1767225600000_i64}]}));
    watchlist.push(
        json!({"code":"159915","name":"测试创业板ETF","price":2.59,"shares":0,"transactions":[]}),
    );
    watchlist.push(json!({"code":"QQQ","name":"未持有境外","shares":0,"transactions":[]}));
    let db = Database::test();
    db.import_account_data(&data).unwrap();
    let s = db.snapshot().unwrap();
    let symbols: Vec<_> = db
        .watch_entries()
        .unwrap()
        .into_iter()
        .map(|w| w.input.symbol)
        .collect();
    assert_eq!(symbols.len(), 3);
    assert!(!symbols
        .iter()
        .any(|s| s.starts_with("588000") || s.starts_with("159915")));
    let fund = s.holdings.iter().find(|h| h.symbol == "588000.SS").unwrap();
    assert_eq!(fund.asset_class, "基金");
    assert_eq!(s.holding_trade_summaries[&fund.id].quantity, "7500");
    assert!(s.holdings.iter().all(|h| !h.symbol.starts_with("159915")));
    assert!(s
        .holding_trades
        .iter()
        .all(|t| t.fees == "0" && t.note.contains("估算费用未导入")));
}
