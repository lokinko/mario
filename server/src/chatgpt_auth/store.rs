//! Local credentials, never account business storage. One cross-process lock
//! covers read/refresh/atomic replacement so rotating tokens cannot race.
//! Non-secret host metadata lives in chatgpt-host.json; credentials live in
//! OS-encrypted accounts.enc with no plaintext fallback.
use super::crypto::{self, CredentialCipher};
use crate::{AppError, AppResult};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const HOST_FILE: &str = "chatgpt-host.json";
const ENCRYPTED_FILE: &str = "accounts.enc";
const LEGACY_FILE: &str = "accounts.json";
const MAX_HOST_BYTES: usize = 64 * 1024;
const MAX_CREDENTIAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_LEGACY_BYTES: usize = 1024 * 1024;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct State {
    pub host_id: String,
    pub active: Option<String>,
    pub accounts: Vec<Account>,
}

#[derive(Serialize, Deserialize)]
struct HostMetadata {
    host_id: String,
}

#[derive(Default, Serialize, Deserialize)]
struct SecretState {
    active: Option<String>,
    accounts: Vec<Account>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Account {
    pub id: String,
    pub client_id: String,
    pub subject: String,
    pub email: String,
    #[serde(default)]
    pub name: String,
    pub tokens: Option<Tokens>,
}

impl Account {
    pub fn tokens_for_session(&self, session: &str) -> AppResult<&Tokens> {
        self.tokens
            .as_ref()
            .filter(|tokens| super::plan_enabled(tokens) && tokens.session == session)
            .ok_or_else(|| AppError::Auth("ChatGPT 授权会话已结束，请重新登录并重启桥接".into()))
    }

