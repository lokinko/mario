use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    db::{Database, SyncDataset, SYNC_DATASET_SCHEMA_VERSION},
    error::{AppError, AppResult},
    secrets,
};

const ENCRYPTION_CONTEXT: &[u8] = b"mario-cloud-sync-v1";
const LEGACY_ENCRYPTION_CONTEXT: &[u8] = b"zhiheng-cloud-sync-v1";
const RECOVERY_KEY_PREFIX: &str = "mario-sync-v1:";
const LEGACY_RECOVERY_KEY_PREFIX: &str = "zhiheng-sync-v1:";

const SETTING_CLOUD_URL: &str = "cloud.url";
const SETTING_CLOUD_KEY: &str = "cloud.publishable_key";
const SETTING_ACCOUNT_ID: &str = "cloud.account_id";
const SETTING_ACCOUNT_EMAIL: &str = "cloud.account_email";
const SETTING_ACCOUNT_PENDING: &str = "cloud.account_pending";
const SETTING_BASE_REVISION: &str = "cloud.base_revision";
const SETTING_BASE_HASH: &str = "cloud.base_hash";
const SETTING_LAST_SYNCED_AT: &str = "cloud.last_synced_at";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudConfig {
    pub url: String,
    pub publishable_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCredentials {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyInput {
    pub recovery_key: String,
    pub confirm_replace: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullInput {
    #[serde(default)]
    pub confirm_replace: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub configured: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    pub email_confirmation_pending: bool,
    pub has_recovery_key: bool,
    pub base_revision: u64,
    pub local_changed_since_sync: bool,
    pub last_synced_at: Option<String>,
    pub privacy_boundary: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResult {
    pub signed_in: bool,
    pub email: String,
    pub email_confirmation_pending: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub direction: String,
    pub revision: u64,
    pub content_hash: String,
    pub record_count: usize,
    pub synced_at: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthUser {
    id: String,
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthSession {
    access_token: Option<String>,
    refresh_token: Option<String>,
    user: Option<AuthUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteBlob {
    revision: u64,
    ciphertext: String,
    nonce: String,
    content_hash: String,
    schema_version: u32,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct Session {
    access_token: String,
    user: AuthUser,
}

#[derive(Debug, Clone)]
struct SupabaseProvider {
    client: Client,
    config: CloudConfig,
}

#[async_trait]
trait CloudSyncProvider {
    async fn sign_up(&self, credentials: &AccountCredentials) -> AppResult<AuthSession>;
    async fn sign_in(&self, credentials: &AccountCredentials) -> AppResult<AuthSession>;
    async fn refresh(&self, refresh_token: &str) -> AppResult<AuthSession>;
    async fn sign_out(&self, access_token: &str) -> AppResult<()>;
    async fn fetch_blob(&self, session: &Session) -> AppResult<Option<RemoteBlob>>;
    async fn write_blob(
        &self,
        session: &Session,
        expected_revision: u64,
        blob: &RemoteBlob,
    ) -> AppResult<u64>;
}

impl SupabaseProvider {
    fn new(config: CloudConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.config.url.trim_end_matches('/'), path)
    }

    fn public_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.header("apikey", &self.config.publishable_key)
    }

    fn authenticated_request(
        &self,
        request: reqwest::RequestBuilder,
        access_token: &str,
    ) -> reqwest::RequestBuilder {
        self.public_request(request).bearer_auth(access_token)
    }

    async fn parse_auth(&self, response: reqwest::Response) -> AppResult<AuthSession> {
        if response.status().is_success() {
            return response
                .json::<AuthSession>()
                .await
                .map_err(AppError::Network);
        }
        Err(auth_error(response).await)
    }
}

#[async_trait]
impl CloudSyncProvider for SupabaseProvider {
    async fn sign_up(&self, credentials: &AccountCredentials) -> AppResult<AuthSession> {
        let response = self
            .public_request(self.client.post(self.endpoint("/auth/v1/signup")))
            .json(credentials)
            .send()
            .await?;
        self.parse_auth(response).await
    }

    async fn sign_in(&self, credentials: &AccountCredentials) -> AppResult<AuthSession> {
        let response = self
            .public_request(
                self.client
                    .post(self.endpoint("/auth/v1/token?grant_type=password")),
            )
            .json(credentials)
            .send()
            .await?;
        self.parse_auth(response).await
    }

    async fn refresh(&self, refresh_token: &str) -> AppResult<AuthSession> {
        let response = self
            .public_request(
                self.client
                    .post(self.endpoint("/auth/v1/token?grant_type=refresh_token")),
            )
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()
            .await?;
        self.parse_auth(response).await
    }

    async fn sign_out(&self, access_token: &str) -> AppResult<()> {
        let response = self
            .authenticated_request(
                self.client.post(self.endpoint("/auth/v1/logout")),
                access_token,
            )
            .send()
            .await?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::UNAUTHORIZED
        {
            Ok(())
        } else {
            Err(cloud_error(response).await)
        }
    }

    async fn fetch_blob(&self, session: &Session) -> AppResult<Option<RemoteBlob>> {
        let response = self
            .authenticated_request(
                self.client.get(self.endpoint(&format!(
                    "/rest/v1/sync_blobs?select=revision,ciphertext,nonce,content_hash,schema_version,updated_at&user_id=eq.{}",
                    session.user.id
                ))),
                &session.access_token,
            )
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(cloud_error(response).await);
        }
        let mut rows = response.json::<Vec<RemoteBlob>>().await?;
        Ok(rows.pop())
    }

    async fn write_blob(
        &self,
        session: &Session,
        expected_revision: u64,
        blob: &RemoteBlob,
    ) -> AppResult<u64> {
        let response = self
            .authenticated_request(
                self.client
                    .post(self.endpoint("/rest/v1/rpc/upsert_sync_blob")),
                &session.access_token,
            )
            .json(&serde_json::json!({
                "p_expected_revision": expected_revision,
                "p_ciphertext": blob.ciphertext,
                "p_nonce": blob.nonce,
                "p_content_hash": blob.content_hash,
                "p_schema_version": blob.schema_version,
            }))
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(AppError::Conflict(
                "其他设备已先完成同步，请先拉取最新云端版本".into(),
            ));
        }
        if !response.status().is_success() {
            let error = cloud_error(response).await;
            if error.to_string().contains("sync_revision_conflict") {
                return Err(AppError::Conflict(
                    "其他设备已先完成同步，请先拉取最新云端版本".into(),
                ));
            }
            return Err(error);
        }
        let value = response.json::<serde_json::Value>().await?;
        value
            .as_u64()
            .or_else(|| value.as_array()?.first()?.as_u64())
            .ok_or_else(|| AppError::Cloud("云端没有返回有效的同步版本号".into()))
    }
}

pub fn config(db: &Database) -> AppResult<Option<CloudConfig>> {
    let Some(url) = db.setting(SETTING_CLOUD_URL)? else {
        return Ok(None);
    };
    let Some(publishable_key) = db.setting(SETTING_CLOUD_KEY)? else {
        return Ok(None);
    };
    if url.is_empty() || publishable_key.is_empty() {
        return Ok(None);
    }
    Ok(Some(CloudConfig {
        url,
        publishable_key,
    }))
}

pub fn save_config(db: &Database, next: &CloudConfig) -> AppResult<CloudStatus> {
    validate_config(next)?;
    let changed = config(db)?.is_some_and(|current| {
        current.url != next.url.trim_end_matches('/')
            || current.publishable_key != next.publishable_key.trim()
    });
    db.set_setting(SETTING_CLOUD_URL, next.url.trim_end_matches('/'))?;
    db.set_setting(SETTING_CLOUD_KEY, next.publishable_key.trim())?;
    if changed {
        clear_account_state(db)?;
    }
    status(db)
}

pub fn status(db: &Database) -> AppResult<CloudStatus> {
    let configured = config(db)?.is_some();
    let account_id = db.setting(SETTING_ACCOUNT_ID)?;
    let signed_in = secrets::cloud_tokens()?.is_some()
        && account_id.as_deref().is_some_and(|id| !id.is_empty());
    let base_hash = db.setting(SETTING_BASE_HASH)?;
    let local_hash = db.export_sync_data()?.content_hash()?;
    Ok(CloudStatus {
        configured,
        signed_in,
        email: db.setting(SETTING_ACCOUNT_EMAIL)?,
        email_confirmation_pending: db
            .setting(SETTING_ACCOUNT_PENDING)?
            .is_some_and(|value| value == "true"),
        has_recovery_key: account_id
            .as_deref()
            .map(secrets::cloud_encryption_key)
            .transpose()?
            .flatten()
            .is_some(),
        base_revision: base_revision(db)?,
        local_changed_since_sync: base_hash.as_deref() != Some(local_hash.as_str()),
        last_synced_at: db.setting(SETTING_LAST_SYNCED_AT)?,
        privacy_boundary: vec![
            "云端仅保存端到端加密后的投资数据包".into(),
            "模型 API Key、云端登录令牌保存在本机系统钥匙串且永不同步".into(),
            "首次版本只允许手动同步；检测到双向修改时停止并提示冲突".into(),
        ],
    })
}

pub async fn sign_up(db: &Database, credentials: &AccountCredentials) -> AppResult<AccountResult> {
    validate_credentials(credentials)?;
    let provider = provider(db)?;
    let response = provider.sign_up(credentials).await?;
    let user = match (response.access_token, response.refresh_token, response.user) {
        (Some(access), Some(refresh), Some(user)) => {
            persist_session(db, &access, &refresh, &user)?;
            return Ok(AccountResult {
                signed_in: true,
                email: user.email.unwrap_or_else(|| credentials.email.clone()),
                email_confirmation_pending: false,
                message: "账户已创建并登录".into(),
            });
        }
        (_, _, Some(user)) => user,
        _ => return Err(AppError::Auth("账户服务没有返回用户信息".into())),
    };
    db.set_setting(SETTING_ACCOUNT_ID, &user.id)?;
    db.set_setting(
        SETTING_ACCOUNT_EMAIL,
        user.email.as_deref().unwrap_or(&credentials.email),
    )?;
    db.set_setting(SETTING_ACCOUNT_PENDING, "true")?;
    Ok(AccountResult {
        signed_in: false,
        email: user.email.unwrap_or_else(|| credentials.email.clone()),
        email_confirmation_pending: true,
        message: "账户已创建，请先通过邮件确认后再登录".into(),
    })
}

pub async fn sign_in(db: &Database, credentials: &AccountCredentials) -> AppResult<AccountResult> {
    validate_credentials(credentials)?;
    let provider = provider(db)?;
    let response = provider.sign_in(credentials).await?;
    let access = response
        .access_token
        .ok_or_else(|| AppError::Auth("账户服务没有返回访问令牌".into()))?;
    let refresh = response
        .refresh_token
        .ok_or_else(|| AppError::Auth("账户服务没有返回刷新令牌".into()))?;
    let user = response
        .user
        .ok_or_else(|| AppError::Auth("账户服务没有返回用户信息".into()))?;
    let email = user
        .email
        .clone()
        .unwrap_or_else(|| credentials.email.clone());
    persist_session(db, &access, &refresh, &user)?;
    Ok(AccountResult {
        signed_in: true,
        email,
        email_confirmation_pending: false,
        message: "登录成功，数据仍保留在本机，等待你手动同步".into(),
    })
}

pub async fn sign_out(db: &Database) -> AppResult<CloudStatus> {
    if let (Some(provider), Some((access, _))) = (
        config(db)?.map(SupabaseProvider::new),
        secrets::cloud_tokens()?,
    ) {
        if let Err(error) = provider.sign_out(&access).await {
            tracing::warn!(%error, "cloud sign-out could not revoke the remote session; clearing local session");
        }
    }
    clear_account_state(db)?;
    status(db)
}

pub fn export_recovery_key(db: &Database) -> AppResult<String> {
    let user_id = current_account_id(db)?;
    let value = secrets::cloud_encryption_key(&user_id)?
        .ok_or_else(|| AppError::Validation("尚未生成同步恢复密钥；首次上传时会自动生成".into()))?;
    canonical_recovery_key(&value)
}

pub fn import_recovery_key(db: &Database, input: &RecoveryKeyInput) -> AppResult<()> {
    let user_id = current_account_id(db)?;
    if secrets::cloud_encryption_key(&user_id)?.is_some() && !input.confirm_replace {
        return Err(AppError::Conflict(
            "本机已有恢复密钥；替换前必须明确确认".into(),
        ));
    }
    let canonical = canonical_recovery_key(&input.recovery_key)?;
    secrets::set_cloud_encryption_key(&user_id, &canonical)
}

pub async fn push(db: &Database) -> AppResult<SyncResult> {
    let (provider, session) = authenticated_provider(db).await?;
    let dataset = db.export_sync_data()?;
    let local_hash = dataset.content_hash()?;
    let remote = provider.fetch_blob(&session).await?;
    let expected_revision = base_revision(db)?;
    let base_hash = db.setting(SETTING_BASE_HASH)?;

    match &remote {
        Some(remote) if remote.revision != expected_revision => {
            let detail = if base_hash.as_deref() == Some(local_hash.as_str()) {
                "云端有较新版本，请先拉取"
            } else {
                "本机与云端都已修改，已停止上传以避免覆盖"
            };
            return Err(AppError::Conflict(detail.into()));
        }
        None if expected_revision > 0 => {
            return Err(AppError::Conflict(
                "云端数据已被移除，无法基于旧版本继续上传".into(),
            ));
        }
        _ => {}
    }
    if let Some(remote) = &remote {
        if remote.content_hash == local_hash {
            finish_sync(db, remote.revision, &local_hash)?;
            return Ok(SyncResult {
                direction: "push".into(),
                revision: remote.revision,
                content_hash: local_hash,
                record_count: dataset.record_count(),
                synced_at: Utc::now().to_rfc3339(),
                message: "本机与云端已经一致，无需重复上传".into(),
            });
        }
    }

    let recovery_key = match secrets::cloud_encryption_key(&session.user.id)? {
        Some(value) => {
            let canonical = canonical_recovery_key(&value)?;
            if canonical != value {
                secrets::set_cloud_encryption_key(&session.user.id, &canonical)?;
            }
            canonical
        }
        None => {
            let value = generate_recovery_key();
            secrets::set_cloud_encryption_key(&session.user.id, &value)?;
            value
        }
    };
    let key = parse_recovery_key(&recovery_key)?;
    let mut blob = encrypt_dataset(&dataset, &key)?;
    blob.revision = expected_revision + 1;
    let revision = provider
        .write_blob(&session, expected_revision, &blob)
        .await?;
    finish_sync(db, revision, &local_hash)?;
    Ok(SyncResult {
        direction: "push".into(),
        revision,
        content_hash: local_hash,
        record_count: dataset.record_count(),
        synced_at: Utc::now().to_rfc3339(),
        message: "已上传端到端加密快照；请安全备份恢复密钥".into(),
    })
}

pub async fn pull(db: &Database, input: &PullInput) -> AppResult<SyncResult> {
    let (provider, session) = authenticated_provider(db).await?;
    let remote = provider
        .fetch_blob(&session)
        .await?
        .ok_or_else(|| AppError::Validation("云端还没有可拉取的数据".into()))?;
    let recovery_key = secrets::cloud_encryption_key(&session.user.id)?.ok_or_else(|| {
        AppError::Validation("本机没有恢复密钥，请先从已同步设备导出并在此导入".into())
    })?;
    let key = parse_recovery_key(&recovery_key)?;
    let dataset = decrypt_dataset(&remote, &key)?;
    let local_hash = db.export_sync_data()?.content_hash()?;
    let remote_hash = dataset.content_hash()?;

    if local_hash != remote_hash && !input.confirm_replace {
        return Err(AppError::Conflict(
            "拉取会替换本机投资数据，请确认后重试；模型密钥和账户配置不会被替换".into(),
        ));
    }
    if local_hash != remote_hash {
        db.import_sync_data(&dataset)?;
    }
    finish_sync(db, remote.revision, &remote_hash)?;
    Ok(SyncResult {
        direction: "pull".into(),
        revision: remote.revision,
        content_hash: remote_hash,
        record_count: dataset.record_count(),
        synced_at: Utc::now().to_rfc3339(),
        message: if local_hash == remote.content_hash {
            "本机与云端已经一致".into()
        } else {
            "已验证并恢复云端加密快照".into()
        },
    })
}

fn provider(db: &Database) -> AppResult<SupabaseProvider> {
    config(db)?
        .map(SupabaseProvider::new)
        .ok_or_else(|| AppError::Validation("请先配置云端服务地址和 Publishable Key".into()))
}

fn current_account_id(db: &Database) -> AppResult<String> {
    if secrets::cloud_tokens()?.is_none() {
        return Err(AppError::Auth("请先登录云端账户".into()));
    }
    db.setting(SETTING_ACCOUNT_ID)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Auth("请先登录云端账户".into()))
}

async fn authenticated_provider(db: &Database) -> AppResult<(SupabaseProvider, Session)> {
    let provider = provider(db)?;
    let (_, refresh_token) =
        secrets::cloud_tokens()?.ok_or_else(|| AppError::Auth("请先登录云端账户".into()))?;
    let response = provider.refresh(&refresh_token).await?;
    let access_token = response
        .access_token
        .ok_or_else(|| AppError::Auth("刷新登录状态失败".into()))?;
    let next_refresh_token = response
        .refresh_token
        .ok_or_else(|| AppError::Auth("刷新登录状态失败".into()))?;
    let user = response
        .user
        .ok_or_else(|| AppError::Auth("刷新登录状态时缺少用户信息".into()))?;
    persist_session(db, &access_token, &next_refresh_token, &user)?;
    Ok((provider, Session { access_token, user }))
}

fn persist_session(
    db: &Database,
    access_token: &str,
    refresh_token: &str,
    user: &AuthUser,
) -> AppResult<()> {
    let previous_user = db.setting(SETTING_ACCOUNT_ID)?;
    if previous_user.as_deref().is_some_and(|id| id != user.id) {
        clear_sync_base(db)?;
    }
    secrets::set_cloud_tokens(access_token, refresh_token)?;
    db.set_setting(SETTING_ACCOUNT_ID, &user.id)?;
    db.set_setting(
        SETTING_ACCOUNT_EMAIL,
        user.email.as_deref().unwrap_or_default(),
    )?;
    db.delete_setting(SETTING_ACCOUNT_PENDING)?;
    Ok(())
}

fn clear_account_state(db: &Database) -> AppResult<()> {
    secrets::delete_cloud_tokens()?;
    for key in [
        SETTING_ACCOUNT_ID,
        SETTING_ACCOUNT_EMAIL,
        SETTING_ACCOUNT_PENDING,
    ] {
        db.delete_setting(key)?;
    }
    clear_sync_base(db)
}

fn clear_sync_base(db: &Database) -> AppResult<()> {
    for key in [
        SETTING_BASE_REVISION,
        SETTING_BASE_HASH,
        SETTING_LAST_SYNCED_AT,
    ] {
        db.delete_setting(key)?;
    }
    Ok(())
}

fn finish_sync(db: &Database, revision: u64, hash: &str) -> AppResult<()> {
    db.set_setting(SETTING_BASE_REVISION, &revision.to_string())?;
    db.set_setting(SETTING_BASE_HASH, hash)?;
    db.set_setting(SETTING_LAST_SYNCED_AT, &Utc::now().to_rfc3339())
}

fn base_revision(db: &Database) -> AppResult<u64> {
    db.setting(SETTING_BASE_REVISION)?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| AppError::Validation("本地同步版本号损坏".into()))
        })
        .transpose()
        .map(|value| value.unwrap_or(0))
}

