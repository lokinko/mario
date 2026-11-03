//! Test-only RSA keys and loopback HTTP; never contacts OpenAI or real accounts.
use super::{
    callback, now, verify_identity, Attempt, Callback, Pending, Server, ISSUER, URL_SAFE_NO_PAD,
};
use axum::{
    http::{header, StatusCode},
    routing::get,
    Router,
};
use base64::Engine;
use jsonwebtoken::{encode, jwk::JwkSet, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::oneshot;

const CLIENT: &str = "oaiapp_test_registration";
const NONCE: &str = "test-fresh-nonce";
const STATE: &str = "test-fresh-state";
const SECRET_CODE: &str = "do-not-reflect-authorization-code";
const SECRET_TOKEN: &str = "do-not-reflect-access-token";

fn test_keys() -> JwkSet {
    let key: Value =
        serde_json::from_slice(include_bytes!("../fixtures/test-only-public.json")).unwrap();
    serde_json::from_value(json!({"keys":[key]})).unwrap()
}

fn claims() -> Value {
    json!({
        "iss":ISSUER,
        "aud":CLIENT,
        "sub":"test-account-subject",
        "email":"test@example.invalid",
        "exp":now() + 3600,
        "nonce":NONCE
    })
}

fn signed(claims: &Value, kid: Option<&str>) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = kid.map(String::from);
    let key = EncodingKey::from_rsa_pem(include_bytes!("../fixtures/test-only-private.pem"))
        .expect("the committed key is a test-only RSA fixture, never a real credential");
    encode(&header, claims, &key).unwrap()
}

#[test]
fn valid_rs256_identity_accepts_the_matching_public_key_and_registration() {
    let token = signed(&claims(), Some("mario-test-key"));
    let identity = verify_identity(&token, CLIENT, Some(NONCE), &test_keys()).unwrap();
    assert_eq!(identity.sub, "test-account-subject");
    assert_eq!(identity.email, "test@example.invalid");
    assert_eq!(identity.nonce.as_deref(), Some(NONCE));
}

#[test]
fn signed_identities_reject_wrong_audience_issuer_expiry_and_nonce() {
    let keys = test_keys();
    for (field, invalid) in [
        ("aud", json!("another-registration")),
        ("iss", json!("https://untrusted.example.invalid")),
        ("exp", json!(now().saturating_sub(60))),
        ("nonce", json!("nonce-from-another-attempt")),
        ("sub", json!("")),
    ] {
        let mut value = claims();
        value[field] = invalid;
        let token = signed(&value, Some("mario-test-key"));
        let result = verify_identity(&token, CLIENT, Some(NONCE), &keys);
        assert!(result.is_err(), "accepted invalid signed claim: {field}");
    }
    let valid = signed(&claims(), Some("mario-test-key"));
    assert!(verify_identity(&valid, "another-registration", Some(NONCE), &keys).is_err());
    assert!(verify_identity(&valid, CLIENT, Some("another-nonce"), &keys).is_err());
}

#[test]
fn signed_identities_require_spec_claims_and_a_known_key_id() {
    let keys = test_keys();
    for field in ["aud", "iss", "exp", "sub"] {
        let mut value = claims();
        value.as_object_mut().unwrap().remove(field);
        let token = signed(&value, Some("mario-test-key"));
        assert!(
            verify_identity(&token, CLIENT, Some(NONCE), &keys).is_err(),
            "accepted missing claim: {field}"
        );
    }
    for kid in [None, Some("another-key")] {
        let token = signed(&claims(), kid);
        assert!(verify_identity(&token, CLIENT, Some(NONCE), &keys).is_err());
    }
}

#[test]
fn tampered_rs256_signature_and_payload_do_not_verify() {
    let valid = signed(&claims(), Some("mario-test-key"));
    let signature_start = valid.rfind('.').unwrap() + 1;
    let mut tampered = valid.clone().into_bytes();
    tampered[signature_start] = if tampered[signature_start] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let tampered = String::from_utf8(tampered).unwrap();
    assert!(verify_identity(&tampered, CLIENT, Some(NONCE), &test_keys()).is_err());

    let parts = valid.split('.').collect::<Vec<_>>();
    let mut modified = claims();
    modified["sub"] = json!("tampered-account");
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&modified).unwrap());
    let tampered = format!("{}.{payload}.{}", parts[0], parts[2]);
    assert!(verify_identity(&tampered, CLIENT, Some(NONCE), &test_keys()).is_err());
}

#[test]
fn refresh_identity_may_omit_nonce_but_login_still_requires_it() {
    let mut value = claims();
    value.as_object_mut().unwrap().remove("nonce");
    let token = signed(&value, Some("mario-test-key"));
    let identity = verify_identity(&token, CLIENT, None, &test_keys()).unwrap();
    assert_eq!(identity.sub, "test-account-subject");
    assert!(identity.nonce.is_none());
    assert!(verify_identity(&token, CLIENT, Some(NONCE), &test_keys()).is_err());
    assert!(verify_identity(&token, "another-registration", None, &test_keys()).is_err());
}

