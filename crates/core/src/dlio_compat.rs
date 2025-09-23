// src/dlio_compat.rs
//
// DLIO-compatible configuration parsing for MLCommons benchmarks
//
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use s3dlio::api::advanced::PoolConfig;
use s3dlio::data_loader::options::LoadingMode;
use s3dlio::{LoaderOptions, ReaderMode};

/// Unified execution plan derived from DLIO config
/// This normalizes and validates all DLIO configuration into an actionable plan
#[derive(Debug, Clone)]
pub struct RunPlan {
    /// Model configuration
    pub model: ModelPlan,

    /// Workflow phases to execute
    pub workflow: WorkflowPlan,

    /// Dataset configuration and paths
    pub dataset: DatasetPlan,

    /// Reader/loader configuration
    pub reader: ReaderPlan,

    /// Checkpointing configuration
    pub checkpointing: Option<CheckpointingPlan>,

    /// Profiling configuration
    pub profiling: Option<ProfilingPlan>,
}

#[derive(Debug, Clone)]
pub struct ModelPlan {
    pub name: String,
    pub model_size_bytes: Option<u64>,
    pub framework: String,
}

#[derive(Debug, Clone)]
pub struct WorkflowPlan {
    pub generate_data: bool,
    pub train: bool,
    pub checkpoint: bool,
    pub evaluation: bool,
}

#[derive(Debug, Clone)]
pub struct DatasetPlan {
    /// Normalized data folder URI (file://, s3://, az://, direct://)
    pub data_folder_uri: String,

    /// Data format (npz, hdf5, tfrecord, csv, jpeg, png, synthetic)
    pub format: String,

    /// Training dataset configuration
    pub train: DatasetSplit,

    /// Evaluation dataset configuration  
    pub eval: Option<DatasetSplit>,
}

