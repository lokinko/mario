//! Self-hosted account/session authority. No third-party identity service.
use crate::{storage::Connection, AppError, AppResult};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

pub struct Accounts {
    connection: Mutex<Connection>,
    pub url: String,
    dummy_hash: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credentials {
    pub email: String,
    pub password: String,
    pub registration_key: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub expires_at: i64,
}

impl Accounts {
    fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Busy("账户服务暂时不可用".into()))?;
        // The account connection outlives a request; recover it after a database
        // restart, before beginning any account transaction. Never replay writes.
        if connection
            .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .is_err()
        {
            *connection = Connection::postgres(&self.url, "mario_auth")?;
        }
        Ok(connection)
    }
    pub fn open(url: String) -> AppResult<Self> {
        let connection = Connection::postgres(&url, "mario_auth")?;
        connection.execute_batch("SELECT pg_advisory_lock(hashtextextended('mario_auth',0));")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,expires_at BIGINT NOT NULL); CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires_at);")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS login_limits(email TEXT PRIMARY KEY, attempts INTEGER NOT NULL, reset_at BIGINT NOT NULL);")?;
        connection.execute_batch("SELECT pg_advisory_unlock(hashtextextended('mario_auth',0));")?;
        let dummy_hash = hash_password("unused-dummy-password")?;
        Ok(Self {
            connection: Mutex::new(connection),
            url,
            dummy_hash,
        })
    }
    pub fn register(&self, input: &Credentials) -> AppResult<Session> {
        let key = std::env::var("MARIO_REGISTRATION_KEY").unwrap_or_default();
        if key.len() < 32
            || !input
                .registration_key
                .as_deref()
                .is_some_and(|v| crate::token_matches(&key, v))
        {
            return Err(AppError::Auth("注册需要管理员提供的有效邀请码".into()));
        }
        let email = validate(input)?;
        let hash = hash_password(&input.password)?;
        let id = uuid::Uuid::new_v4().to_string();
        let connection = self.conn()?;
        let count = connection.execute("INSERT INTO users(id,email,password_hash) VALUES (?1,?2,?3) ON CONFLICT(email) DO NOTHING",params![id,email,hash])?;
        if count == 0 {
            return Err(AppError::Validation("无法注册该账户".into()));
        }
        self.session(&connection, &id, &email)
    }
    pub fn login(&self, input: &Credentials) -> AppResult<Session> {
        let email = validate(input)?;
        let connection = self.conn()?;
        let now = chrono::Utc::now().timestamp();
        connection.execute("DELETE FROM login_limits WHERE reset_at<?1", [now])?;
        let attempts=connection.query_row("INSERT INTO login_limits(email,attempts,reset_at) VALUES (?1,1,?2) ON CONFLICT(email) DO UPDATE SET attempts=login_limits.attempts+1 RETURNING attempts",params![email,now+60],|r|r.get::<_,i64>(0))?;
        if attempts > 10 {
            return Err(AppError::RateLimit("登录尝试过多，请一分钟后重试".into()));
        }
        let found = connection.query_row(
            "SELECT id,password_hash FROM users WHERE email=?1",
            [email.as_str()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        // Spend the same password-work budget for nonexistent accounts.
        drop(connection);
        let (id, hash) = found.unwrap_or_else(|_| (String::new(), self.dummy_hash.clone()));
        let valid = PasswordHash::new(&hash).ok().is_some_and(|h| {
            Argon2::default()
                .verify_password(input.password.as_bytes(), &h)
                .is_ok()
        });
        if !valid || id.is_empty() {
            return Err(AppError::Auth("邮箱或密码错误".into()));
        }
        let mut connection = self.conn()?;
        // A reset on another API instance must not race an already-running
        // password verification and leave an old-password session alive.
        let tx = connection.transaction()?;
        let current = tx.query_row(
            "SELECT password_hash FROM users WHERE id=?1 FOR UPDATE",
            [id.as_str()],
            |r| r.get::<_, String>(0),
        )?;
        if current != hash {
            return Err(AppError::Auth("密码已更新，请重新登录".into()));
        }
        let session = self.session(&tx, &id, &email)?;
        tx.commit()?;
        Ok(session)
    }
    fn session(&self, c: &Connection, id: &str, email: &str) -> AppResult<Session> {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expires_at = chrono::Utc::now().timestamp() + 7 * 24 * 3600;
        c.execute(
            "DELETE FROM sessions WHERE expires_at<?1",
            [chrono::Utc::now().timestamp()],
        )?;
        c.execute(
            "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES (?1,?2,?3)",
            params![digest(&token), id, expires_at],
        )?;
        Ok(Session {
            token,
            user_id: id.into(),
            email: email.into(),
            expires_at,
        })
    }
    pub fn authenticate(&self, token: &str) -> AppResult<String> {
        self.conn()?
            .query_row(
                "SELECT user_id FROM sessions WHERE token_hash=?1 AND expires_at>?2",
                params![digest(token), chrono::Utc::now().timestamp()],
                |r| r.get(0),
            )
            .map_err(|_| AppError::Auth("请重新登录".into()))
    }
    pub fn logout(&self, token: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM sessions WHERE token_hash=?1", [digest(token)])?;
        Ok(())
    }
    pub fn reset_password(&self, email: &str, password: &str) -> AppResult<()> {
        let email = validate(&Credentials {
            email: email.into(),
            password: password.into(),
            registration_key: None,
        })?;
        let hash = hash_password(password)?;
        let mut connection = self.conn()?;
        let tx = connection.transaction()?;
        let id = tx
            .query_row("SELECT id FROM users WHERE email=?1", [email], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|_| AppError::NotFound("账户不存在".into()))?;
        tx.execute(
            "UPDATE users SET password_hash=?1 WHERE id=?2",
            params![hash, id],
        )?;
        tx.execute("DELETE FROM sessions WHERE user_id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
}
fn hash_password(password: &str) -> AppResult<String> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|h| h.to_string())
        .map_err(|_| AppError::Validation("密码处理失败".into()))
}
fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn validate(input: &Credentials) -> AppResult<String> {
    let email = input.email.trim().to_lowercase();
    if email.len() > 254
        || !email.contains('@')
        || email.chars().any(char::is_whitespace)
        || !(12..=256).contains(&input.password.len())
    {
        return Err(AppError::Validation(
            "请输入有效邮箱及 12–256 字节密码".into(),
        ));
    }
    Ok(email)
}
