//! Website OAuth HTTP adaptation. OpenAI tokens never leave this server.
use super::{bearer, HostedState};
use crate::{
    accounts::{OpenAiAttempt, OpenAiIdentity},
    openai_login::{self, OpenAiLogin, ISSUER},
    AppError, AppResult,
};
use axum::{
    extract::{RawQuery, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};

fn login(host: &HostedState) -> AppResult<&OpenAiLogin> {
    host.openai
        .as_ref()
        .ok_or_else(|| AppError::NotFound("管理员尚未配置 ChatGPT 网站登录".into()))
}
pub(super) async fn config(State(host): State<Arc<HostedState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"enabled":host.openai.is_some()}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Start {
    link: bool,
}
pub(super) async fn start(
    State(host): State<Arc<HostedState>>,
    headers: HeaderMap,
    Json(input): Json<Start>,
) -> AppResult<Response> {
    let login = login(&host)?;
    let _permit = host
        .auth_slots
        .try_acquire()
        .map_err(|_| AppError::RateLimit("登录请求过多，请稍后重试".into()))?;
    if headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        != Some(login.config.origin().as_str())
    {
        return Err(AppError::Auth("请从 Mario 网站发起 ChatGPT 登录".into()));
    }
    let link_token = if input.link {
        Some(bearer(&headers)?.to_owned())
    } else {
        None
    };
    if let Some(token) = &link_token {
        host.accounts.authenticate(token)?;
    }
    let state = openai_login::random();
    let browser = openai_login::random();
    let verifier = openai_login::random();
    let nonce = openai_login::random();
    let url = login.authorization_url(&state, &nonce, &verifier).await?;
    let accounts = host.accounts.clone();
    let client = login.config.client_id.clone();
    let redirect = login.config.redirect_uri.clone();
    let saved_browser = browser.clone();
    tokio::task::spawn_blocking(move || {
        accounts.begin_openai(OpenAiAttempt {
            state: &state,
            browser: &saved_browser,
            verifier: &verifier,
            nonce: &nonce,
            client: &client,
            redirect: &redirect,
            link_token: link_token.as_deref(),
        })
    })
    .await
    .map_err(|_| AppError::Auth("登录服务暂不可用".into()))??;
    let mut response = Json(serde_json::json!({"url":url})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_header(login.config.cookie("mario_openai_login", &browser, 600))?,
    );
    Ok(response)
}
fn cookie_header(value: String) -> AppResult<HeaderValue> {
    value
        .parse()
        .map_err(|_| AppError::Validation("网站 OAuth Cookie 配置无效".into()))
}
fn browser_cookie(headers: &HeaderMap) -> AppResult<String> {
    read_cookie(headers, "mario_openai_login", 43)
}
fn read_cookie(headers: &HeaderMap, name: &str, length: usize) -> AppResult<String> {
    let prefix = format!("{name}=");
    let mut values = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().strip_prefix(prefix.as_str()));
    let value = values
        .next()
        .ok_or_else(|| AppError::Auth("未找到登录浏览器，请从 Mario 重新发起".into()))?;
    if values.next().is_some()
        || value.len() != length
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(AppError::Auth("登录浏览器校验失败".into()));
    }
    Ok(value.into())
}
fn callback_fields(raw: &str) -> AppResult<HashMap<String, String>> {
    if raw.len() > 16 * 1024 {
        return Err(AppError::Auth("授权回调过大".into()));
    }
    let url = reqwest::Url::parse(&format!("http://callback.invalid/?{raw}"))
        .map_err(|_| AppError::Auth("授权回调无效".into()))?;
    let mut result = HashMap::new();
    for (key, value) in url.query_pairs() {
        if result
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(AppError::Auth("授权回调字段重复".into()));
        }
    }
    Ok(result)
}
pub(super) async fn callback(
    State(host): State<Arc<HostedState>>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> AppResult<Response> {
    let login = login(&host)?;
    let result = complete(&host, &headers, raw.as_deref().unwrap_or_default()).await;
    let (status, token) = match result {
        Ok(session) => {
            let browser = browser_cookie(&headers)?;
            match host.accounts.stage_openai_result(&browser, session) {
                Ok(token) => ("complete", Some(token)),
                Err(_) => ("failed", None),
            }
        }
        Err(AppError::Conflict(_)) => ("conflict", None),
        Err(_) => ("failed", None),
    };
    let mut response =
        Redirect::to(&format!("{}#openai-login={status}", login.config.app_url)).into_response();
    let failed = token.is_none();
    let cookie = match token {
        Some(token) => login.config.cookie("mario_openai_result", &token, 120),
        None => login.config.cookie("mario_openai_login", "", 0),
    };
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie_header(cookie)?);
    if failed {
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(login.config.cookie("mario_openai_result", "", 0))?,
        );
    }
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    Ok(response)
}
pub(super) async fn finish(
    State(host): State<Arc<HostedState>>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let login = login(&host)?;
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        != Some(login.config.origin().as_str())
    {
        return Err(AppError::Auth("请从 Mario 网站完成登录".into()));
    }
    let result = (|| {
        let browser = browser_cookie(&headers)?;
        let token = read_cookie(&headers, "mario_openai_result", 64)?;
        host.accounts.take_openai_result(&token, &browser)
    })();
    let mut response = match result {
        Ok(session) => {
            Json(serde_json::json!({"linked":session.is_none(),"session":session})).into_response()
        }
        Err(error) => error.into_response(),
    };
    for name in ["mario_openai_login", "mario_openai_result"] {
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie_header(login.config.cookie(name, "", 0))?,
        );
    }
    Ok(response)
}
async fn complete(
    host: &HostedState,
    headers: &HeaderMap,
    raw: &str,
) -> AppResult<Option<crate::accounts::Session>> {
    let login = login(host)?;
    let _permit = host
        .auth_slots
        .try_acquire()
        .map_err(|_| AppError::RateLimit("登录服务繁忙".into()))?;
    let fields = callback_fields(raw)?;
    let browser = browser_cookie(headers)?;
    let state = fields
        .get("state")
        .filter(|s| s.len() == 43)
        .ok_or_else(|| AppError::Auth("登录 state 无效".into()))?;
    let attempt = host.accounts.consume_openai(
        state,
        &browser,
        &login.config.client_id,
        &login.config.redirect_uri,
    )?;
    if fields.contains_key("error") || fields.get("iss").is_some_and(|issuer| issuer != ISSUER) {
        return Err(AppError::Auth("ChatGPT 授权未完成".into()));
    }
    let code = fields
        .get("code")
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(|| AppError::Auth("缺少授权码".into()))?;
    let identity = login
        .exchange(code, &attempt.verifier, &attempt.nonce)
        .await?;
    let accounts = host.accounts.clone();
    let client = login.config.client_id.clone();
    tokio::task::spawn_blocking(move || {
        accounts.finish_openai(
            OpenAiIdentity {
                issuer: ISSUER,
                client: &client,
                subject: &identity.sub,
                email: &identity.email,
                email_verified: identity.email_verified,
            },
            attempt.link_session.as_deref(),
        )
    })
    .await
    .map_err(|_| AppError::Auth("登录服务暂不可用".into()))?
}
#[cfg(test)]
mod tests;
