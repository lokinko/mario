use super::*;
use crate::accounts::{tests::accounts, Credentials};
fn identity<'a>(subject: &'a str, email: &'a str) -> OpenAiIdentity<'a> {
    OpenAiIdentity {
        issuer: "https://auth.openai.com",
        client: "oaiapp_test",
        subject,
        email,
        email_verified: true,
    }
}
#[test]
fn openai_additive_upgrade_preserves_existing_accounts_and_sessions() {
    let a = accounts();
    let session = a
        .register(&Credentials {
            email: "old@example.test".into(),
            password: "long-password-1234".into(),
        })
        .unwrap();
    let url = a.url.clone();
    let schema = a.schema.clone();
    a.conn()
        .unwrap()
        .execute_batch("DROP TABLE openai_login_attempts; DROP TABLE external_identities; DROP TABLE openai_login_results")
        .unwrap();
    drop(a);
    for _ in 0..2 {
        let reopened = Accounts::open_schema(url.clone(), schema.clone()).unwrap();
        assert_eq!(
            reopened.authenticate(&session.token).unwrap(),
            session.user_id
        );
    }
}
#[test]
fn openai_handoff_is_browser_bound_one_use_and_expires() {
    let a = accounts();
    let session = a
        .finish_openai(identity("one", "one@example.test"), None)
        .unwrap();
    let user = session.as_ref().unwrap().user_id.clone();
    let token = a.stage_openai_result("browser", session).unwrap();
    assert!(a.take_openai_result(&token, "wrong-browser").is_err());
    let result = a.take_openai_result(&token, "browser").unwrap().unwrap();
    assert_eq!(result.user_id, user);
    assert_eq!(a.authenticate(&result.token).unwrap(), user);
    assert!(a.take_openai_result(&token, "browser").is_err());
    let token = a.stage_openai_result("browser", None).unwrap();
    assert!(a.take_openai_result(&token, "browser").unwrap().is_none());
    let token = a.stage_openai_result("browser", None).unwrap();
    a.conn()
        .unwrap()
        .execute("UPDATE openai_login_results SET expires_at=0", [])
        .unwrap();
    assert!(a.take_openai_result(&token, "browser").is_err());
}
#[test]
fn openai_accounts_create_once_remain_isolated_and_require_explicit_email_linking() {
    let a = accounts();
    let first = a
        .finish_openai(identity("one", "one@example.test"), None)
        .unwrap()
        .unwrap();
    let again = a
        .finish_openai(identity("one", "changed@example.test"), None)
        .unwrap()
        .unwrap();
    assert_eq!(first.user_id, again.user_id);
    let second = a
        .finish_openai(identity("two", "two@example.test"), None)
        .unwrap()
        .unwrap();
    assert_ne!(first.user_id, second.user_id);
    assert!(matches!(
        a.finish_openai(identity("unlinked", "one@example.test"), None),
        Err(AppError::Conflict(_))
    ));
    let mut unverified = identity("unverified", "new@example.test");
    unverified.email_verified = false;
    assert!(a.finish_openai(unverified, None).is_err());
    assert!(a
        .finish_openai(
            identity("unlinked", "one@example.test"),
            Some(&digest(&first.token))
        )
        .unwrap()
        .is_none());
    assert!(matches!(
        a.finish_openai(
            identity("unlinked", "one@example.test"),
            Some(&digest(&second.token))
        ),
        Err(AppError::Conflict(_))
    ));
    a.logout(&first.token).unwrap();
    assert!(a
        .finish_openai(
            identity("other", "one@example.test"),
            Some(&digest(&first.token))
        )
        .is_err());
    assert_eq!(a.authenticate(&again.token).unwrap(), first.user_id);
}
#[test]
fn openai_transactions_require_browser_client_callback_and_single_atomic_use() {
    let a = accounts();
    a.begin_openai(OpenAiAttempt {
        state: "state",
        browser: "browser",
        verifier: "verifier",
        nonce: "nonce",
        client: "oaiapp_test",
        redirect: "callback",
        link_token: None,
    })
    .unwrap();
    for (browser, client, redirect) in [
        ("wrong", "oaiapp_test", "callback"),
        ("browser", "wrong", "callback"),
        ("browser", "oaiapp_test", "wrong"),
    ] {
        assert!(a
            .consume_openai("state", browser, client, redirect)
            .is_err());
    }
    let transaction = a
        .consume_openai("state", "browser", "oaiapp_test", "callback")
        .unwrap();
    assert_eq!(transaction.verifier, "verifier");
    assert_eq!(transaction.nonce, "nonce");
    assert!(a
        .consume_openai("state", "browser", "oaiapp_test", "callback")
        .is_err());
    a.begin_openai(OpenAiAttempt {
        state: "expired",
        browser: "browser",
        verifier: "verifier",
        nonce: "nonce",
        client: "oaiapp_test",
        redirect: "callback",
        link_token: None,
    })
    .unwrap();
    a.conn()
        .unwrap()
        .execute("UPDATE openai_login_attempts SET expires_at=0", [])
        .unwrap();
    assert!(a
        .consume_openai("expired", "browser", "oaiapp_test", "callback")
        .is_err());
}
#[test]
fn openai_linking_never_replaces_existing_password_or_account_data() {
    let a = accounts();
    let credentials = Credentials {
        email: "existing@example.test".into(),
        password: "long-password-1234".into(),
    };
    let original = a.register(&credentials).unwrap();
    assert!(matches!(
        a.finish_openai(identity("external", &credentials.email), None),
        Err(AppError::Conflict(_))
    ));
    a.finish_openai(
        identity("external", &credentials.email),
        Some(&digest(&original.token)),
    )
    .unwrap();
    assert_eq!(a.login(&credentials).unwrap().user_id, original.user_id);
    assert_eq!(
        a.finish_openai(identity("external", &credentials.email), None)
            .unwrap()
            .unwrap()
            .user_id,
        original.user_id
    );
}
