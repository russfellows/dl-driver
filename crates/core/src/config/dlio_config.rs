// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/core/src/config/dlio_config.rs
use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DlioConfig {
    pub model: Option<Model>,
    pub framework: Option<String>,      // "pytorch" | "tensorflow" | "jax" | ...
    pub workflow: Option<Workflow>,     // generate_data/train/checkpoint toggles
    pub dataset: Dataset,               // data_folder, format, sizes
    pub reader: Reader,                 // batch_size, prefetch, shuffle, read_threads...
    pub checkpoint: Option<Checkpoint>, // optional; used by plugin later
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model { 
    pub name: Option<String>, 
    pub model_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workflow { 
    pub generate_data: Option<bool>, 
    pub train: Option<bool>, 
    pub checkpoint: Option<bool>,
    pub evaluation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub data_folder: String,            // URI: file://, directio://, s3://, az://
    pub format: String,                 // "npz" | "tfrecord" | "hdf5" | ...
    pub num_files_train: Option<usize>,
    pub num_files_eval: Option<usize>,
    pub record_length_bytes: Option<usize>,
    pub num_samples_per_file: Option<usize>,
    pub compression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reader {
    pub batch_size: Option<usize>,
    pub prefetch: Option<usize>,
    pub shuffle: Option<bool>,
    pub read_threads: Option<usize>,
    pub compute_threads: Option<usize>, // you may keep as a no-op for I/O-only
    pub drop_last: Option<bool>,
    pub seed: Option<u64>,
    pub data_loader: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub enabled: Option<bool>,

    // Accept multiple legacy aliases: folder/path/dir
    #[serde(alias = "folder", alias = "path", alias = "dir", alias = "checkpoint_folder")]
    pub uri: Option<String>,            // where to write checkpoints (any backend)

    pub steps_between_checkpoints: Option<u32>,
    pub compression: Option<String>,    // e.g. "zstd"
    pub compression_level: Option<i32>, // e.g. 3
}

/// Normalize URI to handle file:// schemes properly  
/// Ensures file:// URIs use the format expected by s3dlio: file://absolute_path
pub fn normalize_uri(uri: &str) -> String {
    if uri.starts_with("file:///") {
        // Convert file:///absolute/path to file:///absolute/path (keep the absolute path format)
        // s3dlio expects file://path format where path is absolute, so keep as file:///absolute/path
        uri.to_string()
    } else if uri.starts_with("file://") {
        // Already in correct format
        uri.to_string() 
    } else {
        uri.to_string()
    }
}

impl DlioConfig {
    /// Parse DLIO config from JSON string
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("Failed to parse DLIO JSON config: {}", e))
    }

    /// Parse DLIO config from YAML string by converting to JSON first
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        // Parse YAML to generic Value first
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(yaml_str).map_err(|e| anyhow::anyhow!("Failed to parse YAML: {}", e))?;

        // Convert to JSON string
        let json_str =
            serde_json::to_string(&yaml_value).map_err(|e| anyhow::anyhow!("Failed to convert YAML to JSON: {}", e))?;

        // Parse as DLIO config
        Self::from_json(&json_str)
    }

    /// Load DlioConfig from YAML file
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;
        Self::from_yaml(&text)
    }

    /// Get storage URI based on data_folder - detect if it's S3, file, etc.
    pub fn storage_uri(&self) -> &str {
        &self.dataset.data_folder
    }

    /// Determine storage backend type from URI
    pub fn storage_backend(&self) -> StorageBackend {
        let uri = self.storage_uri();
        if uri.starts_with("s3://") {
            StorageBackend::S3
        } else if uri.starts_with("az://") {
            StorageBackend::Azure
        } else if uri.starts_with("direct://") || uri.starts_with("directio://") {
            StorageBackend::DirectIO
        } else {
            StorageBackend::File
        }
    }

    /// Get data folder URI (alias for storage_uri)
    pub fn data_folder_uri(&self) -> &str {
        &self.dataset.data_folder
    }

    /// Check if data generation should run
    pub fn should_generate_data(&self) -> bool {
        self.workflow.as_ref().map_or(false, |w| w.generate_data.unwrap_or(false))
    }

    /// Check if training should run
    pub fn should_train(&self) -> bool {
        self.workflow.as_ref().map_or(false, |w| w.train.unwrap_or(false))
    }

    /// Check if checkpointing should run
    pub fn should_checkpoint(&self) -> bool {
        self.workflow.as_ref().map_or(false, |w| w.checkpoint.unwrap_or(false))
    }

    /// Convert to s3dlio LoaderOptions
    pub fn to_loader_options(&self) -> s3dlio::data_loader::LoaderOptions {
        s3dlio::data_loader::LoaderOptions {
            batch_size: self.reader.batch_size.unwrap_or(1),
            prefetch: self.reader.prefetch.unwrap_or(1),
            shuffle: self.reader.shuffle.unwrap_or(false),
            num_workers: self.reader.read_threads.unwrap_or(1),
            reader_mode: s3dlio::ReaderMode::Sequential,
            loading_mode: s3dlio::LoadingMode::AsyncPool(self.to_pool_config()),
            seed: self.reader.seed.unwrap_or(0),
            ..Default::default()
        }
    }

    /// Convert to s3dlio PoolConfig
    pub fn to_pool_config(&self) -> s3dlio::data_loader::PoolConfig {
        s3dlio::data_loader::PoolConfig {
            pool_size: self.reader.read_threads.unwrap_or(1),
            readahead_batches: self.reader.prefetch.unwrap_or(1),
            ..Default::default()
        }
    }

    /// Convert to RunPlan
    pub fn to_run_plan(&self) -> Result<crate::plan::RunPlan> {
        Ok(crate::plan::RunPlan::from_config(self))
    }
}