fn validate_config(config: &CloudConfig) -> AppResult<()> {
    let url = config.url.trim_end_matches('/');
    if !url.starts_with("https://")
        && !url.starts_with("http://127.0.0.1")
        && !url.starts_with("http://localhost")
    {
        return Err(AppError::Validation(
            "云端地址必须使用 HTTPS；仅本机服务允许 HTTP".into(),
        ));
    }
    if config.publishable_key.trim().len() < 16 {
        return Err(AppError::Validation("Publishable Key 格式无效".into()));
    }
    if is_service_role_key(config.publishable_key.trim()) {
        return Err(AppError::Validation(
            "禁止使用 service_role/secret Key；这里只能填写 Publishable 或 anon Key".into(),
        ));
    }
    Ok(())
}

fn is_service_role_key(value: &str) -> bool {
    if value.starts_with("sb_secret_") {
        return true;
    }
    let Some(payload) = value.split('.').nth(1) else {
        return false;
    };
    URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|decoded| serde_json::from_slice::<serde_json::Value>(&decoded).ok())
        .and_then(|decoded| decoded.get("role")?.as_str().map(str::to_owned))
        .is_some_and(|role| role == "service_role")
}

fn validate_credentials(credentials: &AccountCredentials) -> AppResult<()> {
    if !credentials.email.contains('@') || credentials.email.chars().count() > 254 {
        return Err(AppError::Validation("请输入有效邮箱".into()));
    }
    if credentials.password.chars().count() < 8 {
        return Err(AppError::Validation("密码至少需要 8 个字符".into()));
    }
    Ok(())
}

