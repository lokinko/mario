use super::*;

fn verified_quote() -> SecurityPriceQuote {
    SecurityPriceQuote {
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
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let profile = FinancialProfile {
        monthly_expense: 10_000.0,
        emergency_fund: 60_000.0,
        ..Default::default()
    };
    db.save_profile(&profile).unwrap();
    db.add_holding(&HoldingInput {
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
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
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("verified-valuation.db")).unwrap();
    db.save_profile(&FinancialProfile {
        base_currency: "USD".into(),
        ..Default::default()
    })
    .unwrap();
    let created = db
        .add_holding(&HoldingInput {
            symbol: "AAPL".into(),
            name: "Apple".into(),
            asset_class: "股票".into(),
            market_value: 900.0,
            cost_basis: 800.0,
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
    assert_eq!(valued.holding_valuations.len(), 1);
    assert_eq!(valued.holding_valuations[0].quantity, 10.0);
    assert_eq!(valued.holding_valuations[0].observed_on, "2026-09-04");

    let checkin = db
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "带来源基线".into(),
            external_cash_flow: 0.0,
            note: String::new(),
            reset_baseline: false,
            use_ledger_cash_flows: true,
        })
        .unwrap();
    assert_eq!(checkin.holding_valuations.len(), 1);

    let unchanged = HoldingInput {
        symbol: "AAPL".into(),
        name: "Apple Inc.".into(),
        asset_class: "股票".into(),
        market_value: 1_012.5,
        cost_basis: 810.0,
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
        market_value: 1_020.0,
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
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("valuation-currency.db")).unwrap();
    let created = db
        .add_holding(&HoldingInput {
            symbol: "AAPL".into(),
            name: "错误币种".into(),
            asset_class: "股票".into(),
            market_value: 1_000.0,
            cost_basis: 900.0,
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
fn reverses_only_open_period_events_and_preserves_the_audit_chain() {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::open(&directory.path().join("reversal.db")).unwrap();
    db.add_holding(&HoldingInput {
        symbol: "CASH".into(),
        name: "现金".into(),
        asset_class: "现金".into(),
        market_value: 100_000.0,
        cost_basis: 100_000.0,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-08-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    db.save_portfolio_checkin(&PortfolioCheckInInput {
        period_label: "冲正测试基线".into(),
        external_cash_flow: 0.0,
        note: "冻结测试基线".into(),
        reset_baseline: false,
        use_ledger_cash_flows: false,
    })
    .unwrap();
    let source = db
        .add_portfolio_event(&PortfolioEventInput {
            event_type: PortfolioEventType::Deposit,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount: 1_000.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-08-02".into(),
            note: "误录入金".into(),
        })
        .unwrap();
    let reversal = db
        .reverse_portfolio_event(
            &source.id,
            &PortfolioEventReversalInput {
                occurred_on: "2026-08-03".into(),
                note: "与银行流水核对后确认重复".into(),
            },
        )
        .unwrap();

    assert_eq!(reversal.amount, -1_000.0);
    assert_eq!(reversal.base_amount, -1_000.0);
    assert_eq!(
        reversal.reversal_of_event_id.as_deref(),
        Some(source.id.as_str())
    );
    let events = db.portfolio_events().unwrap();
    let restored_source = events.iter().find(|event| event.id == source.id).unwrap();
    assert_eq!(
        restored_source.reversed_by_event_id.as_deref(),
        Some(reversal.id.as_str())
    );
    assert_eq!(performance::summarize(&events).external_cash_flow, 0.0);
    assert!(matches!(
        db.reverse_portfolio_event(
            &source.id,
            &PortfolioEventReversalInput {
                occurred_on: "2026-08-03".into(),
                note: "再次冲正".into(),
            }
        ),
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        db.reverse_portfolio_event(
            &reversal.id,
            &PortfolioEventReversalInput {
                occurred_on: "2026-08-03".into(),
                note: "冲正冲正记录".into(),
            }
        ),
        Err(AppError::Validation(_))
    ));

    let dataset = db.export_sync_data().unwrap();
    let target = Database::open(&directory.path().join("reversal-target.db")).unwrap();
    target.import_sync_data(&dataset).unwrap();
    let restored = target.portfolio_events().unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(performance::summarize(&restored).external_cash_flow, 0.0);
}

#[test]
fn sync_snapshot_round_trips_domain_data_but_never_settings() {
    let directory = tempfile::tempdir().unwrap();
    let source = Database::open(&directory.path().join("source.db")).unwrap();
    source
        .save_profile(&FinancialProfile {
            monthly_expense: 8_000.0,
            emergency_fund: 48_000.0,
            ..Default::default()
        })
        .unwrap();
    source
        .add_holding(&HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 120_000.0,
            cost_basis: 100_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    let holding_id = source.snapshot().unwrap().holdings[0].id.clone();
    let mut quote = verified_quote();
    quote.symbol = "IDX".into();
    quote.currency = "CNY".into();
    quote.requested_on = "2026-01-01".into();
    quote.observed_on = "2026-01-01".into();
    quote.staleness_days = 0;
    quote.close = 12.0;
    quote.source_url = "https://api.twelvedata.com/time_series?symbol=IDX&interval=1day".into();
    source
        .apply_verified_holding_valuation(&holding_id, 10_000.0, &quote)
        .unwrap();
    let rule = source
        .add_investment_rule(&InvestmentRuleInput {
            category: "仓位".into(),
            statement: "单一主动仓位不超过 8%".into(),
            trigger: "任何新建或加仓决定".into(),
            rationale: "限制永久损失".into(),
            active: true,
            source_review_id: None,
        })
        .unwrap();
    source
        .save_decision(&DecisionEntry {
            id: Some("synced-decision".into()),
            source_analysis_id: None,
            source_action_index: None,
            asset_name: "宽基指数".into(),
            thesis: "长期风险溢价".into(),
            counter_thesis: "估值偏高".into(),
            expected_return_pct: 8.0,
            downside_pct: 20.0,
            confidence_pct: 60.0,
            position_pct: 8.0,
            invalidation: "风险容量下降".into(),
            review_date: "2026-12-01".into(),
            rule_checks: vec![DecisionRuleCheck {
                rule_id: rule.id,
                rule_revision: rule.revision,
                category: rule.category,
                statement: rule.statement,
                trigger: rule.trigger,
                status: "遵守".into(),
                note: "".into(),
            }],
        })
        .unwrap();
    let hidden_memory = source
        .save_memory_preference(
            "synced-decision",
            &MemoryPreferenceInput {
                preference: "hidden".into(),
                note: "先验证永久屏蔽".into(),
            },
        )
        .unwrap();
    assert!(!hidden_memory.selected);
    source
        .save_memory_preference(
            "synced-decision",
            &MemoryPreferenceInput {
                preference: "pinned".into(),
                note: "跨设备保留的能力圈经验".into(),
            },
        )
        .unwrap();
    source
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "同步基线".into(),
            external_cash_flow: 0.0,
            note: "首次组合快照".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        })
        .unwrap();
    source
        .add_portfolio_event(&PortfolioEventInput {
            event_type: PortfolioEventType::Deposit,
            source: "同步券商".into(),
            external_id: "sync-event-1".into(),
            asset_name: String::new(),
            amount: 700.0,
            currency: "USD".into(),
            fx_rate_to_base: Some(7.0),
            fx_rate_source: "ecb_reference".into(),
            fx_rate_observed_on: "2026-01-02".into(),
            occurred_on: "2026-01-02".into(),
            note: "同步测试入金".into(),
        })
        .unwrap();
    source.set_setting("model.name", "never-sync-this").unwrap();

    let dataset = source.export_sync_data().unwrap();
    let second_export = source.export_sync_data().unwrap();
    assert_eq!(
        dataset.content_hash().unwrap(),
        second_export.content_hash().unwrap()
    );

    let mut orphaned_preference = dataset.clone();
    let preference_table = orphaned_preference
        .tables
        .iter_mut()
        .find(|table| table.name == "memory_preferences")
        .unwrap();
    preference_table.rows[0][0] = SyncValue::Text("missing-memory".into());
    let preference_target = Database::open(&directory.path().join("preference-target.db")).unwrap();
    assert!(matches!(
        preference_target.import_sync_data(&orphaned_preference),
        Err(AppError::Validation(_))
    ));

    let mut corrupted_identity = dataset.clone();
    let event_table = corrupted_identity
        .tables
        .iter_mut()
        .find(|table| table.name == "portfolio_events")
        .unwrap();
    event_table.rows[0][4] = SyncValue::Text("wrong-fingerprint".into());
    let corrupted_target = Database::open(&directory.path().join("corrupted-target.db")).unwrap();
    assert!(matches!(
        corrupted_target.import_sync_data(&corrupted_identity),
        Err(AppError::Validation(_))
    ));

    let mut corrupted_fx_date = dataset.clone();
    let event_table = corrupted_fx_date
        .tables
        .iter_mut()
        .find(|table| table.name == "portfolio_events")
        .unwrap();
    event_table.rows[0][10] = SyncValue::Text("2026-01-03".into());
    assert!(matches!(
        corrupted_target.import_sync_data(&corrupted_fx_date),
        Err(AppError::Validation(_))
    ));

    let mut corrupted_valuation = dataset.clone();
    let valuation_table = corrupted_valuation
        .tables
        .iter_mut()
        .find(|table| table.name == "holding_valuations")
        .unwrap();
    valuation_table.rows[0][4] = SyncValue::Real(999.0);
    assert!(matches!(
        corrupted_target.import_sync_data(&corrupted_valuation),
        Err(AppError::Validation(_))
    ));

    let target = Database::open(&directory.path().join("target.db")).unwrap();
    target
        .set_setting("model.name", "keep-local-model")
        .unwrap();
    target.import_sync_data(&dataset).unwrap();
    let restored = target.snapshot().unwrap();
    assert_eq!(restored.holdings.len(), 1);
    assert_eq!(restored.holding_valuations.len(), 1);
    assert_eq!(restored.holding_valuations[0].quantity, 10_000.0);
    assert_eq!(restored.profile.emergency_fund, 48_000.0);
    let restored_decision = target.decisions().unwrap().remove(0);
    assert_eq!(restored_decision.rule_checks.len(), 1);
    assert_eq!(restored_decision.rule_checks[0].status, "遵守");
    let restored_memory = target
        .memories()
        .unwrap()
        .into_iter()
        .find(|item| item.id == "synced-decision")
        .unwrap();
    assert_eq!(restored_memory.preference, "pinned");
    assert_eq!(restored_memory.preference_note, "跨设备保留的能力圈经验");
    assert_eq!(target.portfolio_checkins().unwrap().len(), 1);
    let restored_events = target.portfolio_events().unwrap();
    assert_eq!(restored_events.len(), 1);
    assert_eq!(restored_events[0].source, "同步券商");
    assert_eq!(restored_events[0].external_id, "sync-event-1");
    assert_eq!(restored_events[0].fx_rate_source, "ecb_reference");
    assert_eq!(restored_events[0].fx_rate_observed_on, "2026-01-02");
    assert_eq!(
        target.setting("model.name").unwrap().as_deref(),
        Some("keep-local-model")
    );

    let mut legacy_v8 = dataset.clone();
    legacy_v8.schema_version = 8;
    assert_eq!(legacy_v8.tables.pop().unwrap().name, "holding_valuations");
    legacy_v8.validate().unwrap();
    let v8_target = Database::open(&directory.path().join("v8-target.db")).unwrap();
    v8_target.import_sync_data(&legacy_v8).unwrap();
    assert_eq!(v8_target.memories().unwrap().len(), 1);

    let mut legacy_v7 = legacy_v8;
    legacy_v7.schema_version = 7;
    assert_eq!(legacy_v7.tables.pop().unwrap().name, "memory_preferences");
    legacy_v7.validate().unwrap();
    let v7_target = Database::open(&directory.path().join("v7-target.db")).unwrap();
    v7_target.import_sync_data(&legacy_v7).unwrap();
    assert_eq!(v7_target.portfolio_events().unwrap().len(), 1);

    let mut legacy_v6 = legacy_v7;
    legacy_v6.schema_version = 6;
    let events = legacy_v6
        .tables
        .iter_mut()
        .find(|table| table.name == "portfolio_events")
        .unwrap();
    events.columns.remove(16);
    for row in &mut events.rows {
        row.remove(16);
    }
    legacy_v6.validate().unwrap();
    let v6_target = Database::open(&directory.path().join("v6-target.db")).unwrap();
    v6_target.import_sync_data(&legacy_v6).unwrap();
    assert_eq!(v6_target.portfolio_events().unwrap().len(), 1);

    let mut legacy_v5 = legacy_v6;
    legacy_v5.schema_version = 5;
    let holdings = legacy_v5
        .tables
        .iter_mut()
        .find(|table| table.name == "holdings")
        .unwrap();
    for index in [11, 10] {
        holdings.columns.remove(index);
        for row in &mut holdings.rows {
            row.remove(index);
        }
    }
    let events = legacy_v5
        .tables
        .iter_mut()
        .find(|table| table.name == "portfolio_events")
        .unwrap();
    for index in [10, 9] {
        events.columns.remove(index);
        for row in &mut events.rows {
            row.remove(index);
        }
    }
    legacy_v5.validate().unwrap();
    let v5_target = Database::open(&directory.path().join("v5-target.db")).unwrap();
    v5_target.import_sync_data(&legacy_v5).unwrap();
    assert!(v5_target.portfolio_events().unwrap()[0]
        .fx_rate_source
        .is_empty());

    let mut legacy_v4 = legacy_v5;
    legacy_v4.schema_version = 4;
    let events = legacy_v4
        .tables
        .iter_mut()
        .find(|table| table.name == "portfolio_events")
        .unwrap();
    for index in [4, 3, 2] {
        events.columns.remove(index);
        for row in &mut events.rows {
            row.remove(index);
        }
    }
    legacy_v4.validate().unwrap();
    let v4_target = Database::open(&directory.path().join("v4-target.db")).unwrap();
    v4_target.import_sync_data(&legacy_v4).unwrap();
    let v4_events = v4_target.portfolio_events().unwrap();
    assert_eq!(v4_events.len(), 1);
    assert_eq!(v4_events[0].source, "manual");
    assert!(v4_events[0].external_id.is_empty());

    let mut legacy_v3 = legacy_v4;
    legacy_v3.schema_version = 3;
    assert_eq!(legacy_v3.tables.pop().unwrap().name, "portfolio_events");
    legacy_v3.validate().unwrap();
    let v3_target = Database::open(&directory.path().join("v3-target.db")).unwrap();
    v3_target.import_sync_data(&legacy_v3).unwrap();
    assert!(v3_target.portfolio_events().unwrap().is_empty());

    let mut legacy_v2 = legacy_v3;
    legacy_v2.schema_version = 2;
    let holdings = legacy_v2
        .tables
        .iter_mut()
        .find(|table| table.name == "holdings")
        .unwrap();
    for index in [9, 8] {
        holdings.columns.remove(index);
        for row in &mut holdings.rows {
            row.remove(index);
        }
    }
    legacy_v2.validate().unwrap();
    let v2_target = Database::open(&directory.path().join("v2-target.db")).unwrap();
    v2_target.import_sync_data(&legacy_v2).unwrap();
    assert_eq!(v2_target.portfolio_checkins().unwrap().len(), 1);
    assert_eq!(
        v2_target
            .snapshot()
            .unwrap()
            .valuation_status
            .undated_holding_count,
        1
    );

    let mut legacy_dataset = legacy_v2;
    legacy_dataset.schema_version = 1;
    assert_eq!(
        legacy_dataset.tables.pop().unwrap().name,
        "portfolio_checkins"
    );
    legacy_dataset.validate().unwrap();
    let legacy_target = Database::open(&directory.path().join("legacy-target.db")).unwrap();
    legacy_target.import_sync_data(&legacy_dataset).unwrap();
    assert_eq!(legacy_target.snapshot().unwrap().holdings.len(), 1);
    assert!(legacy_target.portfolio_checkins().unwrap().is_empty());
}

#[test]
fn sync_snapshot_rejects_unknown_or_incomplete_schema() {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::open(&directory.path().join("source.db")).unwrap();
    let mut dataset = db.export_sync_data().unwrap();
    dataset.tables.pop();
    assert!(matches!(dataset.validate(), Err(AppError::Validation(_))));
}

#[test]
fn updates_and_deletes_holding() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let input = HoldingInput {
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
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
    assert!(db.delete_holding(id).unwrap().holdings.is_empty());
}

#[test]
fn portfolio_checkins_separate_external_flows_from_valuation_residuals() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    assert!(matches!(
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "基线".into(),
            external_cash_flow: 0.0,
            note: "".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        }),
        Err(AppError::Validation(_))
    ));
    let snapshot = db
        .add_holding(&HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 100_000.0,
            cost_basis: 90_000.0,
            target_pct: 90.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    assert!(matches!(
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "错误基线".into(),
            external_cash_flow: 10_000.0,
            note: "首次入金".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        }),
        Err(AppError::Validation(_))
    ));
    let baseline = db
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "2026-08 基线".into(),
            external_cash_flow: 0.0,
            note: "首次冻结组合".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        })
        .unwrap();
    assert_eq!(baseline.total_value, 100_000.0);
    assert_eq!(baseline.total_change, None);

    let fund_id = snapshot.holdings[0].id.clone();
    db.update_holding(
        &fund_id,
        &HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 102_000.0,
            cost_basis: 90_000.0,
            target_pct: 90.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-02-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();
    db.add_holding(&HoldingInput {
        symbol: "CASH".into(),
        name: "新增现金".into(),
        asset_class: "现金".into(),
        market_value: 10_000.0,
        cost_basis: 10_000.0,
        target_pct: 10.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-02-01".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    let next = db
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "2026-09".into(),
            external_cash_flow: 10_000.0,
            note: "工资结余入金".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        })
        .unwrap();
    assert_eq!(
        next.previous_check_in_id.as_deref(),
        Some(baseline.id.as_str())
    );
    assert_eq!(next.total_value, 112_000.0);
    assert_eq!(next.total_change, Some(12_000.0));
    assert_eq!(next.valuation_residual, Some(2_000.0));
    assert_eq!(next.allocation_changes.len(), 2);

    db.update_holding(
        &fund_id,
        &HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 120_000.0,
            cost_basis: 90_000.0,
            target_pct: 90.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-03-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();
    let history = db.portfolio_checkins().unwrap();
    assert_eq!(
        history[0].holdings[0]
            .market_value
            .max(history[0].holdings[1].market_value),
        102_000.0
    );
    assert_eq!(history[1].holdings[0].market_value, 100_000.0);
}

#[test]
fn ledger_drives_checkin_cash_flows_and_preserves_frozen_periods() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ledger.db")).unwrap();
    assert!(matches!(
        db.add_portfolio_event(&PortfolioEventInput {
            event_type: PortfolioEventType::Deposit,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount: 1_000.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-01".into(),
            note: "没有基线".into(),
        }),
        Err(AppError::Validation(_))
    ));
    let created = db
        .add_holding(&HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 100_000.0,
            cost_basis: 90_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        })
        .unwrap();
    db.save_portfolio_checkin(&PortfolioCheckInInput {
        period_label: "一月基线".into(),
        external_cash_flow: 0.0,
        note: "开始记录".into(),
        reset_baseline: false,
        use_ledger_cash_flows: true,
    })
    .unwrap();

    for input in [
        PortfolioEventInput {
            event_type: PortfolioEventType::Deposit,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount: 10_000.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-16".into(),
            note: "工资结余入金".into(),
        },
        PortfolioEventInput {
            event_type: PortfolioEventType::Dividend,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: "宽基指数".into(),
            amount: 500.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-20".into(),
            note: "现金分红".into(),
        },
        PortfolioEventInput {
            event_type: PortfolioEventType::Fee,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: "宽基指数".into(),
            amount: 50.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-21".into(),
            note: "交易费用".into(),
        },
        PortfolioEventInput {
            event_type: PortfolioEventType::Buy,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: "宽基指数".into(),
            amount: 20_000.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-22".into(),
            note: "账户内部调仓".into(),
        },
    ] {
        db.add_portfolio_event(&input).unwrap();
    }

    db.update_holding(
        &created.holdings[0].id,
        &HoldingInput {
            symbol: "IDX".into(),
            name: "宽基指数".into(),
            asset_class: "基金".into(),
            market_value: 120_450.0,
            cost_basis: 110_000.0,
            target_pct: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-01-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();
    let checkin = db
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "一月".into(),
            external_cash_flow: 999_999.0,
            note: "流水自动汇总".into(),
            reset_baseline: false,
            use_ledger_cash_flows: true,
        })
        .unwrap();
    assert_eq!(checkin.cash_flow_source, "ledger");
    assert_eq!(checkin.external_cash_flow, 10_000.0);
    assert_eq!(checkin.income, 500.0);
    assert_eq!(checkin.costs, 50.0);
    assert_eq!(checkin.turnover, 20_000.0);
    assert_eq!(checkin.event_ids.len(), 4);
    assert!((checkin.modified_dietz_return_pct.unwrap() - 9.952_380_952).abs() < 1e-6);

    assert!(matches!(
        db.add_portfolio_event(&PortfolioEventInput {
            event_type: PortfolioEventType::Withdrawal,
            source: "manual".into(),
            external_id: String::new(),
            asset_name: String::new(),
            amount: 100.0,
            currency: "CNY".into(),
            fx_rate_to_base: None,
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
            occurred_on: "2026-01-30".into(),
            note: "迟到记录".into(),
        }),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn csv_event_import_is_previewed_atomic_deduplicated_and_revision_bound() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("event-import.db")).unwrap();
    db.add_holding(&HoldingInput {
        symbol: "CASH".into(),
        name: "现金".into(),
        asset_class: "现金".into(),
        market_value: 10_000.0,
        cost_basis: 10_000.0,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-08-31".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    db.save_portfolio_checkin(&PortfolioCheckInInput {
        period_label: "导入基线".into(),
        external_cash_flow: 0.0,
        note: String::new(),
        reset_baseline: false,
        use_ledger_cash_flows: true,
    })
    .unwrap();

    let conflicting = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,trade-001,deposit,2026-09-01,2000,CNY,,,冲突入金\n";
    let conflict_preview = db
        .preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: conflicting.into(),
        })
        .unwrap();
    assert_eq!(conflict_preview.ready_count, 1);
    assert_eq!(conflict_preview.error_count, 1);
    assert!(matches!(
        db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
            csv_text: conflicting.into(),
            preview_revision: conflict_preview.preview_revision,
        }),
        Err(AppError::Validation(_))
    ));
    assert!(db.portfolio_events().unwrap().is_empty());

    let valid = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,trade-001,deposit,2026-09-01,1000,CNY,,,首次入金\n券商甲,fee-001,fee,2026-09-02,5,CNY,,指数基金,交易费用\n";
    let preview = db
        .preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: valid.into(),
        })
        .unwrap();
    assert_eq!(preview.ready_count, 2);
    assert_eq!(preview.duplicate_count, 1);
    assert_eq!(preview.error_count, 0);
    let changed_after_preview = valid.replace("交易费用", "费用说明已改变");
    assert!(matches!(
        db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
            csv_text: changed_after_preview,
            preview_revision: preview.preview_revision.clone(),
        }),
        Err(AppError::Conflict(_))
    ));
    assert!(db.portfolio_events().unwrap().is_empty());
    let result = db
        .commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
            csv_text: valid.into(),
            preview_revision: preview.preview_revision,
        })
        .unwrap();
    assert_eq!(result.inserted_count, 2);
    assert_eq!(result.duplicate_count, 1);
    assert_eq!(db.portfolio_events().unwrap().len(), 2);

    let repeated = db
        .preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: valid.into(),
        })
        .unwrap();
    assert_eq!(repeated.ready_count, 0);
    assert_eq!(repeated.duplicate_count, 3);
    let repeated_result = db
        .commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
            csv_text: valid.into(),
            preview_revision: repeated.preview_revision,
        })
        .unwrap();
    assert_eq!(repeated_result.inserted_count, 0);

    let changed = valid.replace("1000,CNY,,,首次入金", "1200,CNY,,,首次入金");
    let changed_preview = db
        .preview_portfolio_event_import(&PortfolioEventImportRequest { csv_text: changed })
        .unwrap();
    assert!(changed_preview.error_count > 0);

    let late = "source,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,asset_name,note\n券商甲,trade-003,interest,2026-09-03,20,CNY,,现金,利息\n";
    let late_preview = db
        .preview_portfolio_event_import(&PortfolioEventImportRequest {
            csv_text: late.into(),
        })
        .unwrap();
    db.add_portfolio_event(&PortfolioEventInput {
        event_type: PortfolioEventType::Interest,
        source: "券商甲".into(),
        external_id: "trade-003".into(),
        asset_name: "现金".into(),
        amount: 20.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
        occurred_on: "2026-09-03".into(),
        note: "利息".into(),
    })
    .unwrap();
    assert!(matches!(
        db.commit_portfolio_event_import(&PortfolioEventImportCommitRequest {
            csv_text: late.into(),
            preview_revision: late_preview.preview_revision,
        }),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn foreign_currency_events_require_and_freeze_the_declared_rate() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("event-fx.db")).unwrap();
    db.add_holding(&HoldingInput {
        symbol: "CASH".into(),
        name: "现金".into(),
        asset_class: "现金".into(),
        market_value: 10_000.0,
        cost_basis: 10_000.0,
        target_pct: 100.0,
        currency: "CNY".into(),
        fx_rate_to_base: None,
        valuation_date: "2026-08-31".into(),
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
    })
    .unwrap();
    db.save_portfolio_checkin(&PortfolioCheckInInput {
        period_label: "基线".into(),
        external_cash_flow: 0.0,
        note: String::new(),
        reset_baseline: false,
        use_ledger_cash_flows: true,
    })
    .unwrap();
    let mut input = PortfolioEventInput {
        event_type: PortfolioEventType::Withdrawal,
        source: "manual".into(),
        external_id: String::new(),
        asset_name: String::new(),
        amount: 100.0,
        currency: "USD".into(),
        fx_rate_to_base: None,
        fx_rate_source: String::new(),
        fx_rate_observed_on: String::new(),
        occurred_on: "2026-09-01".into(),
        note: "美元出金".into(),
    };
    assert!(matches!(
        db.add_portfolio_event(&input),
        Err(AppError::Validation(_))
    ));
    input.fx_rate_to_base = Some(7.0);
    input.fx_rate_source = "ecb_reference".into();
    input.fx_rate_observed_on = "2026-09-02".into();
    assert!(matches!(
        db.add_portfolio_event(&input),
        Err(AppError::Validation(_))
    ));
    input.fx_rate_observed_on = "2026-08-31".into();
    let event = db.add_portfolio_event(&input).unwrap();
    assert_eq!(event.base_currency, "CNY");
    assert_eq!(event.base_amount, 700.0);
    assert_eq!(event.fx_rate_to_base, Some(7.0));
    assert_eq!(event.fx_rate_source, "ecb_reference");
    assert_eq!(event.fx_rate_observed_on, "2026-08-31");
}

