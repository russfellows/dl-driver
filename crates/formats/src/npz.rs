// crates/formats/src/npz.rs

use anyhow::Result;
use ndarray::{ArrayD, IxDyn};
use ndarray_npy::{read_npy, write_npy};
use std::path::Path;
use crate::Format;

/// Single-array “NPZ” (as .npy) format generator + reader
pub struct NpzFormat {
    shape: Vec<usize>,
}

impl NpzFormat {
    /// Create with the desired multidimensional `shape`.
    pub fn new(shape: Vec<usize>) -> Self {
        NpzFormat { shape }
    }
}

impl Format for NpzFormat {
    fn generate(&self, path: &Path) -> Result<()> {
        // zero-filled array of the given shape
        let arr: ArrayD<u8> = ArrayD::zeros(IxDyn(&self.shape));
        // write directly by path (ndarray-npy expects AsRef<Path>)
        write_npy(path, &arr)?;
        Ok(())
    }

    fn read(&self, path: &Path) -> Result<()> {
        // infer ArrayD<u8>
        let arr: ArrayD<u8> = read_npy(path)?;
        // shape must match
        assert_eq!(arr.shape(), self.shape.as_slice(), "shape mismatch");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn npz_generate_and_read() {
        // a small 4×5 example
        let fmt = NpzFormat::new(vec![4, 5]);
        let tmp = NamedTempFile::new().unwrap();
        fmt.generate(tmp.path()).unwrap();
        fmt.read(tmp.path()).unwrap();
    }
}

