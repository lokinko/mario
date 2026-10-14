mod accounts;
mod agent;
mod ai;
mod application;
mod attachments;
mod authentication;
mod context;
mod db;
mod error;
mod evidence;
mod fx;
mod hosted;
mod http;
mod market_data;
mod memory;
mod models;
mod planning;
mod prices;
mod risk;
mod runtime;
mod secrets;
mod storage;
mod streaming;
mod valuation;

pub use error::{AppError, AppResult};
pub use runtime::run_from_env;

#[cfg(test)]
fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
