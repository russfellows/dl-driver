// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Core library for dl-driver ─ Enhanced with s3dlio data loading and DLIO compatibility.

// Main DLIO compatibility module with train/metric support
pub mod dlio_compat;

// Multi-rank coordination using shared memory and atomics
pub mod coordination;

// Legacy config module for backward compatibility  
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
pub mod workload;

// Re-export unified config system from dlio_compat (has train/metric fields)
pub use dlio_compat::DlioConfig;
pub use plan::RunPlan;

// Legacy exports removed - use DlioConfig directly

// Keep existing exports for compatibility (disabled while fixing)
// pub use dataset::{DatasetMetadata, DatasetReader, S3dlioDatasetReader};
// pub use generation::DatasetGenerator;
pub use metrics::Metrics;
pub use runner::Runner;
pub use workload::WorkloadRunner;

// New MLPerf runner
pub use mlperf::{MlperfRunner, MlperfReport};
