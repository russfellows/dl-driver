use anyhow::Result;
use dl_driver_core::config::DlioConfig;
use serde::{Deserialize, Serialize};

/// Configuration for framework integrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkConfig {
    /// PyTorch-specific configuration
    pub pytorch: Option<PyTorchConfig>,

    /// TensorFlow-specific configuration  
    pub tensorflow: Option<TensorFlowConfig>,

    /// Base DLIO configuration
    pub dlio: DlioConfig,
}

/// PyTorch DataLoader configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyTorchConfig {
    /// Batch size for DataLoader
    pub batch_size: usize,

    /// Number of worker processes for data loading
    pub num_workers: usize,

    /// Whether to shuffle data
    pub shuffle: bool,

    /// Random seed for reproducibility
    pub seed: Option<u64>,

    /// Pin memory for CUDA acceleration
    pub pin_memory: bool,

    /// Drop last incomplete batch
    pub drop_last: bool,

    /// Prefetch factor for data loading
    pub prefetch_factor: Option<usize>,

    /// Enable persistent workers
    pub persistent_workers: bool,
}

/// TensorFlow Dataset configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorFlowConfig {
    /// Batch size for tf.data.Dataset
    pub batch_size: usize,

    /// Buffer size for shuffling
    pub shuffle_buffer_size: Option<usize>,

    /// Random seed for reproducibility
    pub seed: Option<u64>,

    /// Number of parallel calls for map operations
    pub num_parallel_calls: Option<usize>,

    /// Prefetch buffer size
    pub prefetch_buffer_size: Option<usize>,

    /// Enable deterministic operations
    pub deterministic: bool,
}

impl Default for PyTorchConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            num_workers: 4,
            shuffle: true,
            seed: Some(42),
            pin_memory: false,
            drop_last: false,
            prefetch_factor: Some(2),
            persistent_workers: false,
        }
    }
}

impl Default for TensorFlowConfig {
    fn default() -> Self {
        Self {
            batch_size: 32,
            shuffle_buffer_size: Some(1000),
            seed: Some(42),
            num_parallel_calls: None,   // tf.data.AUTOTUNE
            prefetch_buffer_size: None, // tf.data.AUTOTUNE
            deterministic: true,
        }
    }
}

impl FrameworkConfig {
    /// Create a new FrameworkConfig from a DLIO config with PyTorch defaults
    pub fn from_dlio_with_pytorch(dlio: DlioConfig) -> Self {
        Self {
            pytorch: Some(PyTorchConfig::default()),
            tensorflow: None,
            dlio,
        }
    }

    /// Create a new FrameworkConfig from a DLIO config with TensorFlow defaults  
    pub fn from_dlio_with_tensorflow(dlio: DlioConfig) -> Self {
        Self {
            pytorch: None,
            tensorflow: Some(TensorFlowConfig::default()),
            dlio,
        }
    }

    /// Load from YAML file with framework extensions
    pub fn from_yaml_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Validate configuration consistency
    pub fn validate(&self) -> Result<()> {
        // Ensure at least one framework is configured
        if self.pytorch.is_none() && self.tensorflow.is_none() {
            anyhow::bail!("At least one framework (pytorch or tensorflow) must be configured");
        }

        // Validate PyTorch config if present
        if let Some(pytorch) = &self.pytorch {
            if pytorch.batch_size == 0 {
                anyhow::bail!("PyTorch batch_size must be > 0");
            }
            if pytorch.prefetch_factor.is_some() && pytorch.prefetch_factor.unwrap() == 0 {
                anyhow::bail!("PyTorch prefetch_factor must be > 0 if specified");
            }
        }

        // Validate TensorFlow config if present
        if let Some(tensorflow) = &self.tensorflow {
            if tensorflow.batch_size == 0 {
                anyhow::bail!("TensorFlow batch_size must be > 0");
            }
        }

        Ok(())
    }
}
