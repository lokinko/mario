use super::*;

pub(super) fn add_legacy_table_shapes(dataset: &mut SyncDataset) {
    for spec in sync_specs_for_version(dataset.schema_version).unwrap() {
        if !dataset.tables.iter().any(|table| table.name == spec.name) {
            dataset.tables.push(SyncTable {
                name: spec.name.into(),
                columns: spec.columns.iter().map(|column| (*column).into()).collect(),
                rows: Vec::new(),
            });
        }
    }
}

#[test]
fn upgrade_and_legacy_restore_remove_memory_metadata_and_preserve_reports() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    db.conn().unwrap().execute_batch(
        "CREATE TABLE memory_preferences(memory_id TEXT PRIMARY KEY, preference TEXT, note TEXT, updated_at TEXT);
         INSERT INTO memory_preferences VALUES ('a1','pinned','旧备注','2026-10-17T00:00:00Z');
         INSERT INTO settings(key,value) VALUES ('analysis.settings','{\"useMemory\":true,\"webSearch\":false}');
         INSERT INTO analyses(id,question,answer,audit,trace,created_at) VALUES (
           'a1','旧问题','完整历史回答',
           '{\"memoryItemsUsed\":1,\"reviewedMemoryItemsUsed\":0,\"conflictingMemoryItemsUsed\":0,\"contextGroups\":[\"相关历史记忆\",\"财务档案\"],\"model\":\"old\"}',
           '{\"version\":\"old\",\"memoryItems\":[{\"id\":\"a0\",\"preferenceNote\":\"旧备注\"}],\"evidenceCatalog\":[{\"id\":\"memory-a0\",\"claim\":\"原始引用\"}]}',
           '2026-10-17T00:00:00Z');"
    ).unwrap();
    let assert_clean = |db: &Database| {
        let conn = db.conn().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pg_tables WHERE schemaname=current_schema() AND tablename='memory_preferences'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let (answer, audit, trace): (String, String, String) = conn
            .query_row(
                "SELECT answer,audit,trace FROM analyses WHERE id='a1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(answer, "完整历史回答");
        let audit: serde_json::Value = serde_json::from_str(&audit).unwrap();
        assert_eq!(audit["contextGroups"], serde_json::json!(["财务档案"]));
        assert_eq!(audit["model"], "old");
        for key in [
            "memoryItemsUsed",
            "reviewedMemoryItemsUsed",
            "conflictingMemoryItemsUsed",
        ] {
            assert!(audit.get(key).is_none());
        }
        let trace: serde_json::Value = serde_json::from_str(&trace).unwrap();
        assert!(trace.get("memoryItems").is_none());
        assert_eq!(trace["evidenceCatalog"][0]["claim"], "原始引用");
        drop(conn);
        let preset: serde_json::Value =
            serde_json::from_str(&db.setting("analysis.settings").unwrap().unwrap()).unwrap();
        assert!(preset.get("useMemory").is_none());
        assert_eq!(preset["webSearch"], false);
        assert!(!db
            .export_sync_data()
            .unwrap()
            .tables
            .iter()
            .any(|table| table.name == "memory_preferences"));
    };
    let mut legacy = db.export_sync_data().unwrap();
    legacy.schema_version = 21;
    add_legacy_table_shapes(&mut legacy);
    legacy
        .tables
        .iter_mut()
        .find(|table| table.name == "memory_preferences")
        .unwrap()
        .rows
        .push(vec![
            SyncValue::Text("a1".into()),
            SyncValue::Text("pinned".into()),
            SyncValue::Text("旧备注".into()),
            SyncValue::Text("2026-10-17T00:00:00Z".into()),
        ]);
    drop(db);
    for _ in 0..2 {
        let db = account.reopen_from_old_release();
        assert_clean(&db);
        db.import_sync_data(&legacy).unwrap();
        assert_clean(&db);
    }
}

#[test]
fn upgrades_remove_retired_tables_without_changing_active_records() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    let holding = db.add_holding(&detailed_holding_input()).unwrap();
    let before = db.export_sync_data().unwrap();
    db.conn()
        .unwrap()
        .execute_batch(
            "CREATE TABLE goals (id TEXT PRIMARY KEY, name TEXT);
         INSERT INTO goals VALUES ('old-goal','旧目标');
         CREATE TABLE portfolio_events (id TEXT PRIMARY KEY,
           reversal_of_event_id TEXT REFERENCES portfolio_events(id));
         INSERT INTO portfolio_events VALUES ('original',NULL),('reversal','original');",
        )
        .unwrap();
    drop(db);
    for _ in 0..2 {
        let reopened = account.reopen_from_old_release();
        assert_eq!(reopened.export_sync_data().unwrap().tables, before.tables);
        assert_eq!(
            reopened.snapshot().unwrap().holdings[0].id,
            holding.holdings[0].id
        );
        let count: i64 = reopened
            .conn()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pg_tables WHERE schemaname=current_schema() AND tablename IN ('goals','portfolio_events')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let snapshot = serde_json::to_value(reopened.snapshot().unwrap()).unwrap();
        assert!(snapshot.get("goals").is_none());
        assert!(snapshot["plan"].get("goalProjections").is_none());
        assert!(snapshot["plan"].get("committedMonthly").is_none());
    }
}