    pub fn display_label(&self) -> Option<String> {
        for candidate in [&self.email, &self.name] {
            let trimmed = candidate.trim();
            if !trimmed.is_empty() && !trimmed.chars().any(char::is_control) {
                return Some(trimmed.chars().take(200).collect());
            }
        }
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Tokens {
    pub session: String,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
    pub earliest_refresh_at: u64,
}

pub(super) struct Store<'a> {
    dir: PathBuf,
    _lock: File,
    cipher: &'a dyn CredentialCipher,
    pub state: State,
}

impl Store<'static> {
    pub fn open() -> AppResult<Self> {
        let root =
            dirs::config_dir().ok_or_else(|| AppError::Validation("找不到本机配置目录".into()))?;
        Self::at_with_cipher(
            &application_directory(&root)?.join("chatgpt"),
            crypto::platform_cipher(),
        )
    }
}

impl<'a> Store<'a> {
    pub fn at_with_cipher(dir: &Path, cipher: &'a dyn CredentialCipher) -> AppResult<Self> {
        fs::create_dir_all(dir)?;
        reject_link(dir)?;
        protect(dir, true)?;
        let lock_path = dir.join("session.lock");
        checked_exists(&lock_path)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(&lock_path)?;
        protect(&lock_path, false)?;
        lock.try_lock_exclusive().map_err(|_| {
            AppError::Busy("另一个 Mario 登录或凭据刷新正在运行，请稍后重试".into())
        })?;

        let host_path = dir.join(HOST_FILE);
        let enc_path = dir.join(ENCRYPTED_FILE);
        let legacy_path = dir.join(LEGACY_FILE);

        let has_host = checked_exists(&host_path)?;
        let has_enc = checked_exists(&enc_path)?;
        let has_legacy = checked_exists(&legacy_path)?;

        let mut host_from_file = if has_host {
            protect(&host_path, false)?;
            let bytes = read_bounded_file(&host_path, MAX_HOST_BYTES, "本机 ChatGPT 主机配置过大")?;
            let meta: HostMetadata = serde_json::from_slice(&bytes).map_err(|_| {
                AppError::Validation(
                    "本机 ChatGPT 主机配置损坏；请保留文件并检查，不会自动覆盖".into(),
                )
            })?;
            if meta.host_id.trim().is_empty() {
                return Err(AppError::Validation(
                    "本机 ChatGPT 主机标识缺失；请保留文件并检查，不会自动覆盖".into(),
                ));
            }
            Some(meta.host_id)
        } else {
            None
        };

        let (state, should_save, remove_legacy) = if has_enc {
            protect(&enc_path, false)?;
            let bytes =
                read_bounded_file(&enc_path, MAX_CREDENTIAL_BYTES, "本机 ChatGPT 凭据过大")?;
            let plaintext = crypto::open(cipher, &bytes)?;
            let secrets: SecretState = serde_json::from_slice(&plaintext).map_err(|_| {
                AppError::Validation(
                    "本机 ChatGPT 凭据内容损坏；请保留文件并检查，不会自动覆盖".into(),
                )
            })?;
            let host_missing = host_from_file.is_none();
            let host_id = host_from_file
                .take()
                .unwrap_or_else(|| format!("urn:uuid:{}", uuid::Uuid::new_v4()));
            (
                State {
                    host_id,
                    active: secrets.active,
                    accounts: secrets.accounts,
                },
                host_missing,
                false,
            )
        } else if has_legacy {
            // One-time in-place upgrade from legacy plaintext accounts.json.
            // Fails closed without deleting the legacy file if encryption fails.
            protect(&legacy_path, false)?;
            let bytes = read_bounded_file(&legacy_path, MAX_LEGACY_BYTES, "本机 ChatGPT 配置过大")?;
            let mut legacy: State = serde_json::from_slice(&bytes).map_err(|_| {
                AppError::Validation("本机 ChatGPT 配置损坏；请保留文件并检查，不会自动覆盖".into())
            })?;
            if legacy.host_id.trim().is_empty() {
                legacy.host_id = host_from_file
                    .take()
                    .unwrap_or_else(|| format!("urn:uuid:{}", uuid::Uuid::new_v4()));
            }
            (legacy, true, true)
        } else {
            let host_id = host_from_file
                .take()
                .unwrap_or_else(|| format!("urn:uuid:{}", uuid::Uuid::new_v4()));
            (
                State {
                    host_id,
                    ..State::default()
                },
                true,
                false,
            )
        };

        let mut store = Self {
            dir: dir.into(),
            _lock: lock,
            cipher,
            state,
        };
        if should_save {
            store.save()?;
        }
        if remove_legacy {
            fs::remove_file(&legacy_path)?;
            #[cfg(unix)]
            File::open(&store.dir)?.sync_all()?;
        }
        Ok(store)
    }

    pub fn cipher_provider(&self) -> &'static str {
        self.cipher.provider_id()
    }

    pub fn account(&self, id: &str) -> AppResult<&Account> {
        self.state
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| {
                AppError::Validation(
                    "未找到此本机 ChatGPT 注册，请运行 chatgpt:auth -- status".into(),
                )
            })
    }

    pub fn authorized(&self, id: &str, session: &str) -> AppResult<bool> {
        Ok(self.account(id)?.tokens_for_session(session).is_ok())
    }

    pub fn save(&mut self) -> AppResult<()> {
        let secret_bytes = serde_json::to_vec(&SecretState {
            active: self.state.active.clone(),
            accounts: self.state.accounts.clone(),
        })
        .map_err(|_| AppError::Validation("无法序列化本机 ChatGPT 凭据".into()))?;
        let sealed = crypto::seal(self.cipher, &secret_bytes)?;
        let host_bytes = serde_json::to_vec(&HostMetadata {
            host_id: self.state.host_id.clone(),
        })
        .map_err(|_| AppError::Validation("无法保存本机 ChatGPT 主机配置".into()))?;

        // Persist encrypted credentials first; if encryption or disk write fails,
        // no partial host-only state replaces existing credentials.
        atomic_write(&self.dir, ENCRYPTED_FILE, &sealed)?;
        atomic_write(&self.dir, HOST_FILE, &host_bytes)?;
        #[cfg(unix)]
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }
}