#[test]
fn foreign_currency_is_normalized_and_base_changes_invalidate_old_rates() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("currency.db")).unwrap();
    let mut profile = FinancialProfile {
        base_currency: "CNY".into(),
        ..Default::default()
    };
    db.save_profile(&profile).unwrap();

    assert!(matches!(
        db.add_holding(&HoldingInput {
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
            target_pct: 70.0,
            currency: "USD".into(),
            fx_rate_to_base: None,
            valuation_date: "2026-08-31".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        }),
        Err(AppError::Validation(_))
    ));
    let usd_snapshot = db
        .add_holding(&HoldingInput {
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
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
            symbol: "CASH".into(),
            name: "人民币现金".into(),
            asset_class: "现金".into(),
            market_value: 300.0,
            cost_basis: 300.0,
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
    db.save_portfolio_checkin(&PortfolioCheckInInput {
        period_label: "CNY 基线".into(),
        external_cash_flow: 0.0,
        note: "统一估值日".into(),
        reset_baseline: false,
        use_ledger_cash_flows: false,
    })
    .unwrap();

    db.update_holding(
        &usd_id,
        &HoldingInput {
            symbol: "USD".into(),
            name: "美元资产".into(),
            asset_class: "股票".into(),
            market_value: 100.0,
            cost_basis: 90.0,
            target_pct: 70.0,
            currency: "USD".into(),
            fx_rate_to_base: Some(7.0),
            valuation_date: "2026-09-01".into(),
            fx_rate_source: String::new(),
            fx_rate_observed_on: String::new(),
        },
    )
    .unwrap();
    assert!(matches!(
        db.save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "错位日期".into(),
            external_cash_flow: 0.0,
            note: "".into(),
            reset_baseline: false,
            use_ledger_cash_flows: false,
        }),
        Err(AppError::Validation(_))
    ));

    profile.base_currency = "USD".into();
    let changed = db.save_profile(&profile).unwrap();
    assert!(!changed.valuation_status.comparable);
    assert_eq!(changed.total_value, 0.0);
    assert_eq!(
        changed.valuation_status.missing_fx_holdings,
        vec!["人民币现金"]
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
                symbol: "CASH".into(),
                name: "人民币现金".into(),
                asset_class: "现金".into(),
                market_value: 300.0,
                cost_basis: 300.0,
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
    let reset = db
        .save_portfolio_checkin(&PortfolioCheckInInput {
            period_label: "USD 新基线".into(),
            external_cash_flow: 0.0,
            note: "切换基准币种后重新开始比较".into(),
            reset_baseline: true,
            use_ledger_cash_flows: false,
        })
        .unwrap();
    assert_eq!(reset.base_currency, "USD");
    assert_eq!(reset.total_change, None);
}