#[test]
fn all_legacy_backup_versions_restore_active_data_and_skip_retired_records() {
    let source = Database::test();
    source
        .save_profile(&FinancialProfile {
            monthly_income: 4567.0,
            ..Default::default()
        })
        .unwrap();
    let current = source.export_sync_data().unwrap();
    for version in 1..SYNC_DATASET_SCHEMA_VERSION {
        let specs = sync_specs_for_version(version).unwrap();
        let tables = specs
            .iter()
            .map(|spec| {
                let mut table = SyncTable {
                    name: spec.name.into(),
                    columns: spec.columns.iter().map(|c| (*c).into()).collect(),
                    rows: Vec::new(),
                };
                if let Some(active) = current.tables.iter().find(|t| t.name == spec.name) {
                    table.rows = active
                        .rows
                        .iter()
                        .map(|row| {
                            spec.columns
                                .iter()
                                .map(|column| {
                                    row[active.columns.iter().position(|c| c == column).unwrap()]
                                        .clone()
                                })
                                .collect()
                        })
                        .collect();
                } else if matches!(spec.name, "goals" | "portfolio_events") {
                    // Retired payload values have no domain interpretation in the new service.
                    table.rows.push(
                        spec.columns
                            .iter()
                            .map(|_| SyncValue::Text("retired".into()))
                            .collect(),
                    );
                }
                table
            })
            .collect();
        let legacy = SyncDataset {
            schema_version: version,
            exported_at: current.exported_at.clone(),
            tables,
        };
        let target = Database::test();
        target
            .import_sync_data(&legacy)
            .unwrap_or_else(|error| panic!("v{version}: {error}"));
        assert_eq!(target.snapshot().unwrap().profile.monthly_income, 4567.0);
        assert!(target
            .export_sync_data()
            .unwrap()
            .tables
            .iter()
            .all(|t| !matches!(t.name.as_str(), "goals" | "portfolio_events")));
    }
}

#[test]
fn current_backup_round_trip_preserves_data_and_excludes_private_settings() {
    let source = Database::test();
    source
        .save_profile(&FinancialProfile {
            monthly_income: 12345.0,
            ..Default::default()
        })
        .unwrap();
    source.add_holding(&detailed_holding_input()).unwrap();
    source
        .add_investment_rule(&InvestmentRuleInput {
            category: "仓位".into(),
            statement: "核对集中度".into(),
            trigger: "加仓前".into(),
            rationale: "限制风险".into(),
            active: true,
            source_review_id: None,
        })
        .unwrap();
    source
        .add_research_evidence(&ResearchEvidenceInput {
            content: "待核对资料\n来源：https://example.com/source\n原文".into(),
            ..Default::default()
        })
        .unwrap();
    source.conn().unwrap().execute_batch("INSERT INTO analyses(id,question,answer,created_at) VALUES ('analysis','问题','完整回答','2026-01-01T00:00:00Z');").unwrap();
    source
        .set_setting("private-setting", "private-value")
        .unwrap();
    source
        .set_setting("model.api-key", "private-secret")
        .unwrap();
    let exported = source.export_sync_data().unwrap();
    let serialized = serde_json::to_string(&exported).unwrap();
    assert!(!serialized.contains("private-value"));
    assert!(!serialized.contains("private-secret"));
    let target = Database::test();
    target.import_sync_data(&exported).unwrap();
    assert_eq!(target.export_sync_data().unwrap().tables, exported.tables);
    assert_eq!(target.analysis("analysis").unwrap().answer, "完整回答");
}

// Construct an authentic old backup from today's export for compatibility tests.
pub(super) fn add_retired_tables(dataset: &mut SyncDataset) {
    dataset.tables.retain(|t| {
        ![
            "financial_operations",
            "financial_journal",
            "trading_quarantine",
            "cash_state",
            "trade_settlements",
            "corporate_actions",
            "holding_amounts",
        ]
        .contains(&t.name.as_str())
    });
    let specs = sync_specs_for_version(15).unwrap();
    for (index, spec) in specs.iter().enumerate() {
        if matches!(
            spec.name,
            "decisions"
                | "decision_reviews"
                | "portfolio_checkins"
                | "goals"
                | "portfolio_events"
                | "memory_preferences"
        ) && !dataset.tables.iter().any(|table| table.name == spec.name)
        {
            dataset.tables.insert(
                index,
                SyncTable {
                    name: spec.name.into(),
                    columns: spec.columns.iter().map(|column| (*column).into()).collect(),
                    rows: Vec::new(),
                },
            );
        }
    }
}

#[test]
fn retired_records_stay_out_of_exports_and_legacy_imports_preserve_active_data() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    db.save_profile(&FinancialProfile {
        monthly_income: 12345.0,
        ..Default::default()
    })
    .unwrap();
    db.conn().unwrap().execute_batch(
        "CREATE TABLE decisions(id TEXT PRIMARY KEY, payload TEXT);
         CREATE TABLE decision_reviews(decision_id TEXT PRIMARY KEY, payload TEXT);
         INSERT INTO decisions VALUES ('retired', '{}');
         INSERT INTO analyses(id,question,answer,created_at) VALUES ('active','历史问题','完整原文','2026-10-12T00:00:00Z');
         CREATE TABLE memory_preferences(memory_id TEXT PRIMARY KEY, preference TEXT, note TEXT, updated_at TEXT);
         INSERT INTO memory_preferences VALUES ('active','pinned','保留分析','2026-10-12T00:00:00Z');
         INSERT INTO memory_preferences VALUES ('retired','hidden','旧偏好','2026-10-12T00:00:00Z');"
    ).unwrap();
    drop(db);
    let reopened = account.open();
    let exported = reopened.export_sync_data().unwrap();
    assert_eq!(exported.schema_version, SYNC_DATASET_SCHEMA_VERSION);
    assert!(!exported.tables.iter().any(|table| matches!(
        table.name.as_str(),
        "decisions"
            | "decision_reviews"
            | "portfolio_checkins"
            | "goals"
            | "portfolio_events"
            | "memory_preferences"
    )));
    let mut legacy = exported;
    legacy.schema_version = 15;
    add_retired_tables(&mut legacy);
    legacy
        .tables
        .iter_mut()
        .find(|table| table.name == "decisions")
        .unwrap()
        .rows
        .push(vec![
            SyncValue::Text("retired".into()),
            SyncValue::Text("旧记录".into()),
            SyncValue::Text("{}".into()),
            SyncValue::Text("2026-10-12".into()),
            SyncValue::Text("2026-10-12T00:00:00Z".into()),
        ]);
    legacy
        .tables
        .iter_mut()
        .find(|table| table.name == "memory_preferences")
        .unwrap()
        .rows
        .push(vec![
            SyncValue::Text("retired".into()),
            SyncValue::Text("hidden".into()),
            SyncValue::Text("旧偏好".into()),
            SyncValue::Text("2026-10-12T00:00:00Z".into()),
        ]);
    let restored = Database::test();
    restored.import_sync_data(&legacy).unwrap();
    assert_eq!(restored.snapshot().unwrap().profile.monthly_income, 12345.0);
    assert_eq!(restored.analysis("active").unwrap().answer, "完整原文");
    let retired_tables: i64 = restored.conn().unwrap().query_row(
        "SELECT COUNT(*) FROM pg_tables WHERE schemaname=current_schema() AND tablename IN ('decisions','decision_reviews','memory_preferences')", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(retired_tables, 0);
    restored.export_sync_data().unwrap().validate().unwrap();
}

