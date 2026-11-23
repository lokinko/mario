//! OS-backed encryption for local ChatGPT credentials. No plaintext fallback.
use crate::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

const ENVELOPE_VERSION: u8 = 1;

pub(super) trait CredentialCipher {
    fn provider_id(&self) -> &'static str;
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>>;
    fn decrypt(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>>;
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u8,
    provider: String,
    ciphertext: String,
}

pub(super) fn seal(cipher: &dyn CredentialCipher, plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let encrypted = cipher.encrypt(plaintext)?;
    let envelope = Envelope {
        version: ENVELOPE_VERSION,
        provider: cipher.provider_id().into(),
        ciphertext: STANDARD.encode(encrypted),
    };
    serde_json::to_vec(&envelope)
        .map_err(|_| AppError::Validation("无法序列化加密后的本机 ChatGPT 凭据".into()))
}

pub(super) fn open(cipher: &dyn CredentialCipher, raw: &[u8]) -> AppResult<Vec<u8>> {
    let envelope: Envelope = serde_json::from_slice(raw).map_err(|_| {
        AppError::Validation("本机加密 ChatGPT 凭据格式损坏；请保留文件并检查，不会自动覆盖".into())
    })?;
    if envelope.version != ENVELOPE_VERSION {
        return Err(AppError::Validation(
            "本机加密 ChatGPT 凭据版本不受支持，请检查 Mario 版本".into(),
        ));
    }
    if envelope.provider != cipher.provider_id() {
        return Err(AppError::Validation(
            "本机加密 ChatGPT 凭据提供方不匹配，无法在当前系统环境下解密".into(),
        ));
    }
    let bytes = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| AppError::Validation("本机加密 ChatGPT 凭据编码无效".into()))?;
    cipher.decrypt(&bytes)
}

#[cfg(any(unix, test))]
pub(super) fn aead_encrypt(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        ChaCha20Poly1305, Nonce,
    };
    use rand::RngCore;
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AppError::Validation("本机凭据加密密钥无效".into()))?;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::Validation("本机 ChatGPT 凭据加密失败".into()))?;
    let mut output = Vec::with_capacity(nonce.len() + ciphertext.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

#[cfg(any(unix, test))]
pub(super) fn aead_decrypt(key: &[u8; 32], aad: &[u8], data: &[u8]) -> AppResult<Vec<u8>> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        ChaCha20Poly1305, Nonce,
    };
    if data.len() < 28 {
        return Err(AppError::Validation(
            "本机加密 ChatGPT 凭据数据不完整".into(),
        ));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AppError::Validation("本机凭据解密密钥无效".into()))?;
    cipher
        .decrypt(
            Nonce::from_slice(&data[..12]),
            Payload {
                msg: &data[12..],
                aad,
            },
        )
        .map_err(|_| {
            AppError::Validation(
                "无法解密本机 ChatGPT 凭据（可能已损坏或属于其他系统用户）；请检查系统密钥环或重新授权"
                    .into(),
            )
        })
}

#[cfg(unix)]
fn decode_hex_key(raw: &str) -> AppResult<[u8; 32]> {
    let trimmed = raw.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Validation(
            "系统密钥环中的 Mario 包装密钥格式无效".into(),
        ));
    }
    let mut key = [0u8; 32];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
            .map_err(|_| AppError::Validation("系统密钥环中的 Mario 包装密钥格式无效".into()))?;
    }
    Ok(key)
}

#[cfg(unix)]
fn generate_hex_key() -> ([u8; 32], String) {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    let hex = key
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    (key, hex)
}

#[cfg(windows)]
pub(super) struct PlatformCipher;

#[cfg(windows)]
impl PlatformCipher {
    const ENTROPY: &'static [u8] = b"mario.chatgpt.credentials.v1";
}

#[cfg(windows)]
struct DpapiBuffer(*mut u8, usize);