#[test]
fn stores_decision_review_separately_from_original_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    db.save_decision(&DecisionEntry {
        id: None,
        source_analysis_id: None,
        source_action_index: None,
        asset_name: "指数".into(),
        thesis: "长期风险溢价".into(),
        counter_thesis: "估值过高".into(),
        expected_return_pct: 12.0,
        downside_pct: 20.0,
        confidence_pct: 65.0,
        position_pct: 30.0,
        invalidation: "风险容量改变".into(),
        review_date: "2026-12-01".into(),
        rule_checks: vec![],
    })
    .unwrap();
    let id = db.decisions().unwrap()[0].id.clone();
    db.save_decision_review(
        &id,
        &DecisionReviewInput {
            outcome_summary: "价格下跌但逻辑未破坏".into(),
            actual_return_pct: Some(-8.0),
            thesis_status: "部分成立".into(),
            process_rating: 4,
            lessons: "进一步区分价格与逻辑".into(),
        },
    )
    .unwrap();
    let record = db.decisions().unwrap().remove(0);
    assert_eq!(record.thesis, "长期风险溢价");
    assert_eq!(record.review.unwrap().process_rating, 4);
    let memory = db
        .memories()
        .unwrap()
        .into_iter()
        .find(|item| item.kind == "decision")
        .unwrap();
    assert!(memory.reviewed);
    assert!(memory.contradiction);
    assert_eq!(memory.status, "复盘：部分成立");
    assert_eq!(memory.content["originalThesis"], "长期风险溢价");
    assert_eq!(memory.content["review"]["processRating"], 4);
}