#[derive(Debug, Clone)]
pub struct DatasetSplit {
    pub num_files: usize,
    pub num_samples_per_file: usize,
    pub record_length_bytes: usize,
    pub total_samples: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ReaderPlan {
    pub batch_size: usize,
    pub prefetch: usize,
    pub shuffle: bool,
    pub read_threads: usize,
    pub seed: Option<u64>,
    pub loader_options: LoaderOptions,
    pub pool_config: PoolConfig,
}

#[derive(Debug, Clone)]
pub struct CheckpointingPlan {
    pub enabled: bool,
    pub checkpoint_folder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfilingPlan {
    pub enabled: bool,
    pub profiler_type: String,
}

/// DLIO-compatible JSON configuration structure
/// Based on MLCommons DLIO YAML schema translated to JSON
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DlioConfig {
    pub model: Option<ModelConfig>,
    pub framework: Option<String>,
    pub workflow: Option<WorkflowConfig>,
    pub dataset: DatasetConfig,
    pub reader: ReaderConfig,
    pub checkpointing: Option<CheckpointingConfig>,
    pub profiling: Option<ProfilingConfig>,

    // Framework-specific configurations for M4 integration
    pub pytorch_config: Option<PyTorchFrameworkConfig>,
    pub tensorflow_config: Option<TensorFlowFrameworkConfig>,
    pub jax_config: Option<JaxFrameworkConfig>,

    // Alternative nested framework configuration
    pub framework_profiles: Option<FrameworkProfiles>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub name: Option<String>,
    pub model_size: Option<u64>,
    pub framework: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
    pub generate_data: Option<bool>,
    pub train: Option<bool>,
    pub checkpoint: Option<bool>,
    pub evaluation: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetConfig {
    pub data_folder: String,
    pub format: Option<String>,
    pub num_files_train: Option<usize>,
    pub num_files_eval: Option<usize>,
    pub record_length_bytes: Option<usize>,
    pub num_samples_per_file: Option<usize>,
    pub compression: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReaderConfig {
    pub data_loader: Option<String>,
    pub batch_size: Option<usize>,
    pub prefetch: Option<usize>,
    pub shuffle: Option<bool>,
    pub read_threads: Option<usize>,
    pub compute_threads: Option<usize>,
    pub transfer_size: Option<usize>,
    pub file_access_type: Option<String>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointingConfig {
    pub checkpoint_folder: Option<String>,
    pub checkpoint_after_epoch: Option<usize>,
    pub epochs_between_checkpoints: Option<usize>,
    pub steps_between_checkpoints: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfilingConfig {
    pub profiler: Option<String>,
    pub profile_folder: Option<String>,
    pub iostat: Option<bool>,
}

/// Framework-specific configuration structures for M4 integration
/// PyTorch DataLoader configuration within DLIO config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PyTorchFrameworkConfig {
    /// Batch size for PyTorch DataLoader
    pub batch_size: Option<usize>,

    /// Number of worker processes for data loading  
    pub num_workers: Option<usize>,

    /// Whether to shuffle data
    pub shuffle: Option<bool>,

    /// Random seed for reproducibility
    pub seed: Option<u64>,

    /// Pin memory for CUDA acceleration
    pub pin_memory: Option<bool>,

    /// Drop last incomplete batch
    pub drop_last: Option<bool>,

    /// Prefetch factor for data loading
    pub prefetch_factor: Option<usize>,

    /// Enable persistent workers
    pub persistent_workers: Option<bool>,

    /// Return type: "tensor", "bytes", or "reader"
    pub return_type: Option<String>,

    /// Enable distributed training support
    pub distributed: Option<bool>,
}

/// TensorFlow tf.data.Dataset configuration within DLIO config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TensorFlowFrameworkConfig {
    /// Batch size for tf.data.Dataset
    pub batch_size: Option<usize>,

    /// Buffer size for shuffling
    pub shuffle_buffer_size: Option<usize>,

    /// Random seed for reproducibility
    pub seed: Option<u64>,

    /// Number of parallel calls for map operations
    pub num_parallel_calls: Option<i32>, // -1 for AUTOTUNE

    /// Prefetch buffer size (-1 for AUTOTUNE)
    pub prefetch_buffer_size: Option<i32>,

    /// Enable deterministic operations
    pub deterministic: Option<bool>,

    /// Drop remainder for batching
    pub drop_remainder: Option<bool>,

    /// Reshuffle each iteration
    pub reshuffle_each_iteration: Option<bool>,

    /// Enable writable NumPy arrays (extra copy)
    pub writable: Option<bool>,
}

/// JAX configuration within DLIO config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JaxFrameworkConfig {
    /// Random seed for reproducibility
    pub seed: Option<u64>,

    /// Enable writable NumPy arrays (extra copy)
    pub writable: Option<bool>,

    /// Batch size for grouping
    pub batch_size: Option<usize>,

    /// Buffer size for prefetching
    pub prefetch_buffer_size: Option<usize>,
}

/// Nested framework profiles structure (alternative organization)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrameworkProfiles {
    pub pytorch: Option<PyTorchFrameworkConfig>,
    pub tensorflow: Option<TensorFlowFrameworkConfig>,
    pub jax: Option<JaxFrameworkConfig>,
}

impl DlioConfig {
    /// Parse DLIO config from JSON string
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str).with_context(|| "Failed to parse DLIO JSON config")
    }

    /// Parse DLIO config from YAML string by converting to JSON first
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        // Parse YAML to generic Value first
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(yaml_str).with_context(|| "Failed to parse YAML")?;

        // Convert to JSON string
        let json_str =
            serde_json::to_string(&yaml_value).with_context(|| "Failed to convert YAML to JSON")?;

