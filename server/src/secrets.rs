use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.compassinvest.desktop";
const MODEL_API_KEY: &str = "llm-api-key";
const CLOUD_ACCESS_TOKEN: &str = "cloud-access-token";
const CLOUD_REFRESH_TOKEN: &str = "cloud-refresh-token";

fn entry(account: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SERVICE, account)
}

fn read(account: &str) -> AppResult<Option<String>> {
    match entry(account)
        .map_err(|error| AppError::Keyring(error.to_string()))?
        .get_password()
    {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Keyring(error.to_string())),
    }
}

fn write(account: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation("不能保存空密钥或令牌".into()));
    }
    entry(account)
        .map_err(|error| AppError::Keyring(error.to_string()))?
        .set_password(value.trim())
        .map_err(|error| AppError::Keyring(error.to_string()))
}

fn delete(account: &str) -> AppResult<()> {
    match entry(account)
        .map_err(|error| AppError::Keyring(error.to_string()))?
        .delete_credential()
    {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Keyring(error.to_string())),
    }
}

pub fn has_api_key() -> bool {
    get_api_key()
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false)
}

pub fn get_api_key() -> AppResult<String> {
    read(MODEL_API_KEY)?.ok_or_else(|| AppError::Validation("请先在客户端配置模型 API Key".into()))
}

pub fn set_api_key(value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation("API Key 不能为空".into()));
    }
    write(MODEL_API_KEY, value)
}

pub fn delete_api_key() -> AppResult<()> {
    delete(MODEL_API_KEY)
}

pub fn cloud_tokens() -> AppResult<Option<(String, String)>> {
    match (read(CLOUD_ACCESS_TOKEN)?, read(CLOUD_REFRESH_TOKEN)?) {
        (Some(access), Some(refresh)) => Ok(Some((access, refresh))),
        _ => Ok(None),
    }
}

pub fn set_cloud_tokens(access_token: &str, refresh_token: &str) -> AppResult<()> {
    write(CLOUD_ACCESS_TOKEN, access_token)?;
    if let Err(error) = write(CLOUD_REFRESH_TOKEN, refresh_token) {
        let _ = delete(CLOUD_ACCESS_TOKEN);
        return Err(error);
    }
    Ok(())
}

pub fn delete_cloud_tokens() -> AppResult<()> {
    delete(CLOUD_ACCESS_TOKEN)?;
    delete(CLOUD_REFRESH_TOKEN)
}

pub fn cloud_encryption_key(user_id: &str) -> AppResult<Option<String>> {
    read(&format!("cloud-encryption-key:{user_id}"))
}

pub fn set_cloud_encryption_key(user_id: &str, value: &str) -> AppResult<()> {
    write(&format!("cloud-encryption-key:{user_id}"), value)
}
