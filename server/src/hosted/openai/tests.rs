use super::*;
#[test]
fn website_callback_rejects_duplicate_fields_and_binds_browser_cookie() {
    assert!(callback_fields("state=one&state=two&code=secret").is_err());
    assert!(callback_fields(&"x".repeat(16385)).is_err());
    assert_eq!(
        callback_fields("state=one&error=access_denied").unwrap()["state"],
        "one"
    );
    let mut headers = HeaderMap::new();
    assert!(browser_cookie(&headers).is_err());
    headers.insert(
        header::COOKIE,
        format!("mario_openai_login={}", "a".repeat(43))
            .parse()
            .unwrap(),
    );
    assert_eq!(browser_cookie(&headers).unwrap(), "a".repeat(43));
    headers.append(
        header::COOKIE,
        format!("mario_openai_login={}", "b".repeat(43))
            .parse()
            .unwrap(),
    );
    assert!(browser_cookie(&headers).is_err());
}
