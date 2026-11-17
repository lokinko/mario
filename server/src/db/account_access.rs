//! Account consistency adapter; the concrete lock and connection remain private.
use super::Database;
use crate::{ports::account::AccountSession, AppResult};
use std::future::Future;

impl AccountSession for Database {
    fn lock_account(&self) -> AppResult<impl Send + '_> {
        Database::lock_account(self)
    }
    async fn without_account_lock<T: Send>(
        &self,
        work: impl Future<Output = T> + Send,
    ) -> AppResult<T> {
        Database::without_account_lock(self, work).await
    }
}
