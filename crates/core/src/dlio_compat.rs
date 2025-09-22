// src/dlio_compat.rs
//
// DLIO-compatible configuration parsing for MLCommons benchmarks
//
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

use s3dlio::{LoaderOptions, ReaderMode};
use s3dlio::api::advanced::PoolConfig;
use s3dlio::data_loader::options::LoadingMode;



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

impl DlioConfig {
    /// Parse DLIO config from JSON string
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str)
            .with_context(|| "Failed to parse DLIO JSON config")
    }

    /// Parse DLIO config from YAML string by converting to JSON first
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        // Parse YAML to generic Value first
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str)
            .with_context(|| "Failed to parse YAML")?;
        
        // Convert to JSON string
        let json_str = serde_json::to_string(&yaml_value)
            .with_context(|| "Failed to convert YAML to JSON")?;
        
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
        
        let eval_split = if let Some(num_files_eval) = self.dataset.num_files_eval {
            Some(self.calculate_dataset_split(
                num_files_eval,
                self.dataset.num_samples_per_file.unwrap_or(1),
                self.dataset.record_length_bytes.unwrap_or(1024),
            ))
        } else {
            None
        };

        // Build the comprehensive plan
        Ok(RunPlan {
            model: ModelPlan {
                name: self.model.as_ref()
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| "dlio_workload".to_string()),
                model_size_bytes: self.model.as_ref()
                    .and_then(|m| m.model_size),
                framework: self.framework.clone()
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
                format: self.dataset.format.clone()
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
    fn calculate_dataset_split(&self, num_files: usize, samples_per_file: usize, record_bytes: usize) -> DatasetSplit {
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
        self.workflow
            .as_ref()
            .and_then(|w| w.train)
            .unwrap_or(true) // Default to true for DLIO compatibility
    }

    /// Check if checkpointing phase should run
    pub fn should_checkpoint(&self) -> bool {
        self.workflow
            .as_ref()
            .and_then(|w| w.checkpoint)
            .unwrap_or(false)
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
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str)
        .with_context(|| "Failed to parse YAML")?;
    
    serde_json::to_string_pretty(&yaml_value)
        .with_context(|| "Failed to convert to JSON")
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
        
        assert_eq!(config.model.as_ref().unwrap().name.as_deref(), Some("unet3d_workload"));
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
        assert_eq!(config.model.as_ref().unwrap().name.as_deref(), Some("test_workload"));
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
}