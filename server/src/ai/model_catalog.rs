//! The site catalog is public model metadata, fetched only from the official endpoint.
use std::{future::Future, time::Duration};

use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant};

use crate::{ports::model::ModelCatalog, AppError, AppResult};

const MODELS_URL: &str = "https://api.openai.com/v1/models";
const MAX_BYTES: usize = 1024 * 1024;
const SUCCESS_TTL: Duration = Duration::from_secs(600);
const FAILURE_TTL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(crate) struct OpenAiModelCatalog {
    cache: CatalogCache,
}

#[async_trait::async_trait]
impl ModelCatalog for OpenAiModelCatalog {
    async fn list(&self, api_key: &str) -> AppResult<Vec<String>> {
        self.cache
            .get(api_key, fetch_models(MODELS_URL, api_key))
            .await
    }
}

struct CachedCatalog {
    credential_hash: [u8; 32],
    expires: Instant,
    result: Result<Vec<String>, String>,
}
#[derive(Default)]
struct CatalogCache(Mutex<Option<CachedCatalog>>);
impl CatalogCache {
    async fn get(
        &self,
        api_key: &str,
        fetch: impl Future<Output = AppResult<Vec<String>>>,
    ) -> AppResult<Vec<String>> {
        let credential_hash: [u8; 32] = Sha256::digest(api_key.as_bytes()).into();
        // One bounded request at a time; waiting callers reuse its result.
        let mut cached = self.0.lock().await;
        if let Some(entry) = cached.as_ref() {
            if entry.credential_hash == credential_hash && entry.expires > Instant::now() {
                return entry.result.clone().map_err(AppError::Model);
            }
        }
        let result = fetch.await.map_err(|error| error.to_string());
        let ttl = if result.is_ok() {
            SUCCESS_TTL
        } else {
            FAILURE_TTL
        };
        *cached = Some(CachedCatalog {
            credential_hash,
            expires: Instant::now() + ttl,
            result: result.clone(),
        });
        result.map_err(AppError::Model)
    }
}

fn unavailable() -> AppError {
    AppError::Model("暂时无法读取站点模型目录，请稍后重试；当前选择仍然保留".into())
}

async fn fetch_models(url: &str, api_key: &str) -> AppResult<Vec<String>> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| unavailable())?;
    let mut response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success() {
        // Never read or relay upstream error bodies, which may echo credentials.
        return Err(AppError::Model(format!(
            "站点模型目录返回 HTTP {}，请稍后重试或联系管理员检查密钥权限",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    let mut models = parse_models(&bytes)?;
    models.retain(|id| !id.contains(api_key));
    Ok(models)
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<Model>,
}
#[derive(Deserialize)]
struct Model {
    id: String,
}
fn parse_models(bytes: &[u8]) -> AppResult<Vec<String>> {
    let value: ModelList = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    let mut models = Vec::new();
    for model in value.data {
        let id = model.id;
        if id.is_empty()
            || id.len() > 200
            || !id.is_ascii()
            || id
                .bytes()
                .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
        {
            return Err(unavailable());
        }
        if text_candidate(&id) {
            models.push(id);
        }
    }
    models.sort();
    models.dedup();
    Ok(models)
}

// /models does not describe supported endpoints. These are text candidates,
// not a promise of Responses, search, quota or structured-output support.
fn text_candidate(id: &str) -> bool {
    let family = id
        .strip_prefix("ft:")
        .unwrap_or(id)
        .split(':')
        .next()
        .unwrap_or(id);
    let modern_gpt = family
        .strip_prefix("gpt-")
        .and_then(|name| name.split(['-', '.']).next())
        .and_then(|generation| generation.parse::<u32>().ok())
        .is_some_and(|generation| generation >= 5);
    let reasoning = family
        .strip_prefix('o')
        .and_then(|name| name.split('-').next())
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()));
    (modern_gpt || family.starts_with("gpt-4o") || family.starts_with("gpt-4.1") || reasoning)
        && ![
            "audio",
            "realtime",
            "transcribe",
            "tts",
            "image",
            "search",
            "chat-latest",
        ]
        .iter()
        .any(|kind| family.contains(kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode},
        routing::get,
        Json, Router,
    };
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn parses_the_key_catalog_as_candidates_without_inventing_model_ids() {
        let models = parse_models(
            &serde_json::to_vec(&json!({"data":[
                {"id":"gpt-6.1-sol"}, {"id":"gpt-5.4-mini"}, {"id":"o3"},
                {"id":"gpt-4o"}, {"id":"ft:gpt-4.1-mini:org:name"}, {"id":"gpt-5.4-mini"},
                {"id":"gpt-image-2"}, {"id":"text-embedding-3-small"},
                {"id":"gpt-4o-mini-tts"}, {"id":"gpt-5-search-api"},
                {"id":"gpt-4o-realtime-preview"}, {"id":"gpt-4o-transcribe"},
                {"id":"gpt-5-chat-latest"}, {"id":"gpt-3.5-turbo"}
            ]}))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            models,
            [
                "ft:gpt-4.1-mini:org:name",
                "gpt-4o",
                "gpt-5.4-mini",
                "gpt-6.1-sol",
                "o3"
            ]
        );
        for bytes in [
            b"{}".as_slice(),
            br#"{"data":[{"id":7}]}"#,
            br#"{"data":[{"id":"gpt-5\nheader"}]}"#,
        ] {
            assert!(parse_models(bytes).is_err());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn caches_coalesces_expires_and_binds_results_to_the_credential() {
        let cache = Arc::new(CatalogCache::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let fetch = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(vec!["gpt-5.4".into()])
        };
        let (a, b) = tokio::join!(
            cache.get("first-key", fetch()),
            cache.get("first-key", fetch())
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        tokio::time::advance(SUCCESS_TTL).await;
        cache.get("first-key", fetch()).await.unwrap();
        cache.get("other-key", fetch()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let failure = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(unavailable())
        };
        assert!(cache.get("failed-key", failure()).await.is_err());
        assert!(cache.get("failed-key", fetch()).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        tokio::time::advance(FAILURE_TTL).await;
        cache.get("failed-key", fetch()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn authenticates_and_does_not_relay_error_bodies_or_follow_redirects() {
        let app = Router::new()
            .route(
                "/models",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers["authorization"], "Bearer synthetic-catalog-key");
                    Json(json!({"data":[{"id":"gpt-5.4"}]}))
                }),
            )
            .route(
                "/denied",
                get(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        "synthetic-catalog-key private upstream body",
                    )
                }),
            )
            .route(
                "/redirect",
                get(|| async { (StatusCode::FOUND, [("location", "/models")]) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        assert_eq!(
            fetch_models(&format!("{base}/models"), "synthetic-catalog-key")
                .await
                .unwrap(),
            ["gpt-5.4"]
        );
        for path in ["denied", "redirect"] {
            let error = fetch_models(&format!("{base}/{path}"), "synthetic-catalog-key")
                .await
                .unwrap_err()
                .to_string();
            assert!(!error.contains("synthetic-catalog-key"));
            assert!(!error.contains("private upstream body"));
            assert!(error.contains(if path == "denied" { "401" } else { "302" }));
        }
        server.abort();
    }
}
