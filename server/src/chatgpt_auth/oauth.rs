use super::{
    bounded_json, error, now,
    store::{Account, Tokens},
    token_request, AppResult, AUTHORIZE, ISSUER, RESOURCE, SCOPES,
};
use axum::{
    extract::{RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::oneshot;

#[derive(Deserialize)]
pub(super) struct Discovery {
    pub jwks_uri: String,
    pub revocation_endpoint: String,
    issuer: String,
}
pub(super) async fn discovery(client: &reqwest::Client) -> AppResult<Discovery> {
    let response = client
        .get(format!("{ISSUER}/.well-known/openid-configuration"))
        .send()
        .await
        .map_err(|_| error("读取 ChatGPT 身份配置失败"))?;
    if !response.status().is_success() {
        return Err(error("ChatGPT 身份配置不可用"));
    }
    let doc: Discovery = serde_json::from_value(bounded_json(response).await?)
        .map_err(|_| error("ChatGPT 身份配置无效"))?;
    if doc.issuer != ISSUER {
        return Err(error("ChatGPT 身份签发方不匹配"));
    }
    for endpoint in [&doc.jwks_uri, &doc.revocation_endpoint] {
        let url = reqwest::Url::parse(endpoint).map_err(|_| error("ChatGPT 身份端点无效"))?;
        if url.scheme() != "https"
            || url.host_str() != Some("auth.openai.com")
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(error("ChatGPT 身份端点不属于官方授权域名"));
        }
    }
    Ok(doc)
}
#[derive(Clone, Deserialize)]
pub(super) struct Identity {
    pub sub: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    nonce: Option<String>,
    exp: u64,
}
pub(super) async fn identity(
    client: &reqwest::Client,
    token: &str,
    audience: &str,
    nonce: Option<&str>,
) -> AppResult<Identity> {
    let keys = verification_keys(client).await?;
    verify_identity(token, audience, nonce, &keys)
}
pub(super) async fn verification_keys(client: &reqwest::Client) -> AppResult<JwkSet> {
    let discovery = discovery(client).await?;
    let response = client
        .get(discovery.jwks_uri)
        .send()
        .await
        .map_err(|_| error("读取 ChatGPT 签名公钥失败"))?;
    if !response.status().is_success() {
        return Err(error("ChatGPT 签名公钥不可用"));
    }
    let keys: JwkSet = serde_json::from_value(bounded_json(response).await?)
        .map_err(|_| error("ChatGPT 签名公钥格式无效"))?;
    Ok(keys)
}
pub(super) fn verify_identity(
    token: &str,
    audience: &str,
    nonce: Option<&str>,
    keys: &JwkSet,
) -> AppResult<Identity> {
    let header = decode_header(token).map_err(|_| error("ChatGPT 身份令牌格式无效"))?;
    if header.alg != Algorithm::RS256 {
        return Err(error("ChatGPT 身份签名算法不允许"));
    }
    let key = header
        .kid
        .as_ref()
        .and_then(|id| keys.find(id))
        .ok_or_else(|| error("ChatGPT 身份签名公钥未匹配"))?;
    let key = DecodingKey::from_jwk(key).map_err(|_| error("ChatGPT 身份签名公钥无效"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.leeway = 0;
    let identity = decode::<Identity>(token, &key, &validation)
        .map_err(|_| error("ChatGPT 身份签名、受众或有效期校验失败"))?
        .claims;
    if identity.sub.is_empty()
        || identity.exp <= now()
        || nonce.is_some_and(|expected| identity.nonce.as_deref() != Some(expected))
    {
        return Err(error("ChatGPT 身份或登录 nonce 校验失败"));
    }
    Ok(identity)
}
fn random() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
struct Attempt {
    state: String,
    nonce: String,
    verifier: String,
    redirect: String,
    existing: Option<Account>,
}
impl Attempt {
    fn url(&self, host_id: &str) -> AppResult<reqwest::Url> {
        let mut url = reqwest::Url::parse(AUTHORIZE).map_err(|_| error("授权地址无效"))?;
        let mut query = url.query_pairs_mut();
        query.extend_pairs([
            (
                "client_id",
                self.existing
                    .as_ref()
                    .map_or("dynamic_agent_client", |a| a.client_id.as_str()),
            ),
            ("ext_agent_host_id", host_id),
            ("response_type", "code"),
            ("redirect_uri", &self.redirect),
            ("scope", SCOPES),
            ("resource", RESOURCE),
            ("state", &self.state),
            ("nonce", &self.nonce),
            ("code_challenge_method", "S256"),
            (
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes())),
            ),
        ]);
        if let Some(account) = &self.existing {
            if let Some(tokens) = &account.tokens {
                query.append_pair("id_token_hint", &tokens.id_token);
                // Explicit reauthorization of a plan-disabled registration;
                // ordinary sign-ins do not force a new consent screen.
                if !super::plan_enabled(tokens) {
                    query.append_pair("prompt", "consent");
                }
            }
            if !account.email.is_empty() {
                query.append_pair("login_hint", &account.email);
            }
        } else {
            query.append_pair("agent_name_hint", "mario");
        }
        drop(query);
        Ok(url)
    }
    fn callback(&self, raw: &str) -> AppResult<Callback> {
        let mut fields = HashMap::new();
        for (key, value) in reqwest::Url::parse(&format!("http://127.0.0.1/?{raw}"))
            .map_err(|_| error("OAuth 回调无效"))?
            .query_pairs()
        {
            if fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(error("OAuth 回调包含重复字段"));
            }
        }
        if fields.get("state") != Some(&self.state) {
            return Err(error("OAuth state 不匹配"));
        }
        if fields.contains_key("error") {
            return Ok(Callback::Denied);
        }
        let code = fields
            .remove("code")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| error("OAuth 回调缺少授权码"))?;
        let client_id = match (&self.existing, fields.remove("client_id")) {
            (Some(account), Some(id)) if id == account.client_id => id,
            (Some(account), None) => account.client_id.clone(),
            (None, Some(id)) if !id.is_empty() && id != "dynamic_agent_client" => id,
            _ => return Err(error("OAuth 回调注册 ID 不匹配或缺失")),
        };
        Ok(Callback::Code { code, client_id })
    }
}
enum Callback {
    Denied,
    Code { code: String, client_id: String },
}
struct Pending {
    attempt: Attempt,
    sender: Mutex<Option<oneshot::Sender<Callback>>>,
}
async fn callback(
    State(pending): State<Arc<Pending>>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> impl IntoResponse {
    let expected = reqwest::Url::parse(&pending.attempt.redirect)
        .ok()
        .and_then(|u| u.port())
        .map(|p| format!("127.0.0.1:{p}"));
    let result = if headers.get(header::HOST).and_then(|v| v.to_str().ok()) != expected.as_deref()
        || raw.as_ref().is_none_or(|q| q.len() > 16 * 1024)
    {
        Err(error("OAuth 回调来源无效"))
    } else {
        pending.attempt.callback(raw.as_deref().unwrap_or_default())
    };
    let (status, message) = match result {
        Ok(result) => {
            if let Some(sender) = pending.sender.lock().unwrap().take() {
                let denied = matches!(result, Callback::Denied);
                let _ = sender.send(result);
                (
                    StatusCode::OK,
                    if denied {
                        "已取消 ChatGPT 授权，可以关闭此页面。"
                    } else {
                        "ChatGPT 授权已返回，请回到 Mario 终端查看最终身份校验结果。可以关闭此页面。"
                    },
                )
            } else {
                (StatusCode::BAD_REQUEST, "此授权回调已处理。")
            }
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            "授权回调校验失败，请回到原浏览器授权页面重试。",
        ),
    };
    (
        status,
        [
            ("Cache-Control", "no-store"),
            ("Referrer-Policy", "no-referrer"),
            (
                "Content-Security-Policy",
                "default-src 'none'; frame-ancestors 'none'",
            ),
        ],
        message,
    )
}
struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(super) struct Authorized {
    pub tokens: Tokens,
    pub identity: Identity,
    pub client_id: String,
}
pub(super) async fn authorize(
    client: &reqwest::Client,
    host_id: &str,
    existing: Option<&Account>,
) -> AppResult<Authorized> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let redirect = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener.local_addr()?.port()
    );
    let attempt = Attempt {
        state: random(),
        nonce: random(),
        verifier: random(),
        redirect,
        existing: existing.cloned(),
    };
    let url = attempt.url(host_id)?;
    let (sender, receiver) = oneshot::channel();
    let pending = Arc::new(Pending {
        attempt,
        sender: Mutex::new(Some(sender)),
    });
    let router = Router::new()
        .route("/auth/callback", get(callback))
        .with_state(pending.clone());
    let _server = Server(tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    }));
    eprintln!("Continue with ChatGPT：正在打开系统浏览器，请登录并允许 Mario 使用订阅。10 分钟内有效；Ctrl+C 取消。");
    // No auth URL logging: returning sign-ins include an ID-token hint.
    open_browser(url.as_str())?;
    let callback = tokio::time::timeout(Duration::from_secs(600), receiver)
        .await
        .map_err(|_| error("ChatGPT 登录等待超时，请重新登录"))?
        .map_err(|_| error("ChatGPT 登录回调已中止"))?;
    let Callback::Code { code, client_id } = callback else {
        return Err(error("用户取消了 ChatGPT 授权，本机账号未改动"));
    };
    let attempt = &pending.attempt;
    let response = token_request(
        client,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", &code),
            ("code_verifier", &attempt.verifier),
            ("redirect_uri", &attempt.redirect),
            ("resource", RESOURCE),
        ],
    )
    .await?;
    let identity = identity(
        client,
        response
            .id_token
            .as_deref()
            .ok_or_else(|| error("缺少 ChatGPT 身份令牌"))?,
        &client_id,
        Some(&attempt.nonce),
    )
    .await?;
    let tokens = response.credentials(None, now())?;
    Ok(Authorized {
        tokens,
        identity,
        client_id,
    })
}
#[cfg(windows)]
fn open_browser(url: &str) -> AppResult<()> {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        return Err(error("无法打开系统浏览器，请检查默认浏览器设置后重试"));
    }
    Ok(())
}
#[cfg(not(windows))]
fn open_browser(url: &str) -> AppResult<()> {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = std::process::Command::new(command)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| error("无法打开系统浏览器，请检查默认浏览器设置后重试"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(test)]
mod tests_extra;
#[cfg(test)]
mod tests {
    use super::*;
    fn attempt() -> Attempt {
        Attempt {
            state: "expected".into(),
            nonce: "nonce".into(),
            verifier: "verifier".into(),
            redirect: "http://127.0.0.1:12345/auth/callback".into(),
            existing: None,
        }
    }
    #[test]
    fn registration_and_returning_flows_bind_client_state_and_host() {
        let mut pending = attempt();
        let url = pending.url("urn:uuid:host").unwrap();
        let fields: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(fields["client_id"], "dynamic_agent_client");
        assert_eq!(fields["agent_name_hint"], "mario");
        assert_eq!(fields["ext_agent_host_id"], "urn:uuid:host");
        assert_eq!(fields["redirect_uri"], pending.redirect);
        assert_eq!(
            fields["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(b"verifier"))
        );
        assert!(pending
            .callback("code=secret&client_id=issued&state=wrong")
            .is_err());
        assert!(pending.callback("code=secret&state=expected").is_err());
        assert!(pending
            .callback("code=secret&client_id=dynamic_agent_client&state=expected")
            .is_err());
        assert!(pending
            .callback("code=one&code=two&client_id=issued&state=expected")
            .is_err());
        assert!(matches!(
            pending
                .callback("error=access_denied&state=expected")
                .unwrap(),
            Callback::Denied
        ));
        pending.existing = Some(Account {
            id: "local".into(),
            client_id: "issued".into(),
            subject: "subject".into(),
            email: "example@test".into(),
            tokens: None,
        });
        assert!(matches!(
            pending.callback("code=secret&state=expected").unwrap(),
            Callback::Code { .. }
        ));
        assert!(pending
            .callback("code=secret&client_id=other&state=expected")
            .is_err());
        let fields: HashMap<_, _> = pending
            .url("host")
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(fields["client_id"], "issued");
        assert!(!fields.contains_key("agent_name_hint"));
    }
    #[test]
    fn rejects_unsigned_or_untrusted_identity_tokens() {
        let keys = JwkSet { keys: Vec::new() };
        assert!(verify_identity("not-a-token", "client", Some("nonce"), &keys).is_err());
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::new(Algorithm::HS256),&serde_json::json!({"sub":"attacker","iss":ISSUER,"aud":"client","exp":now()+3600,"nonce":"nonce"}),&jsonwebtoken::EncodingKey::from_secret(b"secret")).unwrap();
        assert!(verify_identity(&token, "client", Some("nonce"), &keys).is_err());
    }
}
