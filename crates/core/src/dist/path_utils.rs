/// Path utilities for distributed execution
/// 
/// Provides functions for:
/// - Detecting shared vs local storage backends
/// - Applying path prefixes for agent isolation
/// - Safe URI path joining

use anyhow::Result;

/// Detect if a URI points to shared storage (S3, Azure, GCS)
/// 
/// Shared storage backends don't need path prefixes since all agents
/// can access the same data. Local backends (file://, direct://) need
/// per-agent path isolation.
/// 
/// # Examples
/// ```
/// use dl_driver_core::dist::path_utils::is_shared_storage;
/// 
/// assert!(is_shared_storage("s3://bucket/data"));
/// assert!(is_shared_storage("az://container/data"));
/// assert!(is_shared_storage("gs://bucket/data"));
/// assert!(!is_shared_storage("file:///data"));
/// assert!(!is_shared_storage("direct:///data"));
/// assert!(!is_shared_storage("/absolute/path"));
/// ```
pub fn is_shared_storage(uri: &str) -> bool {
    uri.starts_with("s3://")
        || uri.starts_with("az://")
        || uri.starts_with("gs://")
        || uri.starts_with("wasb://")
        || uri.starts_with("wasbs://")
        || uri.starts_with("abfs://")
        || uri.starts_with("abfss://")
}

/// Apply path prefix to a URI for agent isolation
/// 
/// For local storage (file://, direct://, or absolute paths), this appends
/// the prefix to the base path to isolate each agent's data. For shared storage,
/// returns the original URI unchanged.
/// 
/// The prefix can include a template variable `{id}` which will be replaced
/// with the agent_id.
/// 
/// # Examples
/// ```
/// use dl_driver_core::dist::path_utils::apply_path_prefix;
/// 
/// // Local storage gets prefix appended
/// let result = apply_path_prefix(
///     "file:///data/train", 
///     "{id}/",
///     "agent-0"
/// ).unwrap();
/// assert_eq!(result, "file:///data/train/agent-0");
/// 
/// // Shared storage unchanged
/// let result = apply_path_prefix(
///     "s3://bucket/data",
///     "{id}/",
///     "agent-0"
/// ).unwrap();
/// assert_eq!(result, "s3://bucket/data");
/// ```
pub fn apply_path_prefix(uri: &str, prefix_template: &str, agent_id: &str) -> Result<String> {
    // Shared storage doesn't need prefixes
    if is_shared_storage(uri) {
        return Ok(uri.to_string());
    }

    // Replace template variable in prefix
    let prefix = prefix_template.replace("{id}", agent_id);

    // Handle different URI formats
    if let Some(rest) = uri.strip_prefix("file://") {
        // file:// URIs - append prefix to path
        let path = rest.trim_end_matches('/');
        Ok(format!("file://{}/{}", path, prefix.trim_end_matches('/')))
    } else if let Some(rest) = uri.strip_prefix("direct://") {
        // direct:// URIs (DirectIO) - append prefix to path
        let path = rest.trim_end_matches('/');
        Ok(format!("direct://{}/{}", path, prefix.trim_end_matches('/')))
    } else if uri.starts_with('/') {
        // Absolute filesystem paths - append prefix to path
        let path = uri.trim_end_matches('/');
        Ok(format!("{}/{}", path, prefix.trim_end_matches('/')))
    } else {
        // Relative paths (less common but handle gracefully)
        let path = uri.trim_end_matches('/');
        Ok(format!("{}/{}", path, prefix.trim_end_matches('/')))
    }
}

/// Join a base URI with a suffix path component
/// 
/// Handles trailing/leading slashes correctly for different URI schemes.
/// 
/// # Examples
/// ```
/// use dl_driver_core::dist::path_utils::join_uri_path;
/// 
/// assert_eq!(
///     join_uri_path("s3://bucket/data", "train").unwrap(),
///     "s3://bucket/data/train"
/// );
/// 
/// assert_eq!(
///     join_uri_path("s3://bucket/data/", "train").unwrap(),
///     "s3://bucket/data/train"
/// );
/// 
/// assert_eq!(
///     join_uri_path("file:///data", "train").unwrap(),
///     "file:///data/train"
/// );
/// ```
pub fn join_uri_path(base: &str, suffix: &str) -> Result<String> {
    if base.is_empty() {
        anyhow::bail!("Base URI cannot be empty");
    }
    if suffix.is_empty() {
        return Ok(base.to_string());
    }

    let base = base.trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');

    Ok(format!("{}/{}", base, suffix))
}

