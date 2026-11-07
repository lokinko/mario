//! Self-hosted account/session authority with optional verified external identity.
mod openai;
use crate::{
    storage::{params, Connection},
    AppError, AppResult,
};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
pub(crate) use openai::{OpenAiAttempt, OpenAiIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

const AUTH_SCHEMA: &str = "mario_auth";

pub struct Accounts {
    connection: Mutex<Connection>,
    pub url: String,
    schema: String,
    dummy_hash: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credentials {
    pub email: String,
    pub password: String,
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
            *connection = Connection::postgres(&self.url, &self.schema)?;
        }
        Ok(connection)
    }
    pub fn open(url: String) -> AppResult<Self> {
        Self::open_schema(url, AUTH_SCHEMA.into())
    }
    fn open_schema(url: String, schema: String) -> AppResult<Self> {
        let connection = Connection::postgres(&url, &schema)?;
        connection.execute_batch(&format!(
            "SELECT pg_advisory_lock(hashtextextended('{schema}',0));"
        ))?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,expires_at BIGINT NOT NULL); CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires_at);")?;
        // One-time upgrade of the explicitly designated existing account. New registrations
        // always retain the default role, including registrations using this email later.
        connection.execute_batch("DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='users' AND column_name='is_superuser') THEN
                ALTER TABLE users ADD COLUMN is_superuser BOOLEAN NOT NULL DEFAULT FALSE;
                UPDATE users SET is_superuser=TRUE WHERE email='lokinko.cs@gmail.com';
            END IF;
        END $$;")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS login_limits(email TEXT PRIMARY KEY, attempts INTEGER NOT NULL, reset_at BIGINT NOT NULL);")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS external_identities(issuer TEXT NOT NULL,client_id TEXT NOT NULL,subject TEXT NOT NULL,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,PRIMARY KEY(issuer,client_id,subject)); CREATE INDEX IF NOT EXISTS external_identity_user ON external_identities(user_id); CREATE TABLE IF NOT EXISTS openai_login_attempts(state_hash TEXT PRIMARY KEY,browser_hash TEXT NOT NULL,verifier TEXT NOT NULL,nonce TEXT NOT NULL,client_id TEXT NOT NULL,redirect_uri TEXT NOT NULL,link_session TEXT,expires_at BIGINT NOT NULL); CREATE INDEX IF NOT EXISTS openai_attempt_expiry ON openai_login_attempts(expires_at);")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS openai_login_results(result_hash TEXT PRIMARY KEY,browser_hash TEXT NOT NULL,session_hash TEXT,expires_at BIGINT NOT NULL); CREATE INDEX IF NOT EXISTS openai_result_expiry ON openai_login_results(expires_at);")?;
        connection.execute_batch(&format!(
            "SELECT pg_advisory_unlock(hashtextextended('{schema}',0));"
        ))?;
        let dummy_hash = hash_password("unused-dummy-password")?;
        Ok(Self {
            connection: Mutex::new(connection),
            url,
            schema,
            dummy_hash,
        })
    }
    pub(crate) fn user_ids(&self) -> AppResult<Vec<String>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT id FROM users ORDER BY id")?;
        Ok(statement
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn register(&self, input: &Credentials) -> AppResult<Session> {
        let email = validate(input)?;
        let hash = hash_password(&input.password)?;
        let id = uuid::Uuid::new_v4().to_string();
        let connection = self.conn()?;
        let count = connection.execute("INSERT INTO users(id,email,password_hash) VALUES ($1,$2,$3) ON CONFLICT(email) DO NOTHING",params![id,email,hash])?;
        if count == 0 {
            return Err(AppError::Validation("无法注册该账户".into()));
        }
        self.session(&connection, &id, &email)
    }
    pub fn login(&self, input: &Credentials) -> AppResult<Session> {
        let email = validate(input)?;
        let connection = self.conn()?;
        let now = chrono::Utc::now().timestamp();
        connection.execute("DELETE FROM login_limits WHERE reset_at<$1", [now])?;
        let attempts=connection.query_row("INSERT INTO login_limits(email,attempts,reset_at) VALUES ($1,1,$2) ON CONFLICT(email) DO UPDATE SET attempts=login_limits.attempts+1 RETURNING attempts",params![email,now+60],|r|r.get::<_,i64>(0))?;
        if attempts > 10 {
            return Err(AppError::RateLimit("登录尝试过多，请一分钟后重试".into()));
        }
        let found = connection.query_row(
            "SELECT id,password_hash FROM users WHERE email=$1",
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
            "SELECT password_hash FROM users WHERE id=$1 FOR UPDATE",
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
            "DELETE FROM sessions WHERE expires_at<$1",
            [chrono::Utc::now().timestamp()],
        )?;
        c.execute(
            "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES ($1,$2,$3)",
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
                "SELECT user_id FROM sessions WHERE token_hash=$1 AND expires_at>$2",
                params![digest(token), chrono::Utc::now().timestamp()],
                |r| r.get(0),
            )
            .map_err(|error| match error {
                crate::storage::Error::QueryReturnedNoRows => AppError::Auth("请重新登录".into()),
                error => AppError::Database(error),
            })
    }
    pub(crate) fn is_superuser(&self, user_id: &str) -> AppResult<bool> {
        Ok(self.conn()?.query_row(
            "SELECT is_superuser FROM users WHERE id=$1",
            [user_id],
            |row| row.get(0),
        )?)
    }
    pub fn logout(&self, token: &str) -> AppResult<()> {
        self.conn()?
            .execute("DELETE FROM sessions WHERE token_hash=$1", [digest(token)])?;
        Ok(())
    }
    pub fn reset_password(&self, email: &str, password: &str) -> AppResult<()> {
        let email = validate(&Credentials {
            email: email.into(),
            password: password.into(),
        })?;
        let hash = hash_password(password)?;
        let mut connection = self.conn()?;
        let tx = connection.transaction()?;
        let id = tx
            .query_row("SELECT id FROM users WHERE email=$1", [email], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|_| AppError::NotFound("账户不存在".into()))?;
        tx.execute(
            "UPDATE users SET password_hash=$1 WHERE id=$2",
            params![hash, id],
        )?;
        tx.execute("DELETE FROM sessions WHERE user_id=$1", [id])?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, response::IntoResponse};

    pub(super) fn accounts() -> Accounts {
        let url = std::env::var("MARIO_TEST_DATABASE_URL")
            .expect("set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database");
        Accounts::open_schema(url, format!("auth_{}", uuid::Uuid::new_v4().simple())).unwrap()
    }

    #[test]
    fn superuser_migration_only_promotes_the_existing_designated_account_once() {
        let original = accounts();
        let owner = original
            .register(&Credentials {
                email: "lokinko.cs@gmail.com".into(),
                password: "long-password-1234".into(),
            })
            .unwrap();
        let other = original
            .register(&Credentials {
                email: "other@example.test".into(),
                password: "long-password-1234".into(),
            })
            .unwrap();
        assert!(
            !original.is_superuser(&owner.user_id).unwrap(),
            "registration never grants privileges"
        );
        original
            .conn()
            .unwrap()
            .execute_batch("ALTER TABLE users DROP COLUMN is_superuser")
            .unwrap();
        let url = original.url.clone();
        let schema = original.schema.clone();
        drop(original);
        let upgraded = Accounts::open_schema(url.clone(), schema.clone()).unwrap();
        assert!(upgraded.is_superuser(&owner.user_id).unwrap());
        assert!(!upgraded.is_superuser(&other.user_id).unwrap());
        assert_eq!(upgraded.authenticate(&owner.token).unwrap(), owner.user_id);
        upgraded
            .conn()
            .unwrap()
            .execute(
                "UPDATE users SET is_superuser=FALSE WHERE id=$1",
                [&owner.user_id],
            )
            .unwrap();
        drop(upgraded);
        assert!(
            !Accounts::open_schema(url, schema)
                .unwrap()
                .is_superuser(&owner.user_id)
                .unwrap(),
            "restarting never restores revoked privileges"
        );
        assert!(serde_json::from_value::<Credentials>(serde_json::json!({
            "email": "attacker@example.test", "password": "long-password-1234", "isSuperuser": true,
        }))
        .is_err());
    }

    #[test]
    fn public_registration_needs_only_email_and_password() {
        let accounts = accounts();
        for index in 0..2 {
            let payload = serde_json::json!({
                "email": format!(" USER{index}@Example.Test "),
                "password": "long-password-1234",
            });
            let input: Credentials = serde_json::from_value(payload).unwrap();
            let session = accounts.register(&input).unwrap();
            assert_eq!(session.email, format!("user{index}@example.test"));
            assert_eq!(
                accounts.authenticate(&session.token).unwrap(),
                session.user_id
            );
            let stored = accounts
                .conn()
                .unwrap()
                .query_row(
                    "SELECT password_hash FROM users WHERE id=$1",
                    [session.user_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
            assert!(Argon2::default()
                .verify_password(
                    input.password.as_bytes(),
                    &PasswordHash::new(&stored).unwrap()
                )
                .is_ok());
            assert!(matches!(
                accounts.register(&input),
                Err(AppError::Validation(_))
            ));
        }
        assert_eq!(
            accounts
                .conn()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn public_registration_still_rejects_invalid_credentials_before_writing() {
        let accounts = accounts();
        for (email, password) in [
            ("invalid", "long-password-1234"),
            ("user@example.test", "short"),
        ] {
            let input: Credentials =
                serde_json::from_value(serde_json::json!({"email":email,"password":password}))
                    .unwrap();
            assert!(matches!(
                accounts.register(&input),
                Err(AppError::Validation(_))
            ));
        }
        assert_eq!(
            accounts
                .conn()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            accounts
                .conn()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn valid_sessions_survive_repeated_checks_and_revoked_or_expired_sessions_return_401() {
        let accounts = accounts();
        accounts
            .conn()
            .unwrap()
            .execute(
                "INSERT INTO users(id,email,password_hash) VALUES ('user','user@example.test','unused')",
                [],
            )
            .unwrap();
        let session = accounts
            .session(&accounts.conn().unwrap(), "user", "user@example.test")
            .unwrap();
        for _ in 0..2 {
            assert_eq!(accounts.authenticate(&session.token).unwrap(), "user");
        }
        accounts.logout(&session.token).unwrap();
        assert_eq!(
            accounts
                .authenticate(&session.token)
                .unwrap_err()
                .into_response()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        accounts
            .conn()
            .unwrap()
            .execute(
                "INSERT INTO sessions VALUES ($1,'user',0)",
                [digest("expired")],
            )
            .unwrap();
        assert_eq!(
            accounts
                .authenticate("expired")
                .unwrap_err()
                .into_response()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn session_query_failures_return_500_instead_of_invalidating_login() {
        let accounts = accounts();
        accounts
            .conn()
            .unwrap()
            .execute_batch("DROP TABLE sessions")
            .unwrap();
        let error = accounts.authenticate("valid-token").unwrap_err();
        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(
            error.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
