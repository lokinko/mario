use super::*;
use accounts::{Accounts, Credentials};
use axum::http::StatusCode;

struct HostedState {
    accounts: Arc<Accounts>,
    slots: tokio::sync::Semaphore,
    auth_slots: tokio::sync::Semaphore,
}

pub async fn serve<F>(
    url: String,
    address: SocketAddr,
    web_dir: Option<PathBuf>,
    shutdown: F,
) -> AppResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    crate::secrets::validate_master_key()?;
    let state = Arc::new(HostedState {
        accounts: Arc::new(Accounts::open(url)?),
        slots: tokio::sync::Semaphore::new(24),
        auth_slots: tokio::sync::Semaphore::new(2),
    });
    let private = api_router()
        .route("/api/sync/version", get(version))
        .route("/api/data/export", get(export))
        .route("/api/data/import", post(import))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    let cors = cors_layer()?;
    let app=private.merge(Router::new()
        .route("/api/auth/login",post(login))
        .route("/api/auth/register",post(register))
        .route("/api/auth/logout",post(logout))
        .route("/api/server",get(|| async { Json(serde_json::json!({"mode":"hosted","database":"postgresql","protocol":1})) }))
        .layer(axum::extract::DefaultBodyLimit::max(16*1024))
        .with_state(state))
        .layer(axum::extract::DefaultBodyLimit::max(8*1024*1024))
        .layer(cors)
        .layer(middleware::map_response(no_store))
        .layer(TraceLayer::new_for_http());
    let app = if let Some(dir) = web_dir {
        app.fallback_service(tower_http::services::ServeDir::new(dir))
    } else {
        app
    };
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address,"hosted service started");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
pub fn cors_layer() -> AppResult<CorsLayer> {
    let mut origins = vec![
        "http://localhost:1420",
        "http://127.0.0.1:1420",
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if let Ok(extra) = std::env::var("MARIO_ALLOWED_ORIGINS") {
        origins.extend(
            extra
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty()),
        );
    }
    let origins = origins
        .iter()
        .map(|s| {
            s.parse::<HeaderValue>()
                .map_err(|_| AppError::Validation("MARIO_ALLOWED_ORIGINS 无效".into()))
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::IF_MATCH,
        ])
        .expose_headers(["x-data-revision".parse::<header::HeaderName>().unwrap()]))
}
fn bearer(headers: &axum::http::HeaderMap) -> AppResult<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() <= 256)
        .ok_or_else(|| AppError::Auth("请登录".into()))
}
async fn authenticate(
    State(host): State<Arc<HostedState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let result=async {
        let _permit=host.slots.try_acquire().map_err(|_|AppError::Busy("服务繁忙，请稍后重试".into()))?;
        let user=host.accounts.authenticate(bearer(request.headers())?)?;
        let db=Database::postgres(&host.accounts.url,&user)?;
        db.lock_account()?;
        let revision=db.revision()?;
        let is_write=request.method()!=Method::GET && !request.uri().path().ends_with("/preview") && !request.uri().path().ends_with("/test");
        if is_write && request.headers().get(header::IF_MATCH).and_then(|v|v.to_str().ok())!=Some(revision.as_str()) {
            return Ok::<_,AppError>((StatusCode::CONFLICT,Json(serde_json::json!({"error":"资料已变化，请刷新核对后再保存；当前输入不会自动提交。","revision":revision}))).into_response());
        }
        let state=Arc::new(AppState { mutation_lock:tokio::sync::Mutex::new(()),db,auth_token:None,fx_provider:Arc::new(EcbFxRateProvider::new()?),security_price_provider:Arc::new(TwelveDataSecurityPriceProvider::new()?) });
        request.extensions_mut().insert(state.clone());
        let mut response=next.run(request).await;
        response.headers_mut().insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store"));
        response.headers_mut().insert("x-data-revision",state.db.revision()?.parse().map_err(|_|AppError::Validation("数据版本无效".into()))?);
        state.db.unlock_account()?;
        Ok(response)
    }.await;
    result.unwrap_or_else(IntoResponse::into_response)
}
async fn login(
    State(state): State<Arc<HostedState>>,
    Json(input): Json<Credentials>,
) -> AppResult<Json<accounts::Session>> {
    account_action(state, input, false).await
}
async fn register(
    State(state): State<Arc<HostedState>>,
    Json(input): Json<Credentials>,
) -> AppResult<Json<accounts::Session>> {
    account_action(state, input, true).await
}
async fn account_action(
    state: Arc<HostedState>,
    input: Credentials,
    register: bool,
) -> AppResult<Json<accounts::Session>> {
    let _permit = state
        .auth_slots
        .try_acquire()
        .map_err(|_| AppError::RateLimit("登录请求过多，请稍后重试".into()))?;
    let accounts = state.accounts.clone();
    let result = tokio::task::spawn_blocking(move || {
        if register {
            accounts.register(&input)
        } else {
            accounts.login(&input)
        }
    })
    .await
    .map_err(|_| AppError::Auth("登录服务不可用".into()))??;
    Ok(Json(result))
}
async fn logout(
    State(state): State<Arc<HostedState>>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    state.accounts.logout(bearer(&headers)?)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
async fn version(Extension(state): Extension<Arc<AppState>>) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"revision":state.db.revision()?})))
}
async fn export(Extension(state): Extension<Arc<AppState>>) -> AppResult<Json<db::SyncDataset>> {
    Ok(Json(state.db.export_sync_data()?))
}
async fn import(
    Extension(state): Extension<Arc<AppState>>,
    Json(data): Json<db::SyncDataset>,
) -> AppResult<Json<serde_json::Value>> {
    if !state.db.is_empty_for_import()? {
        return Err(AppError::Conflict("仅允许向空账户导入数据".into()));
    }
    state.db.import_sync_data(&data)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
