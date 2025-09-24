use crate::framework_config::PyTorchConfig;
use anyhow::Result;
use dl_driver_core::dlio_compat::DlioConfig;
use s3dlio::LoaderOptions;

/// Format types supported by the PyTorch adapter
#[derive(Debug, Clone, PartialEq)]
pub enum FormatType {
    Npz,
    Hdf5,
    TfRecord,
}

/// PyTorch DataLoader configuration manager for dl-driver
///
/// Note: For M4, the main PyTorch integration is implemented in Python
/// (py_api/src/frameworks/pytorch.py) which wraps s3dlio's mature PyTorch classes.
/// This Rust struct provides configuration management and validation.
pub struct PyTorchDataLoader {
    /// PyTorch-specific configuration
    pytorch_config: PyTorchConfig,

    /// Format type for data parsing
    format_type: FormatType,

    /// Current epoch for tracking
    current_epoch: usize,

    /// Random seed state for reproducibility
    seed_state: Option<u64>,

    /// Data folder URI
    data_folder: String,
}

impl PyTorchDataLoader {
    /// Create a new PyTorch data loader configuration from DLIO config
    pub fn from_dlio_config(
        dlio_config: &DlioConfig,
        pytorch_config: PyTorchConfig,
        data_folder: String,
    ) -> Result<Self> {
        // Detect format from config
        let format_type = Self::detect_format(dlio_config)?;

        // Validate data folder URI
        Self::validate_data_folder(&data_folder)?;

        let seed_state = pytorch_config.seed;

        Ok(PyTorchDataLoader {
            pytorch_config,
            format_type,
            current_epoch: 0,
            seed_state,
            data_folder,
        })
    }

    /// Convert DLIO config to s3dlio LoaderOptions for Python integration
    pub fn to_loader_options(&self, dlio_config: &DlioConfig) -> LoaderOptions {
        // Use the existing DlioConfig method and enhance with PyTorch config
        let mut opts = dlio_config.to_loader_options();

        // Override with PyTorch-specific settings
        opts.batch_size = self.pytorch_config.batch_size;
        if let Some(seed) = self.pytorch_config.seed {
            opts.seed = seed;
        }
        opts.shuffle = self.pytorch_config.shuffle;

        opts
    }

    /// Get PyTorch configuration
    pub fn pytorch_config(&self) -> &PyTorchConfig {
        &self.pytorch_config
    }

    /// Get format type
    pub fn format_type(&self) -> &FormatType {
        &self.format_type
    }

    /// Get data folder URI
    pub fn data_folder(&self) -> &str {
        &self.data_folder
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> usize {
        self.current_epoch
    }

    /// Increment epoch counter
    pub fn next_epoch(&mut self) -> usize {
        self.current_epoch += 1;
        self.current_epoch
    }

    /// Reset epoch counter
    pub fn reset_epoch(&mut self) {
        self.current_epoch = 0;
    }

    /// Get current seed state
    pub fn seed_state(&self) -> Option<u64> {
        self.seed_state
    }

    /// Update seed state for next epoch (for reproducible shuffling)
    pub fn update_seed_state(&mut self, new_seed: Option<u64>) {
        self.seed_state = new_seed;
    }

    /// Detect format type from DLIO configuration
    fn detect_format(dlio_config: &DlioConfig) -> Result<FormatType> {
        match dlio_config.dataset.format.as_str() {
            "npz" => Ok(FormatType::Npz),
            "hdf5" => Ok(FormatType::Hdf5),
            "tfrecord" => Ok(FormatType::TfRecord),
            other => Err(anyhow::anyhow!("Unsupported format: {}", other)),
        }
    }

    /// Validate data folder URI
    fn validate_data_folder(data_folder: &str) -> Result<()> {
        // Basic URI validation - ensure it has a supported scheme
        if data_folder.starts_with("file://")
            || data_folder.starts_with("s3://")
            || data_folder.starts_with("az://")
            || data_folder.starts_with("direct://")
        {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Unsupported data folder URI: {}. Must use file://, s3://, az://, or direct:// scheme", 
                data_folder
            ))
        }
    }
}
