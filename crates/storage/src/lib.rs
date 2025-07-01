pub mod posix;
pub use posix::PosixBackend;

/// A simple synchronous storage interface.
pub trait StorageBackend {
    /// Write `data` under key (relative path) `key`.
    fn put(&self, key: &str, data: &[u8]) -> std::io::Result<()>;
    /// Read the entire object at `key`.
    fn get(&self, key: &str) -> std::io::Result<Vec<u8>>;
    /// Delete the object at `key`.
    fn delete(&self, key: &str) -> std::io::Result<()>;
    /// List the names (files only) under the directory `prefix`.
    fn list(&self, prefix: &str) -> std::io::Result<Vec<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn posix_put_get_delete_list() {
        let dir = tempdir().unwrap();
        let backend = PosixBackend::new(dir.path());
        let key = "foo/bar.txt";
        let data = b"hello";
        backend.put(key, data).unwrap();

        let got = backend.get(key).unwrap();
        assert_eq!(&got, data);

        let mut listing = backend.list("foo").unwrap();
        listing.sort();
        assert_eq!(listing, vec!["bar.txt".to_string()]);

        backend.delete(key).unwrap();
        assert!(backend.get(key).is_err());
    }
}

