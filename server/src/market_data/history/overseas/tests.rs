use super::*;
use serde_json::json;

fn us(symbol: &str, exchange: &str, market: u16, instrument: &str) -> Value {
    json!({"Code":symbol,"Classify":"UsStock","JYS":exchange,"MktNum":market.to_string(),"QuoteID":format!("{market}.{symbol}"),"TypeUS":instrument})
}
fn resolved(symbol: &str) -> Resolved {
    resolve(
        &json!({"QuotationCodeTable":{"Data":[us(symbol,"NASDAQ",105,"1")]}}),
        symbol,
        "https://searchapi.eastmoney.com/api/suggest/get".into(),
    )
    .unwrap()
}
fn action() -> Value {
    json!({"SECUCODE":"AAPL.O","SECURITY_CODE":"AAPL","PLAN_PROCESS":"实施","EX_DIVIDEND_DATE":"2020-08-31 00:00:00","SPLIT_RATIO":"1:4"})
}

#[test]
fn lookup_uses_exact_market_identity_and_never_the_first_fuzzy_result() {
    let payload = json!({"QuotationCodeTable":{"Data":[us("QQQI","NASDAQ",105,"5"),us("QQQ","NASDAQ",105,"5")]}});
    let r = resolve(&payload, "QQQ", String::new()).unwrap();
    assert_eq!(r.identity.secid(), "105.QQQ");
    assert_eq!(r.instrument_type, "ETF");
    let hk = json!({"Code":"00700","Classify":"HK","JYS":"HK","MktNum":"116","QuoteID":"116.00700","TypeUS":"3","SecurityType":"19"});
    assert_eq!(
        resolve(
            &json!({"QuotationCodeTable":{"Data":[hk.clone()]}}),
            "0700.HK",
            String::new()
        )
        .unwrap()
        .identity
        .currency,
        "HKD"
    );
    for rows in [
        json!([us("QQQI", "NASDAQ", 105, "5")]),
        json!([us("QQQ", "NASDAQ", 106, "5")]),
        json!([us("QQQ", "NASDAQ", 105, "5"), us("QQQ", "NASDAQ", 105, "5")]),
        json!([us("QQQ", "LSE", 105, "5")]),
    ] {
        assert!(resolve(
            &json!({"QuotationCodeTable":{"Data":rows}}),
            "QQQ",
            String::new()
        )
        .is_err());
    }
    for symbol in ["80700.HK", "9096.HK", "BRK-B", "VOD.L", "EURUSD=X"] {
        assert!(resolve(
            &json!({"QuotationCodeTable":{"Data":[hk.clone()]}}),
            symbol,
            String::new()
        )
        .is_err());
    }
    let nyse = resolve(
        &json!({"QuotationCodeTable":{"Data":[us("KO","NYSE",106,"1")]}}),
        "KO",
        String::new(),
    )
    .unwrap();
    assert_eq!(nyse.identity.secid(), "106.KO");
    let amex = resolve(
        &json!({"QuotationCodeTable":{"Data":[us("VOO","AMEX",107,"5")]}}),
        "VOO",
        String::new(),
    )
    .unwrap();
    assert_eq!(amex.action_code(), "VOO.A");
}

#[test]
fn split_rows_keep_full_history_and_do_not_apply_proposals_or_reverse_ratios() {
    let r = resolved("AAPL");
    let day = "2026-11-10".parse().unwrap();
    let mut proposal = action();
    proposal["PLAN_PROCESS"] = "预案".into();
    let mut future = action();
    future["EX_DIVIDEND_DATE"] = "2027-01-01".into();
    let splits = action_rows(&[action(), proposal, future], &r, day).unwrap();
    assert_eq!(splits.len(), 1);
    assert_eq!(
        (
            &*splits[0].effective_on,
            &*splits[0].numerator,
            &*splits[0].denominator
        ),
        ("2020-08-31", "4", "1")
    );
    let mut reverse = action();
    reverse["SPLIT_RATIO"] = "10:1".into();
    let splits = action_rows(&[reverse], &r, day).unwrap();
    assert_eq!(
        (&*splits[0].numerator, &*splits[0].denominator),
        ("1", "10")
    );
    for (key, value) in [
        ("SECUCODE", "AAPL.N"),
        ("SECURITY_CODE", "MSFT"),
        ("PLAN_PROCESS", "未知"),
        ("EX_DIVIDEND_DATE", "bad"),
        ("SPLIT_RATIO", "1:0"),
    ] {
        let mut wrong = action();
        wrong[key] = value.into();
        assert!(action_rows(&[wrong], &r, day).is_err());
    }
    let mut conflicting = action();
    conflicting["SPLIT_RATIO"] = "1:7".into();
    assert!(action_rows(&[action(), conflicting], &r, day).is_err());
}

#[test]
fn empty_report_requires_other_evidence_and_supplement_is_bound_to_the_same_security() {
    assert!(page(&json!({"code":9201,"success":false,"result":null}))
        .unwrap()
        .is_none());
    assert!(page(&json!({"code":9501,"success":false,"result":null})).is_err());
    let r = Resolved {
        instrument_type: "ETF",
        ..resolved("QQQ")
    };
    let payload = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"QQQ","currency":"USD","instrumentType":"ETF","exchangeTimezoneName":"America/New_York"},"events":{"splits":{"one":{"date":953562600,"numerator":2,"denominator":1}}}}]}});
    let splits = supplemental(&payload, "QQQ", &r).unwrap();
    assert_eq!(splits[0].effective_on, "2000-03-20");
    assert_eq!(splits[0].numerator, "2");
    for (key, value) in [
        ("symbol", "QQQI"),
        ("currency", "HKD"),
        ("instrumentType", "EQUITY"),
        ("exchangeTimezoneName", "Asia/Shanghai"),
    ] {
        let mut wrong = payload.clone();
        wrong["chart"]["result"][0]["meta"][key] = value.into();
        assert!(supplemental(&wrong, "QQQ", &r).is_err());
    }
    let mut wrong = payload;
    wrong["chart"]["result"][0]["events"]["splits"] = json!([]);
    assert!(supplemental(&wrong, "QQQ", &r).is_err());
}

#[tokio::test]
async fn split_pagination_requires_all_pages_and_a_stable_total() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    for mode in [
        "complete",
        "missing_page",
        "changed_total",
        "wrong_security",
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let router=axum::Router::new().route("/",axum::routing::get(move ||{
            let number=count.fetch_add(1,Ordering::SeqCst);
            async move {
                let payload=if number==0 {json!({"code":0,"success":true,"result":{"pages":2,"count":101,"data":vec![action();100]}})}
                else if mode=="missing_page" {json!({"code":9201,"success":false,"result":null})}
                else {let mut row=action();if mode=="wrong_security"{row["SECUCODE"]="MSFT.O".into();}json!({"code":0,"success":true,"result":{"pages":2,"count":if mode=="changed_total"{102}else{101},"data":[row]}})};
                axum::Json(payload)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let result = PublicMarketSource::new()
            .unwrap()
            .overseas_actions(&resolved("AAPL"), &format!("http://{address}/"))
            .await;
        if mode == "complete" {
            let (splits, urls) = result.unwrap();
            assert_eq!(splits.unwrap().len(), 1);
            assert_eq!(urls.len(), 2);
        } else {
            assert!(result.is_err(), "{mode}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        server.abort();
    }
}
