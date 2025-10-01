//! Simple operation log replay functionality
//!
//! This module provides straightforward replay of operation logs with:
//! - Timing control (maintain inter-arrival delays by default)
//! - Basic remapping for cross-environment support
//! - Concurrent execution for I/O performance
//! - Integration with existing oplog_ingest functionality

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info};

use crate::oplog_ingest::{OpLogReader, OpLogRec};

/// Simple replay configuration
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Path to operation log file
    pub op_log_path: String,
    /// Base URI for storage operations (e.g., file:///tmp/test, s3://bucket, direct:///mnt/data)
    pub base_uri: String,
    /// Maximum concurrent operations (default: 10)
    pub concurrency: usize,
    /// Skip timing delays and run as fast as possible
    pub fast_mode: bool,
    /// Timeout for individual operations in seconds
    pub timeout_seconds: u64,
    /// Simple path remapping (from -> to)
    pub path_remaps: HashMap<String, String>,
    /// Simple endpoint remapping (from -> to)
    pub endpoint_remaps: HashMap<String, String>,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            op_log_path: String::new(),
            base_uri: String::new(),
            concurrency: 10,
            fast_mode: false,
            timeout_seconds: 30,
            path_remaps: HashMap::new(),
            endpoint_remaps: HashMap::new(),
        }
    }
}

/// Simple operation for replay
#[derive(Debug, Clone)]
pub struct ReplayOperation {
    pub operation_type: String,
    pub path: String,
    pub bytes: Option<u64>,
    pub delay_ms: Option<u64>,
}

impl ReplayOperation {
    /// Convert OpLogRec to ReplayOperation with remapping and base URI
    pub fn from_op_log_rec(rec: &OpLogRec, config: &ReplayConfig, prev_timestamp_ns: Option<u64>) -> Self {
        // Calculate delay from previous operation
        let delay_ms = if config.fast_mode {
            None
        } else if let (Some(current_ns), Some(prev_ns)) = (rec.t_start_ns, prev_timestamp_ns) {
            let delay_ns = current_ns.saturating_sub(prev_ns);
            Some(delay_ns / 1_000_000) // Convert to milliseconds
        } else {
            None
        };

        // Apply path remapping
        let mut path = rec.file.as_ref().unwrap_or(&String::new()).clone();
        for (from, to) in &config.path_remaps {
            if path.contains(from) {
                path = path.replace(from, to);
            }
        }

        // Apply endpoint remapping (simple string replacement)
        for (from, to) in &config.endpoint_remaps {
            if path.contains(from) {
                path = path.replace(from, to);
            }
        }

        // Construct complete URI from base_uri + relative path
        let complete_path = Self::construct_complete_uri(&config.base_uri, &path);

        ReplayOperation {
            operation_type: rec.operation.clone(),
            path: complete_path,
            bytes: rec.bytes,
            delay_ms,
        }
    }

    /// Construct complete URI from base URI + relative path
    fn construct_complete_uri(base_uri: &str, relative_path: &str) -> String {
        // If path is already an absolute URI, return as-is
        if relative_path.contains("://") {
            return relative_path.to_string();
        }
        
        // Remove leading slash from relative path to avoid double slashes
        let clean_path = relative_path.strip_prefix('/').unwrap_or(relative_path);
        
        // Ensure base URI ends with slash for proper joining
        let base = if base_uri.ends_with('/') {
            base_uri.to_string()
        } else {
            format!("{}/", base_uri)
        };
        
        format!("{}{}", base, clean_path)
    }
}

/// Simple replay statistics
#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    pub total_operations: usize,
    pub completed_operations: usize,
    pub failed_operations: usize,
    pub total_bytes: u64,
    pub start_time: Option<Instant>,
    pub end_time: Option<Instant>,
}

