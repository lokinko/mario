use super::*;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};

fn keys() -> JwkSet {
    let key: Value = serde_json::from_slice(include_bytes!(
        "../chatgpt_auth/fixtures/test-only-public.json"
    ))
    .unwrap();
    serde_json::from_value(json!({"keys":[key]})).unwrap()
}
fn claims() -> Value {
    json!({"iss":ISSUER,"aud":"oaiapp_test","sub":"subject","email":"user@example.test","email_verified":true,
        "iat":chrono::Utc::now().timestamp(),"exp":chrono::Utc::now().timestamp()+3600,"nonce":"nonce"})
}
fn signed(value: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("mario-test-key".into());
    encode(
        &header,
        value,
        &EncodingKey::from_rsa_pem(include_bytes!(
            "../chatgpt_auth/fixtures/test-only-private.pem"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn openai_website_config_requires_registered_clients_and_safe_exact_callback() {
    let redirect = "https://example.test/mario/api/auth/openai/callback";
    let c = Config::parse("oaiapp_test".into(), redirect.into(), "none", None).unwrap();
    assert_eq!(c.app_url, "https://example.test/mario/");
    assert_eq!(c.cookie_path(), "/mario/");
    assert!(c
        .cookie("mario_openai_login", "test", 600)
        .contains("HttpOnly; SameSite=Lax; Max-Age=600; Secure"));
    for url in [
        "http://example.test/api/auth/openai/callback",
        "https://user:secret@example.test/api/auth/openai/callback",
        "https://example.test/wrong",
        "https://example.test/api/auth/openai/callback?x=1",
    ] {
        assert!(Config::parse("oaiapp_test".into(), url.into(), "none", None).is_err());
    }
    assert!(Config::parse("dynamic_agent_client".into(), redirect.into(), "none", None).is_err());
    assert!(Config::parse(
        "oaiapp_test".into(),
        redirect.into(),
        "none",
        Some("secret".into())
    )
    .is_err());
    assert!(Config::parse(
        "oaiapp_test".into(),
        redirect.into(),
        "client_secret_basic",
        None
    )
    .is_err());
    assert!(Config::parse(
        "oaiapp_test".into(),
        redirect.into(),
        "client_secret_basic",
        Some("secret".into())
    )
    .is_ok());
    assert!(official_endpoint(
        "https://auth.openai.com/.well-known/jwks.json"
    ));
    for endpoint in [
        "https://auth.openai.com.evil.test/keys",
        "http://auth.openai.com/keys",
        "https://auth.openai.com:8443/keys",
        "https://secret@auth.openai.com/keys",
    ] {
        assert!(!official_endpoint(endpoint));
    }
}
#[test]
fn openai_website_identity_binds_signature_issuer_nonce_audience_and_time() {
    let good = claims();
    let identity = verify_identity(&signed(&good), "oaiapp_test", "nonce", &keys()).unwrap();
    assert_eq!(identity.sub, "subject");
    assert!(identity.email_verified);
    for (field, value) in [
        ("iss", json!("https://attacker.test")),
        ("aud", json!("other")),
        ("nonce", json!("other")),
        ("sub", json!("")),
        ("exp", json!(0)),
        ("iat", json!(chrono::Utc::now().timestamp() + 600)),
        ("azp", json!("other")),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(
            verify_identity(&signed(&bad), "oaiapp_test", "nonce", &keys()).is_err(),
            "accepted {field}"
        );
    }
    let mut multiple = good.clone();
    multiple["aud"] = json!(["oaiapp_test", "other"]);
    assert!(verify_identity(&signed(&multiple), "oaiapp_test", "nonce", &keys()).is_err());
    multiple["azp"] = json!("oaiapp_test");
    assert!(verify_identity(&signed(&multiple), "oaiapp_test", "nonce", &keys()).is_ok());
    let unsigned = encode(
        &Header::new(Algorithm::HS256),
        &good,
        &EncodingKey::from_secret(b"attacker"),
    )
    .unwrap();
    assert!(verify_identity(&unsigned, "oaiapp_test", "nonce", &keys()).is_err());
    for field in ["iat", "exp", "iss", "aud", "nonce", "sub"] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(verify_identity(&signed(&missing), "oaiapp_test", "nonce", &keys()).is_err());
    }
}
