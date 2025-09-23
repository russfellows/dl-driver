//! Core library for dl-driver ─ Enhanced with s3dlio data loading and DLIO compatibility.

pub mod config;
pub mod dataset;
pub mod dlio_compat;
pub mod generation;
pub mod metrics;
pub mod runner;
pub mod workload;

pub use config::Config;
pub use dataset::{DatasetMetadata, DatasetReader, S3dlioDatasetReader};
pub use dlio_compat::{DlioConfig, RunPlan};
pub use generation::DatasetGenerator;
pub use metrics::Metrics;
pub use runner::Runner;
pub use workload::WorkloadRunner;
