//! Explicit exceptions to optimistic concurrency.
use axum::http::Method;
pub(crate) fn requires_revision(method: &Method, path: &str) -> bool {
    // Public catalogue edits compare their own durable revision in the repository.
    if method == Method::PUT && path == "/api/watch/catalog" {
        return false;
    }
    method != Method::GET
        && !matches!(
            path,
            "/api/analysis/preview"
                | "/api/activity"
                | "/api/model-config/test"
                | "/api/financial-operations/resolve"
                | "/api/watch/catalog/resolve"
                | "/api/watch/grid-preview"
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
        assert!(!requires_revision(&Method::POST, "/api/activity"));
        assert!(!requires_revision(&Method::PUT, "/api/watch/catalog"));
        assert!(!requires_revision(
            &Method::POST,
            "/api/watch/catalog/resolve"
        ));
        assert!(requires_revision(
            &Method::POST,
            "/api/watch/catalog/refresh"
        ));
        assert!(requires_revision(&Method::DELETE, "/api/watch/catalog"));
        assert!(!requires_revision(&Method::POST, "/api/watch/grid-preview"));
        assert!(requires_revision(&Method::PUT, "/api/watch/600036.SS/grid"));
    }
}
