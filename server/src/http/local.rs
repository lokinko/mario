//! Local development authentication and serialization; never owns business workflows.
use super::{api_router, policy, RequestContext};
use crate::{
    application::portfolio::MarketServices, authentication::token_matches, db::Database, fx,
    prices, AppError, AppResult,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, Method},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::{future::Future, net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
struct LocalState {
    context: Arc<RequestContext>,
    mutation_lock: tokio::sync::Mutex<()>,
    auth_token: Option<String>,
}
pub(crate) async fn serve<F>(
    data_dir: PathBuf,
    address: SocketAddr,
    auth_token: Option<String>,
    web_dir: Option<PathBuf>,
    shutdown: F,
) -> AppResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    fx::start_daily_refresh();
    prices::start_refresh();
    let state = Arc::new(LocalState {
        context: Arc::new(RequestContext {
            db: Database::open(&data_dir.join("mario.db"))?,
            market: Arc::new(MarketServices::new()?),
            analysis_executor: Arc::new(crate::agent::AgentExecutor),
        }),
        auth_token: auth_token.clone(),
        mutation_lock: tokio::sync::Mutex::new(()),
    });
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:1420".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = api_router()
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_local_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // Only compiled public assets are served outside the authenticated API routes.
    let app = if let Some(dir) = web_dir {
        app.fallback_service(tower_http::services::ServeDir::new(dir))
    } else {
        app
    };
    tracing::info!(%address, data_dir = %data_dir.display(), authenticated = auth_token.is_some(), "local service started");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::Io)?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(AppError::Io)?;
    Ok(())
}

async fn require_local_auth(
    State(state): State<Arc<LocalState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(expected) = state.auth_token.as_deref() {
        let provided = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !provided.is_some_and(|value| token_matches(expected, value)) {
            return AppError::LocalAuth("请求缺少本次应用启动生成的访问令牌".into())
                .into_response();
        }
    }
    let _guard = if policy::needs_local_lock(request.method(), request.uri().path()) {
        Some(state.mutation_lock.lock().await)
    } else {
        None
    };
    request.extensions_mut().insert(state.context.clone());
    next.run(request).await
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_auth_token_comparison_rejects_missing_or_different_values() {
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(token_matches(token, token));
        assert!(!token_matches(token, "different"));
    }

    #[tokio::test]
    async fn local_web_server_requires_authentication() {
        let data_dir = tempfile::tempdir().unwrap();
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve(
            data_dir.path().to_path_buf(),
            format!("127.0.0.1:{port}").parse().unwrap(),
            Some(token.into()),
            None,
            async move {
                let _ = shutdown_rx.await;
            },
        ));
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/api/health");
        let mut authenticated_status = None;
        for _ in 0..40 {
            if let Ok(response) = client.get(&url).bearer_auth(token).send().await {
                authenticated_status = Some(response.status());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert_eq!(authenticated_status, Some(reqwest::StatusCode::OK));
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        shutdown_tx.send(()).unwrap();
        server.await.unwrap().unwrap();
        assert!(data_dir.path().join("mario.db").exists());
    }
}