/// Extract the storage backend type from a URI
/// 
/// Returns a string identifier for the backend: "s3", "azure", "gcs", 
/// "directio", or "file".
pub fn detect_backend(uri: &str) -> &'static str {
    if uri.starts_with("s3://") {
        "s3"
    } else if uri.starts_with("az://") || uri.starts_with("wasb://") 
        || uri.starts_with("wasbs://") || uri.starts_with("abfs://") 
        || uri.starts_with("abfss://") {
        "azure"
    } else if uri.starts_with("gs://") {
        "gcs"
    } else if uri.starts_with("direct://") {
        "directio"
    } else {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_shared_storage() {
        // Shared storage
        assert!(is_shared_storage("s3://bucket/data"));
        assert!(is_shared_storage("az://container/data"));
        assert!(is_shared_storage("gs://bucket/data"));
        assert!(is_shared_storage("wasb://container/data"));
        assert!(is_shared_storage("abfs://container/data"));

        // Local storage
        assert!(!is_shared_storage("file:///data"));
        assert!(!is_shared_storage("direct:///data"));
        assert!(!is_shared_storage("/absolute/path"));
        assert!(!is_shared_storage("relative/path"));
    }

    #[test]
    fn test_apply_path_prefix_shared() {
        // Shared storage should not be modified
        let uri = "s3://bucket/data/train";
        let result = apply_path_prefix(uri, "{id}/", "agent-0").unwrap();
        assert_eq!(result, uri);

        let uri = "az://container/data";
        let result = apply_path_prefix(uri, "prefix/", "agent-1").unwrap();
        assert_eq!(result, uri);
    }

    #[test]
    fn test_apply_path_prefix_file() {
        let uri = "file:///data/train";
        let result = apply_path_prefix(uri, "{id}/", "agent-0").unwrap();
        assert_eq!(result, "file:///agent-0/data/train");

        let uri = "file:///mnt/test/data";
        let result = apply_path_prefix(uri, "run1/{id}/", "agent-2").unwrap();
        assert_eq!(result, "file:///run1/agent-2/mnt/test/data");
    }

    #[test]
    fn test_apply_path_prefix_direct() {
        let uri = "direct:///nvme/data";
        let result = apply_path_prefix(uri, "{id}/", "agent-0").unwrap();
        assert_eq!(result, "direct:///agent-0/nvme/data");
    }

    #[test]
    fn test_apply_path_prefix_absolute() {
        let uri = "/data/train";
        let result = apply_path_prefix(uri, "{id}/", "agent-0").unwrap();
        assert_eq!(result, "/agent-0/data/train");
    }

    #[test]
    fn test_join_uri_path() {
        assert_eq!(
            join_uri_path("s3://bucket/data", "train").unwrap(),
            "s3://bucket/data/train"
        );

        assert_eq!(
            join_uri_path("s3://bucket/data/", "train").unwrap(),
            "s3://bucket/data/train"
        );

        assert_eq!(
            join_uri_path("s3://bucket/data/", "/train").unwrap(),
            "s3://bucket/data/train"
        );

        assert_eq!(
            join_uri_path("file:///data", "train").unwrap(),
            "file:///data/train"
        );
    }

    #[test]
    fn test_join_uri_path_edge_cases() {
        // Empty suffix
        assert_eq!(
            join_uri_path("s3://bucket/data", "").unwrap(),
            "s3://bucket/data"
        );

        // Empty base should error
        assert!(join_uri_path("", "train").is_err());
    }

    #[test]
    fn test_detect_backend() {
        assert_eq!(detect_backend("s3://bucket/data"), "s3");
        assert_eq!(detect_backend("az://container/data"), "azure");
        assert_eq!(detect_backend("gs://bucket/data"), "gcs");
        assert_eq!(detect_backend("direct:///nvme/data"), "directio");
        assert_eq!(detect_backend("file:///data"), "file");
        assert_eq!(detect_backend("/absolute/path"), "file");
    }
}
