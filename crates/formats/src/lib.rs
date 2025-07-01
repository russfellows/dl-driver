// crates/formats/src/lib.rs
//
pub mod npz;

pub use npz::NpzFormat;

/// A simple data‐format interface.
pub trait Format {
    /// Generate data and write to `path`.
    fn generate(&self, path: &std::path::Path) -> anyhow::Result<()>;
    /// Read & validate the data at `path`.
    fn read(&self, path: &std::path::Path) -> anyhow::Result<()>;
}

