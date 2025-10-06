//! Streaming operation log replay infrastructure (STUB - NOT OPERATIONAL)
//!
//! ⚠️ **IMPORTANT**: This module provides the **infrastructure** for operation log replay
//! but does NOT execute real storage I/O operations. All operations are currently **simulated**.
//!
//! ## Current Status (v0.7.1)
//! - ✅ Op-log parsing and streaming (via s3dlio-oplog)
//! - ✅ Timing and concurrency control
//! - ✅ URI remapping and transformation
//! - ❌ **Real I/O execution** (uses `simulate_operation()` only)
//!
//! ## For Real I/O Replay
//! Use **sai3-bench** (https://github.com/russfellows/sai3-bench) which provides:
//! - Full real I/O execution via s3dlio ObjectStore
//! - Advanced remapping (1:1, 1→N, N→1, regex)
//! - Microsecond timing precision
//! - Production-grade metrics with HDR histograms
//! - Distributed load generation
//!
//! ## Future Integration
//! This module provides stubs for potential future integration with sai3-bench's
//! replay engine. See `docs/REPLAY_ANALYSIS.md` for the full rationale.
//!
//! ## Current Functionality
//! The streaming architecture processes large op-logs (multi-GB) with constant memory
//! by leveraging s3dlio-oplog's 1MB chunk buffering and background decompression.
//! All operations are simulated with minimal delays for testing the infrastructure.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

// Import s3dlio-oplog for streaming reads
use s3dlio_oplog::{OpLogStreamReader, OpLogEntry};

// Legacy OpLogRec still used for ReplayOperation conversion
use crate::oplog_ingest::OpLogRec;

