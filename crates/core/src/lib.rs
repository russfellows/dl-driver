// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Core library for dl-driver ─ Enhanced with s3dlio data loading and DLIO compatibility.

// Main DLIO compatibility module with train/metric support
pub mod dlio_compat;

// Multi-rank coordination using shared memory and atomics
pub mod coordination;

// Legacy config module for backward compatibility - COMMENTED OUT to resolve conflicts
// There are two DlioConfig types causing issues. The dlio_compat version is the primary one.
// pub mod config;
// Temporarily disabled - needs update for new config system
// pub mod dataset;
pub mod plan;
// Temporarily disabled - needs update for new config system  
// pub mod generation;
pub mod metrics;
pub mod mlperf;
pub mod oplog_ingest;  // Op-log parsing and ingestion
pub mod plugins;
pub mod profiles;      // Realistic AI/ML framework workload patterns
pub mod runner;
pub mod validate;      // Workload validation against reference logs
pub mod workload;

// Re-export unified config system from dlio_compat (has train/metric fields)
pub use dlio_compat::DlioConfig;
pub use plan::RunPlan;

// Legacy exports removed - use DlioConfig directly

// Keep existing exports for compatibility (disabled while fixing)
// pub use dataset::{DatasetMetadata, DatasetReader, S3dlioDatasetReader};
// pub use generation::DatasetGenerator;
pub use metrics::Metrics;
pub use oplog_ingest::{OpLogRec, OpLogReader, Envelope, summarize_ops};
pub use profiles::{Profile, ProfileConfig, get_profile, list_profiles};
pub use runner::Runner;
pub use validate::{ValidationConfig, ValidationResult, ValidationSummary, validate_against_reference, print_validation_results, validate_and_exit, create_validation_config};
pub use workload::WorkloadRunner;

// New MLPerf runner
pub use mlperf::{MlperfRunner, MlperfReport};
