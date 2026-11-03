//! Official OAuth for the LOCAL worker only. No API routes expose tokens or login.
mod oauth;
mod store;
use crate::{AppError, AppResult};
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use store::{Account, Store, Tokens};

const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

fn error(message: &str) -> AppError {
    AppError::Validation(message.into())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| error("无法初始化 ChatGPT 授权连接"))
}
async fn bounded_json(mut response: reqwest::Response) -> AppResult<serde_json::Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("读取 ChatGPT 授权响应失败"))?
    {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(error("ChatGPT 授权响应超出限制"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| error("ChatGPT 授权响应格式无效"))
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    earliest_refresh_at: Option<u64>,
}
impl TokenResponse {
    fn credentials(self, previous: Option<&Tokens>, received: u64) -> AppResult<Tokens> {
        let scopes: Vec<String> = match self.scope {
            Some(scope) => scope.split_whitespace().map(String::from).collect(),
            None => previous
                .ok_or_else(|| error("ChatGPT 没有返回授权范围"))?
                .scopes
                .clone(),
        };
        if !scopes.iter().any(|scope| scope == "openid") {
            return Err(error("ChatGPT 未授予身份权限"));
        }
        if !self.token_type.eq_ignore_ascii_case("bearer")
            || self.access_token.is_empty()
            || self.expires_in == 0
            || self.expires_in > 24 * 3600
        {
            return Err(error("ChatGPT 返回了无效的令牌信息"));
        }
        let id_token = self
            .id_token
            .or_else(|| previous.map(|v| v.id_token.clone()))
            .filter(|v| !v.is_empty())
            .ok_or_else(|| error("ChatGPT 没有返回身份令牌"))?;
        Ok(Tokens {
            session: previous
                .map_or_else(|| uuid::Uuid::new_v4().to_string(), |v| v.session.clone()),
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            id_token,
            scopes,
            expires_at: received.saturating_add(self.expires_in),
            earliest_refresh_at: self.earliest_refresh_at.unwrap_or(0),
        })
    }
}
async fn token_request(
    client: &reqwest::Client,
    fields: &[(&str, &str)],
) -> AppResult<TokenResponse> {
    let response = client
        .post(TOKEN)
        .form(fields)
        .send()
        .await
        .map_err(|_| error("ChatGPT 授权网络连接失败；请稍后重试"))?;
    let status = response.status();
    let body = bounded_json(response).await?;
    if !status.is_success() {
        return Err(token_error(&body, status.as_u16()));
    }
    serde_json::from_value(body)
        .map_err(|_| AppError::Auth("ChatGPT 令牌响应字段无效，请重新授权".into()))
}
fn token_error(body: &serde_json::Value, status: u16) -> AppError {
    let code = body["error"]
        .as_str()
        .or_else(|| body["error"]["code"].as_str());
    if matches!(status, 400 | 401)
        && matches!(
            code,
            Some(
                "invalid_grant"
                    | "invalid_refresh_token"
                    | "refresh_token_expired"
                    | "refresh_token_invalidated"
                    | "refresh_token_reused"
                    | "token_expired"
            )
        )
    {
        AppError::Auth(
            "ChatGPT 可续期授权已失效，请运行 chatgpt:auth -- login --account <注册 ID> 重新授权"
                .into(),
        )
    } else {
        error("ChatGPT 拒绝了授权请求，请检查授权、工作区权限或稍后重试")
    }
}
fn plan_enabled(tokens: &Tokens) -> bool {
    !tokens.refresh_token.is_empty()
        && SCOPES
            .split_whitespace()
            .all(|scope| tokens.scopes.iter().any(|value| value == scope))
}
pub(crate) fn is_authorized(id: &str, session: &str) -> AppResult<bool> {
    let store = Store::open()?;
    store.authorized(id, session)
}
pub(crate) fn session(id: &str) -> AppResult<String> {
    let store = Store::open()?;
    store
        .account(id)?
        .tokens
        .as_ref()
        .filter(|tokens| plan_enabled(tokens))
        .map(|tokens| tokens.session.clone())
        .ok_or_else(|| {
            error("此 ChatGPT 注册未开启订阅使用或已退出，请 login --account <注册 ID> 重新授权")
        })
}

pub(crate) fn has_active_account() -> AppResult<bool> {
    let store = Store::open()?;
    Ok(store
        .state
        .active
        .as_deref()
        .and_then(|id| store.state.accounts.iter().find(|account| account.id == id))
        .and_then(|account| account.tokens.as_ref())
        .is_some_and(plan_enabled))
}
pub(crate) fn active_account() -> AppResult<String> {
    let store = Store::open()?;
    let id = store
        .state
        .active
        .as_deref()
        .ok_or_else(|| error("请先运行 npm run chatgpt:auth -- login"))?;
    let account = store.account(id)?;
    if account.tokens.is_none() {
        return Err(error("当前 ChatGPT 账号已退出，请重新登录"));
    }
    Ok(id.into())
}
/// The worker pins this ID; selecting another account cannot retarget its tasks.
pub(crate) async fn access_token(id: &str) -> AppResult<String> {
    let mut store = Store::open()?;
    let account = store.account(id)?.clone();
    let previous = account
        .tokens
        .as_ref()
        .ok_or_else(|| error("ChatGPT 已退出，请重新登录并重启桥接"))?;
    if !plan_enabled(previous) {
        return Err(error(
            "此 ChatGPT 注册尚未开启订阅使用，请 login --account <注册 ID> 重新授权",
        ));
    }
    let time = now();
    if previous.expires_at > time + 120 {
        return Ok(previous.access_token.clone());
    }
    if time < previous.earliest_refresh_at {
        if previous.expires_at > time + 10 {
            return Ok(previous.access_token.clone());
        }
        return Err(error("ChatGPT 尚未允许刷新令牌，请稍后重试"));
    }
    let client = client()?;
    // Fetch verification material BEFORE rotating a one-use refresh token.
    // After a successful exchange, validation is local and cannot lose the new
    // refresh token merely because a second discovery/JWKS request failed.
    let keys = oauth::verification_keys(&client).await?;
    let response = token_request(
        &client,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &account.client_id),
            ("refresh_token", &previous.refresh_token),
            ("resource", RESOURCE),
        ],
    )
    .await;
    let response = match response {
        Ok(response) => response,
        Err(failure) => {
            if matches!(failure, AppError::Auth(_)) {
                clear_tokens(&mut store, id)?;
            }
            return Err(failure);
        }
    };
    // No fallible network operations after rotation. A new/untrusted signing
    // key or identity fails closed and asks for a fresh, validated sign-in.
    let validated = (|| {
        if let Some(id_token) = response.id_token.as_deref() {
            let identity = oauth::verify_identity(id_token, &account.client_id, None, &keys)?;
            if identity.sub != account.subject {
                return Err(error("刷新后的 ChatGPT 身份不匹配，请重新授权"));
            }
        }
        response.credentials(Some(previous), now())
    })();
    let tokens = match validated {
        Ok(tokens) if !tokens.refresh_token.is_empty() => tokens,
        _ => {
            clear_tokens(&mut store, id)?;
            return Err(error(
                "刷新后的 ChatGPT 凭据校验失败，旧令牌已停用，请重新授权",
            ));
        }
    };
    let enabled = plan_enabled(&tokens);
    let token = tokens.access_token.clone();
    store
        .state
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap()
        .tokens = Some(tokens);
    store.save().map_err(|_| {
        error("ChatGPT 已刷新但无法原子保存本机凭据，请检查磁盘权限并重新授权；不会重放推理")
    })?;
    if !enabled {
        return Err(error("ChatGPT 订阅使用权限已关闭，请重新授权"));
    }
    Ok(token)
}
fn clear_tokens(store: &mut Store, id: &str) -> AppResult<()> {
    store
        .state
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| error("本机注册不存在"))?
        .tokens = None;
    store.save()
}

