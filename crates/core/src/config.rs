use anyhow::Result;
use serde::Deserialize;
use std::{fs, path::Path, collections::HashMap};

/// Complete DLIO configuration structure - fully compatible with original DLIO
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub model: Option<ModelConfig>,
    pub framework: Option<String>,
    pub workflow: WorkflowConfig,
    pub dataset: DatasetConfig,
    pub reader: ReaderConfig,
    pub train: Option<TrainConfig>,
    pub evaluation: Option<EvaluationConfig>,
    pub checkpoint: Option<CheckpointConfig>,
    pub output: Option<OutputConfig>,
    pub profiling: Option<ProfilingConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    #[serde(alias = "model_size")] // Support legacy field name
    pub model_size_bytes: Option<u64>,
    #[serde(rename = "type")]
    pub model_type: Option<String>, // transformer, CNN, etc.
    pub model_datatype: Option<String>, // fp16, fp32, int8, uint8, bf16
    pub optimizer_datatype: Option<String>, // fp16, fp32, int8, uint8, bf16
    pub optimization_groups: Option<Vec<i32>>,
    pub num_layers: Option<i32>,
    pub layer_parameters: Option<Vec<i32>>,
    pub parallelism: Option<ParallelismConfig>,
    pub transformer: Option<TransformerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ParallelismConfig {
    pub tensor: Option<i32>,
    pub pipeline: Option<i32>, 
    pub data: Option<i32>,
    pub zero_stage: Option<i32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TransformerConfig {
    pub vocab_size: Option<i32>,
    pub hidden_size: Option<i32>,
    pub ffn_hidden_size: Option<i32>,
    pub num_attention_heads: Option<i32>,
    pub num_kv_heads: Option<i32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorkflowConfig {
    pub generate_data: Option<bool>,
    pub train: Option<bool>,
    pub evaluation: Option<bool>,
    pub checkpoint: Option<bool>,
    pub profiling: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatasetConfig {
    pub data_folder: String,
    pub format: String,
    pub num_files_train: u32,
    pub num_files_eval: Option<u32>,
    pub num_samples_per_file: Option<u32>,
    #[serde(alias = "record_length")] // Support legacy field name  
    pub record_length_bytes: Option<u64>,
    #[serde(alias = "record_length_stdev")]
    pub record_length_bytes_stdev: Option<u64>,
    #[serde(alias = "record_length_resize")]
    pub record_length_bytes_resize: Option<u64>,
    pub num_subfolders_train: Option<u32>,
    pub num_subfolders_eval: Option<u32>,
    pub file_prefix: Option<String>,
    pub compression: Option<String>, // none, gzip, lz4, etc.
    pub compression_level: Option<i32>,
    pub enable_chunking: Option<bool>, // for HDF5
    pub chunk_size: Option<u32>,
    pub keep_files: Option<bool>,
    pub record_dims: Option<Vec<i32>>, // for multi-dimensional data
    pub record_element_type: Option<String>, // uint8, float32, etc.
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReaderConfig {
    pub data_loader: String,
    pub batch_size: u32,
    pub batch_size_eval: Option<u32>,
    pub read_threads: Option<u32>,
    pub pin_memory: Option<bool>,
    pub computation_threads: Option<u32>,
    pub prefetch_size: Option<u32>,
    pub file_shuffle: Option<String>, // off, seed, random
    pub sample_shuffle: Option<String>, // off, seed, random
    pub transfer_size: Option<u64>, // for tensorflow data loader
    pub preprocess_time: Option<f64>, // emulated preprocess time
    pub preprocess_time_stdev: Option<f64>,
    // For data transformations
    pub transformed_record_dims: Option<Vec<i32>>,
    pub transformed_record_element_type: Option<String>,
    // Custom plugin support
    pub reader_classname: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TrainConfig {
    pub epochs: u32,
    pub computation_time: Option<f64>, // Can be specified as distribution
    pub computation_time_stdev: Option<f64>,
    pub total_training_steps: Option<i32>, // for running less than one epoch
    pub seed_change_epoch: Option<bool>,
    pub seed: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EvaluationConfig {
    pub eval_time: Option<f64>, // emulated computation time per eval step
    pub eval_time_stdev: Option<f64>,
    pub epochs_between_evals: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CheckpointConfig {
    pub checkpoint_folder: String,
    pub checkpoint_after_epoch: Option<u32>,
    pub epochs_between_checkpoints: Option<u32>,
    pub steps_between_checkpoints: Option<u32>,
    pub model_size: Option<u64>, // DLIO uses this for checkpoint size calculation
    pub checkpoint_mechanism_classname: Option<String>, // for custom plugins
    pub checkpoint_fsync: Option<bool>,
    pub time_between_checkpoints: Option<f64>,
    pub num_checkpoints_write: Option<i32>,
    pub num_checkpoints_read: Option<i32>,
    pub checkpoint_rank_sync: Option<bool>,
    pub recovery_rank_shift: Option<i32>,
    pub randomize_tensor: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OutputConfig {
    pub folder: Option<String>, // output folder name
    pub log_file: Option<String>,
    pub metric: Option<HashMap<String, i32>>, // exclude_start_steps, exclude_end_steps
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfilingConfig {
    pub iostat_devices: Option<Vec<String>>, // devices to trace
}

impl Config {
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let text = fs::read_to_string(&path)?;
        let config: Config = serde_yaml::from_str(&text)?;
        Ok(config)
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
        } else if uri.starts_with("direct://") {
            StorageBackend::DirectIO
        } else {
            StorageBackend::File
        }
    }
}

#[derive(Debug, Clone)]
pub enum StorageBackend {
    S3,
    Azure,
    File,
    DirectIO,
}

// Include unit tests for config functionality
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_backend_detection() {
        // Test each URI scheme detection
        let test_cases = vec![
            ("file:///tmp/test", StorageBackend::File),
            ("s3://bucket/path", StorageBackend::S3), 
            ("az://account/container/path", StorageBackend::Azure),
            ("direct:///tmp/test", StorageBackend::DirectIO),
            ("/local/path", StorageBackend::File), // default to file
            ("https://example.com", StorageBackend::File), // unknown becomes file
        ];

        for (uri, expected_backend) in test_cases {
            let config = Config {
                model: None,
                framework: None,
                workflow: WorkflowConfig {
                    generate_data: Some(true),
                    train: Some(true),
                    evaluation: None,
                    checkpoint: Some(false),
                    profiling: None,
                },
                dataset: DatasetConfig {
                    data_folder: uri.to_string(),
                    format: "npz".to_string(),
                    num_files_train: 1,
                    num_files_eval: None,
                    num_samples_per_file: Some(1),
                    record_length_bytes: Some(1024),
                    record_length_bytes_stdev: Some(0),
                    record_length_bytes_resize: Some(1024),
                    num_subfolders_train: None,
                    num_subfolders_eval: None,
                    file_prefix: None,
                    compression: None,
                    compression_level: None,
                    enable_chunking: None,
                    chunk_size: None,
                    keep_files: None,
                    record_dims: None,
                    record_element_type: None,
                },
                reader: ReaderConfig {
                    data_loader: "pytorch".to_string(),
                    batch_size: 1,
                    batch_size_eval: None,
                    read_threads: Some(1),
                    pin_memory: None,
                    computation_threads: None,
                    prefetch_size: None,
                    file_shuffle: Some("seed".to_string()),
                    sample_shuffle: Some("seed".to_string()),
                    transfer_size: None,
                    preprocess_time: None,
                    preprocess_time_stdev: None,
                    transformed_record_dims: None,
                    transformed_record_element_type: None,
                    reader_classname: None,
                },
                train: Some(TrainConfig {
                    epochs: 1,
                    computation_time: Some(0.01),
                    computation_time_stdev: None,
                    total_training_steps: None,
                    seed_change_epoch: None,
                    seed: None,
                }),
                evaluation: None,
                checkpoint: None,
                output: None,
                profiling: None,
            };

            let detected = config.storage_backend();
            assert_eq!(std::mem::discriminant(&detected), std::mem::discriminant(&expected_backend), 
                      "URI '{}' should detect as {:?}, got {:?}", uri, expected_backend, detected);
        }
    }

    #[test]
    fn test_expanded_config_parsing() {
        // Test that we can parse a DLIO config with all new fields
        let yaml_content = r#"
model:
  name: test_unet3d
  model_size_bytes: 499153191
  type: transformer
  model_datatype: fp16
  optimizer_datatype: fp32
  parallelism:
    tensor: 1
    pipeline: 1
    zero_stage: 0
  transformer:
    vocab_size: 32000
    hidden_size: 2048

framework: pytorch

workflow:
  generate_data: true
  train: true
  evaluation: false
  checkpoint: true
  profiling: false

dataset:
  data_folder: file:///tmp/test-workload/
  format: npz
  num_files_train: 10
  num_files_eval: 5
  num_samples_per_file: 1
  record_length_bytes: 1048576
  file_prefix: img
  record_element_type: uint8

reader:
  data_loader: pytorch
  batch_size: 4
  batch_size_eval: 2
  read_threads: 2
  pin_memory: true
  prefetch_size: 2
  file_shuffle: seed
  sample_shuffle: seed

train:
  epochs: 5
  computation_time: 1.36
  seed: 123

evaluation:
  eval_time: 0.5
  epochs_between_evals: 2

checkpoint:
  checkpoint_folder: checkpoints/test
  model_size: 499153191

output:
  folder: hydra_log/test

profiling:
  iostat_devices: [sda, sdb]
"#;
        
        let config: Config = serde_yaml::from_str(yaml_content).unwrap();
        
        // Test model section
        let model = config.model.unwrap();
        assert_eq!(model.name, "test_unet3d");
        assert_eq!(model.model_size_bytes, Some(499153191));
        assert_eq!(model.model_type, Some("transformer".to_string()));
        
        // Test new workflow fields
        assert_eq!(config.workflow.evaluation, Some(false));
        assert_eq!(config.workflow.profiling, Some(false));
        
        // Test new dataset fields
        assert_eq!(config.dataset.num_files_eval, Some(5));
        assert_eq!(config.dataset.file_prefix, Some("img".to_string()));
        assert_eq!(config.dataset.record_element_type, Some("uint8".to_string()));
        
        // Test new reader fields
        assert_eq!(config.reader.batch_size_eval, Some(2));
        assert_eq!(config.reader.pin_memory, Some(true));
        assert_eq!(config.reader.prefetch_size, Some(2));
        
        // Test new config sections
        assert!(config.evaluation.is_some());
        assert!(config.checkpoint.is_some());
        assert!(config.output.is_some());
        assert!(config.profiling.is_some());
        
        let eval = config.evaluation.unwrap();
        assert_eq!(eval.eval_time, Some(0.5));
        assert_eq!(eval.epochs_between_evals, Some(2));
    }

    #[test]
    fn test_storage_uri_extraction() {
        let config = Config {
            model: None,
            framework: None,
            workflow: WorkflowConfig {
                generate_data: Some(true),
                train: Some(true),
                evaluation: None,
                checkpoint: Some(false),
                profiling: None,
            },
            dataset: DatasetConfig {
                data_folder: "s3://my-bucket/my-path/".to_string(),
                format: "npz".to_string(),
                num_files_train: 10,
                num_files_eval: None,
                num_samples_per_file: Some(1),
                record_length_bytes: Some(2048),
                record_length_bytes_stdev: Some(0),
                record_length_bytes_resize: Some(2048),
                num_subfolders_train: None,
                num_subfolders_eval: None,
                file_prefix: None,
                compression: None,
                compression_level: None,
                enable_chunking: None,
                chunk_size: None,
                keep_files: None,
                record_dims: None,
                record_element_type: None,
            },
            reader: ReaderConfig {
                data_loader: "pytorch".to_string(),
                batch_size: 4,
                batch_size_eval: None,
                read_threads: Some(2),
                pin_memory: None,
                computation_threads: None,
                prefetch_size: None,
                file_shuffle: Some("seed".to_string()),
                sample_shuffle: Some("seed".to_string()),
                transfer_size: None,
                preprocess_time: None,
                preprocess_time_stdev: None,
                transformed_record_dims: None,
                transformed_record_element_type: None,
                reader_classname: None,
            },
            train: Some(TrainConfig {
                epochs: 5,
                computation_time: Some(0.05),
                computation_time_stdev: None,
                total_training_steps: None,
                seed_change_epoch: None,
                seed: None,
            }),
            evaluation: None,
            checkpoint: None,
            output: None,
            profiling: None,
        };

        assert_eq!(config.storage_uri(), "s3://my-bucket/my-path/");
        assert_eq!(config.dataset.num_files_train, 10);
        assert_eq!(config.reader.batch_size, 4);
    }
}