#[derive(Debug, Clone)]
pub enum StorageBackend {
    S3,
    Azure,
    File,
    DirectIO,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_backend_detection() {
        let test_cases = vec![
            ("file:///tmp/test", StorageBackend::File),
            ("s3://bucket/path", StorageBackend::S3),
            ("az://account/container/path", StorageBackend::Azure),
            ("direct:///tmp/direct", StorageBackend::DirectIO),
            ("directio:///tmp/direct", StorageBackend::DirectIO),
        ];

        for (uri, expected) in test_cases {
            let config = DlioConfig {
                dataset: Dataset {
                    data_folder: uri.to_string(),
                    format: "npz".to_string(),
                    num_files_train: None,
                    num_files_eval: None,
                    record_length_bytes: None,
                    num_samples_per_file: None,
                    compression: None,
                },
                model: None,
                framework: None,
                workflow: None,
                reader: Reader {
                    batch_size: None,
                    prefetch: None,
                    shuffle: None,
                    read_threads: None,
                    compute_threads: None,
                    drop_last: None,
                    seed: None,
                    data_loader: None,
                },
                checkpoint: None,
            };

            match (config.storage_backend(), expected) {
                (StorageBackend::S3, StorageBackend::S3) => {},
                (StorageBackend::Azure, StorageBackend::Azure) => {},
                (StorageBackend::File, StorageBackend::File) => {},
                (StorageBackend::DirectIO, StorageBackend::DirectIO) => {},
                _ => panic!("Backend detection mismatch for URI: {}", uri),
            }
        }
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
model:
  name: "test_model"
framework: "pytorch"
workflow:
  generate_data: true
  train: true
dataset:
  data_folder: "file:///tmp/test"
  format: "npz"
  num_files_train: 100
reader:
  batch_size: 32
  prefetch: 4
  shuffle: true
  read_threads: 2
"#;
        
        let config = DlioConfig::from_yaml(yaml).expect("Should parse YAML");
        assert_eq!(config.model.as_ref().unwrap().name, Some("test_model".to_string()));
        assert_eq!(config.framework, Some("pytorch".to_string()));
        assert_eq!(config.dataset.data_folder, "file:///tmp/test");
        assert_eq!(config.reader.batch_size, Some(32));
    }
}