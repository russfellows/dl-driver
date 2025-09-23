// src/dataset.rs
//
// Unified dataset abstraction for dl-driver
//
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use futures_core::Stream;

use crate::dlio_compat::{ReaderPlan, RunPlan};
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset};

/// Generic dataset reader trait for unified data access
/// This abstracts over different storage backends and data formats
#[async_trait]
pub trait DatasetReader {
    /// The type of items yielded by this reader
    type Item;

    /// Create a stream of data items from this dataset
    async fn stream(&self) -> Result<Box<dyn Stream<Item = Result<Self::Item>> + Send + Unpin>>;

    /// Get metadata about this dataset
    fn metadata(&self) -> DatasetMetadata;
}

/// Metadata about a dataset
#[derive(Debug, Clone)]
pub struct DatasetMetadata {
    pub total_files: usize,
    pub total_samples: usize,
    pub total_bytes: u64,
    pub format: String,
    pub backend: String,
}

/// s3dlio-based implementation of DatasetReader
/// This is the primary implementation using s3dlio's MultiBackendDataset + AsyncPoolDataLoader
pub struct S3dlioDatasetReader {
    dataset: MultiBackendDataset,
    reader_plan: ReaderPlan,
    metadata: DatasetMetadata,
}

impl S3dlioDatasetReader {
    /// Create a new S3dlioDatasetReader from a RunPlan
    pub async fn from_run_plan(run_plan: &RunPlan) -> Result<Self> {
        // Create the dataset from URI
        let dataset = MultiBackendDataset::from_prefix(&run_plan.dataset.data_folder_uri).await?;

        // Build metadata
        let metadata = DatasetMetadata {
            total_files: run_plan.dataset.train.num_files,
            total_samples: run_plan.dataset.train.total_samples,
            total_bytes: run_plan.dataset.train.total_bytes,
            format: run_plan.dataset.format.clone(),
            backend: detect_backend_from_uri(&run_plan.dataset.data_folder_uri),
        };

        Ok(Self {
            dataset,
            reader_plan: run_plan.reader.clone(),
            metadata,
        })
    }

    /// Create a new S3dlioDatasetReader from URI and reader configuration
    pub async fn from_uri_and_reader(uri: &str, reader_plan: &ReaderPlan) -> Result<Self> {
        // Create the dataset from URI
        let dataset = MultiBackendDataset::from_prefix(uri).await?;

        // Build basic metadata (without full RunPlan context)
        let metadata = DatasetMetadata {
            total_files: 0,   // Unknown without full plan
            total_samples: 0, // Unknown without full plan
            total_bytes: 0,   // Unknown without full plan
            format: "unknown".to_string(),
            backend: detect_backend_from_uri(uri),
        };

        Ok(Self {
            dataset,
            reader_plan: reader_plan.clone(),
            metadata,
        })
    }
}

#[async_trait]
impl DatasetReader for S3dlioDatasetReader {
    type Item = (String, Vec<u8>);

    async fn stream(&self) -> Result<Box<dyn Stream<Item = Result<Self::Item>> + Send + Unpin>> {
        // Create a fresh loader with the stored configuration
        let loader = AsyncPoolDataLoader::new(
            self.dataset.clone(),
            self.reader_plan.loader_options.clone(),
        );
        let stream = loader.stream();

        // Convert the s3dlio stream to our expected format
        let mapped_stream = stream.map(|batch_result| {
            match batch_result {
                Ok(batch) => {
                    // For now, return the first item in the batch
                    // TODO: Properly handle batches
                    if !batch.is_empty() {
                        Ok(("batch_item".to_string(), batch[0].clone()))
                    } else {
                        Err(anyhow::anyhow!("Empty batch received"))
                    }
                }
                Err(e) => Err(anyhow::anyhow!("Batch error: {}", e)),
            }
        });

        Ok(Box::new(mapped_stream))
    }

    fn metadata(&self) -> DatasetMetadata {
        self.metadata.clone()
    }
}

/// Detect backend type from URI scheme
fn detect_backend_from_uri(uri: &str) -> String {
    if uri.starts_with("file://") {
        "File".to_string()
    } else if uri.starts_with("s3://") {
        "S3".to_string()
    } else if uri.starts_with("az://") {
        "Azure".to_string()
    } else if uri.starts_with("direct://") {
        "DirectIO".to_string()
    } else {
        "Unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_dataset_reader_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().join("test_data");
        std::fs::create_dir_all(&data_path).unwrap();

        // Create a test file
        let test_file = data_path.join("test.txt");
        std::fs::write(&test_file, b"test data").unwrap();

        let uri = format!("file://{}", data_path.display());

        // Create a basic reader configuration
        let reader_plan = ReaderPlan {
            batch_size: 1,
            prefetch: 1,
            shuffle: false,
            read_threads: 1,
            seed: None,
            loader_options: Default::default(),
            pool_config: Default::default(),
        };

        let reader = S3dlioDatasetReader::from_uri_and_reader(&uri, &reader_plan).await;

        // Should create successfully even with minimal metadata
        assert!(reader.is_ok());
        if let Ok(reader) = reader {
            let metadata = reader.metadata();
            assert_eq!(metadata.backend, "File");
        }
    }
}