pub(crate) async fn command(args: &[String]) -> AppResult<()> {
    match args.first().map(String::as_str) {
        Some("login") => {
            let selection = match &args[1..] {
                [] => None,
                [flag] if flag == "--new" => Some(None),
                [flag, id] if flag == "--account" => Some(Some(id.as_str())),
                _ => return Err(usage()),
            };
            login(selection).await
        }
        Some("status") if args.len() == 1 => {
            let store = Store::open()?;
            if store.state.accounts.is_empty() {
                eprintln!("尚无 ChatGPT 授权账号。运行 npm run chatgpt:auth -- login");
            }
            for account in &store.state.accounts {
                eprintln!(
                    "{} {} · {} · {}",
                    if store.state.active.as_deref() == Some(&account.id) {
                        "*"
                    } else {
                        " "
                    },
                    account.id,
                    account.email,
                    match account.tokens.as_ref() {
                        Some(tokens) if plan_enabled(tokens) =>
                            "已保存订阅授权（实际可用性需连接测试）",
                        Some(_) => "已登录，订阅使用未开启；login --account <注册 ID> 重新授权",
                        None => "已退出",
                    }
                );
            }
            Ok(())
        }
        Some("select") if args.len() == 2 => {
            let mut store = Store::open()?;
            if store.account(&args[1])?.tokens.is_none() {
                return Err(error("此账号已退出，请先 login --account <注册 ID>"));
            }
            store.state.active = Some(args[1].clone());
            store.save()?;
            eprintln!(
                "已切换本机默认账号；已运行的桥接仍绑定原账号，切换桥接账号请重新配对并重启。"
            );
            Ok(())
        }
        Some("logout") if args.len() <= 2 => logout(args.get(1).map(String::as_str)).await,
        _ => Err(usage()),
    }
}
fn usage() -> AppError {
    error("用法：chatgpt:auth -- login [--new | --account <注册 ID>] | status | select <注册 ID> | logout [注册 ID]")
}
async fn login(selection: Option<Option<&str>>) -> AppResult<()> {
    // Held across consent intentionally: another process cannot rotate/overwrite
    // credentials while this registration is being validated. No database used.
    let mut store = Store::open()?;
    let existing = match selection {
        Some(None) => None,
        Some(Some(id)) => Some(store.account(id)?.clone()),
        None => store
            .state
            .active
            .as_deref()
            .map(|id| store.account(id).cloned())
            .transpose()?,
    };
    let client = client()?;
    let result = oauth::authorize(&client, &store.state.host_id, existing.as_ref()).await?;
    let tokens = result.tokens;
    let enabled = plan_enabled(&tokens);
    let identity = result.identity;
    if let Some(account) = existing.as_ref() {
        if account.subject != identity.sub || account.client_id != result.client_id {
            return Err(error(
                "ChatGPT 返回了不同的账号身份，原凭据未改动；新增账号请用 login --new",
            ));
        }
    }
    let id = if let Some(account) = store
        .state
        .accounts
        .iter_mut()
        .find(|a| a.client_id == result.client_id && a.subject == identity.sub)
    {
        account.email = identity.email;
        account.tokens = Some(tokens);
        account.id.clone()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        store.state.accounts.push(Account {
            id: id.clone(),
            client_id: result.client_id,
            subject: identity.sub,
            email: identity.email,
            tokens: Some(tokens),
        });
        id
    };
    store.state.active = Some(id.clone());
    store.save()?;
    if enabled {
        eprintln!("ChatGPT 官方授权已保存至本机，注册 ID：{id}。可运行 npm run chatgpt:bridge；用量：https://chatgpt.com/settings/usage");
    } else {
        eprintln!("已保存 ChatGPT 身份注册：{id}，但尚未开启订阅使用；不会调用模型。请运行 npm run chatgpt:auth -- login --account {id} 重新同意完整授权。");
    }
    Ok(())
}
async fn logout(id: Option<&str>) -> AppResult<()> {
    let mut store = Store::open()?;
    let id = id
        .or(store.state.active.as_deref())
        .ok_or_else(|| error("没有可退出的 ChatGPT 账号"))?
        .to_owned();
    let account = store.account(&id)?.clone();
    // Stop local use first. Release the lock before retrying remote revocation,
    // so running workers can observe logout and cancel their HTTP futures.
    clear_tokens(&mut store, &id)?;
    drop(store);
    let mut confirmed = account
        .tokens
        .as_ref()
        .is_none_or(|tokens| tokens.refresh_token.is_empty());
    eprintln!(
        "本机令牌已清除，正在尝试撤销远端可续期会话；中断此步骤时请到 ChatGPT 设置确认断开。"
    );
    if let Some(tokens) = account
        .tokens
        .filter(|tokens| !tokens.refresh_token.is_empty())
    {
        let revoke = async {
            let client = client()?;
            let discovery = oauth::discovery(&client).await?;
            for attempt in 0..3 {
                match client
                    .post(&discovery.revocation_endpoint)
                    .form(&[
                        ("token", tokens.refresh_token.as_str()),
                        ("token_type_hint", "refresh_token"),
                        ("client_id", &account.client_id),
                    ])
                    .send()
                    .await
                {
                    Ok(response) if response.status() == reqwest::StatusCode::OK => {
                        return Ok::<_, AppError>(true)
                    }
                    Ok(response) if !response.status().is_server_error() => return Ok(false),
                    _ => {}
                }
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                }
            }
            Ok(false)
        };
        confirmed = revoke.await.unwrap_or(false);
    }
    if confirmed {
        eprintln!("已退出并撤销此 ChatGPT 可续期会话；注册信息与本机 host ID 保留。");
    } else {
        eprintln!("已清除本机令牌，但未确认远端撤销。请到 ChatGPT 设置断开 Mario：https://chatgpt.com/settings/usage");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response() -> TokenResponse {
        TokenResponse {
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            id_token: Some("id".into()),
            token_type: "Bearer".into(),
            expires_in: 3600,
            scope: Some(SCOPES.into()),
            earliest_refresh_at: None,
        }
    }
    #[test]
    fn terminal_refresh_codes_are_distinct_from_temporary_or_client_failures() {
        for code in [
            "invalid_grant",
            "invalid_refresh_token",
            "token_expired",
            "refresh_token_expired",
            "refresh_token_invalidated",
            "refresh_token_reused",
        ] {
            let body =
                serde_json::json!({"error":code, "error_description":"never-echo-this-secret"});
            let failure = token_error(&body, 400);
            assert!(matches!(failure, AppError::Auth(_)));
            assert!(!failure.to_string().contains("never-echo-this-secret"));
            assert!(!matches!(token_error(&body, 503), AppError::Auth(_)));
        }
        assert!(!matches!(
            token_error(&serde_json::json!({"error":"invalid_client"}), 401),
            AppError::Auth(_)
        ));
        assert!(!matches!(
            token_error(&serde_json::json!({"error":"temporarily_unavailable"}), 503),
            AppError::Auth(_)
        ));
    }
    #[test]
    fn validates_grants_and_replaces_all_rotating_fields_together() {
        let previous = response().credentials(None, 100).unwrap();
        let mut refreshed = response();
        refreshed.access_token = "new-access".into();
        refreshed.refresh_token = "new-refresh".into();
        refreshed.id_token = None;
        refreshed.scope = None;
        let next = refreshed.credentials(Some(&previous), 200).unwrap();
        assert_eq!(next.access_token, "new-access");
        assert_eq!(next.refresh_token, "new-refresh");
        assert_eq!(next.id_token, "id");
        assert_eq!(next.expires_at, 3800);
        assert_eq!(next.session, previous.session);
        assert!(plan_enabled(&next));
        let mut partial = response();
        partial.scope = Some("openid email".into());
        partial.refresh_token.clear();
        let partial = partial.credentials(None, 0).unwrap();
        assert!(!plan_enabled(&partial));
        assert_ne!(partial.session, previous.session);
        let mut rejected = response();
        rejected.token_type = "Basic".into();
        assert!(rejected.credentials(None, 0).is_err());
        let mut rejected = response();
        rejected.id_token = None;
        assert!(rejected.credentials(None, 0).is_err());
        let mut rejected = response();
        rejected.scope = None;
        assert!(rejected.credentials(None, 0).is_err());
    }
}