fn generate_recovery_key() -> String {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    format!("{RECOVERY_KEY_PREFIX}{}", URL_SAFE_NO_PAD.encode(key))
}

fn parse_recovery_key(value: &str) -> AppResult<[u8; 32]> {
    let value = value.trim();
    let encoded = value
        .strip_prefix(RECOVERY_KEY_PREFIX)
        .or_else(|| value.strip_prefix(LEGACY_RECOVERY_KEY_PREFIX))
        .ok_or_else(|| AppError::Validation("恢复密钥格式无效".into()))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Validation("恢复密钥格式无效".into()))?;
    decoded
        .try_into()
        .map_err(|_| AppError::Validation("恢复密钥长度无效".into()))
}

fn canonical_recovery_key(value: &str) -> AppResult<String> {
    Ok(format!(
        "{RECOVERY_KEY_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(parse_recovery_key(value)?)
    ))
}

fn encrypt_dataset(dataset: &SyncDataset, key: &[u8; 32]) -> AppResult<RemoteBlob> {
    encrypt_dataset_with_context(dataset, key, ENCRYPTION_CONTEXT)
}

fn encrypt_dataset_with_context(
    dataset: &SyncDataset,
    key: &[u8; 32],
    context: &[u8],
) -> AppResult<RemoteBlob> {
    let plaintext = serde_json::to_vec(dataset)?;
    let content_hash = dataset.content_hash()?;
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: context,
            },
        )
        .map_err(|_| AppError::Cloud("无法加密同步数据".into()))?;
    Ok(RemoteBlob {
        revision: 0,
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        content_hash,
        schema_version: SYNC_DATASET_SCHEMA_VERSION,
        updated_at: Utc::now().to_rfc3339(),
    })
}