async fn local_callback() -> (String, Arc<Pending>, oneshot::Receiver<Callback>, Server) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = oneshot::channel();
    let pending = Arc::new(Pending {
        attempt: Attempt {
            state: STATE.into(),
            nonce: NONCE.into(),
            verifier: "test-pkce-verifier".into(),
            redirect: format!("{base}/auth/callback"),
            existing: None,
        },
        sender: Mutex::new(Some(sender)),
    });
    let router = Router::new()
        .route("/auth/callback", get(callback))
        .with_state(pending.clone());
    let server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (base, pending, receiver, server)
}

fn local_client() -> reqwest::Client {
    // Do not allow environment proxies or redirects to take these test requests
    // anywhere except the ephemeral loopback listener constructed above.
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

fn assert_still_pending(pending: &Pending, receiver: &mut oneshot::Receiver<Callback>) {
    assert!(pending.sender.lock().unwrap().is_some());
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

async fn assert_private_response(response: reqwest::Response, expected: StatusCode) -> String {
    assert_eq!(response.status(), expected);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let csp = response.headers()["content-security-policy"]
        .to_str()
        .unwrap();
    assert!(csp.contains("default-src 'none'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    let body = response.text().await.unwrap();
    for secret in [SECRET_CODE, SECRET_TOKEN, STATE, CLIENT] {
        assert!(!body.contains(secret), "callback reflected sensitive input");
    }
    body
}

#[tokio::test]
async fn callback_http_rejects_untrusted_attempts_without_consuming_then_accepts_once() {
    let (base, pending, mut receiver, _server) = local_callback().await;
    let client = local_client();
    let valid = format!(
        "{base}/auth/callback?state={STATE}&client_id={CLIENT}&code={SECRET_CODE}&access_token={SECRET_TOKEN}"
    );
    for host in [
        "attacker.example.invalid".to_owned(),
        base.replace("http://127.0.0.1", "localhost"),
    ] {
        let response = client
            .get(&valid)
            .header(header::HOST, host)
            .send()
            .await
            .unwrap();
        assert_private_response(response, StatusCode::BAD_REQUEST).await;
        assert_still_pending(&pending, &mut receiver);
    }
    for query in [
        format!("state=wrong&client_id={CLIENT}&code={SECRET_CODE}"),
        format!("state={STATE}&state={STATE}&client_id={CLIENT}&code={SECRET_CODE}"),
        format!("state={STATE}&client_id={CLIENT}&code={SECRET_CODE}&code=duplicate"),
        format!(
            "state={STATE}&client_id={CLIENT}&code={SECRET_CODE}&padding={}",
            "x".repeat(17 * 1024)
        ),
    ] {
        let response = client
            .get(format!("{base}/auth/callback?{query}"))
            .send()
            .await
            .unwrap();
        assert_private_response(response, StatusCode::BAD_REQUEST).await;
        assert_still_pending(&pending, &mut receiver);
    }

    let response = client.get(&valid).send().await.unwrap();
    assert_private_response(response, StatusCode::OK).await;
    let callback = tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .unwrap()
        .unwrap();
    match callback {
        Callback::Code { code, client_id } => {
            assert_eq!(code, SECRET_CODE);
            assert_eq!(client_id, CLIENT);
        }
        Callback::Denied => panic!("a successful authorization callback became a denial"),
    }
    assert!(pending.sender.lock().unwrap().is_none());
    let response = client.get(&valid).send().await.unwrap();
    assert_private_response(response, StatusCode::BAD_REQUEST).await;
}

#[tokio::test]
async fn callback_http_access_denied_requires_matching_state_and_cancels_only_once() {
    let (base, pending, mut receiver, _server) = local_callback().await;
    let client = local_client();
    let response = client
        .get(format!(
            "{base}/auth/callback?error=access_denied&state=wrong&error_description={SECRET_TOKEN}"
        ))
        .send()
        .await
        .unwrap();
    assert_private_response(response, StatusCode::BAD_REQUEST).await;
    assert_still_pending(&pending, &mut receiver);

    let denial = format!(
        "{base}/auth/callback?error=access_denied&state={STATE}&error_description={SECRET_TOKEN}"
    );
    let response = client.get(&denial).send().await.unwrap();
    let body = assert_private_response(response, StatusCode::OK).await;
    assert!(body.contains("已取消"));
    let callback = tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(callback, Callback::Denied));
    assert!(pending.sender.lock().unwrap().is_none());
    let response = client.get(&denial).send().await.unwrap();
    assert_private_response(response, StatusCode::BAD_REQUEST).await;
}
