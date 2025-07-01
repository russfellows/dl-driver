//
//
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use crate::StorageBackend;

pub struct PosixBackend {
    root: PathBuf,
}

impl PosixBackend {
    /// Store everything under `root` on the local filesystem.
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }
}

impl StorageBackend for PosixBackend {
    fn put(&self, key: &str, data: &[u8]) -> io::Result<()> {
        let path = self.root.join(key);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(path, data)
    }

    fn get(&self, key: &str) -> io::Result<Vec<u8>> {
        let path = self.root.join(key);
        fs::read(path)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let path = self.root.join(key);
        fs::remove_file(path)
    }

    fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        let dir = self.root.join(prefix);
        let mut names = Vec::new();
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let f = entry?;
                if f.path().is_file() {
                    if let Some(n) = f.file_name().to_str() {
                        names.push(n.to_string());
                    }
                }
            }
        }
        Ok(names)
    }
}

