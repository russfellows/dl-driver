//! Core library for dl-driver ─ Enhanced with s3dlio data loading and DLIO compatibility.

pub mod config;
// Temporarily disabled - needs update for new config system
// pub mod dataset;
pub mod plan;
// Temporarily disabled - needs update for new config system  
// pub mod generation;
pub mod metrics;
pub mod mlperf;
pub mod plugins;
pub mod runner;
// Removed: WorkloadRunner is legacy code, no longer needed
// pub mod workload;

// Compatibility alias for existing imports
pub mod dlio_compat {
    pub use crate::config::{DlioConfig, yaml_to_json};
}

// Re-export unified config system
pub use config::DlioConfig;
pub use plan::RunPlan;

// Legacy exports removed - use DlioConfig directly

// Keep existing exports for compatibility (disabled while fixing)
// pub use dataset::{DatasetMetadata, DatasetReader, S3dlioDatasetReader};
// pub use generation::DatasetGenerator;
pub use metrics::Metrics;
pub use runner::Runner;

// New MLPerf runner
pub use mlperf::{MlperfRunner, MlperfReport};
