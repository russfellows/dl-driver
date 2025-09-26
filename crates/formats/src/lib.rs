// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/formats/src/lib.rs
//
pub mod hdf5;
pub mod npz;
pub mod tfrecord;
// TODO: Re-enable integration layer after core functionality is stable
// pub mod formats_integration;

pub use hdf5::{Hdf5Format, Hdf5StreamingFormat};
pub use npz::{NpzFormat, NpzStreamingFormat};
pub use tfrecord::{TfRecordFormat, TfRecordStreamingFormat};

/// A simple data‐format interface.
pub trait Format {
    /// Generate data and write to `path`.
    fn generate(&self, path: &std::path::Path) -> anyhow::Result<()>;
    /// Read & validate the data at `path`.
    fn read(&self, path: &std::path::Path) -> anyhow::Result<()>;
}

/// A trait that extends Format for in-memory streaming operations
pub trait StreamingFormat: Format {
    /// Generate data and return as bytes for streaming to storage backends
    fn generate_bytes(&self, filename: &str) -> anyhow::Result<Vec<u8>>;

    /// Read and validate data from bytes (useful for s3dlio streaming)
    fn read_from_bytes(&self, data: &[u8]) -> anyhow::Result<()>;

    /// Get the expected file extension for this format
    fn file_extension(&self) -> &'static str;

    /// Get format metadata (size estimates, etc.)
    fn format_metadata(&self) -> FormatMetadata;
}

/// Metadata about a format for size estimation and planning
#[derive(Debug, Clone)]
pub struct FormatMetadata {
    pub expected_size_bytes: Option<usize>,
    pub compression_ratio: Option<f64>,
    pub is_binary: bool,
    pub supports_streaming: bool,
}

/// Format factory for creating format instances from DLIO config strings
pub struct FormatFactory;

impl FormatFactory {
    /// Create a format instance based on the format string and optional configuration
    pub fn create_format(
        format_name: &str,
        shape: Option<Vec<usize>>,
        record_length: Option<usize>,
        num_records: Option<usize>,
    ) -> anyhow::Result<Box<dyn Format>> {
        let default_shape = vec![224, 224, 3]; // Default image-like shape
        let default_record_length = 1024;
        let default_num_records = 100;

        match format_name.to_lowercase().as_str() {
            "npz" => {
                let shape = shape.unwrap_or(default_shape);
                Ok(Box::new(NpzFormat::new(shape, 3))) // Default: data, labels, metadata arrays
            }
            "hdf5" => {
                let shape = shape.unwrap_or(default_shape);
                Ok(Box::new(Hdf5Format::new(shape, None)))
            }
            "tfrecord" => {
                let num_records = num_records.unwrap_or(default_num_records);
                let record_size = record_length.unwrap_or(default_record_length);
                Ok(Box::new(TfRecordFormat::new(num_records, record_size)))
            }
            _ => {
                anyhow::bail!("Unsupported format: {}", format_name)
            }
        }
    }

    /// Create a streaming format instance based on the format string and optional configuration
    pub fn create_streaming_format(
        format_name: &str,
        shape: Option<Vec<usize>>,
        record_length: Option<usize>,
        num_records: Option<usize>,
    ) -> anyhow::Result<Box<dyn StreamingFormat>> {
        let default_shape = vec![224, 224, 3]; // Default image-like shape
        let default_record_length = 1024;
        let default_num_records = 100;

        match format_name.to_lowercase().as_str() {
            "npz" => {
                let shape = shape.unwrap_or(default_shape);
                Ok(Box::new(NpzStreamingFormat::new(shape, 3))) // Default: data, labels, metadata arrays
            }
            "hdf5" => {
                let shape = shape.unwrap_or(default_shape);
                Ok(Box::new(Hdf5Format::new(shape, None)))
            }
            "tfrecord" => {
                let num_records = num_records.unwrap_or(default_num_records);
                let record_size = record_length.unwrap_or(default_record_length);
                Ok(Box::new(TfRecordFormat::new(num_records, record_size)))
            }
            _ => {
                anyhow::bail!("Unsupported format: {}", format_name)
            }
        }
    }

    /// Get all supported format names
    pub fn supported_formats() -> Vec<&'static str> {
        vec!["npz", "hdf5", "tfrecord"]
    }
}