#[test]
fn requires_every_decision_to_have_a_review_date() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let result = db.save_decision(&DecisionEntry {
        id: None,
        source_analysis_id: None,
        source_action_index: None,
        asset_name: "指数".into(),
        thesis: "长期风险溢价".into(),
        counter_thesis: "估值过高".into(),
        expected_return_pct: 8.0,
        downside_pct: 20.0,
        confidence_pct: 60.0,
        position_pct: 30.0,
        invalidation: "风险容量下降".into(),
        review_date: "".into(),
        rule_checks: vec![],
    });
    assert!(matches!(result, Err(AppError::Validation(_))));
}

#[test]
fn review_reminders_are_opt_in_deduplicated_and_follow_due_work() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 9, 6).unwrap();

    let initial = db.review_reminder_summary(today).unwrap();
    assert!(!initial.enabled);
    assert!(initial.periodic_review_due);
    assert!(!initial.should_notify);

    db.save_reminder_settings(true).unwrap();
    db.save_decision(&DecisionEntry {
        id: Some("due-decision".into()),
        source_analysis_id: None,
        source_action_index: None,
        asset_name: "宽基指数".into(),
        thesis: "长期配置".into(),
        counter_thesis: "风险容量下降".into(),
        expected_return_pct: 7.0,
        downside_pct: 20.0,
        confidence_pct: 60.0,
        position_pct: 30.0,
        invalidation: "应急金不足".into(),
        review_date: "2026-09-06".into(),
        rule_checks: vec![],
    })
    .unwrap();
    db.save_decision(&DecisionEntry {
        id: Some("future-decision".into()),
        source_analysis_id: None,
        source_action_index: None,
        asset_name: "债券基金".into(),
        thesis: "降低波动".into(),
        counter_thesis: "利率快速上行".into(),
        expected_return_pct: 3.0,
        downside_pct: 5.0,
        confidence_pct: 70.0,
        position_pct: 20.0,
        invalidation: "久期风险变化".into(),
        review_date: "2026-10-01".into(),
        rule_checks: vec![],
    })
    .unwrap();

    let due = db.review_reminder_summary(today).unwrap();
    assert_eq!(due.due_decision_count, 1);
    assert!(due.should_notify);
    assert!(matches!(
        db.acknowledge_review_reminder(today, "stale-fingerprint"),
        Err(AppError::Validation(_))
    ));
    let acknowledged = db
        .acknowledge_review_reminder(today, &due.fingerprint)
        .unwrap();
    assert!(!acknowledged.should_notify);

    db.save_decision_review(
        "due-decision",
        &DecisionReviewInput {
            outcome_summary: "按计划复核".into(),
            actual_return_pct: None,
            thesis_status: "尚不明确".into(),
            process_rating: 4,
            lessons: "继续观察证伪条件".into(),
        },
    )
    .unwrap();
    let changed = db.review_reminder_summary(today).unwrap();
    assert_eq!(changed.due_decision_count, 0);
    assert!(changed.periodic_review_due);
    assert!(changed.should_notify);
    assert_ne!(changed.fingerprint, due.fingerprint);

    db.save_system_review(&SystemReviewInput {
        period_label: "2026 Q3".into(),
        adherence_score: 4,
        process_summary: "按计划执行".into(),
        rule_violations: "无".into(),
        lessons: "保持低频复盘".into(),
        next_actions: "下季度复核".into(),
        next_review_date: "2026-12-31".into(),
    })
    .unwrap();
    let clear = db.review_reminder_summary(today).unwrap();
    assert_eq!(clear.due_decision_count, 0);
    assert!(!clear.periodic_review_due);
    assert!(!clear.should_notify);

    db.save_reminder_settings(false).unwrap();
    assert!(!db.review_reminder_summary(today).unwrap().should_notify);
}

