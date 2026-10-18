use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;

fn master_key() -> AppResult<Vec<u8>> {
    let raw = std::env::var("MARIO_MASTER_KEY").unwrap_or_default();
    STANDARD
        .decode(raw)
        .ok()
        .filter(|v| v.len() == 32)
        .ok_or_else(|| {
            AppError::Validation("MARIO_MASTER_KEY 必须是 Base64 编码的 32 字节随机密钥".into())
        })
}
#[cfg(test)]
pub(crate) fn use_test_master_key() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        if master_key().is_err() {
            std::env::set_var("MARIO_MASTER_KEY", STANDARD.encode([7u8; 32]));
        }
    });
}
pub fn validate_master_key() -> AppResult<()> {
    master_key().map(|_| ())
}
pub fn encrypt(value: &str, scope: &str) -> AppResult<String> {
    let key = master_key()?;
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| AppError::Validation("主密钥无效".into()))?;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: value.as_bytes(),
                aad: scope.as_bytes(),
            },
        )
        .map_err(|_| AppError::Validation("密钥加密失败".into()))?;
    Ok(STANDARD.encode([nonce.to_vec(), ciphertext].concat()))
}
pub fn decrypt(value: &str, scope: &str) -> AppResult<String> {
    let key = master_key()?;
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| AppError::Validation("密钥数据无效".into()))?;
    if bytes.len() < 28 {
        return Err(AppError::Validation("密钥数据无效".into()));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| AppError::Validation("主密钥无效".into()))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: scope.as_bytes(),
            },
        )
        .map_err(|_| AppError::Validation("无法解密账户密钥，请检查服务器主密钥".into()))?;
    String::from_utf8(plaintext).map_err(|_| AppError::Validation("密钥编码无效".into()))
}
