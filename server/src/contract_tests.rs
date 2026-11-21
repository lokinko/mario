//! Real serialization and request-policy fixtures consumed by the TypeScript contract gate.
use crate::models::{accounting::HoldingAmounts, watch::rules::WatchRules};
use serde_json::json;

#[test]
#[ignore = "emits fixtures for npm run check:contracts; no database or network"]
fn transport_contract_fixture() {
    let paths: Vec<String> = serde_json::from_str(
        &std::env::var("MARIO_CONTRACT_PATHS").expect("contract gate supplies HTTP paths"),
    )
    .unwrap();
    let mut policies = Vec::new();
    for path in paths {
        for method in ["GET", "POST", "PUT", "DELETE", "PATCH"] {
            policies.push(json!({
                "path": path, "method": method,
                "requiresRevision": crate::http::policy::requires_revision(
                    &method.parse().unwrap(), &path),
            }));
        }
    }
    let amounts = [
        HoldingAmounts {
            price_status: "confirmed".into(),
            market_value: "99999999999999.99".into(),
            cost_basis: "0.01".into(),
            quantity: Some("0.00000001".into()),
            cost_price: Some("1000000.00000001".into()),
            current_price: Some("123.4500".into()),
        },
        HoldingAmounts {
            price_status: "unconfirmed".into(),
            market_value: "0".into(),
            cost_basis: "0".into(),
            quantity: None,
            cost_price: None,
            current_price: None,
        },
    ];
    let mut rules = vec![WatchRules::default()];
    for reference in ["lower", "middle", "upper"] {
        for mode in ["near", "at_or_below", "at_or_above"] {
            let rule: WatchRules = serde_json::from_value(json!({
                "minYieldBps": 333,
                "daily": { "reference": reference, "mode": mode, "offsetBps": 150 },
                "weekly": { "reference": reference, "mode": mode, "offsetBps": 0 },
            }))
            .unwrap();
            rule.validate().unwrap();
            rules.push(rule);
        }
    }
    let mut trades = Vec::new();
    let mut trade_updates = Vec::new();
    for mode in ["history", "settle"] {
        for kind in ["buy", "sell", "dividend", "expire"] {
            let input: crate::models::portfolio::HoldingTradeInput =
                serde_json::from_value(json!({
                    "requestId":"contract-request", "holdingId":"contract-holding",
                    "expectedRevision":null, "newHolding":null, "kind":kind,
                    "occurredOn":"2026-01-02", "sequence":1, "quantity":"0.00000001",
                    "price":"99999999999999.99", "grossAmount":"0.01", "fees":"0", "taxes":"0",
                    "account":"isolated", "settlementMode":mode
                }))
                .unwrap();
            let mut update = serde_json::to_value(&input).unwrap();
            for key in [
                "requestId",
                "holdingId",
                "newHolding",
                "expectedCashBaseline",
            ] {
                update.as_object_mut().unwrap().remove(key);
            }
            update["expectedRevision"] = json!("revision");
            trade_updates.push(
                serde_json::from_value::<crate::models::portfolio::HoldingTradeUpdateInput>(update)
                    .unwrap(),
            );
            trades.push(input);
        }
    }
    let fx: crate::models::fx::FxBook = serde_json::from_value(json!({
        "baseCurrency":"CNY", "checkedAt":1, "requestedOn":"2026-01-02", "error":"",
        "manual":{"USD":{"exactRate":"7.0100", "rate":7.01, "observedOn":"2026-01-02"}},
        "quotes":[{"exactRate":null, "rate":7.0, "fromCurrency":"USD", "toCurrency":"CNY",
            "requestedOn":"2026-01-02", "observedOn":"2026-01-02", "stalenessDays":0,
            "providerCode":"contract", "providerName":"Contract", "sourceUrl":"", "methodologyUrl":"", "disclaimer":""}]
    })).unwrap();
    let result: crate::models::analysis::AnalysisResult = serde_json::from_value(json!({
        "id":"contract-result", "answer":"完整回复", "stages":[], "createdAt":"2026-01-02T00:00:00Z", "disclaimer":"fixture",
        "transparency":{"provider":"fixture", "model":"fixture", "contextGroups":[], "payloadBytes":0,
            "contextRevision":"fixture", "externalDataUsed":false, "apiKeySent":false}
    })).unwrap();
    let events = [
        crate::models::inference::AnalysisEvent::Status {
            message: "生成中".into(),
        },
        crate::models::inference::AnalysisEvent::Draft {
            proposal: "P1".into(),
            answer: "部分回复".into(),
        },
        crate::models::inference::AnalysisEvent::Complete {
            result: Box::new(result),
        },
        crate::models::inference::AnalysisEvent::Error {
            message: "fixture failure".into(),
        },
    ];
    let mut configs = ["openai-chat", "openai-responses", "anthropic", "codex-bridge"].map(|provider| {
        serde_json::from_value::<crate::models::connection::ModelConfig>(json!({"provider":provider,"baseUrl":"https://example.test","model":"fixture","hasApiKey":false})).unwrap()
    }).to_vec();
    configs.push(serde_json::from_value(json!({"provider":"openai-responses","baseUrl":"https://api.openai.com/v1","model":"site-model","hasApiKey":true,"source":"server","serverModel":"site-model"})).unwrap());
    let bridges = [crate::models::connection::CodexBridgeStatus::default(), serde_json::from_value(json!({
        "paired":true,"online":true,"model":"fixture","models":[{"model":"fixture","displayName":"Fixture","description":"catalog fixture","isDefault":true}]
    })).unwrap()];
    let catalogs = [crate::models::connection::ServerModelCatalog {
        models: vec!["gpt-5.4".into(), "gpt-5.4-mini".into()],
        default_model: "gpt-5.4".into(),
    }];
    let account_catalog_inputs = [
        crate::models::connection::AccountModelCatalogInput {
            provider: crate::models::provider::ModelProviderKind::OpenaiResponses,
            base_url: "https://api.openai.com/v1".into(),
            api_key: None,
        },
        crate::models::connection::AccountModelCatalogInput {
            provider: crate::models::provider::ModelProviderKind::Anthropic,
            base_url: "https://api.anthropic.com/v1".into(),
            api_key: Some("contract-key".into()),
        },
    ];
    let account_catalogs = [crate::models::connection::AccountModelCatalog {
        models: vec!["gpt-5.4".into(), "claude-sonnet-4-5".into()],
    }];
    let profiles = [
        crate::models::portfolio::FinancialProfile::default(),
        crate::models::portfolio::FinancialProfile {
            provided_fields: Some(vec![
                crate::models::portfolio::ProfileField::MonthlyIncome,
                crate::models::portfolio::ProfileField::MonthlyExpense,
                crate::models::portfolio::ProfileField::EmergencyFund,
                crate::models::portfolio::ProfileField::Liabilities,
                crate::models::portfolio::ProfileField::HorizonYears,
                crate::models::portfolio::ProfileField::MaxDrawdownPct,
            ]),
            ..Default::default()
        },
    ];
    println!(
        "MARIO_CONTRACT:{}",
        json!({ "amounts": amounts, "profiles": profiles, "rules": rules, "policies": policies, "trades":trades, "tradeUpdates":trade_updates, "fx":fx, "events":events, "configs":configs, "bridges":bridges, "catalogs":catalogs, "accountCatalogInputs":account_catalog_inputs, "accountCatalogs":account_catalogs })
    );
}