#[test]
fn reminder_settings_are_device_local_and_never_cloud_synced() {
    let directory = tempfile::tempdir().unwrap();
    let source = Database::open(&directory.path().join("source.db")).unwrap();
    source.save_reminder_settings(true).unwrap();
    let dataset = source.export_sync_data().unwrap();

    let target = Database::open(&directory.path().join("target.db")).unwrap();
    target.save_reminder_settings(false).unwrap();
    target.import_sync_data(&dataset).unwrap();

    assert!(!target.reminder_settings().unwrap().enabled);
    assert!(dataset.tables.iter().all(|table| table.name != "settings"));
}

#[test]
fn decision_provenance_must_reference_an_existing_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let base = DecisionEntry {
        id: None,
        source_analysis_id: Some("analysis-1".into()),
        source_action_index: Some(0),
        asset_name: "指数".into(),
        thesis: "长期风险溢价".into(),
        counter_thesis: "估值过高".into(),
        expected_return_pct: 8.0,
        downside_pct: 20.0,
        confidence_pct: 60.0,
        position_pct: 30.0,
        invalidation: "风险容量下降".into(),
        review_date: "2026-12-01".into(),
        rule_checks: vec![],
    };
    assert!(matches!(
        db.save_decision(&base),
        Err(AppError::Validation(_))
    ));

    let now = Utc::now().to_rfc3339();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id, question, answer, audit, trace, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "analysis-1",
                "如何处理风险",
                "先降低脆弱性",
                "{}",
                r#"{"structuredReport":{"verdict":"降低脆弱性","facts":[],"inferences":[],"unknowns":["估值"],"options":[],"actions":[{"action":"再平衡","rationale":"集中度过高","reversible":true,"reviewTrigger":"集中度下降"}],"reviewTriggers":["一个月后"]}}"#,
                now
            ],
        )
        .unwrap();
    let mut invalid_action = base.clone();
    invalid_action.source_action_index = Some(1);
    assert!(matches!(
        db.save_decision(&invalid_action),
        Err(AppError::Validation(_))
    ));
    db.save_decision(&base).unwrap();
    let record = db.decisions().unwrap().remove(0);
    assert_eq!(record.source_analysis_id.as_deref(), Some("analysis-1"));
    assert_eq!(record.source_action_index, Some(0));
}