#[test]
fn independent_text_reviews_preserve_content_through_restart_and_import() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    for content in ["", " \n\t "] {
        let input: SystemReviewInput =
            serde_json::from_value(serde_json::json!({"content": content})).unwrap();
        assert!(matches!(
            db.save_system_review(&input),
            Err(AppError::Validation(_))
        ));
    }
    assert!(
        db.review_reminder_summary(NaiveDate::from_ymd_opt(2026, 10, 12).unwrap())
            .unwrap()
            .periodic_review_due
    );
    let content = " 我的观点：继续观察。\n\n顾虑：资料不足。\n思考：<原文> **保留**。 ";
    let input: SystemReviewInput =
        serde_json::from_value(serde_json::json!({"content": content})).unwrap();
    let review = db.save_system_review(&input).unwrap();
    assert_eq!(review.content.as_deref(), Some(content));
    assert!(review.next_review_date.is_empty());
    assert_eq!(review.adherence_score, None);
    assert!(
        !db.review_reminder_summary(NaiveDate::from_ymd_opt(2026, 10, 12).unwrap())
            .unwrap()
            .periodic_review_due
    );
    drop(db);
    let reopened = account.open();
    assert_eq!(
        reopened.system_reviews().unwrap()[0].content.as_deref(),
        Some(content)
    );
    let target = Database::test();
    target
        .import_sync_data(&reopened.export_sync_data().unwrap())
        .unwrap();
    assert_eq!(
        target.system_reviews().unwrap()[0].content.as_deref(),
        Some(content)
    );
}

fn verified_quote() -> SecurityPriceQuote {
    SecurityPriceQuote {
        exact_close: None,
        splits: Vec::new(),
        symbol: "AAPL".into(),
        currency: "USD".into(),
        close: 101.25,
        requested_on: "2026-09-05".into(),
        observed_on: "2026-09-04".into(),
        staleness_days: 1,
        provider_code: "twelve_data_raw_close".into(),
        provider_name: "Twelve Data".into(),
        exchange: "NASDAQ".into(),
        mic_code: "XNAS".into(),
        instrument_type: "Common Stock".into(),
        price_basis: "unadjusted_daily_close".into(),
        source_url: "https://api.twelvedata.com/time_series?symbol=AAPL&interval=1day".into(),
        methodology_url: "https://twelvedata.com/docs/market-data/time-series".into(),
        disclaimer: "测试用的明确限制".into(),
    }
}