impl ReplayStats {
    pub fn duration(&self) -> Option<Duration> {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            Some(end - start)
        } else {
            None
        }
    }

    pub fn operations_per_second(&self) -> f64 {
        if let Some(duration) = self.duration() {
            let secs = duration.as_secs_f64();
            if secs > 0.0 {
                self.completed_operations as f64 / secs
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    pub fn throughput_mbps(&self) -> f64 {
        if let Some(duration) = self.duration() {
            let secs = duration.as_secs_f64();
            if secs > 0.0 {
                (self.total_bytes as f64) / (1024.0 * 1024.0) / secs
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

/// Simple replay engine
pub struct SimpleReplayEngine {
    config: ReplayConfig,
    stats: ReplayStats,
}

impl SimpleReplayEngine {
    pub fn new(config: ReplayConfig) -> Self {
        Self {
            config,
            stats: ReplayStats::default(),
        }
    }

    /// Run the replay from start to finish
    pub async fn run_replay(&mut self) -> Result<ReplayStats> {
        info!("Starting replay of {}", self.config.op_log_path);
        self.stats.start_time = Some(Instant::now());

        // Parse the operation log using existing functionality
        let reader = OpLogReader::from_file(&self.config.op_log_path)
            .context("Failed to parse operation log")?;
        let parsed_log = reader.records();

        self.stats.total_operations = parsed_log.len();
        info!("Parsed {} operations from log", self.stats.total_operations);

        // Convert to replay operations with timing and remapping
        let mut replay_ops = Vec::new();
        let mut prev_timestamp_ns = None;

        for rec in parsed_log {
            let replay_op = ReplayOperation::from_op_log_rec(rec, &self.config, prev_timestamp_ns);
            replay_ops.push(replay_op);
            prev_timestamp_ns = rec.t_start_ns;
        }

        // Execute operations with concurrency
        if self.config.concurrency > 1 {
            self.execute_concurrent(replay_ops).await?;
        } else {
            self.execute_sequential(replay_ops).await?;
        }

        self.stats.end_time = Some(Instant::now());

        info!(
            "Replay completed: {}/{} operations, {:.2} ops/sec, {:.2} MB/s",
            self.stats.completed_operations,
            self.stats.total_operations,
            self.stats.operations_per_second(),
            self.stats.throughput_mbps()
        );

        Ok(self.stats.clone())
    }

    async fn execute_sequential(&mut self, operations: Vec<ReplayOperation>) -> Result<()> {
        for op in operations {
            // Apply timing delay
            if let Some(delay_ms) = op.delay_ms {
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }

            // Execute the operation
            self.execute_operation(&op).await?;
        }
        Ok(())
    }

    async fn execute_concurrent(&mut self, operations: Vec<ReplayOperation>) -> Result<()> {
        use tokio::sync::Semaphore;
        use std::sync::Arc;

        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let stats = Arc::new(tokio::sync::Mutex::new(ReplayStats::default()));
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);

        let mut tasks = Vec::new();

        for op in operations {
            let sem = semaphore.clone();
            let stats_ref = stats.clone();
            
            let task = tokio::spawn(async move {
                // Apply timing delay before acquiring semaphore
                if let Some(delay_ms) = op.delay_ms {
                    if delay_ms > 0 {
                        sleep(Duration::from_millis(delay_ms)).await;
                    }
                }

                // Acquire concurrency permit
                let _permit = sem.acquire().await.unwrap();

                // Execute operation with timeout
                let result = tokio::time::timeout(timeout_duration, Self::simulate_operation(&op)).await;
                
                // Update stats
                {
                    let mut stats_guard = stats_ref.lock().await;
                    match result {
                        Ok(Ok(())) => {
                            stats_guard.completed_operations += 1;
                            stats_guard.total_bytes += op.bytes.unwrap_or(0);
                        }
                        Ok(Err(_)) | Err(_) => {
                            stats_guard.failed_operations += 1;
                        }
                    }
                }

                // Convert timeout error to anyhow error
                match result {
                    Ok(r) => r,
                    Err(_) => anyhow::bail!("Operation timed out"),
                }
            });

            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await?;
        }

        // Copy final stats back to self
        let final_stats = stats.lock().await;
        self.stats.completed_operations = final_stats.completed_operations;
        self.stats.failed_operations = final_stats.failed_operations;
        self.stats.total_bytes = final_stats.total_bytes;

        Ok(())
    }

    async fn execute_operation(&mut self, op: &ReplayOperation) -> Result<()> {
        // For now, simulate the operation with timeout support
        // In a full implementation, this would use s3dlio to execute the actual operation
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        let result = tokio::time::timeout(timeout_duration, Self::simulate_operation(op)).await;

        match result {
            Ok(Ok(())) => {
                self.stats.completed_operations += 1;
                self.stats.total_bytes += op.bytes.unwrap_or(0);
                Ok(())
            }
            Ok(Err(e)) => {
                self.stats.failed_operations += 1;
                Err(e)
            }
            Err(_) => {
                self.stats.failed_operations += 1;
                anyhow::bail!("Operation timed out after {} seconds", self.config.timeout_seconds)
            }
        }
    }

    async fn simulate_operation(op: &ReplayOperation) -> Result<()> {
        // Simulate work with a small delay
        sleep(Duration::from_millis(1)).await;
        
        debug!("Executed {} on {} ({} bytes)", 
               op.operation_type, 
               op.path, 
               op.bytes.unwrap_or(0));
        
        Ok(())
    }

    pub fn get_stats(&self) -> &ReplayStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_simple_replay() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.jsonl");
        
        // Create a simple test log
        let test_data = r#"{"operation": "GET", "file": "/test/file1", "bytes": 1024, "t_start_ns": 1000000000}
{"operation": "PUT", "file": "/test/file2", "bytes": 2048, "t_start_ns": 1500000000}"#;
        
        fs::write(&log_path, test_data).unwrap();

        let config = ReplayConfig {
            op_log_path: log_path.to_str().unwrap().to_string(),
            base_uri: "file:///tmp/test".to_string(),
            concurrency: 2,
            fast_mode: false,
            timeout_seconds: 30,
            path_remaps: vec![("/test".to_string(), "/replay".to_string())].into_iter().collect(),
            endpoint_remaps: HashMap::new(),
        };

        let mut engine = SimpleReplayEngine::new(config);
        let stats = engine.run_replay().await.unwrap();

        assert_eq!(stats.total_operations, 2);
        assert!(stats.completed_operations > 0);
    }
}