fn decrypt_dataset(blob: &RemoteBlob, key: &[u8; 32]) -> AppResult<SyncDataset> {
    if !(1..=SYNC_DATASET_SCHEMA_VERSION).contains(&blob.schema_version) {
        return Err(AppError::Validation(format!(
            "云端数据版本 {} 暂不受支持",
            blob.schema_version
        )));
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(&blob.nonce)
        .map_err(|_| AppError::Validation("云端加密随机数损坏".into()))?;
    if nonce.len() != 24 {
        return Err(AppError::Validation("云端加密随机数长度无效".into()));
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&blob.ciphertext)
        .map_err(|_| AppError::Validation("云端密文格式损坏".into()))?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = [ENCRYPTION_CONTEXT, LEGACY_ENCRYPTION_CONTEXT]
        .into_iter()
        .find_map(|context| {
            cipher
                .decrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: context,
                    },
                )
                .ok()
        })
        .ok_or_else(|| AppError::Validation("无法解密云端数据：恢复密钥错误或密文已损坏".into()))?;
    if plaintext.len() > 25 * 1024 * 1024 {
        return Err(AppError::Validation("云端数据包超过 25 MB 安全上限".into()));
    }
    let dataset = serde_json::from_slice::<SyncDataset>(&plaintext)?;
    if dataset.schema_version != blob.schema_version {
        return Err(AppError::Validation("云端数据版本标记不一致".into()));
    }
    dataset.validate()?;
    let content_hash = dataset.content_hash()?;
    if content_hash != blob.content_hash {
        return Err(AppError::Validation("云端数据完整性校验失败".into()));
    }
    Ok(dataset)
}

