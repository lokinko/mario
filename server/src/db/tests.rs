use super::*;

#[test]
fn upgrades_remove_retired_tables_without_changing_active_records() {
    let account = crate::db::TestAccount::new();
    let db = account.open();
    let holding = db.add_holding(&detailed_holding_input()).unwrap();
    let before = db.export_sync_data().unwrap();
    db.conn()
        .unwrap()
        .execute_batch(
            "CREATE TABLE trade_settlements (trade_id TEXT PRIMARY KEY, settlement_mode TEXT);",
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
                "SELECT COUNT(*) FROM pg_tables WHERE schemaname=current_schema() AND tablename='trade_settlements'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn outdated_backup_versions_are_rejected_without_mutation() {
    let source = Database::test();
    source
        .save_profile(&FinancialProfile {
            monthly_income: 4567.0,
            ..Default::default()
        })
        .unwrap();
    let current = source.export_sync_data().unwrap();
    let target = Database::test();
    target
        .save_profile(&FinancialProfile {
            monthly_income: 999.0,
            ..Default::default()
        })
        .unwrap();
    for version in [
        1,
        15,
        SYNC_DATASET_SCHEMA_VERSION - 1,
        SYNC_DATASET_SCHEMA_VERSION + 1,
    ] {
        let mut mismatch = current.clone();
        mismatch.schema_version = version;
        assert!(matches!(
            target.import_sync_data(&mismatch),
            Err(AppError::Validation(_))
        ));
    }
    assert_eq!(target.snapshot().unwrap().profile.monthly_income, 999.0);
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
    let trade: crate::models::portfolio::HoldingTradeInput = serde_json::from_value(serde_json::json!({
        "requestId": Uuid::new_v4().to_string(),
        "newHolding": {
            "symbol": "AAPL", "name": "Apple", "assetClass": "股票",
            "currency": "USD"
        },
        "kind": "buy", "occurredOn": "2026-09-05",
        "sequence": 1, "quantity": "10", "price": "100", "grossAmount": "1000", "fees": "2", "taxes": "0", "account": "券商"
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
        transparency: crate::models::analysis::AnalysisTransparency {
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
            memory_notes_used: 0,
            external_data_used: false,
            api_key_sent: false,
        },
        workflow_trace: crate::models::analysis::AnalysisWorkflowTrace {
            version: "investment-workflow-v1".into(),
            structured_report: Some(crate::models::analysis::StructuredAnalysis {
                answer: "先控制风险".into(),
                suggestions: vec![crate::models::analysis::AnalysisSuggestion {
                    action: "核对目标权重".into(),
                    why: "风险预算已超出".into(),
                    fact_indices: vec![0],
                    unless_when: "风险回到预算内".into(),
                }],
                facts: vec![crate::models::analysis::AnalysisClaim {
                    statement: "风险预算已超出".into(),
                    basis: "user_data".into(),
                    evidence_ids: Vec::new(),
                    source_refs: vec!["/question".into()],
                }],
                question: None,
            }),
            output_validation: Some(crate::models::analysis::OutputValidationTrace {
                status: "valid".into(),
                attempts: 1,
                errors: Vec::new(),
                rejected_output: None,
            }),
            ..crate::models::analysis::AnalysisWorkflowTrace::default()
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
        Some("investment-workflow-v1")
    );
    assert_eq!(history[0].verdict.as_deref(), Some("先控制风险"));
    let history_json = serde_json::to_value(&history[0]).unwrap();
    assert!(history_json.get("workflowTrace").is_none());
    assert_eq!(history_json["workflowVersion"], "investment-workflow-v1");
    assert!(audit.structured_output_validated);
    let stored = db.analysis("analysis-1").unwrap();
    assert_eq!(stored.question, "如何控制风险？");
    assert_eq!(stored.answer, "先控制风险");
    assert_eq!(
        stored.workflow_trace.unwrap().version,
        "investment-workflow-v1"
    );
    let mut valued_result = result.clone();
    valued_result.id = "analysis-valued".into();
    valued_result.workflow_trace.valuation_evidence =
        vec![crate::models::portfolio::HoldingValuationEvidence {
            holding_id: "h1".into(),
            symbol: "600036.SS".into(),
            quantity: 100.5,
            unit_price: 38.5,
            market_value: 3869.25,
            currency: "CNY".into(),
            requested_on: "2026-11-20".into(),
            observed_on: "2026-11-20".into(),
            staleness_days: 0,
            provider_code: "test".into(),
            provider_name: "Test".into(),
            exchange: "SSE".into(),
            mic_code: "XSHG".into(),
            instrument_type: "equity".into(),
            price_basis: "close".into(),
            source_url: "https://example.test/quote".into(),
            methodology_url: "https://example.test/method".into(),
            disclaimer: String::new(),
            captured_at: "2026-11-20T08:00:00Z".into(),
        }];
    db.save_analysis(&valued_result, "核对结论").unwrap();
    let valued_trace = db
        .analysis("analysis-valued")
        .unwrap()
        .workflow_trace
        .unwrap();
    assert_eq!(valued_trace.valuation_evidence[0].market_value, 3869.25);
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
fn ignores_unknown_fields_in_stored_workflow_trace() {
    let db = Database::test();
    db.conn()
        .unwrap()
        .execute(
            "INSERT INTO analyses (id,question,answer,audit,trace,created_at)
             VALUES ('extra-trace','分析','回答',NULL,$1,'2026-01-01')",
            [r#"{"version":"investment-workflow-v1","unknownExtra":"ignored","calls":[]}"#],
        )
        .unwrap();

    assert_eq!(
        db.analysis_history().unwrap()[0]
            .workflow_version
            .as_deref(),
        Some("investment-workflow-v1")
    );
    let trace = db.analysis("extra-trace").unwrap().workflow_trace.unwrap();
    assert_eq!(trace.version, "investment-workflow-v1");
    assert!(trace.structured_report.is_none());
    assert!(trace.output_validation.is_none());
    assert!(trace.evidence_catalog.is_empty());
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
fn research_evidence_tags_are_editable_without_changing_content_and_survive_backups() {
    let db = Database::test();
    let created = db
        .add_research_evidence(&ResearchEvidenceInput {
            content: "年报：分红率维持".into(),
            asset_name: " 招商银行 600036.SS ".into(),
            category: "年报".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        (created.asset_name.as_str(), created.category.as_str()),
        ("招商银行 600036.SS", "年报")
    );
    let retagged = db
        .set_research_evidence_tags(
            &created.id,
            &crate::models::research::ResearchEvidenceTagsInput {
                asset_name: String::new(),
                category: "公告".into(),
            },
        )
        .unwrap();
    assert_eq!(retagged.asset_name, "");
    assert_eq!(retagged.category, "公告");
    assert_eq!(retagged.claim, created.claim);
    for (asset_name, category) in [
        ("甲\n乙", ""),
        ("", "超过二十个字符的信息类型名称超过二十个字符"),
    ] {
        assert!(db
            .set_research_evidence_tags(
                &created.id,
                &crate::models::research::ResearchEvidenceTagsInput {
                    asset_name: asset_name.into(),
                    category: category.into(),
                },
            )
            .is_err());
    }
    assert!(db
        .set_research_evidence_tags("missing", &Default::default())
        .is_err());

    let export = db.export_sync_data().unwrap();
    let copy = Database::test();
    copy.import_sync_data(&export).unwrap();
    assert_eq!(copy.research_evidence().unwrap()[0].category, "公告");
}

#[test]
fn rejects_retired_structured_write_formats() {
    assert!(
        serde_json::from_value::<ResearchEvidenceInput>(serde_json::json!({
            "assetName":"指数", "claim":"旧表单"
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
fn old_cash_and_security_dates_do_not_block_rebalancing() {
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
        valuation_date: (today - chrono::Duration::days(30)).to_string(),
        ..detailed_holding_input()
    })
    .unwrap();

    let snapshot = db.snapshot().unwrap();
    assert_eq!(snapshot.valuation_status.valuation_dates.len(), 2);
    assert!(snapshot.valuation_status.aligned_valuation_date.is_none());
    assert!(snapshot.valuation_status.issues.is_empty());
    assert!(!snapshot.plan.rebalancing.is_empty());
    assert!(!snapshot
        .findings
        .iter()
        .any(|finding| finding.title.contains("估值日期")));

    investment.valuation_date = (today - chrono::Duration::days(8)).to_string();
    let stale = db.update_holding(&investment_id, &investment).unwrap();
    assert!(stale.valuation_status.issues.is_empty());
    assert!(!stale.plan.rebalancing.is_empty());
    assert_eq!(
        stale.plan.as_of.as_deref(),
        Some((today - chrono::Duration::days(30)).to_string().as_str())
    );
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

#[test]
fn incomplete_target_weights_only_block_rebalancing() {
    let db = Database::test();
    let mut input = detailed_holding_input();
    input.target_pct = 99.0;
    let s = db.add_holding(&input).unwrap();
    assert!(s.valuation_status.issues.is_empty());
    assert!(s.valuation_status.comparable);
    assert!(s.total_value > 0.0);
    assert_eq!(s.plan.target_total_pct, 99.0);
    assert_eq!(
        s.plan.rebalancing_blocked_by,
        vec![crate::models::portfolio::IssueCode::InvalidTargetWeights]
    );
    assert!(s.plan.rebalancing.is_empty());
}