/// Streaming replay configuration
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Path to operation log file (supports .jsonl, .tsv, .csv, .zst compressed)
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
    /// Continue execution even if individual operations fail (default: true)
    pub continue_on_error: bool,
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
            continue_on_error: true,  // Continue by default for robustness
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

    /// Convert s3dlio-oplog OpLogEntry to ReplayOperation with remapping (STREAMING)
    /// 
    /// This is the primary conversion method for streaming replay using s3dlio-oplog.
    /// It provides the same remapping and timing functionality as from_op_log_rec but
    /// works with the streaming OpLogEntry format.
    pub fn from_oplog_entry(
        entry: &OpLogEntry,
        config: &ReplayConfig,
        prev_start: Option<&chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        // Calculate delay from previous operation
        let delay_ms = if config.fast_mode {
            None
        } else if let Some(prev) = prev_start {
            let delay = entry.start.signed_duration_since(*prev);
            if delay.num_milliseconds() > 0 {
                Some(delay.num_milliseconds() as u64)
            } else {
                None
            }
        } else {
            None
        };

        // Construct full URI from endpoint + file
        let mut uri = format!("{}{}", entry.endpoint, entry.file);
        
        // Apply path/endpoint remapping to URI
        for (from, to) in &config.path_remaps {
            if uri.contains(from) {
                uri = uri.replace(from, to);
            }
        }
        for (from, to) in &config.endpoint_remaps {
            if uri.contains(from) {
                uri = uri.replace(from, to);
            }
        }

        // Construct complete URI if base_uri provided
        let complete_uri = if !config.base_uri.is_empty() {
            Self::construct_complete_uri(&config.base_uri, &uri)
        } else {
            uri
        };

        ReplayOperation {
            operation_type: format!("{:?}", entry.op), // GET, PUT, DELETE, LIST, STAT
            path: complete_uri,
            bytes: Some(entry.bytes),
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

    /// Run the streaming replay from start to finish
    /// 
    /// This method uses s3dlio-oplog's OpLogStreamReader for constant-memory replay
    /// with background decompression. Large op-logs (multi-GB) are processed with
    /// ~10MB memory usage regardless of file size.
    pub async fn run_replay(&mut self) -> Result<ReplayStats> {
        info!("Starting streaming replay of {}", self.config.op_log_path);
        self.stats.start_time = Some(Instant::now());

        // Create streaming reader - spawns background decompression thread
        let stream = OpLogStreamReader::from_file(&self.config.op_log_path)
            .context("Failed to open operation log stream")?;

        info!("✅ Opened streaming reader with background decompression");

        // Execute operations directly from stream (no buffering!)
        if self.config.concurrency > 1 {
            self.execute_concurrent_streaming(stream).await?;
        } else {
            self.execute_sequential_streaming(stream).await?;
        }

        self.stats.end_time = Some(Instant::now());

        info!(
            "🎉 Streaming replay completed: {}/{} operations ({} failed), {:.2} ops/sec, {:.2} MB/s",
            self.stats.completed_operations,
            self.stats.total_operations,
            self.stats.failed_operations,
            self.stats.operations_per_second(),
            self.stats.throughput_mbps()
        );

        Ok(self.stats.clone())
    }

    /// Execute operations sequentially from streaming iterator (constant memory)
    async fn execute_sequential_streaming(
        &mut self,
        stream: OpLogStreamReader,
    ) -> Result<()> {
        let mut prev_start: Option<chrono::DateTime<chrono::Utc>> = None;

        for entry_result in stream {
            let entry = entry_result.context("Failed to read entry from stream")?;
            
            self.stats.total_operations += 1;

            // Convert to replay operation
            let op = ReplayOperation::from_oplog_entry(&entry, &self.config, prev_start.as_ref());
            prev_start = Some(entry.start);

            // Apply timing delay
            if let Some(delay_ms) = op.delay_ms {
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }

            // Execute the operation
            if let Err(e) = self.execute_operation(&op).await {
                warn!("Operation failed: {}", e);
                self.stats.failed_operations += 1;
                if !self.config.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Execute operations concurrently from streaming iterator (constant memory)
    async fn execute_concurrent_streaming(
        &mut self,
        stream: OpLogStreamReader,
    ) -> Result<()> {
        use tokio::sync::Semaphore;
        use std::sync::Arc;

        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let stats = Arc::new(tokio::sync::Mutex::new(ReplayStats::default()));
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        let continue_on_error = self.config.continue_on_error;
        
        let mut tasks = Vec::new();
        let mut prev_start: Option<chrono::DateTime<chrono::Utc>> = None;

        // Stream entries and spawn tasks without buffering full workload
        for entry_result in stream {
            let entry = entry_result.context("Failed to read entry from stream")?;
            
            {
                let mut stats_guard = stats.lock().await;
                stats_guard.total_operations += 1;
            }

            // Convert to replay operation
            let op = ReplayOperation::from_oplog_entry(&entry, &self.config, prev_start.as_ref());
            prev_start = Some(entry.start);

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
                let result = tokio::time::timeout(
                    timeout_duration,
                    Self::simulate_operation(&op)
                ).await;
                
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

                // Return error if not continuing on error
                if !continue_on_error {
                    match result {
                        Ok(r) => r,
                        Err(_) => anyhow::bail!("Operation timed out"),
                    }
                } else {
                    Ok(())
                }
            });

            tasks.push(task);
            
            // Limit in-flight tasks to prevent unbounded memory growth
            // For 10K concurrency, this prevents accumulating millions of tasks
            if tasks.len() >= 10_000 {
                // Wait for some tasks to complete
                let mut completed_count = 0;
                tasks.retain(|t| {
                    if t.is_finished() {
                        completed_count += 1;
                        false
                    } else {
                        true
                    }
                });
                
                if completed_count > 0 {
                    debug!("Drained {} completed tasks, {} remaining in queue", 
                           completed_count, tasks.len());
                }
            }
        }

        // Wait for remaining tasks
        for task in tasks {
            let _ = task.await?;
        }

        // Copy final stats
        let final_stats = stats.lock().await;
        self.stats.completed_operations = final_stats.completed_operations;
        self.stats.failed_operations = final_stats.failed_operations;
        self.stats.total_bytes = final_stats.total_bytes;
        self.stats.total_operations = final_stats.total_operations;

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
        // ⚠️ STUB FUNCTION - NOT OPERATIONAL
        // This is a placeholder for potential future integration with sai3-bench's real I/O engine.
        // Currently only simulates operation timing with a minimal delay.
        //
        // For real I/O replay, use sai3-bench (https://github.com/russfellows/sai3-bench)
        // which executes actual ObjectStore operations via s3dlio.
        //
        // A full implementation would:
        // 1. Construct appropriate ObjectStore from op.path URI scheme
        // 2. Call store.get(uri), store.put(uri, data), etc. based on op.operation_type
        // 3. Handle errors and retries per config
        // 4. Measure actual I/O latency
        
        sleep(Duration::from_millis(1)).await;
        
        debug!("SIMULATED {} on {} ({} bytes)", 
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
            continue_on_error: true,
        };

        let mut engine = SimpleReplayEngine::new(config);
        let stats = engine.run_replay().await.unwrap();

        assert_eq!(stats.total_operations, 2);
        assert!(stats.completed_operations > 0);
    }
}