#[cfg(windows)]
impl Drop for DpapiBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                std::ptr::write_bytes(self.0, 0, self.1);
                windows_sys::Win32::Foundation::LocalFree(self.0.cast());
            }
        }
    }
}

#[cfg(windows)]
impl CredentialCipher for PlatformCipher {
    fn provider_id(&self) -> &'static str {
        "windows-dpapi"
    }

    fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let len = u32::try_from(plaintext.len())
            .map_err(|_| AppError::Validation("本机 ChatGPT 凭据过大，无法加密".into()))?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: len,
            pbData: plaintext.as_ptr().cast_mut(),
        };
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: Self::ENTROPY.len() as u32,
            pbData: Self::ENTROPY.as_ptr().cast_mut(),
        };
        let description: Vec<u16> = "Mario ChatGPT Credentials"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                description.as_ptr(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 || output.pbData.is_null() {
            return Err(AppError::Validation(
                "Windows DPAPI 加密本机 ChatGPT 凭据失败，不会回退为明文保存".into(),
            ));
        }
        let guard = DpapiBuffer(output.pbData, output.cbData as usize);
        let bytes = unsafe { std::slice::from_raw_parts(guard.0, guard.1) }.to_vec();
        Ok(bytes)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        use windows_sys::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let len = u32::try_from(ciphertext.len())
            .map_err(|_| AppError::Validation("本机加密 ChatGPT 凭据过大".into()))?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: len,
            pbData: ciphertext.as_ptr().cast_mut(),
        };
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: Self::ENTROPY.len() as u32,
            pbData: Self::ENTROPY.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 || output.pbData.is_null() {
            return Err(AppError::Validation(
                "Windows DPAPI 无法解密本机 ChatGPT 凭据（可能已损坏或属于其他 Windows 用户）"
                    .into(),
            ));
        }
        let guard = DpapiBuffer(output.pbData, output.cbData as usize);
        let bytes = unsafe { std::slice::from_raw_parts(guard.0, guard.1) }.to_vec();
        Ok(bytes)
    }
}

#[cfg(target_os = "macos")]
pub(super) struct PlatformCipher;

#[cfg(target_os = "macos")]
impl PlatformCipher {
    const SERVICE: &'static str = "ai.madeagents.mario.chatgpt";
    const ACCOUNT: &'static str = "local-credential-key";
    const AAD: &'static [u8] = b"mario.chatgpt.credentials.v1:macos-keychain";

    fn lookup_key() -> AppResult<Option<[u8; 32]>> {
        let output = std::process::Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-s",
                Self::SERVICE,
                "-a",
                Self::ACCOUNT,
                "-w",
            ])
            .output()
            .map_err(|_| {
                AppError::Validation(
                    "无法访问 macOS 钥匙串（/usr/bin/security），本机凭据不支持明文回退".into(),
                )
            })?;
        if output.status.success() {
            let text = String::from_utf8(output.stdout)
                .map_err(|_| AppError::Validation("macOS 钥匙串返回的包装密钥格式无效".into()))?;
            return Ok(Some(decode_hex_key(&text)?));
        }
        if output.status.code() == Some(44) {
            return Ok(None);
        }
        Err(AppError::Validation(
            "无法从 macOS 钥匙串读取 Mario 包装密钥，请先解锁登录钥匙串后重试".into(),
        ))
    }

    fn load_or_create_key() -> AppResult<[u8; 32]> {
        if let Some(existing) = Self::lookup_key()? {
            return Ok(existing);
        }
        let (key, hex) = generate_hex_key();
        use std::io::Write;
        let mut child = std::process::Command::new("/usr/bin/security")
            .arg("-i")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| {
                AppError::Validation(
                    "无法写入 macOS 钥匙串（/usr/bin/security），本机凭据不支持明文回退".into(),
                )
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            let command = format!(
                "add-generic-password -U -s {} -a {} -w {hex}\n",
                Self::SERVICE,
                Self::ACCOUNT
            );
            stdin
                .write_all(command.as_bytes())
                .map_err(|_| AppError::Validation("向 macOS 钥匙串写入包装密钥失败".into()))?;
        }
        let status = child
            .wait()
            .map_err(|_| AppError::Validation("等待 macOS 钥匙串写入完成失败".into()))?;
        if !status.success() {
            return Err(AppError::Validation(
                "macOS 钥匙串拒绝保存 Mario 包装密钥，请解锁钥匙串并重试".into(),
            ));
        }
        Ok(key)
    }
}