#[test]
fn old_decision_payload_deserializes_without_provenance() {
    let payload = r#"{
        "id":null,"assetName":"指数","thesis":"长期持有","counterThesis":"估值风险",
        "expectedReturnPct":8,"downsidePct":20,"confidencePct":60,"positionPct":30,
        "invalidation":"目标变化","reviewDate":"2026-12-01"
    }"#;
    let entry: DecisionEntry = serde_json::from_str(payload).unwrap();
    assert_eq!(entry.source_analysis_id, None);
    assert_eq!(entry.source_action_index, None);
    assert!(entry.rule_checks.is_empty());
}

#[test]
fn updates_goal_funding_and_recalculates_projection() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let input = GoalInput {
        name: "养老".into(),
        target_amount: 1_000_000.0,
        current_amount: 100_000.0,
        monthly_contribution: 2_000.0,
        target_date: "2036-12-31".into(),
        priority: "重要".into(),
    };
    let created = db.add_goal(&input).unwrap();
    let id = created.goals[0].id.clone();
    let initial_success = created.plan.goal_projections[0].estimated_success_pct;
    let mut updated = input;
    updated.monthly_contribution = 8_000.0;
    let snapshot = db.update_goal(&id, &updated).unwrap();
    assert_eq!(snapshot.goals[0].monthly_contribution, 8_000.0);
    assert!(snapshot.plan.goal_projections[0].estimated_success_pct > initial_success);
    assert!(db.delete_goal(&id).unwrap().goals.is_empty());
}

