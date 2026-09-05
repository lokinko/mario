use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.compassinvest.desktop";
const ACCOUNT: &str = "llm-api-key";

fn entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SERVICE, ACCOUNT)
}

pub fn has_api_key() -> bool {
    get_api_key()
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false)
}

pub fn get_api_key() -> AppResult<String> {
    match entry()
        .map_err(|e| AppError::Keyring(e.to_string()))?
        .get_password()
    {
        Ok(value) => Ok(value),
        Err(keyring::Error::NoEntry) => {
            Err(AppError::Validation("请先在客户端配置模型 API Key".into()))
        }
        Err(error) => Err(AppError::Keyring(error.to_string())),
    }
}

pub fn set_api_key(value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation("API Key 不能为空".into()));
    }
    entry()
        .map_err(|e| AppError::Keyring(e.to_string()))?
        .set_password(value.trim())
        .map_err(|e| AppError::Keyring(e.to_string()))
}