        // Parse as DLIO config
        Self::from_json(&json_str)
    }

    /// Convert this DLIO config to s3dlio LoaderOptions
    pub fn to_loader_options(&self) -> LoaderOptions {
        let reader = &self.reader;

        LoaderOptions {
            batch_size: reader.batch_size.unwrap_or(1),
            prefetch: reader.prefetch.unwrap_or(4),
            shuffle: reader.shuffle.unwrap_or(false),
            num_workers: reader.read_threads.unwrap_or(1),
            reader_mode: ReaderMode::Sequential, // Start with sequential for DLIO compatibility
            loading_mode: LoadingMode::AsyncPool(self.to_pool_config()),
            ..Default::default()
        }
    }

    /// Create PoolConfig for AsyncPoolDataLoader
    pub fn to_pool_config(&self) -> PoolConfig {
        // These settings aren't in DLIO YAML - use reasonable defaults
        // Can be overridden via CLI flags
        PoolConfig {
            pool_size: self.reader.read_threads.unwrap_or(4) * 4, // Scale up for async
            readahead_batches: self.reader.prefetch.unwrap_or(8),
            batch_timeout: std::time::Duration::from_secs(10),
            max_inflight: 64,
        }
    }

    /// Get the data folder URI for object store creation
    pub fn data_folder_uri(&self) -> &str {
        &self.dataset.data_folder
    }

    /// Detect storage backend from data_folder URI
    pub fn detect_storage_backend(&self) -> &str {
        let uri = &self.dataset.data_folder;

        if uri.starts_with("s3://") {
            "s3"
        } else if uri.starts_with("az://") {
            "azure"
        } else if uri.starts_with("direct://") {
            "direct"
        } else if uri.starts_with("file://") || !uri.contains("://") {
            "file"
        } else {
            "unknown"
        }
    }

    /// Convert this DLIO config to a comprehensive RunPlan
    pub fn to_run_plan(&self) -> Result<RunPlan> {
        // Normalize data folder URI
        let data_folder_uri = self.normalize_data_folder_uri(&self.dataset.data_folder)?;

        // Calculate dataset splits
        let train_split = self.calculate_dataset_split(
            self.dataset.num_files_train.unwrap_or(1),
            self.dataset.num_samples_per_file.unwrap_or(1),
            self.dataset.record_length_bytes.unwrap_or(1024),
        );

        let eval_split = self.dataset.num_files_eval.map(|num_files_eval| {
            self.calculate_dataset_split(
                num_files_eval,
                self.dataset.num_samples_per_file.unwrap_or(1),
                self.dataset.record_length_bytes.unwrap_or(1024),
            )
        });

        // Build the comprehensive plan
        Ok(RunPlan {
            model: ModelPlan {
                name: self
                    .model
                    .as_ref()
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| "dlio_workload".to_string()),
                model_size_bytes: self.model.as_ref().and_then(|m| m.model_size),
                framework: self
                    .framework
                    .clone()
                    .or_else(|| self.model.as_ref().and_then(|m| m.framework.clone()))
                    .unwrap_or_else(|| "unknown".to_string()),
            },

            workflow: WorkflowPlan {
                generate_data: self.should_generate_data(),
                train: self.should_train(),
                checkpoint: self.should_checkpoint(),
                evaluation: self.should_evaluate(),
            },

            dataset: DatasetPlan {
                data_folder_uri,
                format: self
                    .dataset
                    .format
                    .clone()
                    .unwrap_or_else(|| "npz".to_string()),
                train: train_split,
                eval: eval_split,
            },

            reader: ReaderPlan {
                batch_size: self.reader.batch_size.unwrap_or(1),
                prefetch: self.reader.prefetch.unwrap_or(4),
                shuffle: self.reader.shuffle.unwrap_or(false),
                read_threads: self.reader.read_threads.unwrap_or(1),
                seed: self.reader.seed,
                loader_options: self.to_loader_options(),
                pool_config: self.to_pool_config(),
            },

            checkpointing: self.checkpointing.as_ref().map(|c| CheckpointingPlan {
                enabled: c.checkpoint_after_epoch.unwrap_or(0) > 0,
                checkpoint_folder: c.checkpoint_folder.clone(),
            }),

            profiling: self.profiling.as_ref().map(|p| ProfilingPlan {
                enabled: true,
                profiler_type: p.profiler.clone().unwrap_or_else(|| "none".to_string()),
            }),
        })
    }

    /// Normalize data folder URI to ensure proper scheme
    fn normalize_data_folder_uri(&self, data_folder: &str) -> Result<String> {
        // If already has scheme, validate it
        if data_folder.contains("://") {
            let scheme = data_folder.split("://").next().unwrap_or("");
            match scheme {
                "file" | "s3" | "az" | "direct" => Ok(data_folder.to_string()),
                _ => Err(anyhow::anyhow!("Unsupported URI scheme: {}", scheme)),
            }
        } else {
            // Add file:// scheme to relative/absolute paths
            if data_folder.starts_with('/') {
                Ok(format!("file://{}", data_folder))
            } else {
                // Convert relative path to absolute
                let absolute_path = std::env::current_dir()?
                    .join(data_folder)
                    .canonicalize()
                    .unwrap_or_else(|_| std::env::current_dir().unwrap().join(data_folder));
                Ok(format!("file://{}", absolute_path.display()))
            }
        }
    }

    /// Calculate dataset split configuration with size calculations
    fn calculate_dataset_split(
        &self,
        num_files: usize,
        samples_per_file: usize,
        record_bytes: usize,
    ) -> DatasetSplit {
        let total_samples = num_files * samples_per_file;
        let total_bytes = (total_samples * record_bytes) as u64;

        DatasetSplit {
            num_files,
            num_samples_per_file: samples_per_file,
            record_length_bytes: record_bytes,
            total_samples,
            total_bytes,
        }
    }

    /// Check if data generation phase should run
    pub fn should_generate_data(&self) -> bool {
        self.workflow
            .as_ref()
            .and_then(|w| w.generate_data)
            .unwrap_or(false)
    }

    /// Check if training phase should run
    pub fn should_train(&self) -> bool {
        self.workflow.as_ref().and_then(|w| w.train).unwrap_or(true) // Default to true for DLIO compatibility
    }

    /// Check if checkpointing phase should run
    pub fn should_checkpoint(&self) -> bool {
        self.workflow
            .as_ref()
            .and_then(|w| w.checkpoint)
            .unwrap_or(false)
    }

    /// M4 Framework Integration Methods
    /// Get PyTorch framework configuration
    pub fn get_pytorch_config(&self) -> Option<&PyTorchFrameworkConfig> {
        // Check direct pytorch_config first
        if let Some(ref config) = self.pytorch_config {
            return Some(config);
        }

        // Check nested framework_profiles
        self.framework_profiles
            .as_ref()
            .and_then(|fp| fp.pytorch.as_ref())
    }

    /// Get TensorFlow framework configuration
    pub fn get_tensorflow_config(&self) -> Option<&TensorFlowFrameworkConfig> {
        // Check direct tensorflow_config first
        if let Some(ref config) = self.tensorflow_config {
            return Some(config);
        }

        // Check nested framework_profiles
        self.framework_profiles
            .as_ref()
            .and_then(|fp| fp.tensorflow.as_ref())
    }

    /// Get JAX framework configuration
    pub fn get_jax_config(&self) -> Option<&JaxFrameworkConfig> {
        // Check direct jax_config first
        if let Some(ref config) = self.jax_config {
            return Some(config);
        }

        // Check nested framework_profiles
        self.framework_profiles
            .as_ref()
            .and_then(|fp| fp.jax.as_ref())
    }

    /// Detect which framework is configured
    pub fn detect_framework(&self) -> Option<String> {
        // Check explicit framework field first
        if let Some(ref fw) = self.framework {
            return Some(fw.clone());
        }

        // Check which framework configs are present
        if self.get_pytorch_config().is_some() {
            return Some("pytorch".to_string());
        }

        if self.get_tensorflow_config().is_some() {
            return Some("tensorflow".to_string());
        }

        if self.get_jax_config().is_some() {
            return Some("jax".to_string());
        }

        None
    }

    /// Convert PyTorch config to s3dlio LoaderOptions
    pub fn to_pytorch_loader_options(&self) -> LoaderOptions {
        let mut opts = self.to_loader_options();

        if let Some(pytorch_config) = self.get_pytorch_config() {
            if let Some(batch_size) = pytorch_config.batch_size {
                opts.batch_size = batch_size;
            }
            if let Some(shuffle) = pytorch_config.shuffle {
                opts.shuffle = shuffle;
            }
            if let Some(seed) = pytorch_config.seed {
                opts.seed = seed;
            }
            if let Some(prefetch) = pytorch_config.prefetch_factor {
                opts.prefetch = prefetch;
            }
        }

        opts
    }

    /// Convert TensorFlow config to s3dlio LoaderOptions
    pub fn to_tensorflow_loader_options(&self) -> LoaderOptions {
        let mut opts = self.to_loader_options();

        if let Some(tf_config) = self.get_tensorflow_config() {
            if let Some(batch_size) = tf_config.batch_size {
                opts.batch_size = batch_size;
            }
            if let Some(seed) = tf_config.seed {
                opts.seed = seed;
            }
            // TensorFlow handles shuffling at tf.data level, not loader level
            opts.shuffle = false;
        }

        opts
    }

    /// Check if evaluation phase should run
    pub fn should_evaluate(&self) -> bool {
        self.workflow
            .as_ref()
            .and_then(|w| w.evaluation)
            .unwrap_or(false)
    }
}