#[test]
fn migrates_existing_goal_and_holding_columns_without_losing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE goals (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, target_amount REAL NOT NULL,
               target_date TEXT NOT NULL, priority TEXT NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE holdings (
               id TEXT PRIMARY KEY, symbol TEXT NOT NULL, name TEXT NOT NULL,
               asset_class TEXT NOT NULL, market_value REAL NOT NULL, cost_basis REAL NOT NULL,
               currency TEXT NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE analyses (
               id TEXT PRIMARY KEY, question TEXT NOT NULL, answer TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE TABLE portfolio_events (
               id TEXT PRIMARY KEY, event_type TEXT NOT NULL, asset_name TEXT NOT NULL,
               amount REAL NOT NULL, currency TEXT NOT NULL, fx_rate_to_base REAL,
               base_currency TEXT NOT NULL, base_amount REAL NOT NULL,
               occurred_on TEXT NOT NULL, note TEXT NOT NULL, created_at TEXT NOT NULL
             );
             INSERT INTO goals VALUES ('g1','养老',1000000,'2036-12-31','重要','2026-01-01');
             INSERT INTO holdings VALUES ('h1','IDX','指数','基金',100000,90000,'CNY','2026-01-01');
             INSERT INTO analyses VALUES ('a1','旧问题','旧回答','2026-01-01');
             INSERT INTO portfolio_events VALUES (
               'e1','deposit','',1000,'CNY',NULL,'CNY',1000,
               '2026-01-02','旧版入金','2026-01-02T00:00:00Z'
             );",
        )
        .unwrap();
    drop(connection);

    let db = Database::open(&path).unwrap();
    let snapshot = db.snapshot().unwrap();
    assert_eq!(snapshot.goals[0].current_amount, 0.0);
    assert_eq!(snapshot.goals[0].monthly_contribution, 0.0);
    assert_eq!(snapshot.holdings[0].target_pct, 0.0);
    assert_eq!(snapshot.holdings[0].fx_rate_to_base, None);
    assert!(snapshot.holdings[0].valuation_date.is_empty());
    assert!(snapshot.holdings[0].fx_rate_source.is_empty());
    assert!(snapshot.holdings[0].fx_rate_observed_on.is_empty());
    let events = db.portfolio_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source, "manual");
    assert!(events[0].external_id.is_empty());
    assert!(events[0].fx_rate_source.is_empty());
    assert!(events[0].fx_rate_observed_on.is_empty());
    assert_eq!(snapshot.profile.base_currency, "CNY");
    assert_eq!(snapshot.valuation_status.undated_holding_count, 1);
    assert!(db.analysis_history().unwrap()[0].transparency.is_none());
}

#[test]
fn stores_analysis_transparency_audit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
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
            memory_items_used: 2,
            reviewed_memory_items_used: 1,
            conflicting_memory_items_used: 1,
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
    assert_eq!(audit.memory_items_used, 2);
    assert_eq!(audit.model_calls, 3);
    assert!(!audit.api_key_sent);
    assert_eq!(
        history[0].workflow_version.as_deref(),
        Some("investment-workflow-v4")
    );
    assert_eq!(history[0].verdict.as_deref(), Some("先控制风险"));
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
    assert!(matches!(db.analysis("missing"), Err(AppError::NotFound(_))));
    let memory = db
        .memories()
        .unwrap()
        .into_iter()
        .find(|item| item.kind == "analysis")
        .unwrap();
    assert!(!memory.reviewed);
    assert!(memory.status.contains("未经结果验证"));
    assert_eq!(memory.content["workflowVersion"], "investment-workflow-v4");
}

#[test]
fn reads_analysis_audit_created_before_evidence_fields_existed() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id, question, answer, audit, created_at)
             VALUES ('legacy','旧问题','旧回答',?1,'2026-01-01')",
            [r#"{"provider":"mock","model":"old","contextGroups":[],"payloadBytes":1,"contextRevision":"ctx-old","memoryItemsUsed":0,"externalDataUsed":false,"apiKeySent":false}"#],
        )
        .unwrap();
    let audit = db.analysis_history().unwrap()[0]
        .transparency
        .clone()
        .unwrap();
    assert_eq!(audit.evidence_items_used, 0);
    assert!(!audit.citations_required);
    assert_eq!(audit.model_calls, 0);
    assert_eq!(audit.reviewed_memory_items_used, 0);
    assert_eq!(audit.conflicting_memory_items_used, 0);
    assert!(!audit.structured_output_validated);
    assert_eq!(audit.output_repairs, 0);
    assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
}

#[test]
fn adds_workflow_trace_column_to_an_existing_analysis_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let connection = Connection::open(&path).unwrap();
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

    let db = Database::open(&path).unwrap();
    let trace_columns: i64 = db
        .conn()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('analyses') WHERE name = 'trace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(trace_columns, 1);
    assert!(db.analysis_history().unwrap()[0].workflow_version.is_none());
}

