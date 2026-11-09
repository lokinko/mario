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
    println!(
        "MARIO_CONTRACT:{}",
        json!({ "amounts": amounts, "rules": rules, "policies": policies })
    );
}
