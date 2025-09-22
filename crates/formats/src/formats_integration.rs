// crates/formats/src/formats_integration.rs
//
// Integration layer between dl-driver format readers and s3dlio
//
// This module provides adapters that allow our format implementations
// to work with s3dlio's streaming interfaces while maintaining independence.

use anyhow::Result;
use futures_core::Stream;
use futures::StreamExt;
use bytes::Bytes;
use crate::{Format, npz::NpzStreamingFormat, hdf5::Hdf5StreamingFormat, tfrecord::TfRecordStreamingFormat};

/// A trait that bridges our Format trait with s3dlio streaming patterns
pub trait StreamingFormat: Format {
    /// Generate data and return as bytes for streaming to storage backends
    fn generate_bytes(&self, filename: &str) -> Result<Vec<u8>>;
    
    /// Read and validate data from bytes (useful for s3dlio streaming)
    fn read_from_bytes(&self, data: &[u8]) -> Result<()>;
    
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

/// Adapter that makes any StreamingFormat work with s3dlio patterns
pub struct S3dlioFormatAdapter<F: StreamingFormat> {
    format: F,
    base_path: String,
}

impl<F: StreamingFormat> S3dlioFormatAdapter<F> {
    pub fn new(format: F, base_path: String) -> Self {
        Self { format, base_path }
    }
    
    /// Generate a stream of (filename, data) tuples compatible with s3dlio
    pub async fn generate_file_stream(
        &self,
        num_files: usize,
        file_prefix: &str,
    ) -> Result<impl Stream<Item = Result<(String, Bytes)>>> {
        use futures::stream;
        use std::sync::Arc;
        
        let format = Arc::new(&self.format);
        let extension = self.format.file_extension();
        let prefix = file_prefix.to_string();
        
        let stream = stream::iter(0..num_files).then(move |i| {
            let format = format.clone();
            let prefix = prefix.clone();
            let ext = extension;
            
            async move {
                let filename = format!("{}_{:06}.{}", prefix, i, ext);
                
                match format.generate_bytes(&filename) {
                    Ok(data) => Ok((filename, Bytes::from(data))),
                    Err(e) => Err(e),
                }
            }
        });
        
        Ok(stream)
    }
}

/// Factory for creating s3dlio-compatible format adapters
pub struct S3dlioFormatFactory;

impl S3dlioFormatFactory {
    /// Create an adapter for the specified format
    pub fn create_adapter(
        format_name: &str,
        base_path: String,
        shape: Option<Vec<usize>>,
        record_length: Option<usize>,
        num_records: Option<usize>,
    ) -> Result<Box<dyn StreamingFormatAdapter>> {
        match format_name.to_lowercase().as_str() {
            "npz" => {
                let format = NpzStreamingFormat::new(
                    shape.unwrap_or_else(|| vec![224, 224, 3])
                );
                Ok(Box::new(S3dlioFormatAdapter::new(format, base_path)))
            },
            "hdf5" => {
                let format = Hdf5StreamingFormat::new(
                    shape.unwrap_or_else(|| vec![224, 224, 3]), 
                    None
                );
                Ok(Box::new(S3dlioFormatAdapter::new(format, base_path)))
            },
            "tfrecord" => {
                let format = TfRecordStreamingFormat::new(
                    num_records.unwrap_or(100),
                    record_length.unwrap_or(1024)
                );
                Ok(Box::new(S3dlioFormatAdapter::new(format, base_path)))
            },
            _ => anyhow::bail!("Unsupported format for s3dlio integration: {}", format_name),
        }
    }
}

/// Trait object for streaming format adapters
pub trait StreamingFormatAdapter: Send + Sync {
    fn generate_file_stream_boxed(
        &self,
        num_files: usize,
        file_prefix: &str,
    ) -> futures::future::BoxFuture<'_, Result<Box<dyn Stream<Item = Result<(String, Bytes)>> + Send + Unpin>>>;
    
    fn format_metadata(&self) -> FormatMetadata;
}