/// Convert YAML string to JSON string (utility function)
pub fn yaml_to_json(yaml_str: &str) -> Result<String> {
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(yaml_str).with_context(|| "Failed to parse YAML")?;

    serde_json::to_string_pretty(&yaml_value).with_context(|| "Failed to convert to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_dlio_config() {
        let json = r#"
        {
            "model": {
                "name": "my_workload"
            },
            "framework": "pytorch",
            "workflow": {
                "generate_data": true,
                "train": true,
                "checkpoint": false
            },
            "dataset": {
                "data_folder": "/path/to/data",
                "format": "npz",
                "num_files_train": 100,
                "record_length_bytes": 1048576
            },
            "reader": {
                "data_loader": "pytorch",
                "batch_size": 16,
                "read_threads": 4,
                "compute_threads": 2,
                "prefetch": 8,
                "shuffle": true
            }
        }
        "#;

        let config = DlioConfig::from_json(json).expect("Should parse minimal config");

        assert_eq!(config.framework.as_deref(), Some("pytorch"));
        assert_eq!(config.dataset.data_folder, "/path/to/data");
        assert_eq!(config.reader.batch_size, Some(16));
        assert_eq!(config.reader.read_threads, Some(4));
        assert!(config.should_generate_data());
        assert!(config.should_train());
        assert!(!config.should_checkpoint());
    }

    #[test]
    fn test_parse_unet3d_config() {
        let json = r#"
        {
            "model": {
                "name": "unet3d_workload",
                "model_size": 499153191
            },
            "framework": "pytorch",
            "workflow": {
                "generate_data": true,
                "train": true,
                "checkpoint": false
            },
            "dataset": {
                "data_folder": "/path/to/unet3d/data",
                "format": "npz",
                "num_files_train": 100,
                "record_length_bytes": 1048576
            },
            "reader": {
                "data_loader": "pytorch",
                "batch_size": 4,
                "prefetch": 4,
                "shuffle": false,
                "read_threads": 2,
                "compute_threads": 2
            }
        }
        "#;

        let config = DlioConfig::from_json(json).expect("Should parse UNet3D config");

        assert_eq!(
            config.model.as_ref().unwrap().name.as_deref(),
            Some("unet3d_workload")
        );
        assert_eq!(config.model.as_ref().unwrap().model_size, Some(499153191));
        assert_eq!(config.reader.batch_size, Some(4));
        assert_eq!(config.reader.shuffle, Some(false));
    }

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml = r#"
model:
  name: test_workload
framework: pytorch
dataset:
  data_folder: /test/path
  format: npz
reader:
  batch_size: 32
  shuffle: true
        "#;

        let config = DlioConfig::from_yaml(yaml).expect("Should parse YAML");
        assert_eq!(
            config.model.as_ref().unwrap().name.as_deref(),
            Some("test_workload")
        );
        assert_eq!(config.reader.batch_size, Some(32));
    }

    #[test]
    fn test_loader_options_conversion() {
        let json = r#"
        {
            "dataset": {
                "data_folder": "/test"
            },
            "reader": {
                "batch_size": 8,
                "prefetch": 16,
                "shuffle": true,
                "read_threads": 6
            }
        }
        "#;

        let config = DlioConfig::from_json(json).expect("Should parse config");
        let loader_opts = config.to_loader_options();

        assert_eq!(loader_opts.batch_size, 8);
        assert_eq!(loader_opts.prefetch, 16);
        assert_eq!(loader_opts.shuffle, true);
        assert_eq!(loader_opts.num_workers, 6);
    }

    /// Test backend detection from data_folder URIs
    #[test]
    fn test_backend_detection() {
        let test_cases = [
            ("file:///tmp/data", "file"),
            ("s3://bucket/path", "s3"),
            ("az://account/container/path", "azure"),
            ("direct:///mnt/nvme/data", "direct"),
            ("/local/path", "file"), // implicit file
        ];

        for (uri, expected_backend) in test_cases {
            let json = format!(
                r#"{{
                "dataset": {{
                    "data_folder": "{}"
                }},
                "reader": {{}}
            }}"#,
                uri
            );

            let config = DlioConfig::from_json(&json).expect("Should parse config with valid URI");

            let backend = config.detect_storage_backend();
            assert_eq!(backend, expected_backend, "Failed for URI: {}", uri);
        }
    }

    /// Test framework profile integration
    #[test]
    fn test_framework_profiles() {
        let json = r#"{
            "framework": "pytorch",
            "dataset": {
                "data_folder": "/test"
            },
            "reader": {},
            "framework_profiles": {
                "pytorch": {
                    "num_workers": 4,
                    "pin_memory": true,
                    "persistent_workers": true,
                    "prefetch_factor": 3,
                    "drop_last": false
                }
            }
        }"#;

        let config =
            DlioConfig::from_json(json).expect("Should parse config with framework profiles");

        assert_eq!(config.framework.as_deref(), Some("pytorch"));

        let pytorch_config = config.get_pytorch_config();
        assert!(pytorch_config.is_some());

        let pytorch = pytorch_config.unwrap();
        assert_eq!(pytorch.num_workers, Some(4));
        assert_eq!(pytorch.pin_memory, Some(true));
        assert_eq!(pytorch.persistent_workers, Some(true));
        assert_eq!(pytorch.prefetch_factor, Some(3));
        assert_eq!(pytorch.drop_last, Some(false));
    }

    /// Test RunPlan conversion with comprehensive fields
    #[test]
    fn test_run_plan_conversion() {
        let json = r#"{
            "model": {
                "name": "test_model",
                "model_size": 500000000
            },
            "framework": "pytorch",
            "workflow": {
                "generate_data": true,
                "train": true,
                "checkpoint": true
            },
            "dataset": {
                "data_folder": "file:///mnt/vast1/test_data",
                "format": "npz",
                "num_files_train": 100,
                "num_samples_per_file": 64,
                "record_length_bytes": 8192
            },
            "reader": {
                "batch_size": 16,
                "prefetch": 4,
                "shuffle": true,
                "read_threads": 8
            }
        }"#;

        let config = DlioConfig::from_json(json).expect("Should parse config for RunPlan");

        let run_plan = config.to_run_plan().expect("Should convert to RunPlan");

        // Verify model plan
        assert_eq!(run_plan.model.name, "test_model");
        assert_eq!(run_plan.model.framework, "pytorch");

        // Verify workflow plan
        assert!(run_plan.workflow.generate_data);
        assert!(run_plan.workflow.train);
        assert!(run_plan.workflow.checkpoint);

        // Verify dataset plan
        assert_eq!(
            run_plan.dataset.data_folder_uri,
            "file:///mnt/vast1/test_data"
        );
        assert_eq!(run_plan.dataset.format, "npz");
        assert_eq!(run_plan.dataset.train.num_files, 100);
        assert_eq!(run_plan.dataset.train.num_samples_per_file, 64);

        // Verify reader plan
        assert_eq!(run_plan.reader.batch_size, 16);
        assert_eq!(run_plan.reader.prefetch, 4);
        assert!(run_plan.reader.shuffle);
    }

    /// Test error handling for invalid configurations
    #[test]
    fn test_error_handling_invalid_json() {
        let invalid_json = r#"{ "invalid": json syntax }"#;
        let result = DlioConfig::from_json(invalid_json);
        assert!(result.is_err(), "Should fail on invalid JSON");
    }

    /// Test data_folder URI normalization
    #[test]
    fn test_data_folder_uri_normalization() {
        let test_cases = [
            ("file:///tmp/data", "file:///tmp/data"),
            ("s3://bucket/key", "s3://bucket/key"),
            ("az://account/container", "az://account/container"),
        ];

        for (input, expected) in test_cases {
            let json = format!(
                r#"{{
                "dataset": {{
                    "data_folder": "{}"
                }},
                "reader": {{}}
            }}"#,
                input
            );

            let config = DlioConfig::from_json(&json).expect("Should parse config");

            let normalized_uri = config.data_folder_uri();
            assert_eq!(normalized_uri, expected, "Failed to normalize: {}", input);
        }
    }
}
