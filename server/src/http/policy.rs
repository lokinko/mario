//! Explicit exceptions to optimistic concurrency.
use axum::http::Method;
pub(crate) fn requires_revision(method: &Method, path: &str) -> bool {
    method != Method::GET
        && !matches!(
            path,
            "/api/analysis/preview"
                | "/api/model-config/test"
                | "/api/financial-operations/resolve"
        )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_changes_are_writes_but_analysis_previews_and_connection_tests_are_not() {
        assert!(requires_revision(&Method::POST, "/api/fx-settings/refresh"));
        assert!(!requires_revision(&Method::POST, "/api/analysis/preview"));
        assert!(!requires_revision(&Method::POST, "/api/model-config/test"));
        assert!(requires_revision(&Method::POST, "/api/future/preview"));
        assert!(!requires_revision(&Method::GET, "/api/snapshot"));
    }
}
