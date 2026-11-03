//! Local credentials, never account business storage. One cross-process lock
//! covers read/refresh/atomic replacement so rotating tokens cannot race.
use crate::{AppError, AppResult};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct State {
    pub host_id: String,
    pub active: Option<String>,
    pub accounts: Vec<Account>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Account {
    pub id: String,
    pub client_id: String,
    pub subject: String,
    pub email: String,
    pub tokens: Option<Tokens>,
}
impl Account {
    pub fn tokens_for_session(&self, session: &str) -> AppResult<&Tokens> {
        self.tokens
            .as_ref()
            .filter(|tokens| super::plan_enabled(tokens) && tokens.session == session)
            .ok_or_else(|| AppError::Auth("ChatGPT 授权会话已结束，请重新登录并重启桥接".into()))
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
pub(super) struct Store {
    dir: PathBuf,
    _lock: File,
    pub state: State,
}
impl Store {
    pub fn open() -> AppResult<Self> {
        let root =
            dirs::config_dir().ok_or_else(|| AppError::Validation("找不到本机配置目录".into()))?;
        Self::at(&application_directory(&root)?.join("chatgpt"))
    }
    fn at(dir: &Path) -> AppResult<Self> {
        fs::create_dir_all(dir)?;
        reject_link(dir)?;
        protect(dir, true)?;
        let path = dir.join("session.lock");
        checked_exists(&path)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(&path)?;
        protect(&path, false)?;
        lock.try_lock_exclusive().map_err(|_| {
            AppError::Busy("另一个 Mario 登录或凭据刷新正在运行，请稍后重试".into())
        })?;
        let path = dir.join("accounts.json");
        let exists = checked_exists(&path)?;
        let state = if exists {
            protect(&path, false)?;
            use std::io::Read;
            let mut bytes = Vec::new();
            File::open(path)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(AppError::Validation("本机 ChatGPT 配置过大".into()));
            }
            serde_json::from_slice(&bytes).map_err(|_| {
                AppError::Validation("本机 ChatGPT 配置损坏；请保留文件并检查，不会自动覆盖".into())
            })?
        } else {
            State {
                host_id: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
                ..State::default()
            }
        };
        let mut store = Self {
            dir: dir.into(),
            _lock: lock,
            state,
        };
        if !exists {
            store.save()?;
        }
        Ok(store)
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
        let mut tmp = tempfile::NamedTempFile::new_in(&self.dir)?;
        protect(tmp.path(), false)?;
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|_| AppError::Validation("无法保存本机 ChatGPT 配置".into()))?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(self.dir.join("accounts.json"))
            .map_err(|e| AppError::Io(e.error))?;
        #[cfg(unix)]
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }
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
    use super::*;
    #[test]
    fn persists_host_and_serializes_sessions_without_leaking_tokens_to_business_data() {
        let dir = tempfile::tempdir().unwrap();
        let host;
        {
            let mut store = Store::at(dir.path()).unwrap();
            host = store.state.host_id.clone();
            assert!(Store::at(dir.path()).is_err());
            store.state.active = Some("registration".into());
            store.save().unwrap();
        }
        let store = Store::at(dir.path()).unwrap();
        assert_eq!(store.state.host_id, host);
        assert_eq!(store.state.active.as_deref(), Some("registration"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("accounts.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn corrupt_credentials_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("accounts.json"), b"broken").unwrap();
        assert!(Store::at(dir.path()).is_err());
        assert_eq!(
            fs::read(dir.path().join("accounts.json")).unwrap(),
            b"broken"
        );
    }

    #[test]
    fn oversized_credentials_are_bounded_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        let bytes = vec![b'x'; 1024 * 1024 + 1];
        fs::write(&path, &bytes).unwrap();
        let error = match Store::at(dir.path()) {
            Ok(_) => panic!("accepted oversized local credentials"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("配置过大"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
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
    fn token_lookup_rejects_replaced_logged_out_and_disabled_sessions() {
        let mut account = Account {
            id: "registration".into(),
            client_id: "client".into(),
            subject: "subject".into(),
            email: String::new(),
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
        let mut store = Store::at(dir.path()).unwrap();
        let host = store.state.host_id.clone();
        store.state.accounts = vec![
            Account {
                id: "registration-a".into(),
                client_id: "workspace-client-a".into(),
                subject: "same-subject".into(),
                email: "same@example.invalid".into(),
                tokens: Some(local_tokens("session-a")),
            },
            Account {
                id: "registration-b".into(),
                client_id: "workspace-client-b".into(),
                subject: "same-subject".into(),
                email: "same@example.invalid".into(),
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
        let reopened = Store::at(dir.path()).unwrap();
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
        assert!(Store::at(&linked_dir).is_err());
        for name in ["session.lock", "accounts.json"] {
            let dir = tempfile::tempdir().unwrap();
            let file = root.path().join(format!("target-{name}"));
            fs::write(&file, b"unchanged").unwrap();
            symlink(&file, dir.path().join(name)).unwrap();
            assert!(Store::at(dir.path()).is_err());
            assert_eq!(fs::read(&file).unwrap(), b"unchanged");
        }
    }

    #[cfg(windows)]
    fn assert_owner_only_dacl(path: &Path) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::{
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            EqualSid, GetAce, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE,
            DACL_SECURITY_INFORMATION, INHERITED_ACE, OWNER_SECURITY_INFORMATION,
            SE_DACL_PROTECTED,
        };
        let current = current_sid().unwrap();
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
    fn windows_dacl_stays_private_before_and_after_atomic_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::at(dir.path()).unwrap();
        for path in [
            dir.path().to_owned(),
            dir.path().join("accounts.json"),
            dir.path().join("session.lock"),
        ] {
            assert_owner_only_dacl(&path);
        }
        store.state.active = Some("replacement-marker".into());
        store.save().unwrap();
        assert_owner_only_dacl(dir.path());
        assert_owner_only_dacl(&dir.path().join("accounts.json"));
        drop(store);
        let reopened = Store::at(dir.path()).unwrap();
        assert_eq!(reopened.state.active.as_deref(), Some("replacement-marker"));
        assert_owner_only_dacl(&dir.path().join("accounts.json"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_owner_validation_rejects_a_different_sid() {
        let dir = tempfile::tempdir().unwrap();
        let _store = Store::at(dir.path()).unwrap();
        let different = windows_sid("S-1-1-0".into()).unwrap();
        let error = check_owner(&dir.path().join("accounts.json"), &different).unwrap_err();
        assert!(error.to_string().contains("不属于当前 Windows 用户"));
        assert_owner_only_dacl(&dir.path().join("accounts.json"));
    }
}
