/// Distributed execution module for multi-host DLIO workloads
/// 
/// This module provides gRPC-based coordination for running DLIO workloads
/// across multiple hosts. It includes:
/// - Agent service for executing workloads
/// - Controller client for coordinating multiple agents
/// - Path utilities for storage backend detection and URI rewriting
/// - Types for workload requests and metric summaries

pub mod types;
pub mod path_utils;

// Include distributed config from config module
pub use crate::config::distributed::DistributedConfig;

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("dl_driver.dist");
}

// Re-export key types for convenience
pub use types::{WorkloadRequest, WorkloadResult};
pub use path_utils::{is_shared_storage, apply_path_prefix, join_uri_path};
