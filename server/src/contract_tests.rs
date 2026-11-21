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
    let daily_review_settings = [
        crate::models::daily_review::DailyReviewSettings::default(),
        crate::models::daily_review::DailyReviewSettings {
            enabled: false,
            schedule_time: "08:45".into(),
            scope: crate::models::daily_review::DailyReviewScope::HoldingsOnly,
            web_search: false,
        },
    ];
    let daily_review_run = crate::models::daily_review::DailyReviewRun {
        id: "run-contract".into(),
        review_day: "2026-11-20".into(),
        trigger: crate::models::daily_review::DailyReviewTrigger::Manual,
        status: crate::models::daily_review::DailyReviewRunStatus::Degraded,
        ai_status: crate::models::daily_review::DailyReviewAiStatus::SkippedNoModel,
        ai_model: None,
        degraded_reason: Some("未配置可用模型".into()),
        item_count: 1,
        announcement_count: 0,
        created_at: "2026-11-21T00:00:00+00:00".into(),
    };
    let daily_review_feed = crate::models::daily_review::DailyReviewFeedResponse {
        settings: daily_review_settings[0].clone(),
        timezone: "Asia/Shanghai".into(),
        latest_review_day: Some("2026-11-20".into()),
        unread_count: 1,
        runs: vec![daily_review_run.clone()],
        items: vec![crate::models::daily_review::DailyReviewFeedItem {
            id: "item-contract".into(),
            run_id: "run-contract".into(),
            review_day: "2026-11-20".into(),
            kind: crate::models::daily_review::DailyReviewItemKind::PortfolioSummary,
            origin: crate::models::daily_review::DailyReviewItemOrigin::DeterministicFact,
            importance: crate::models::daily_review::DailyReviewImportance::High,
            title: "契约测试复盘摘要".into(),
            summary: "测试摘要".into(),
            body: "测试正文".into(),
            symbol: Some("600036.SS".into()),
            asset_name: Some("招商银行".into()),
            metrics: vec![crate::models::daily_review::DailyReviewMetric {
                label: "日变动".into(),
                value: "+1.20%".into(),
                tone: crate::models::daily_review::MetricTone::Positive,
                sensitive: true,
            }],
            sources: vec![crate::models::daily_review::DailyReviewSource {
                label: "巨潮公告".into(),
                url: "https://static.cninfo.com.cn/finalpage/2026-11-20/test.PDF".into(),
                publisher: "巨潮资讯网".into(),
                published_on: "2026-11-20".into(),
            }],
            tags: vec!["总览".into()],
            ask_prompt: "请分析昨日变化".into(),
            read_at: None,
            created_at: "2026-11-21T00:00:00+00:00".into(),
        }],
        has_more: false,
        next_before: None,
    };
    let daily_review_run_result = crate::models::daily_review::DailyReviewRunResult {
        run: daily_review_run,
        feed: daily_review_feed.clone(),
    };
    let daily_review_read_result = crate::models::daily_review::DailyReviewReadResult {
        ok: true,
        unread_count: 0,
    };
    println!(
        "MARIO_CONTRACT:{}",
        json!({ "amounts": amounts, "profiles": profiles, "rules": rules, "policies": policies, "trades":trades, "tradeUpdates":trade_updates, "fx":fx, "events":events, "configs":configs, "bridges":bridges, "catalogs":catalogs, "accountCatalogInputs":account_catalog_inputs, "accountCatalogs":account_catalogs, "dailyReviewSettings": daily_review_settings, "dailyReviewFeed": daily_review_feed, "dailyReviewRunResult": daily_review_run_result, "dailyReviewReadResult": daily_review_read_result })
    );
}
