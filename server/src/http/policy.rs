//! Explicit exceptions to optimistic concurrency and local serialization.
use axum::http::Method;
pub(crate) fn requires_revision(method: &Method, path: &str) -> bool {
    method != Method::GET && !matches!(path, "/api/analysis/preview" | "/api/model-config/test")
}
pub(crate) fn needs_local_lock(method: &Method, path: &str) -> bool {
    method != Method::GET
        || matches!(
            path,
            "/api/snapshot" | "/api/fx-settings" | "/api/market-data/fx-rate"
        )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_that_cache_fx_are_writes_but_analysis_previews_and_connection_tests_are_not() {
        assert!(requires_revision(
            &Method::POST,
            "/api/portfolio-events/import/preview"
        ));
        assert!(!requires_revision(&Method::POST, "/api/analysis/preview"));
        assert!(!requires_revision(&Method::POST, "/api/model-config/test"));
        assert!(requires_revision(&Method::POST, "/api/future/preview"));
        assert!(!requires_revision(&Method::GET, "/api/snapshot"));
        assert!(needs_local_lock(&Method::GET, "/api/snapshot"));
        assert!(!needs_local_lock(&Method::GET, "/api/analyses"));
    }
}