async fn auth_error(response: reqwest::Response) -> AppError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = error_detail(&body);
    AppError::Auth(format!("{}（HTTP {}）", detail, status.as_u16()))
}

async fn cloud_error(response: reqwest::Response) -> AppError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = error_detail(&body);
    AppError::Cloud(format!("{}（HTTP {}）", detail, status.as_u16()))
}

fn error_detail(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            ["msg", "message", "error_description", "hint", "error"]
                .iter()
                .find_map(|key| value.get(key).and_then(serde_json::Value::as_str))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                "云端服务未返回错误详情".into()
            } else {
                trimmed.chars().take(240).collect()
            }
        })
}

pub fn hash_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, routing::get, routing::post, Json, Router};

    fn dataset() -> SyncDataset {
        let directory = tempfile::tempdir().unwrap();
        Database::open(&directory.path().join("sync.db"))
            .unwrap()
            .export_sync_data()
            .unwrap()
    }

    #[test]
    fn encrypted_bundle_round_trip_and_wrong_key_fails() {
        let source = dataset();
        let key = parse_recovery_key(&generate_recovery_key()).unwrap();
        let blob = encrypt_dataset(&source, &key).unwrap();
        let restored = decrypt_dataset(&blob, &key).unwrap();
        assert_eq!(
            restored.content_hash().unwrap(),
            source.content_hash().unwrap()
        );

        let wrong_key = parse_recovery_key(&generate_recovery_key()).unwrap();
        assert!(decrypt_dataset(&blob, &wrong_key).is_err());

        let legacy_blob =
            encrypt_dataset_with_context(&source, &key, LEGACY_ENCRYPTION_CONTEXT).unwrap();
        assert_eq!(
            decrypt_dataset(&legacy_blob, &key)
                .unwrap()
                .content_hash()
                .unwrap(),
            source.content_hash().unwrap()
        );
    }

    #[test]
    fn decrypts_legacy_v1_through_v5_bundles_and_rejects_mismatched_markers() {
        let key = parse_recovery_key(&generate_recovery_key()).unwrap();
        let mut v5_source = dataset();
        v5_source.schema_version = 5;
        let holdings = v5_source
            .tables
            .iter_mut()
            .find(|table| table.name == "holdings")
            .unwrap();
        for index in [11, 10] {
            holdings.columns.remove(index);
            for row in &mut holdings.rows {
                row.remove(index);
            }
        }
        let events = v5_source
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        for index in [10, 9] {
            events.columns.remove(index);
            for row in &mut events.rows {
                row.remove(index);
            }
        }
        v5_source.validate().unwrap();
        let mut v5_blob = encrypt_dataset(&v5_source, &key).unwrap();
        v5_blob.schema_version = 5;
        assert_eq!(decrypt_dataset(&v5_blob, &key).unwrap().schema_version, 5);

        let mut v4_source = v5_source;
        v4_source.schema_version = 4;
        let events = v4_source
            .tables
            .iter_mut()
            .find(|table| table.name == "portfolio_events")
            .unwrap();
        for index in [4, 3, 2] {
            events.columns.remove(index);
            for row in &mut events.rows {
                row.remove(index);
            }
        }
        v4_source.validate().unwrap();
        let mut v4_blob = encrypt_dataset(&v4_source, &key).unwrap();
        v4_blob.schema_version = 4;
        assert_eq!(decrypt_dataset(&v4_blob, &key).unwrap().schema_version, 4);

        let mut v3_source = v4_source;
        v3_source.schema_version = 3;
        assert_eq!(v3_source.tables.pop().unwrap().name, "portfolio_events");
        v3_source.validate().unwrap();
        let mut v3_blob = encrypt_dataset(&v3_source, &key).unwrap();
        v3_blob.schema_version = 3;
        assert_eq!(decrypt_dataset(&v3_blob, &key).unwrap().schema_version, 3);

        let mut v2_source = v3_source.clone();
        v2_source.schema_version = 2;
        let holdings = v2_source
            .tables
            .iter_mut()
            .find(|table| table.name == "holdings")
            .unwrap();
        for index in [9, 8] {
            holdings.columns.remove(index);
            for row in &mut holdings.rows {
                row.remove(index);
            }
        }
        v2_source.validate().unwrap();
        let mut v2_blob = encrypt_dataset(&v2_source, &key).unwrap();
        v2_blob.schema_version = 2;
        assert_eq!(decrypt_dataset(&v2_blob, &key).unwrap().schema_version, 2);

        let mut source = v2_source;
        source.schema_version = 1;
        assert_eq!(source.tables.pop().unwrap().name, "portfolio_checkins");
        source.validate().unwrap();
        let mut blob = encrypt_dataset(&source, &key).unwrap();
        blob.schema_version = 1;

        let restored = decrypt_dataset(&blob, &key).unwrap();
        assert_eq!(restored.schema_version, 1);
        blob.schema_version = 2;
        assert!(matches!(
            decrypt_dataset(&blob, &key),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn recovery_key_has_versioned_format() {
        let value = generate_recovery_key();
        assert!(value.starts_with(RECOVERY_KEY_PREFIX));
        assert_eq!(parse_recovery_key(&value).unwrap().len(), 32);
        let legacy = value.replacen(RECOVERY_KEY_PREFIX, LEGACY_RECOVERY_KEY_PREFIX, 1);
        assert_eq!(parse_recovery_key(&legacy).unwrap().len(), 32);
        assert_eq!(canonical_recovery_key(&legacy).unwrap(), value);
        assert!(parse_recovery_key("bad").is_err());
    }

    #[test]
    fn rejects_secret_and_legacy_service_role_keys() {
        assert!(is_service_role_key("sb_secret_do-not-use"));
        let payload = URL_SAFE_NO_PAD.encode(br#"{"role":"service_role"}"#);
        assert!(is_service_role_key(&format!("header.{payload}.signature")));
        let anon_payload = URL_SAFE_NO_PAD.encode(br#"{"role":"anon"}"#);
        assert!(!is_service_role_key(&format!(
            "header.{anon_payload}.signature"
        )));
    }

    #[tokio::test]
    async fn supabase_adapter_uses_auth_and_postgrest_protocols() {
        let app = Router::new()
            .route(
                "/auth/v1/token",
                post(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("apikey").unwrap(), "publishable-test-key");
                    Json(serde_json::json!({
                        "access_token": "access",
                        "refresh_token": "refresh",
                        "user": { "id": "user-1", "email": "user@example.com" }
                    }))
                }),
            )
            .route(
                "/rest/v1/sync_blobs",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer access");
                    Json(Vec::<RemoteBlob>::new())
                }),
            )
            .route(
                "/rest/v1/rpc/upsert_sync_blob",
                post(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer access");
                    Json(serde_json::json!(1))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let provider = SupabaseProvider::new(CloudConfig {
            url: format!("http://{address}"),
            publishable_key: "publishable-test-key".into(),
        });
        let response = provider
            .sign_in(&AccountCredentials {
                email: "user@example.com".into(),
                password: "password".into(),
            })
            .await
            .unwrap();
        let session = Session {
            access_token: response.access_token.unwrap(),
            user: response.user.unwrap(),
        };
        assert!(provider.fetch_blob(&session).await.unwrap().is_none());
        let blob = RemoteBlob {
            revision: 1,
            ciphertext: "ciphertext".into(),
            nonce: "nonce".into(),
            content_hash: "hash".into(),
            schema_version: 1,
            updated_at: Utc::now().to_rfc3339(),
        };
        assert_eq!(provider.write_blob(&session, 0, &blob).await.unwrap(), 1);
    }
}