#[cfg(target_os = "macos")]
impl CredentialCipher for PlatformCipher {
    fn provider_id(&self) -> &'static str {
        "macos-keychain"
    }

    fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let key = Self::load_or_create_key()?;
        aead_encrypt(&key, Self::AAD, plaintext)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        let key = Self::lookup_key()?.ok_or_else(|| {
            AppError::Validation(
                "macOS 钥匙串中缺少 Mario 包装密钥，无法解密现有本机 ChatGPT 凭据".into(),
            )
        })?;
        aead_decrypt(&key, Self::AAD, ciphertext)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) struct PlatformCipher;

#[cfg(all(unix, not(target_os = "macos")))]
impl PlatformCipher {
    const SERVICE: &'static str = "ai.madeagents.mario.chatgpt";
    const ACCOUNT: &'static str = "local-credential-key";
    const AAD: &'static [u8] = b"mario.chatgpt.credentials.v1:linux-secret-service";

    fn lookup_key() -> AppResult<Option<[u8; 32]>> {
        let output = std::process::Command::new("secret-tool")
            .args(["lookup", "service", Self::SERVICE, "account", Self::ACCOUNT])
            .output()
            .map_err(|_| {
                AppError::Validation(
                    "未检测到可用的 Linux 系统密钥环（secret-tool / Secret Service）；本机凭据不支持明文回退，请安装并启用 libsecret 密钥环后重试".into(),
                )
            })?;
        if output.status.success() {
            let text = String::from_utf8(output.stdout)
                .map_err(|_| AppError::Validation("系统密钥环返回的包装密钥格式无效".into()))?;
            if text.trim().is_empty() {
                return Ok(None);
            }
            return Ok(Some(decode_hex_key(&text)?));
        }
        if output.status.code() == Some(1) && output.stderr.is_empty() {
            return Ok(None);
        }
        Err(AppError::Validation(
            "无法访问 Linux 系统密钥环（Secret Service），请确认桌面密钥环已解锁后重试".into(),
        ))
    }

    fn load_or_create_key() -> AppResult<[u8; 32]> {
        if let Some(existing) = Self::lookup_key()? {
            return Ok(existing);
        }
        let (key, hex) = generate_hex_key();
        use std::io::Write;
        let mut child = std::process::Command::new("secret-tool")
            .args([
                "store",
                "--label=Mario ChatGPT Credentials",
                "service",
                Self::SERVICE,
                "account",
                Self::ACCOUNT,
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| {
                AppError::Validation(
                    "未检测到可用的 Linux 系统密钥环（secret-tool / Secret Service）；本机凭据不支持明文回退".into(),
                )
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(hex.as_bytes())
                .map_err(|_| AppError::Validation("向 Linux 系统密钥环写入包装密钥失败".into()))?;
        }
        let status = child
            .wait()
            .map_err(|_| AppError::Validation("等待 Linux 系统密钥环写入完成失败".into()))?;
        if !status.success() {
            return Err(AppError::Validation(
                "Linux 系统密钥环拒绝保存 Mario 包装密钥，请确认密钥环已解锁后重试".into(),
            ));
        }
        Ok(key)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl CredentialCipher for PlatformCipher {
    fn provider_id(&self) -> &'static str {
        "linux-secret-service"
    }

    fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let key = Self::load_or_create_key()?;
        aead_encrypt(&key, Self::AAD, plaintext)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        let key = Self::lookup_key()?.ok_or_else(|| {
            AppError::Validation(
                "Linux 系统密钥环中缺少 Mario 包装密钥，无法解密现有本机 ChatGPT 凭据".into(),
            )
        })?;
        aead_decrypt(&key, Self::AAD, ciphertext)
    }
}

#[cfg(not(any(windows, unix)))]
pub(super) struct PlatformCipher;

#[cfg(not(any(windows, unix)))]
impl CredentialCipher for PlatformCipher {
    fn provider_id(&self) -> &'static str {
        "unsupported"
    }

    fn encrypt(&self, _: &[u8]) -> AppResult<Vec<u8>> {
        Err(AppError::Validation(
            "当前平台不支持系统级加密保存本机 ChatGPT 凭据".into(),
        ))
    }

    fn decrypt(&self, _: &[u8]) -> AppResult<Vec<u8>> {
        Err(AppError::Validation(
            "当前平台不支持系统级解密本机 ChatGPT 凭据".into(),
        ))
    }
}

pub(super) fn platform_cipher() -> &'static dyn CredentialCipher {
    &PlatformCipher
}

