//! Model endpoint validation shared by providers and execution adapters.
use crate::{AppError, AppResult};
pub fn validate_endpoint(base_url: &str) -> AppResult<()> {
    let url =
        reqwest::Url::parse(base_url).map_err(|_| AppError::Validation("模型地址无效".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.port() == Some(0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "请填写有效的 HTTP/HTTPS API 基础地址，地址不能包含凭据、查询参数或片段".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_custom_model_hosts_ports_and_paths() {
        for endpoint in [
            "https://api.example.test/v1",
            "https://gateway.example.test:8443/proxy/models/v1/",
            "http://models.example.test:8080/api/v1",
            "http://192.168.1.10:8000/v1",
            "http://localhost:11434/v1",
            "http://[::1]:1234/v1",
        ] {
            validate_endpoint(endpoint).unwrap();
        }
    }

    #[test]
    fn rejects_invalid_urls_and_embedded_credentials() {
        for endpoint in [
            "not-a-url",
            "file:///etc/passwd",
            "ftp://models.example.test/v1",
            "https://user:secret@models.example.test/v1",
            "https://models.example.test/v1?api_key=secret",
            "https://models.example.test/v1#fragment",
            "http://models.example.test:0/v1",
        ] {
            assert!(validate_endpoint(endpoint).is_err(), "{endpoint}");
        }
    }
}
