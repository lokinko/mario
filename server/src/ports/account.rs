//! Account consistency coordination shared by use cases; no business data or storage.
use crate::AppResult;
use std::future::Future;

/// External I/O runs without the account lock; callers recheck their frozen inputs.
pub(crate) trait AccountSession: Send + Sync {
    fn lock_account(&self) -> AppResult<impl Send + '_>;
    fn without_account_lock<T: Send>(
        &self,
        work: impl Future<Output = T> + Send,
    ) -> impl Future<Output = AppResult<T>> + Send;
}
