//! Durable, one-use website OAuth transactions and explicit identity mapping.
use super::{digest, hash_password, params, Accounts, Session};
use crate::{AppError, AppResult};

pub(crate) struct OpenAiTransaction {
    pub verifier: String,
    pub nonce: String,
    pub link_session: Option<String>,
}
pub(crate) struct OpenAiAttempt<'a> {
    pub state: &'a str,
    pub browser: &'a str,
    pub verifier: &'a str,
    pub nonce: &'a str,
    pub client: &'a str,
    pub redirect: &'a str,
    pub link_token: Option<&'a str>,
}
pub(crate) struct OpenAiIdentity<'a> {
    pub issuer: &'a str,
    pub client: &'a str,
    pub subject: &'a str,
    pub email: &'a str,
    pub email_verified: bool,
}
impl Accounts {
    pub(crate) fn stage_openai_result(
        &self,
        browser: &str,
        session: Option<Session>,
    ) -> AppResult<String> {
        let token = session
            .as_ref()
            .map(|s| s.token.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                )
            });
        let c = self.conn()?;
        let now = chrono::Utc::now().timestamp();
        c.execute(
            "DELETE FROM openai_login_results WHERE expires_at<$1",
            [now],
        )?;
        c.execute("INSERT INTO openai_login_results(result_hash,browser_hash,session_hash,expires_at) VALUES ($1,$2,$3,$4)",params![digest(&token),digest(browser),session.as_ref().map(|s|digest(&s.token)),now+120])?;
        Ok(token)
    }
    pub(crate) fn take_openai_result(
        &self,
        token: &str,
        browser: &str,
    ) -> AppResult<Option<Session>> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let session_hash=tx.query_row("DELETE FROM openai_login_results WHERE result_hash=$1 AND browser_hash=$2 AND expires_at>$3 RETURNING session_hash",params![digest(token),digest(browser),chrono::Utc::now().timestamp()],|r|r.get::<_,Option<String>>(0))
            .map_err(|error| match error { crate::storage::Error::QueryReturnedNoRows=>AppError::Auth("登录交接已过期或已使用，请重新登录".into()),error=>error.into()})?;
        let session = if let Some(hash) = session_hash {
            Some(tx.query_row("SELECT s.user_id,u.email,s.expires_at FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>$2",params![hash,chrono::Utc::now().timestamp()],|r|Ok(Session {token:token.into(),user_id:r.get(0)?,email:r.get(1)?,expires_at:r.get(2)?}))?)
        } else {
            None
        };
        tx.commit()?;
        Ok(session)
    }
    pub(crate) fn begin_openai(&self, attempt: OpenAiAttempt<'_>) -> AppResult<()> {
        if let Some(token) = attempt.link_token {
            self.authenticate(token)?;
        }
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        // Serialize the global cap across API instances as well as local tasks.
        tx.execute_batch(
            "SELECT pg_advisory_xact_lock(hashtextextended('website-openai-attempts',0))",
        )?;
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "DELETE FROM openai_login_attempts WHERE expires_at<$1",
            [now],
        )?;
        let count = tx.query_row("SELECT COUNT(*) FROM openai_login_attempts", [], |r| {
            r.get::<_, i64>(0)
        })?;
        if count >= 1000 {
            return Err(AppError::RateLimit("登录请求过多，请稍后重试".into()));
        }
        tx.execute("INSERT INTO openai_login_attempts(state_hash,browser_hash,verifier,nonce,client_id,redirect_uri,link_session,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            params![digest(attempt.state),digest(attempt.browser),attempt.verifier,attempt.nonce,attempt.client,attempt.redirect,attempt.link_token.map(digest),now+600])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn consume_openai(
        &self,
        state: &str,
        browser: &str,
        client: &str,
        redirect: &str,
    ) -> AppResult<OpenAiTransaction> {
        self.conn()?.query_row("DELETE FROM openai_login_attempts WHERE state_hash=$1 AND browser_hash=$2 AND client_id=$3 AND redirect_uri=$4 AND expires_at>$5 RETURNING verifier,nonce,link_session",
            params![digest(state),digest(browser),client,redirect,chrono::Utc::now().timestamp()],
            |r| Ok(OpenAiTransaction { verifier:r.get(0)?, nonce:r.get(1)?, link_session:r.get(2)? }))
            .map_err(|error| match error { crate::storage::Error::QueryReturnedNoRows => AppError::Auth("登录已过期、已使用或浏览器不匹配，请重新登录".into()), error => error.into() })
    }
    pub(crate) fn finish_openai(
        &self,
        identity: OpenAiIdentity<'_>,
        link_session: Option<&str>,
    ) -> AppResult<Option<Session>> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let key = serde_json::to_string(&(identity.issuer, identity.client, identity.subject))?;
        tx.query_row(
            "SELECT 1 FROM pg_advisory_xact_lock(hashtextextended($1,0))",
            [key],
            |_| Ok(()),
        )?;
        let mapped = match tx.query_row("SELECT user_id FROM external_identities WHERE issuer=$1 AND client_id=$2 AND subject=$3", params![identity.issuer,identity.client,identity.subject], |r|r.get::<_,String>(0)) {
            Ok(user) => Some(user),
            Err(crate::storage::Error::QueryReturnedNoRows) => None,
            Err(error) => return Err(error.into()),
        };
        let linked = link_session.is_some();
        let user = if let Some(session) = link_session {
            let user = tx
                .query_row(
                    "SELECT user_id FROM sessions WHERE token_hash=$1 AND expires_at>$2 FOR SHARE",
                    params![session, chrono::Utc::now().timestamp()],
                    |r| r.get::<_, String>(0),
                )
                .map_err(|error| match error {
                    crate::storage::Error::QueryReturnedNoRows => {
                        AppError::Auth("原 Mario 会话已失效，请登录后重新关联".into())
                    }
                    error => error.into(),
                })?;
            if mapped.as_ref().is_some_and(|mapped| mapped != &user) {
                return Err(AppError::Conflict(
                    "此 ChatGPT 身份已关联其他 Mario 账号".into(),
                ));
            }
            user
        } else if let Some(user) = mapped {
            user
        } else {
            let email = identity.email.trim().to_lowercase();
            if !identity.email_verified
                || email.len() > 254
                || !email.contains('@')
                || email.chars().any(char::is_whitespace)
            {
                return Err(AppError::Auth(
                    "创建 Mario 账号需要已验证的邮箱；请先用邮箱密码注册并关联 ChatGPT".into(),
                ));
            }
            let user = uuid::Uuid::new_v4().to_string();
            // No known password is assigned to an OAuth-created account.
            let hash = hash_password(&format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()))?;
            if tx.execute("INSERT INTO users(id,email,password_hash) VALUES ($1,$2,$3) ON CONFLICT(email) DO NOTHING",params![user,email,hash])? == 0 {
                return Err(AppError::Conflict("该邮箱已有 Mario 账号，请先用原账号登录，再在账号设置关联 ChatGPT".into()));
            }
            user
        };
        tx.execute("INSERT INTO external_identities(issuer,client_id,subject,user_id) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING", params![identity.issuer,identity.client,identity.subject,user])?;
        let email = tx.query_row(
            "SELECT email FROM users WHERE id=$1",
            [user.as_str()],
            |r| r.get::<_, String>(0),
        )?;
        let session = if linked {
            None
        } else {
            Some(self.session(&tx, &user, &email)?)
        };
        tx.commit()?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests;