#[test]
fn reads_workflow_trace_created_before_structured_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id,question,answer,audit,trace,created_at)
             VALUES ('old-trace','旧分析','旧回答',NULL,?1,'2026-01-01')",
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
    assert!(trace.memory_items.is_empty());
    assert!(trace.structured_report.is_none());
    assert!(trace.output_validation.is_none());
    assert!(trace.evidence_catalog.is_empty());
}

#[test]
fn versions_investment_rules_without_overwriting_history() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
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
fn freezes_rule_checks_and_tracks_process_association_after_review() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let input = InvestmentRuleInput {
        category: "仓位".into(),
        statement: "单一主动仓位不超过 8%".into(),
        trigger: "任何新建或加仓决定".into(),
        rationale: "限制单一判断错误的永久损失".into(),
        active: true,
        source_review_id: None,
    };
    let rule = db.add_investment_rule(&input).unwrap();
    let decision = |id: &str, status: &str, note: &str| DecisionEntry {
        id: Some(id.into()),
        source_analysis_id: None,
        source_action_index: None,
        asset_name: "主动基金".into(),
        thesis: "策略存在长期超额".into(),
        counter_thesis: "超额可能只是风格暴露".into(),
        expected_return_pct: 10.0,
        downside_pct: 20.0,
        confidence_pct: 60.0,
        position_pct: 8.0,
        invalidation: "连续两期风格调整后仍无超额".into(),
        review_date: "2026-12-01".into(),
        rule_checks: vec![DecisionRuleCheck {
            rule_id: rule.id.clone(),
            rule_revision: rule.revision,
            category: "伪造类别".into(),
            statement: "伪造规则".into(),
            trigger: "伪造触发条件".into(),
            status: status.into(),
            note: note.into(),
        }],
    };

    let mut missing_check = decision("missing-check", "遵守", "");
    missing_check.rule_checks.clear();
    assert!(matches!(
        db.save_decision(&missing_check),
        Err(AppError::Validation(_))
    ));
    assert!(matches!(
        db.save_decision(&decision("missing-note", "偏离", "")),
        Err(AppError::Validation(_))
    ));

    for (id, status, note, rating) in [
        ("followed-1", "遵守", "", 5),
        ("followed-2", "遵守", "", 4),
        ("deviated-1", "偏离", "因为已有相关敞口", 2),
    ] {
        db.save_decision(&decision(id, status, note)).unwrap();
        db.save_decision_review(
            id,
            &DecisionReviewInput {
                outcome_summary: "按原计划完成复核".into(),
                actual_return_pct: None,
                thesis_status: "尚不明确".into(),
                process_rating: rating,
                lessons: "继续记录规则与过程之间的关系".into(),
            },
        )
        .unwrap();
    }

    let stored = db
        .decisions()
        .unwrap()
        .into_iter()
        .find(|item| item.id == "followed-1")
        .unwrap();
    assert_eq!(stored.rule_checks[0].category, "仓位");
    assert_eq!(stored.rule_checks[0].statement, input.statement);
    assert_eq!(stored.rule_checks[0].trigger, input.trigger);

    let revised = db
        .update_investment_rule(
            &rule.id,
            &InvestmentRuleInput {
                statement: "单一主动仓位不超过 6%".into(),
                ..input
            },
        )
        .unwrap();
    let summary = db.rule_effectiveness().unwrap();
    assert_eq!(summary.total_decisions, 3);
    assert_eq!(summary.evaluated_decisions, 3);
    assert!((summary.adherence_pct.unwrap() - 200.0 / 3.0).abs() < 1e-10);
    let item = &summary.rules[0];
    assert_eq!(item.current_revision, revised.revision);
    assert_eq!(item.observed_revisions, vec![1]);
    assert_eq!(item.followed_count, 2);
    assert_eq!(item.deviated_count, 1);
    assert_eq!(item.followed_process_average, Some(4.5));
    assert_eq!(item.deviated_process_average, Some(2.0));
    assert_eq!(item.signal, "遵守时过程评分更高");
}

#[test]
fn system_review_freezes_portfolio_and_method_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    db.add_holding(&HoldingInput {
        symbol: "IDX".into(),
        name: "指数".into(),
        asset_class: "基金".into(),
        market_value: 100_000.0,
        cost_basis: 90_000.0,
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
            period_label: "2026 Q3".into(),
            adherence_score: 4,
            process_summary: "按计划定投，没有追涨".into(),
            rule_violations: "无".into(),
            lessons: "继续降低无效交易".into(),
            next_actions: "下季度检查再平衡".into(),
            next_review_date: "2026-12-31".into(),
        })
        .unwrap();
    let holding_id = db.snapshot().unwrap().holdings[0].id.clone();
    db.update_holding(
        &holding_id,
        &HoldingInput {
            symbol: "IDX".into(),
            name: "指数".into(),
            asset_class: "基金".into(),
            market_value: 200_000.0,
            cost_basis: 90_000.0,
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
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let created = db
        .add_research_evidence(&ResearchEvidenceInput {
            asset_name: "全球指数".into(),
            title: "基金年度报告".into(),
            publisher: "基金管理人".into(),
            source_url: "https://example.com/annual-report".into(),
            source_tier: "一手来源".into(),
            evidence_type: "公司披露".into(),
            stance: "背景".into(),
            as_of_date: "2026-06-30".into(),
            claim: "报告披露了费用与跟踪误差".into(),
            notes: "复核费用变化".into(),
        })
        .unwrap();
    let archived = db.set_research_evidence_status(&created.id, false).unwrap();

    assert!(!archived.active);
    assert_eq!(archived.claim, created.claim);
    assert_eq!(db.research_evidence().unwrap().len(), 1);
}

#[test]
fn rejects_non_https_research_sources() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db")).unwrap();
    let result = db.add_research_evidence(&ResearchEvidenceInput {
        asset_name: "指数".into(),
        title: "未知资料".into(),
        publisher: "未知".into(),
        source_url: "http://example.com/report".into(),
        source_tier: "媒体报道".into(),
        evidence_type: "新闻".into(),
        stance: "支持".into(),
        as_of_date: "2026-06-30".into(),
        claim: "未经验证的摘要".into(),
        notes: String::new(),
    });
    assert!(matches!(result, Err(AppError::Validation(_))));
}