fn read_bounded_file(path: &Path, limit: usize, too_large_message: &str) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(AppError::Validation(too_large_message.into()));
    }
    Ok(bytes)
}

fn atomic_write(dir: &Path, filename: &str, bytes: &[u8]) -> AppResult<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    protect(tmp.path(), false)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(dir.join(filename))
        .map_err(|e| AppError::Io(e.error))?;
    Ok(())
}

fn application_directory(root: &Path) -> AppResult<PathBuf> {
    // Missing ~/.config is normal on a first launch; existing ancestors,
    // including dangling links/reparse points, must still be validated.
    for ancestor in root.ancestors() {
        checked_exists(ancestor)?;
    }
    let app = root.join("mario");
    fs::create_dir_all(&app)?;
    reject_link(&app)?;
    protect(&app, true)?;
    Ok(app)
}

fn checked_exists(path: &Path) -> AppResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            reject_link(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn reject_link(path: &Path) -> AppResult<()> {
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(AppError::Validation("凭据路径不能是重解析点".into()));
        }
    }
    if meta.file_type().is_symlink() {
        return Err(AppError::Validation("凭据路径不能是符号链接".into()));
    }
    Ok(())
}

#[cfg(unix)]
fn protect(path: &Path, directory: bool) -> AppResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::Validation("凭据路径不能是符号链接".into()));
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(AppError::Validation(
            "凭据路径不属于当前用户，拒绝更改权限".into(),
        ));
    }
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )?;
    Ok(())
}

#[cfg(windows)]
struct LocalAllocation(*mut std::ffi::c_void);
#[cfg(windows)]
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::LocalFree(self.0);
            }
        }
    }
}
#[cfg(windows)]
struct WindowsSid {
    text: String,
    raw: LocalAllocation,
}
#[cfg(windows)]
fn windows_sid(text: String) -> AppResult<WindowsSid> {
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    let encoded: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut raw = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(encoded.as_ptr(), &mut raw) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(WindowsSid {
        text,
        raw: LocalAllocation(raw),
    })
}
#[cfg(windows)]
fn current_sid() -> AppResult<WindowsSid> {
    use std::{os::windows::process::CommandExt, process::Command, sync::OnceLock};
    // Immutable process identity only: account/session state is never cached.
    static CURRENT_SID: OnceLock<String> = OnceLock::new();
    if let Some(sid) = CURRENT_SID.get() {
        return windows_sid(sid.clone());
    }
    // Resolve the actual process identity, not a caller-controlled USERNAME.
    let root = std::env::var_os("SystemRoot")
        .ok_or_else(|| AppError::Validation("缺少 Windows 系统目录".into()))?;
    let output = Command::new(PathBuf::from(root).join("System32/whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .split('"')
        .find(|part| {
            part.starts_with("S-1-")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b'S' || b == b'-')
        })
        .filter(|_| output.status.success())
        .ok_or_else(|| AppError::Validation("无法确认 Windows 凭据所有者".into()))?;
    let sid = CURRENT_SID.get_or_init(|| sid.into());
    windows_sid(sid.clone())
}
#[cfg(windows)]
fn check_owner(path: &Path, expected: &WindowsSid) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::{
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        EqualSid, OWNER_SECURITY_INFORMATION,
    };
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = std::ptr::null_mut();
    let mut security = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut security,
        )
    };
    let _security = LocalAllocation(security);
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status as i32).into());
    }
    if owner.is_null() || unsafe { EqualSid(owner, expected.raw.0) } == 0 {
        return Err(AppError::Validation(
            "凭据路径不属于当前 Windows 用户，拒绝更改权限".into(),
        ));
    }
    Ok(())
}
#[cfg(windows)]
fn protect(path: &Path, directory: bool) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SetFileSecurityW,
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let sid = current_sid()?;
    // An owner can rewrite its object's DACL regardless of ordinary grants.
    // Never "repair" a foreign-owned path into apparently private storage.
    check_owner(path, &sid)?;
    let sddl = format!(
        "D:P(A;{};FA;;;{})",
        if directory { "OICI" } else { "" },
        sid.text
    );
    let descriptor: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut security = std::ptr::null_mut();
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            1,
            &mut security,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let security = LocalAllocation(security);
        if SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            security.0,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
