//! Website identity-only OIDC. No ChatGPT plan scopes or credential persistence.
use crate::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(crate) const ISSUER: &str = "https://auth.openai.com";
const CALLBACK: &str = "api/auth/openai/callback";
pub(crate) struct Config {
    pub client_id: String,
    pub redirect_uri: String,
    pub app_url: String,
    secret: Option<String>,
}
pub(crate) struct OpenAiLogin {
    pub config: Config,
    client: reqwest::Client,
}
fn invalid(message: &str) -> AppError {
    AppError::Validation(message.into())
}
pub(crate) fn random() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
impl Config {
    fn parse(
        client_id: String,
        redirect_uri: String,
        method: &str,
        secret: Option<String>,
    ) -> AppResult<Self> {
        if !client_id.starts_with("oaiapp_")
            || client_id.len() > 255
            || client_id.chars().any(char::is_whitespace)
        {
            return Err(invalid(
                "MARIO_OPENAI_CLIENT_ID 必须为 OpenAI 签发的网站 OAuth client ID",
            ));
        }
        let url = reqwest::Url::parse(&redirect_uri)
            .map_err(|_| invalid("MARIO_OPENAI_REDIRECT_URI 无效"))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(url.scheme() == "https" || (url.scheme() == "http" && local))
            || url.as_str() != redirect_uri
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.path().ends_with(&format!("/{CALLBACK}"))
            || !url
                .path()
                .strip_suffix(CALLBACK)
                .unwrap_or("")
                .split('/')
                .all(|segment| {
                    segment
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                })
        {
            return Err(invalid("网站 OAuth 回调必须为 HTTPS（本机可用 HTTP），以 /api/auth/openai/callback 结尾且不含凭据、查询或片段"));
        }
        let secret = secret.filter(|value| !value.is_empty());
        if !matches!(
            (method, secret.is_some()),
            ("none", false) | ("client_secret_basic", true)
        ) {
            return Err(invalid("MARIO_OPENAI_TOKEN_AUTH_METHOD 只接受 none（无 secret）或 client_secret_basic（需 secret），须与 OpenAI 注册一致"));
        }
        let app_url = redirect_uri.strip_suffix(CALLBACK).unwrap().to_owned();
        Ok(Self {
            client_id,
            redirect_uri,
            app_url,
            secret,
        })
    }
    pub fn cookie_path(&self) -> &str {
        // Config validation guarantees a full URL and a trailing slash.
        let start = self.app_url.find("://").unwrap() + 3;
        let path = self.app_url[start..].find('/').unwrap() + start;
        &self.app_url[path..]
    }
    pub fn origin(&self) -> String {
        reqwest::Url::parse(&self.app_url)
            .unwrap()
            .origin()
            .ascii_serialization()
    }
    pub fn cookie(&self, name: &str, value: &str, max_age: u32) -> String {
        format!(
            "{name}={value}; Path={}; HttpOnly; SameSite=Lax; Max-Age={}{}",
            self.cookie_path(),
            max_age,
            if self.redirect_uri.starts_with("https:") {
                "; Secure"
            } else {
                ""
            }
        )
    }
}
impl OpenAiLogin {
    pub fn from_env() -> AppResult<Option<Self>> {
        let read = |name| std::env::var(name).ok().filter(|value| !value.is_empty());
        let Some(client_id) = read("MARIO_OPENAI_CLIENT_ID") else {
            if ["MARIO_OPENAI_REDIRECT_URI", "MARIO_OPENAI_CLIENT_SECRET"]
                .iter()
                .any(|name| read(name).is_some())
            {
                return Err(invalid(
                    "网站 OAuth 配置不完整：缺少 MARIO_OPENAI_CLIENT_ID",
                ));
            }
            return Ok(None);
        };
        let config = Config::parse(
            client_id,
            read("MARIO_OPENAI_REDIRECT_URI")
                .ok_or_else(|| invalid("缺少 MARIO_OPENAI_REDIRECT_URI"))?,
            &read("MARIO_OPENAI_TOKEN_AUTH_METHOD").unwrap_or_else(|| "none".into()),
            read("MARIO_OPENAI_CLIENT_SECRET"),
        )?;
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| invalid("无法初始化 OpenAI 网站登录"))?;
        Ok(Some(Self { config, client }))
    }
    async fn discovery(&self) -> AppResult<Discovery> {
        let response = self
            .client
            .get(format!("{ISSUER}/.well-known/openid-configuration"))
            .send()
            .await
            .map_err(|_| invalid("OpenAI 登录服务暂不可用，请稍后重试"))?;
        let doc: Discovery = serde_json::from_value(bounded_json(response).await?)
            .map_err(|_| invalid("OpenAI 身份配置无效"))?;
        if doc.issuer != ISSUER
            || [
                &doc.authorization_endpoint,
                &doc.token_endpoint,
                &doc.jwks_uri,
            ]
            .iter()
            .any(|value| !official_endpoint(value))
        {
            return Err(invalid("OpenAI 身份配置端点校验失败"));
        }
        Ok(doc)
    }
    pub async fn authorization_url(
        &self,
        state: &str,
        nonce: &str,
        verifier: &str,
    ) -> AppResult<String> {
        let doc = self.discovery().await?;
        let mut url = reqwest::Url::parse(&doc.authorization_endpoint)
            .map_err(|_| invalid("OpenAI 授权地址无效"))?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", self.config.client_id.as_str()),
            ("redirect_uri", &self.config.redirect_uri),
            ("response_type", "code"),
            ("scope", "openid profile email"),
            ("state", state),
            ("nonce", nonce),
            ("code_challenge_method", "S256"),
            (
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            ),
        ]);
        Ok(url.into())
    }
    pub async fn exchange(&self, code: &str, verifier: &str, nonce: &str) -> AppResult<Identity> {
        let doc = self.discovery().await?;
        let response = self
            .client
            .get(doc.jwks_uri)
            .send()
            .await
            .map_err(|_| invalid("OpenAI 签名公钥暂不可用"))?;
        let keys: JwkSet = serde_json::from_value(bounded_json(response).await?)
            .map_err(|_| invalid("OpenAI 签名公钥无效"))?;
        let mut request = self.client.post(doc.token_endpoint).form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &self.config.client_id),
            ("redirect_uri", &self.config.redirect_uri),
            ("code_verifier", verifier),
        ]);
        if let Some(secret) = &self.config.secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }
        let response = request
            .send()
            .await
            .map_err(|_| invalid("OpenAI 授权交换失败，请重新登录"))?;
        let value = bounded_json(response).await?;
        let token = value["id_token"]
            .as_str()
            .ok_or_else(|| invalid("OpenAI 未返回身份令牌"))?;
        verify_identity(token, &self.config.client_id, nonce, &keys)
    }
}
#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}
fn official_endpoint(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("auth.openai.com")
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}
async fn bounded_json(mut response: reqwest::Response) -> AppResult<serde_json::Value> {
    if !response.status().is_success() {
        return Err(invalid("OpenAI 身份请求失败，请重新登录或稍后重试"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| invalid("OpenAI 身份响应中断"))?
    {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(invalid("OpenAI 身份响应过大"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid("OpenAI 身份响应无效"))
}
#[derive(Deserialize)]
pub(crate) struct Identity {
    pub sub: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    nonce: String,
    aud: serde_json::Value,
    #[serde(default)]
    azp: Option<String>,
    iat: i64,
}
fn verify_identity(token: &str, client: &str, nonce: &str, keys: &JwkSet) -> AppResult<Identity> {
    let denied = || invalid("OpenAI 身份签名、受众或登录有效期校验失败");
    let header = decode_header(token).map_err(|_| denied())?;
    if header.alg != Algorithm::RS256 {
        return Err(denied());
    }
    let key = header
        .kid
        .as_ref()
        .and_then(|kid| keys.find(kid))
        .ok_or_else(denied)?;
    let key = DecodingKey::from_jwk(key).map_err(|_| denied())?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[client]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["iss", "aud", "sub", "exp", "iat", "nonce"]);
    validation.leeway = 5;
    validation.validate_nbf = true;
    let identity = decode::<Identity>(token, &key, &validation)
        .map_err(|_| denied())?
        .claims;
    if identity.sub.is_empty()
        || identity.sub.len() > 1024
        || identity.nonce != nonce
        || identity.iat > chrono::Utc::now().timestamp() + 5
        || identity.azp.as_deref().is_some_and(|azp| azp != client)
        || (identity.aud.as_array().is_some_and(|aud| aud.len() > 1)
            && identity.azp.as_deref() != Some(client))
    {
        return Err(denied());
    }
    Ok(identity)
}

#[cfg(test)]
mod tests;