#[cfg(test)]
pub(super) mod tests {
    use super::{aead_decrypt, aead_encrypt, open, seal, CredentialCipher};
    use crate::{AppError, AppResult};

    pub(in crate::chatgpt_auth) struct TestCipher {
        pub id: &'static str,
        pub key: [u8; 32],
    }

    impl CredentialCipher for TestCipher {
        fn provider_id(&self) -> &'static str {
            self.id
        }

        fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
            aead_encrypt(&self.key, self.id.as_bytes(), plaintext)
        }

        fn decrypt(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
            aead_decrypt(&self.key, self.id.as_bytes(), ciphertext)
        }
    }

    pub(in crate::chatgpt_auth) struct UnavailableCipher;

    impl CredentialCipher for UnavailableCipher {
        fn provider_id(&self) -> &'static str {
            "unavailable"
        }

        fn encrypt(&self, _: &[u8]) -> AppResult<Vec<u8>> {
            Err(AppError::Validation("测试：系统加密不可用".into()))
        }

        fn decrypt(&self, _: &[u8]) -> AppResult<Vec<u8>> {
            Err(AppError::Validation("测试：系统解密不可用".into()))
        }
    }

    #[test]
    fn envelope_encrypts_plaintext_and_rejects_tamper_or_wrong_provider() {
        let cipher = TestCipher {
            id: "test-os-cipher",
            key: [9u8; 32],
        };
        let secret = br#"{"access_token":"secret-oauth-token"}"#;
        let sealed = seal(&cipher, secret).unwrap();
        let text = String::from_utf8(sealed.clone()).unwrap();
        assert!(!text.contains("secret-oauth-token"));
        assert_eq!(open(&cipher, &sealed).unwrap(), secret);

        let wrong_provider = TestCipher {
            id: "other-provider",
            key: [9u8; 32],
        };
        assert!(open(&wrong_provider, &sealed).is_err());

        let wrong_key = TestCipher {
            id: "test-os-cipher",
            key: [3u8; 32],
        };
        assert!(open(&wrong_key, &sealed).is_err());

        let mut tampered: serde_json::Value = serde_json::from_slice(&sealed).unwrap();
        tampered["version"] = serde_json::json!(2);
        assert!(open(&cipher, &serde_json::to_vec(&tampered).unwrap()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_dpapi_round_trips_and_never_leaves_plaintext() {
        let cipher = super::platform_cipher();
        assert_eq!(cipher.provider_id(), "windows-dpapi");
        let secret = br#"{"refresh_token":"dpapi-test-secret"}"#;
        let sealed = seal(cipher, secret).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("dpapi-test-secret"));
        assert_eq!(open(cipher, &sealed).unwrap(), secret);
        assert!(open(cipher, b"not-json").is_err());
    }
}