#[cfg(not(any(windows, unix)))]
fn protect(_: &Path, _: bool) -> AppResult<()> {
    Err(AppError::Validation(
        "当前平台不支持安全保存 OAuth 凭据".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::crypto::tests::{TestCipher, UnavailableCipher};
    use super::{
        Account, Store, Tokens, ENCRYPTED_FILE, HOST_FILE, LEGACY_FILE, MAX_CREDENTIAL_BYTES,
        MAX_LEGACY_BYTES,
    };
    use crate::AppError;
    use std::{fs, path::Path};

    const CIPHER: TestCipher = TestCipher {
        id: "test-cipher",
        key: [5u8; 32],
    };

    fn open_test(dir: &Path) -> crate::AppResult<Store<'static>> {
        Store::at_with_cipher(dir, &CIPHER)
    }

    fn local_tokens(session: &str) -> Tokens {
        Tokens {
            session: session.into(),
            access_token: "test-only-access".into(),
            refresh_token: "test-only-refresh".into(),
            id_token: "test-only-id".into(),
            scopes: super::super::SCOPES
                .split_whitespace()
                .map(String::from)
                .collect(),
            expires_at: super::super::now() + 3600,
            earliest_refresh_at: 0,
        }
    }

    #[test]
    fn persists_host_separately_and_encrypts_credentials_at_rest() {
        let dir = tempfile::tempdir().unwrap();
        let host;
        {
            let mut store = open_test(dir.path()).unwrap();
            host = store.state.host_id.clone();
            assert!(open_test(dir.path()).is_err());
            store.state.active = Some("registration".into());
            store.state.accounts.push(Account {
                id: "registration".into(),
                client_id: "oaiapp_secret_client".into(),
                subject: "subject-1".into(),
                email: "user@example.invalid".into(),
                name: "Test User".into(),
                tokens: Some(local_tokens("session-1")),
            });
            store.save().unwrap();
        }
        assert!(!dir.path().join(LEGACY_FILE).exists());
        let host_raw = fs::read_to_string(dir.path().join(HOST_FILE)).unwrap();
        assert!(host_raw.contains(&host));
        for secret in [
            "test-only-access",
            "test-only-refresh",
            "oaiapp_secret_client",
        ] {
            assert!(!host_raw.contains(secret));
        }
        let enc_raw = fs::read_to_string(dir.path().join(ENCRYPTED_FILE)).unwrap();
        assert!(enc_raw.contains("test-cipher"));
        for secret in [
            "test-only-access",
            "test-only-refresh",
            "test-only-id",
            "oaiapp_secret_client",
            "user@example.invalid",
        ] {
            assert!(
                !enc_raw.contains(secret),
                "encrypted store leaked plaintext {secret}"
            );
        }

        let store = open_test(dir.path()).unwrap();
        assert_eq!(store.state.host_id, host);
        assert_eq!(store.state.active.as_deref(), Some("registration"));
        assert_eq!(
            store
                .account("registration")
                .unwrap()
                .display_label()
                .as_deref(),
            Some("user@example.invalid")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in [HOST_FILE, ENCRYPTED_FILE] {
                assert_eq!(
                    fs::metadata(dir.path().join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }
    }

    #[test]
    fn migrates_legacy_plaintext_accounts_in_place_and_fails_closed_without_cipher() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = serde_json::json!({
            "host_id": "urn:uuid:legacy-host",
            "active": "legacy-reg",
            "accounts": [{
                "id": "legacy-reg",
                "client_id": "oaiapp_legacy",
                "subject": "legacy-sub",
                "email": "legacy@example.invalid",
                "tokens": {
                    "session": "legacy-session",
                    "access_token": "legacy-access-secret",
                    "refresh_token": "legacy-refresh-secret",
                    "id_token": "legacy-id-secret",
                    "scopes": super::super::SCOPES.split_whitespace().collect::<Vec<_>>(),
                    "expires_at": super::super::now() + 3600,
                    "earliest_refresh_at": 0
                }
            }]
        });
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        fs::write(dir.path().join(LEGACY_FILE), &legacy_bytes).unwrap();

        // Unavailable OS encryption must fail closed and preserve legacy file untouched.
        assert!(Store::at_with_cipher(dir.path(), &UnavailableCipher).is_err());
        assert_eq!(
            fs::read(dir.path().join(LEGACY_FILE)).unwrap(),
            legacy_bytes
        );
        assert!(!dir.path().join(ENCRYPTED_FILE).exists());

        // With working OS encryption, legacy accounts.json migrates atomically and is removed.
        {
            let store = open_test(dir.path()).unwrap();
            assert_eq!(store.state.host_id, "urn:uuid:legacy-host");
            assert_eq!(store.state.active.as_deref(), Some("legacy-reg"));
            assert!(store.authorized("legacy-reg", "legacy-session").unwrap());
        }
        assert!(!dir.path().join(LEGACY_FILE).exists());
        assert!(dir.path().join(HOST_FILE).exists());
        let encrypted = fs::read_to_string(dir.path().join(ENCRYPTED_FILE)).unwrap();
        assert!(!encrypted.contains("legacy-access-secret"));
        assert!(!encrypted.contains("legacy-refresh-secret"));
    }

    #[test]
    fn fresh_store_fails_closed_without_os_encryption_and_writes_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Store::at_with_cipher(dir.path(), &UnavailableCipher).is_err());
        assert!(!dir.path().join(ENCRYPTED_FILE).exists());
        assert!(!dir.path().join(LEGACY_FILE).exists());
    }

    #[test]
    fn corrupt_credentials_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(ENCRYPTED_FILE), b"broken").unwrap();
        assert!(open_test(dir.path()).is_err());
        assert_eq!(
            fs::read(dir.path().join(ENCRYPTED_FILE)).unwrap(),
            b"broken"
        );

        let legacy_dir = tempfile::tempdir().unwrap();
        fs::write(legacy_dir.path().join(LEGACY_FILE), b"broken-legacy").unwrap();
        assert!(open_test(legacy_dir.path()).is_err());
        assert_eq!(
            fs::read(legacy_dir.path().join(LEGACY_FILE)).unwrap(),
            b"broken-legacy"
        );
    }

    #[test]
    fn oversized_credentials_are_bounded_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ENCRYPTED_FILE);
        let bytes = vec![b'x'; MAX_CREDENTIAL_BYTES + 1];
        fs::write(&path, &bytes).unwrap();
        let error = match open_test(dir.path()) {
            Ok(_) => panic!("accepted oversized local credentials"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("过大"));
        assert_eq!(fs::read(&path).unwrap(), bytes);

        let legacy_dir = tempfile::tempdir().unwrap();
        let legacy_path = legacy_dir.path().join(LEGACY_FILE);
        let legacy_bytes = vec![b'y'; MAX_LEGACY_BYTES + 1];
        fs::write(&legacy_path, &legacy_bytes).unwrap();
        assert!(open_test(legacy_dir.path()).is_err());
        assert_eq!(fs::read(&legacy_path).unwrap(), legacy_bytes);
    }

    #[test]
    fn token_lookup_rejects_replaced_logged_out_and_disabled_sessions() {
        let mut account = Account {
            id: "registration".into(),
            client_id: "client".into(),
            subject: "subject".into(),
            email: String::new(),
            name: String::new(),
            tokens: Some(local_tokens("original")),
        };
        assert!(account.tokens_for_session("original").is_ok());
        // Simulate login completing between the worker check and token lookup.
        account.tokens = Some(local_tokens("replacement"));
        let error = account.tokens_for_session("original").err().unwrap();
        assert!(matches!(error, AppError::Auth(_)));
        assert!(!error.to_string().contains("test-only-access"));
        assert!(account.tokens_for_session("replacement").is_ok());
        account.tokens.as_mut().unwrap().refresh_token.clear();
        assert!(account.tokens_for_session("replacement").is_err());
        account.tokens = None;
        assert!(account.tokens_for_session("replacement").is_err());
    }

    #[test]
    fn pinned_sessions_survive_selection_but_not_logout_or_new_login() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = open_test(dir.path()).unwrap();
        let host = store.state.host_id.clone();
        store.state.accounts = vec![
            Account {
                id: "registration-a".into(),
                client_id: "workspace-client-a".into(),
                subject: "same-subject".into(),
                email: "same@example.invalid".into(),
                name: String::new(),
                tokens: Some(local_tokens("session-a")),
            },
            Account {
                id: "registration-b".into(),
                client_id: "workspace-client-b".into(),
                subject: "same-subject".into(),
                email: "same@example.invalid".into(),
                name: String::new(),
                tokens: Some(local_tokens("session-b")),
            },
        ];
        store.state.active = Some("registration-a".into());
        store.save().unwrap();
        assert!(store.authorized("registration-a", "session-a").unwrap());
        assert!(!store.authorized("registration-a", "session-b").unwrap());
        assert!(store.authorized("registration-b", "session-b").unwrap());

        store.state.active = Some("registration-b".into());
        store.save().unwrap();
        assert!(store.authorized("registration-a", "session-a").unwrap());
        super::super::clear_tokens(&mut store, "registration-a").unwrap();
        assert!(!store.authorized("registration-a", "session-a").unwrap());
        assert!(store.authorized("registration-b", "session-b").unwrap());
        assert_eq!(store.state.host_id, host);
        assert_eq!(store.state.active.as_deref(), Some("registration-b"));
        assert_eq!(store.state.accounts.len(), 2);
        assert_eq!(
            store.account("registration-a").unwrap().client_id,
            "workspace-client-a"
        );
        assert_eq!(
            store.account("registration-a").unwrap().subject,
            "same-subject"
        );

        store.state.accounts[0].tokens = Some(local_tokens("new-session-a"));
        store.save().unwrap();
        assert!(!store.authorized("registration-a", "session-a").unwrap());
        assert!(store.authorized("registration-a", "new-session-a").unwrap());
        store.state.accounts[1]
            .tokens
            .as_mut()
            .unwrap()
            .scopes
            .retain(|scope| scope != "chatgpt.tokens.use.direct");
        store.save().unwrap();
        assert!(!store.authorized("registration-b", "session-b").unwrap());
        drop(store);
        let reopened = open_test(dir.path()).unwrap();
        assert_eq!(reopened.state.host_id, host);
        assert!(reopened
            .authorized("registration-a", "new-session-a")
            .unwrap());
        assert!(!reopened.authorized("registration-b", "session-b").unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_directory_lock_and_credentials_without_touching_targets() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let linked_dir = root.path().join("linked-dir");
        symlink(&target, &linked_dir).unwrap();
        assert!(open_test(&linked_dir).is_err());
        for name in ["session.lock", HOST_FILE, ENCRYPTED_FILE, LEGACY_FILE] {
            let dir = tempfile::tempdir().unwrap();
            let file = root.path().join(format!("target-{name}"));
            fs::write(&file, b"unchanged").unwrap();
            symlink(&file, dir.path().join(name)).unwrap();
            assert!(open_test(dir.path()).is_err());
            assert_eq!(fs::read(&file).unwrap(), b"unchanged");
        }
    }

    #[cfg(windows)]
    fn assert_owner_only_dacl(path: &Path) {
        use super::{check_owner, current_sid, LocalAllocation};
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::{
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            EqualSid, GetAce, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE,
            DACL_SECURITY_INFORMATION, INHERITED_ACE, OWNER_SECURITY_INFORMATION,
            SE_DACL_PROTECTED,
        };
        let current = current_sid().unwrap();
        check_owner(path, &current).unwrap();
        let encoded: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut owner = std::ptr::null_mut();
        let mut dacl = std::ptr::null_mut();
        let mut security = std::ptr::null_mut();
        unsafe {
            let status = GetNamedSecurityInfoW(
                encoded.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut security,
            );
            let security = LocalAllocation(security);
            assert_eq!(status, 0);
            assert!(!owner.is_null());
            assert_ne!(EqualSid(owner, current.raw.0), 0);
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                GetSecurityDescriptorControl(security.0, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert!(!dacl.is_null());
            assert_eq!((*dacl).AceCount, 1);
            let mut ace = std::ptr::null_mut();
            assert_ne!(GetAce(dacl, 0, &mut ace), 0);
            assert!(!ace.is_null());
            let allowed = &*ace.cast::<ACCESS_ALLOWED_ACE>();
            // ACCESS_ALLOWED_ACE_TYPE is zero; FA is FILE_ALL_ACCESS 0x1f01ff.
            assert_eq!(allowed.Header.AceType, 0);
            assert_eq!(u32::from(allowed.Header.AceFlags) & INHERITED_ACE, 0);
            assert_eq!(allowed.Mask, 0x001f01ff);
            let ace_sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
            assert_ne!(EqualSid(ace_sid, owner), 0);
            assert_ne!(EqualSid(ace_sid, current.raw.0), 0);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_dpapi_store_and_dacl_stay_private_before_and_after_atomic_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let mut store =
            Store::at_with_cipher(dir.path(), super::crypto::platform_cipher()).unwrap();
        for path in [
            dir.path().to_owned(),
            dir.path().join(HOST_FILE),
            dir.path().join(ENCRYPTED_FILE),
            dir.path().join("session.lock"),
        ] {
            assert_owner_only_dacl(&path);
        }
        store.state.active = Some("replacement-marker".into());
        store.state.accounts.push(Account {
            id: "replacement-marker".into(),
            client_id: "oaiapp_dpapi_test".into(),
            subject: "sub-dpapi".into(),
            email: "dpapi@example.invalid".into(),
            name: "DPAPI User".into(),
            tokens: Some(local_tokens("dpapi-session")),
        });
        store.save().unwrap();
        assert_owner_only_dacl(dir.path());
        assert_owner_only_dacl(&dir.path().join(HOST_FILE));
        assert_owner_only_dacl(&dir.path().join(ENCRYPTED_FILE));
        let raw = fs::read_to_string(dir.path().join(ENCRYPTED_FILE)).unwrap();
        assert!(raw.contains("windows-dpapi"));
        assert!(!raw.contains("test-only-access"));
        assert!(!raw.contains("oaiapp_dpapi_test"));
        drop(store);
        let reopened = Store::at_with_cipher(dir.path(), super::crypto::platform_cipher()).unwrap();
        assert_eq!(reopened.state.active.as_deref(), Some("replacement-marker"));
        assert!(reopened
            .authorized("replacement-marker", "dpapi-session")
            .unwrap());
        assert_owner_only_dacl(&dir.path().join(ENCRYPTED_FILE));
    }

    #[cfg(windows)]
    #[test]
    fn windows_owner_validation_rejects_a_different_sid() {
        let dir = tempfile::tempdir().unwrap();
        let _store = open_test(dir.path()).unwrap();
        let different = super::windows_sid("S-1-1-0".into()).unwrap();
        let error = super::check_owner(&dir.path().join(ENCRYPTED_FILE), &different).unwrap_err();
        assert!(error.to_string().contains("不属于当前 Windows 用户"));
        assert_owner_only_dacl(&dir.path().join(ENCRYPTED_FILE));
    }
}