#[test]
fn persists_profile_and_holding() {
    let db = Database::test();
    let profile = FinancialProfile {
        monthly_expense: 10_000.0,
        emergency_fund: 60_000.0,
        ..Default::default()
    };
    db.save_profile(&profile).unwrap();
    db.add_holding(&HoldingInput {
        exact_values: None,
        cash_amount: None,
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
        cost_price: None,
        quantity: None,
        current_price: None,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-01-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    let snapshot = db.snapshot().unwrap();
    assert_eq!(snapshot.holdings.len(), 1);
    assert_eq!(snapshot.emergency_months, 6.0);
}

#[test]
fn freezes_verified_holding_valuation_and_invalidates_it_on_manual_change() {
    let db = Database::test();
    db.save_profile(&FinancialProfile {
        base_currency: "USD".into(),
        ..Default::default()
    })
    .unwrap();
    let created = db
        .add_holding(&HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "AAPL".into(),
            name: "Apple".into(),
            asset_class: "股票".into(),
            market_value: 900.0,
            cost_basis: 800.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 100.0,
            currency: "USD".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-09-05".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    let holding_id = created.holdings[0].id.clone();
    let valued = db
        .apply_verified_holding_valuation(&holding_id, 10.0, &verified_quote())
        .unwrap();
    assert_eq!(valued.holdings[0].market_value, 1_012.5);
    assert_eq!(
        db.daily_history(&crate::models::daily::HistoryQuery::default())
            .unwrap()
            .records[0]
            .total_assets,
        Some(1_012.5)
    );
    assert_eq!(valued.holding_valuations.len(), 1);
    assert_eq!(valued.holding_valuations[0].quantity, 10.0);
    assert_eq!(valued.holding_valuations[0].observed_on, "2026-09-04");

    let unchanged = HoldingInput {
        exact_values: None,
        cash_amount: None,
        symbol: "AAPL".into(),
        name: "Apple Inc.".into(),
        asset_class: "股票".into(),
        market_value: 1_012.5,
        cost_basis: 810.0,
        cost_price: None,
        quantity: Some(10.0),
        current_price: Some(101.25),
        target_pct: 100.0,
        currency: "USD".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-09-05".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    };
    assert_eq!(
        db.update_holding(&holding_id, &unchanged)
            .unwrap()
            .holding_valuations
            .len(),
        1
    );
    let changed = HoldingInput {
        exact_values: None,
        cash_amount: None,
        current_price: Some(102.0),
        ..unchanged
    };
    assert!(db
        .update_holding(&holding_id, &changed)
        .unwrap()
        .holding_valuations
        .is_empty());
}

#[test]
fn rejects_verified_price_when_provider_currency_differs_from_holding() {
    let db = Database::test();
    let created = db
        .add_holding(&HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "AAPL".into(),
            name: "错误币种".into(),
            asset_class: "股票".into(),
            market_value: 1_000.0,
            cost_basis: 900.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-09-05".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    assert!(matches!(
        db.apply_verified_holding_valuation(&created.holdings[0].id, 10.0, &verified_quote()),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn trade_managed_codes_can_be_corrected_cleared_and_filled_without_changing_trades() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    db.save_profile(&FinancialProfile {
        base_currency: "USD".into(),
        ..Default::default()
    })
    .unwrap();
    let trade: crate::models::HoldingTradeInput = serde_json::from_value(serde_json::json!({
        "expectedCashBaseline":"00000000-0000-0000-0000-000000000001","requestId": Uuid::new_v4().to_string(),
        "newHolding": {
            "symbol": "AAPL", "name": "Apple", "assetClass": "股票",
            "currency": "USD"
        },
        "kind": "buy", "occurredOn": "2026-09-05",
        "sequence": 1, "quantity": "10", "price": "100", "grossAmount": "1000", "fees": "2", "taxes": "0", "account": "券商", "settlementMode":"settle"
    }))
    .unwrap();
    super::cash::fund(&db, "USD");
    let created = db.save_holding_trade(&trade).unwrap();
    let id = &created.holdings[0].id;
    let valued = db
        .apply_verified_holding_valuation(id, 10.0, &verified_quote())
        .unwrap();
    assert_eq!(valued.holding_valuations.len(), 1);
    let mut edit: HoldingInput =
        serde_json::from_value(serde_json::to_value(&valued.holdings[0]).unwrap()).unwrap();
    for symbol in [" NEW ", "", " AAPL "] {
        edit.symbol = symbol.into();
        let updated = db.update_holding(id, &edit).unwrap();
        assert_eq!(updated.holdings[0].symbol, symbol.trim());
        assert_eq!(updated.holdings[0].quantity, valued.holdings[0].quantity);
        assert_eq!(
            updated.holdings[0].cost_price,
            valued.holdings[0].cost_price
        );
        assert_eq!(
            updated.holdings[0].cost_basis,
            valued.holdings[0].cost_basis
        );
        assert_eq!(
            updated.holdings[0].market_value,
            valued.holdings[0].market_value
        );
        assert_eq!(updated.holding_trades, valued.holding_trades);
        assert!(updated.holding_valuations.is_empty());
        if symbol.trim() != "AAPL" {
            assert!(db
                .apply_verified_holding_valuation(id, 10.0, &verified_quote())
                .is_err());
            assert_eq!(db.snapshot().unwrap().holdings[0].symbol, symbol.trim());
        }
    }
    assert_eq!(
        db.apply_verified_holding_valuation(id, 10.0, &verified_quote())
            .unwrap()
            .holding_valuations
            .len(),
        1
    );
    drop(db);
    let reopened = account.open().snapshot().unwrap();
    assert_eq!(reopened.holdings[0].symbol, "AAPL");
    assert_eq!(reopened.holding_trades, valued.holding_trades);
}

#[test]
fn v16_backups_skip_retired_portfolio_snapshots_and_keep_active_data() {
    let db = Database::test();
    db.save_profile(&FinancialProfile {
        monthly_income: 4321.0,
        ..Default::default()
    })
    .unwrap();
    let mut legacy = db.export_sync_data().unwrap();
    legacy.schema_version = 16;
    add_retired_tables(&mut legacy);
    legacy.tables.retain(|table| {
        !matches!(
            table.name.as_str(),
            "decisions" | "decision_reviews" | "portfolio_checkins"
        )
    });
    let spec = sync_specs_for_version(16)
        .unwrap()
        .into_iter()
        .find(|spec| spec.name == "portfolio_checkins")
        .unwrap();
    legacy.tables.push(SyncTable {
        name: spec.name.into(),
        columns: spec.columns.iter().map(|column| (*column).into()).collect(),
        rows: vec![vec![
            SyncValue::Text("retired-snapshot".into()),
            SyncValue::Text("{\"totalValue\":999999}".into()),
            SyncValue::Text("2026-09-01T00:00:00Z".into()),
        ]],
    });
    let target = Database::test();
    target.import_sync_data(&legacy).unwrap();
    assert_eq!(target.snapshot().unwrap().profile.monthly_income, 4321.0);
    let exported = target.export_sync_data().unwrap();
    assert_eq!(exported.schema_version, SYNC_DATASET_SCHEMA_VERSION);
    assert!(exported
        .tables
        .iter()
        .all(|table| table.name != "portfolio_checkins"));
    assert!(!target
        .conn()
        .unwrap()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pg_tables WHERE schemaname=current_schema() AND tablename='portfolio_checkins')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
}

#[test]
fn sync_snapshot_rejects_unknown_or_incomplete_schema() {
    let db = Database::test();
    let mut dataset = db.export_sync_data().unwrap();
    dataset.tables.pop();
    assert!(matches!(dataset.validate(), Err(AppError::Validation(_))));
}

#[test]
fn updates_and_deletes_holding() {
    let db = Database::test();
    let input = HoldingInput {
        exact_values: None,
        cash_amount: None,
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
        cost_price: None,
        quantity: None,
        current_price: None,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-01-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    };
    let created = db.add_holding(&input).unwrap();
    let id = &created.holdings[0].id;
    let mut updated = input;
    updated.market_value = 120_000.0;
    let snapshot = db.update_holding(id, &updated).unwrap();
    assert_eq!(snapshot.holdings[0].market_value, 120_000.0);
    assert!(db
        .delete_holding(id, &Default::default())
        .unwrap()
        .holdings
        .is_empty());
}

#[test]
fn foreign_currency_is_normalized_and_base_changes_invalidate_old_rates() {
    let db = Database::test();
    let mut profile = FinancialProfile {
        base_currency: "CNY".into(),
        ..Default::default()
    };
    db.save_profile(&profile).unwrap();

    let incomplete = db
        .add_holding(&HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 70.0,
            currency: "USD".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    assert!(!incomplete.valuation_status.comparable);
    db.delete_holding(&incomplete.holdings[0].id, &Default::default())
        .unwrap();
    let usd_snapshot = db
        .add_holding(&HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 70.0,
            currency: "USD".into(),
            fx_rate_to_base: Some(7.0),
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    assert_eq!(usd_snapshot.holdings[0].fx_rate_source, "user_declared");
    assert_eq!(usd_snapshot.holdings[0].fx_rate_observed_on, "2026-08-31");
    let usd_id = usd_snapshot.holdings[0].id.clone();
    let snapshot = db
        .add_holding(&HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "CASH".into(),
            name: "人民币现金".into(),
            asset_class: "现金".into(),
            market_value: 300.0,
            cost_basis: 300.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 30.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    let cny_id = snapshot
        .holdings
        .iter()
        .find(|holding| holding.currency == "CNY")
        .unwrap()
        .id
        .clone();
    assert_eq!(snapshot.total_value, 1_000.0);
    assert_eq!(snapshot.concentration_pct, 70.0);
    assert!(snapshot.valuation_status.comparable);
    assert_eq!(
        snapshot.valuation_status.aligned_valuation_date.as_deref(),
        Some("2026-08-31")
    );

    db.update_holding(
        &usd_id,
        &HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 70.0,
            currency: "USD".into(),
            fx_rate_to_base: Some(7.0),
            valuation_date: "2026-09-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();

    profile.base_currency = "USD".into();
    let changed = db.save_profile(&profile).unwrap();
    assert!(!changed.valuation_status.comparable);
    assert_eq!(changed.total_value, 0.0);
    assert_eq!(
        changed.valuation_status.missing_fx_holdings,
        vec!["CNY 存款"]
    );
    let usd_after_base_change = changed
        .holdings
        .iter()
        .find(|holding| holding.id == usd_id)
        .unwrap();
    assert_eq!(usd_after_base_change.fx_rate_to_base, None);
    assert!(usd_after_base_change.fx_rate_source.is_empty());
    assert!(usd_after_base_change.fx_rate_observed_on.is_empty());
    let normalized = db
        .update_holding(
            &cny_id,
            &HoldingInput {
                exact_values: None,
                cash_amount: None,
                symbol: "CASH".into(),
                name: "人民币现金".into(),
                asset_class: "现金".into(),
                market_value: 300.0,
                cost_basis: 300.0,
                cost_price: None,
                quantity: None,
                current_price: None,
                target_pct: 30.0,
                currency: "CNY".into(),
                fx_rate_to_base: Some(0.14),
                valuation_date: "2026-09-01".into(),
                fx_rate_source: String::new(),
                fx_rate_observed_on: String::new(),
            },
        )
        .unwrap();
    assert_eq!(normalized.total_value, 142.0);
}

#[test]
fn stores_analysis_transparency_audit() {
    let db = Database::test();
    let result = AnalysisResult {
        id: "analysis-1".into(),
        answer: "先控制风险".into(),
        stages: vec!["本地风险检查".into()],
        transparency: crate::models::AnalysisTransparency {
            provider: "mock".into(),
            model: "mock-model".into(),
            context_groups: vec!["本次问题".into(), "规则型风险检查".into()],
            payload_bytes: 512,
            context_revision: "ctx-test".into(),
            evidence_items_used: 0,
            citations_required: false,
            model_calls: 3,
            total_latency_ms: 1200,
            input_tokens: Some(400),
            output_tokens: Some(100),
            structured_output_validated: true,
            output_repairs: 0,
            external_data_used: false,
            api_key_sent: false,
        },
        workflow_trace: crate::models::AnalysisWorkflowTrace {
            version: "investment-workflow-v4".into(),
            structured_report: Some(crate::models::StructuredAnalysis {
                verdict: "先控制风险".into(),
                facts: vec![crate::models::AnalysisClaim {
                    statement: "风险预算已超出".into(),
                    basis: "user_data".into(),
                    evidence_ids: Vec::new(),
                    source_refs: Vec::new(),
                }],
                inferences: Vec::new(),
                unknowns: vec!["未来现金流".into()],
                options: vec![crate::models::AnalysisOption {
                    name: "分批调整".into(),
                    suitable_when: "风险超限".into(),
                    tradeoffs: vec!["可能错过上涨".into()],
                    risks: vec!["执行偏差".into()],
                }],
                actions: vec![crate::models::AnalysisAction {
                    supporting_fact_indices: vec![0],
                    evidence_limits: vec!["旧记录的限制".into()],
                    action: "核对目标权重".into(),
                    rationale: "避免错误交易".into(),
                    reversible: true,
                    review_trigger: "一周后".into(),
                }],
                review_triggers: vec!["风险回到预算内".into()],
            }),
            output_validation: Some(crate::models::OutputValidationTrace {
                status: "valid".into(),
                attempts: 1,
                errors: Vec::new(),
            }),
            ..crate::models::AnalysisWorkflowTrace::default()
        },
        created_at: "2026-01-01T00:00:00Z".into(),
        disclaimer: "测试".into(),
    };
    db.save_analysis(&result, "如何控制风险？").unwrap();
    let history = db.analysis_history().unwrap();
    let audit = history[0].transparency.as_ref().unwrap();
    assert_eq!(audit.model_calls, 3);
    assert!(!audit.api_key_sent);
    assert_eq!(
        history[0].workflow_version.as_deref(),
        Some("investment-workflow-v4")
    );
    assert_eq!(history[0].verdict.as_deref(), Some("先控制风险"));
    assert_eq!(history[0].grounding_status.as_deref(), Some("unavailable"));
    let history_json = serde_json::to_value(&history[0]).unwrap();
    assert!(history_json.get("workflowTrace").is_none());
    assert_eq!(history_json["workflowVersion"], "investment-workflow-v4");
    assert!(audit.structured_output_validated);
    let stored = db.analysis("analysis-1").unwrap();
    assert_eq!(stored.question, "如何控制风险？");
    assert_eq!(stored.answer, "先控制风险");
    assert_eq!(
        stored.workflow_trace.unwrap().version,
        "investment-workflow-v4"
    );
    let mut reviewed_result = result.clone();
    reviewed_result.id = "analysis-checked".into();
    reviewed_result.workflow_trace.advice_grounding =
        vec![crate::models::information::AdviceGrounding {
            action_index: 0,
            status: "contradicted".into(),
            reason: "与原始资料相反".into(),
            checks: vec![],
        }];
    db.save_analysis(&reviewed_result, "核对结论").unwrap();
    let checked_history = db
        .analysis_history()
        .unwrap()
        .into_iter()
        .find(|item| item.id == "analysis-checked")
        .unwrap();
    assert_eq!(
        checked_history.grounding_status.as_deref(),
        Some("contradicted")
    );
    assert_eq!(
        db.analysis("analysis-checked")
            .unwrap()
            .workflow_trace
            .unwrap()
            .advice_grounding[0]
            .reason,
        "与原始资料相反"
    );
    assert!(matches!(db.analysis("missing"), Err(AppError::NotFound(_))));
}

#[test]
fn reads_analysis_audit_created_before_evidence_fields_existed() {
    let db = Database::test();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id, question, answer, audit, created_at)
             VALUES ('legacy','旧问题','旧回答',$1,'2026-01-01')",
            [r#"{"provider":"mock","model":"old","contextGroups":[],"payloadBytes":1,"contextRevision":"ctx-old","externalDataUsed":false,"apiKeySent":false}"#],
        )
        .unwrap();
    let audit = db.analysis_history().unwrap()[0]
        .transparency
        .clone()
        .unwrap();
    assert_eq!(audit.evidence_items_used, 0);
    assert!(!audit.citations_required);
    assert_eq!(audit.model_calls, 0);
    assert!(!audit.structured_output_validated);
    assert_eq!(audit.output_repairs, 0);
    assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
}

#[test]
fn adds_workflow_trace_column_to_an_existing_analysis_database() {
    let account = crate::db::TestAccount::new();
    let connection = account.connection();
    connection
        .execute_batch(
            "CREATE TABLE analyses (
               id TEXT PRIMARY KEY,
               question TEXT NOT NULL,
               answer TEXT NOT NULL,
               audit TEXT,
               created_at TEXT NOT NULL
             );
             INSERT INTO analyses (id,question,answer,audit,created_at)
             VALUES ('legacy','旧问题','旧回答',NULL,'2026-01-01');",
        )
        .unwrap();
    drop(connection);

    let db = account.open();
    let trace_columns: i64 = db
        .conn()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='analyses' AND column_name='trace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(trace_columns, 1);
    assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
}

#[test]
fn reads_legacy_workflow_trace() {
    let db = Database::test();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id,question,answer,audit,trace,created_at)
             VALUES ('old-trace','旧分析','旧回答',NULL,$1,'2026-01-01')",
            [r#"{"version":"investment-workflow-v2","researchPlan":"旧计划","alternatives":[],"critique":null,"calls":[]}"#],
        )
        .unwrap();

    assert_eq!(
        db.analysis_history().unwrap()[0]
            .workflow_version
            .as_deref(),
        Some("investment-workflow-v2")
    );
    let trace = db.analysis("old-trace").unwrap().workflow_trace.unwrap();
    assert_eq!(trace.version, "investment-workflow-v2");
    assert!(trace.structured_report.is_none());
    assert!(trace.output_validation.is_none());
    assert!(trace.evidence_catalog.is_empty());
}

#[test]
fn versions_investment_rules_without_overwriting_history() {
    let db = Database::test();
    let original = InvestmentRuleInput {
        category: "仓位".into(),
        statement: "单一主动仓位不超过 10%".into(),
        trigger: "任何新建或加仓决定".into(),
        rationale: "限制单一判断错误的永久损失".into(),
        active: true,
        source_review_id: None,
    };
    let created = db.add_investment_rule(&original).unwrap();
    let revised = db
        .update_investment_rule(
            &created.id,
            &InvestmentRuleInput {
                statement: "单一主动仓位不超过 8%".into(),
                ..original
            },
        )
        .unwrap();

    assert_eq!(revised.revision, 2);
    let history = db.investment_rule_history(&created.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].statement, "单一主动仓位不超过 8%");
    assert_eq!(history[1].statement, "单一主动仓位不超过 10%");
}

#[test]
fn system_review_freezes_portfolio_and_method_snapshot() {
    let db = Database::test();
    db.add_holding(&HoldingInput {
        exact_values: None,
        cash_amount: None,
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
        cost_price: None,
        quantity: None,
        current_price: None,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-01-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    db.add_investment_rule(&InvestmentRuleInput {
        category: "行为".into(),
        statement: "重大决定等待 24 小时".into(),
        trigger: "出现计划外交易冲动".into(),
        rationale: "降低情绪交易".into(),
        active: true,
        source_review_id: None,
    })
    .unwrap();
    let review = db
        .save_system_review(&SystemReviewInput {
            content: "按计划定投，没有追涨。继续降低无效交易，下季度检查再平衡。".into(),
        })
        .unwrap();
    let holding_id = db.snapshot().unwrap().holdings[0].id.clone();
    db.update_holding(
        &holding_id,
        &HoldingInput {
            exact_values: None,
            cash_amount: None,
            symbol: "IDX".into(),
            name: "指数".into(),
            asset_class: "基金".into(),
            market_value: 200_000.0,
            cost_basis: 90_000.0,
            cost_price: None,
            quantity: None,
            current_price: None,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-02-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();

    let stored = db.system_reviews().unwrap().remove(0);
    assert_eq!(stored.id, review.id);
    assert_eq!(stored.snapshot.portfolio_value, 100_000.0);
    assert_eq!(stored.snapshot.active_rules, 1);
}

#[test]
fn stores_immutable_research_evidence_and_allows_archiving() {
    let db = Database::test();
    let created = db
        .add_research_evidence(&ResearchEvidenceInput {
            content: "基金年度报告：报告披露了费用与跟踪误差\n复核费用变化".into(),
            ..Default::default()
        })
        .unwrap();
    let archived = db.set_research_evidence_status(&created.id, false).unwrap();

    assert!(!archived.active);
    assert_eq!(archived.claim, created.claim);
    assert_eq!(db.research_evidence().unwrap().len(), 1);
}

#[test]
fn reads_frozen_structured_reviews_after_retiring_the_old_write_format() {
    let db = Database::test();
    db.conn().unwrap().execute(
        "INSERT INTO system_reviews(id,payload,snapshot,next_review_date,created_at) VALUES ($1,$2,$3,$4,$5)",
        params!["old-review", r#"{"periodLabel":"2026 Q3","adherenceScore":4,"processSummary":"原始过程","lessons":"原始经验","nextReviewDate":"2026-12-31"}"#,
            r#"{"portfolioValue":100,"emergencyMonths":3,"concentrationPct":0,"riskStatus":"ready","highRiskFindings":0,"activeRules":0}"#,
            "2026-12-31", "2026-09-30T00:00:00Z"]
    ).unwrap();
    let stored = db.system_reviews().unwrap().remove(0);
    assert_eq!(stored.content, None);
    assert_eq!(stored.period_label, "2026 Q3");
    assert_eq!(stored.process_summary, "原始过程");
    assert_eq!(stored.lessons, "原始经验");
    assert_eq!(stored.adherence_score, Some(4));
    assert_eq!(stored.snapshot.portfolio_value, 100.0);
}

#[test]
fn rejects_retired_structured_write_formats() {
    assert!(
        serde_json::from_value::<ResearchEvidenceInput>(serde_json::json!({
            "assetName":"指数", "claim":"旧表单"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SystemReviewInput>(serde_json::json!({
            "periodLabel":"旧周期", "processSummary":"旧复盘"
        }))
        .is_err()
    );
}

fn detailed_holding_input() -> HoldingInput {
    HoldingInput {
        exact_values: None,
        cash_amount: None,
        symbol: "IDX".into(),
        name: "手动持仓".into(),
        asset_class: "基金".into(),
        market_value: 999.0,
        cost_basis: 999.0,
        cost_price: Some(8.0),
        quantity: Some(100.5),
        current_price: Some(10.0),
        target_pct: 0.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-01-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    }
}

#[test]
fn different_recent_valuation_dates_allow_actions_but_stale_values_still_block() {
    let db = Database::test();
    let today = NaiveDate::parse_from_str(&crate::models::market::today(), "%Y-%m-%d").unwrap();
    let mut investment = detailed_holding_input();
    investment.valuation_date = today.to_string();
    investment.target_pct = 100.0;
    let created = db.add_holding(&investment).unwrap();
    let investment_id = created.holdings[0].id.clone();
    db.add_holding(&HoldingInput {
        symbol: String::new(),
        name: "人民币存款".into(),
        asset_class: "现金".into(),
        market_value: 10_000.0,
        cost_basis: 0.0,
        cost_price: None,
        quantity: None,
        current_price: None,
        valuation_date: (today - chrono::Duration::days(1)).to_string(),
        ..detailed_holding_input()
    })
    .unwrap();

    let snapshot = db.snapshot().unwrap();
    assert_eq!(snapshot.valuation_status.valuation_dates.len(), 2);
    assert!(snapshot.valuation_status.aligned_valuation_date.is_none());
    assert!(snapshot.valuation_status.decision_ready);
    assert!(snapshot.valuation_status.warnings.is_empty());
    assert!(!snapshot.plan.rebalancing.is_empty());
    assert!(!snapshot
        .findings
        .iter()
        .any(|finding| finding.title.contains("估值日期")));

    investment.valuation_date = (today - chrono::Duration::days(8)).to_string();
    let stale = db.update_holding(&investment_id, &investment).unwrap();
    assert!(!stale.valuation_status.decision_ready);
    assert!(stale.plan.rebalancing.is_empty());
    assert!(stale
        .valuation_status
        .warnings
        .iter()
        .any(|warning| warning.contains("已超过 7 天")));
}

#[test]
fn manual_holding_details_survive_restart_history_and_sync() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    let created = db.add_holding(&detailed_holding_input()).unwrap();
    let holding = &created.holdings[0];
    assert_eq!(holding.market_value, 1005.0);
    assert_eq!(holding.cost_basis, 804.0);
    let id = holding.id.clone();
    let mut input = detailed_holding_input();
    input.current_price = Some(12.0);
    let updated = db.update_holding(&id, &input).unwrap();
    assert_eq!(updated.holdings[0].market_value, 1206.0);
    let history = db
        .daily_history(&crate::models::daily::HistoryQuery::default())
        .unwrap();
    assert_eq!(
        history.records[0].assets[0].holding.current_price,
        Some(12.0)
    );
    let export = db.export_sync_data().unwrap();
    let copy = Database::test();
    copy.import_sync_data(&export).unwrap();
    assert_eq!(copy.snapshot().unwrap().holdings[0].quantity, Some(100.5));
    drop(db);
    let db = account.open();
    let persisted = &db.snapshot().unwrap().holdings[0];
    assert_eq!(persisted.cost_price, Some(8.0));
    assert_eq!(persisted.current_price, Some(12.0));
    let mut old_export = export.clone();
    add_retired_tables(&mut old_export);
    old_export.tables.retain(|t| t.name != "holding_trades");
    old_export.schema_version = 13;
    let table = old_export
        .tables
        .iter_mut()
        .find(|t| t.name == "holdings")
        .unwrap();
    table.columns.truncate(14);
    for row in &mut table.rows {
        row.truncate(14);
    }
    copy.import_sync_data(&old_export).unwrap();
    let legacy = &copy.snapshot().unwrap().holdings[0];
    assert_eq!(legacy.quantity, None);
    assert_eq!(legacy.market_value, 1206.0);
}

#[test]
fn manual_holding_details_reject_invalid_values_and_inconsistent_sync() {
    let db = Database::test();
    for (quantity, price) in [
        (None, Some(10.0)),
        (Some(10.0), None),
        (Some(-1.0), Some(10.0)),
        (Some(1e15), Some(10.0)),
        (Some(1.0), Some(f64::INFINITY)),
    ] {
        let mut input = detailed_holding_input();
        input.quantity = quantity;
        input.current_price = price;
        assert!(db.add_holding(&input).is_err());
    }
    let mut zero = detailed_holding_input();
    zero.quantity = Some(0.0);
    zero.current_price = Some(0.0);
    assert_eq!(db.add_holding(&zero).unwrap().holdings[0].market_value, 0.0);
    let mut dataset = db.export_sync_data().unwrap();
    let table = dataset
        .tables
        .iter_mut()
        .find(|t| t.name == "holdings")
        .unwrap();
    table.rows[0][16] = SyncValue::Real(-1.0);
    assert!(db.import_sync_data(&dataset).is_err());
    assert_eq!(db.snapshot().unwrap().holdings[0].current_price, Some(0.0));
}

pub(super) fn strip_holding_details_for_legacy(dataset: &mut SyncDataset) {
    dataset.tables.retain(|t| t.name != "holding_trades");
    let table = dataset
        .tables
        .iter_mut()
        .find(|table| table.name == "holdings")
        .unwrap();
    table.columns.truncate(14);
    for row in &mut table.rows {
        row.truncate(14);
    }
}

#[test]
fn manual_total_updates_keep_unit_prices_consistent_and_reject_impossible_quantities() {
    let db = Database::test();
    let created = db.add_holding(&detailed_holding_input()).unwrap();
    let id = &created.holdings[0].id;
    let updated = db
        .update_holding_amount(
            id,
            &crate::models::daily::AmountInput {
                amount: 2010.0.to_string(),
                expected_revision: created.holding_revisions[id].clone(),
                request_id: "price-update".into(),
            },
        )
        .unwrap();
    assert_eq!(updated.holdings[0].current_price, Some(20.0));
    assert_eq!(updated.holdings[0].cost_basis, 804.0);
    let mut input = detailed_holding_input();
    input.quantity = Some(1e-10);
    let tiny = db.update_holding(id, &input).unwrap();
    assert!(db
        .update_holding_amount(
            id,
            &crate::models::daily::AmountInput {
                amount: 1e15.to_string(),
                expected_revision: tiny.holding_revisions[id].clone(),
                request_id: "oversized-unit-price".into(),
            }
        )
        .is_err());
    input.quantity = Some(0.0);
    let zero = db.update_holding(id, &input).unwrap();
    let saved_zero = db
        .update_holding_amount(
            id,
            &crate::models::daily::AmountInput {
                amount: 0.0.to_string(),
                expected_revision: zero.holding_revisions[id].clone(),
                request_id: "zero-quantity".into(),
            },
        )
        .unwrap();
    assert_eq!(saved_zero.holdings[0].current_price, Some(10.0));
    assert!(db
        .update_holding_amount(
            id,
            &crate::models::daily::AmountInput {
                amount: 1.0.to_string(),
                expected_revision: saved_zero.holding_revisions[id].clone(),
                request_id: "impossible-total".into(),
            }
        )
        .is_err());
}
