/// gRPC Agent Service for distributed DLIO workload execution
/// 
/// This module implements the agent server that receives workload requests,
/// applies path prefixes, coordinates start times, and executes DLIO workloads.

use anyhow::Result;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::{Request, Response, Status};
use tracing::{info, warn, error};

use crate::dlio_compat::DlioConfig;
use crate::workload::WorkloadRunner;
use crate::dist::proto::{
    dist_agent_server::DistAgent, 
    HealthCheckRequest, 
    HealthCheckResponse,
    RunWorkloadRequest, 
    WorkloadSummary,
};

/// Agent service implementation for distributed execution
pub struct AgentService {
    /// Agent identifier (e.g., "agent-0", "host1:50051")
    agent_id: String,
}

impl AgentService {
    /// Create a new agent service
    pub fn new(agent_id: String) -> Self {
        info!("Creating AgentService with ID: {}", agent_id);
        AgentService { agent_id }
    }

    /// Wait until the coordinated start time
    async fn wait_for_start(start_unix_ms: i64) -> Result<(), Status> {
        if start_unix_ms <= 0 {
            // No coordinated start, begin immediately
            return Ok(());
        }

        let start_time = UNIX_EPOCH + Duration::from_millis(start_unix_ms as u64);
        let now = SystemTime::now();

        match start_time.duration_since(now) {
            Ok(wait_duration) => {
                info!(
                    "Waiting {:?} until coordinated start time",
                    wait_duration
                );
                tokio::time::sleep(wait_duration).await;
                info!("Coordinated start time reached, beginning workload");
                Ok(())
            }
            Err(_) => {
                // Start time is in the past, begin immediately
                warn!(
                    "Start time is in the past ({}ms), beginning immediately",
                    start_unix_ms
                );
                Ok(())
            }
        }
    }

    /// Execute a DLIO workload and return metrics
    async fn execute_workload(
        &self,
        config: DlioConfig,
        agent_id: &str,
    ) -> Result<WorkloadSummary, Status> {
        info!("Agent {} starting workload execution", agent_id);

        // Extract config values for AI/ML metrics calculation
        let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1) as u64;
        let batch_size = config.reader.batch_size.unwrap_or(1) as u64;

        // Create and run the workload
        let start_time = SystemTime::now();
        
        let mut runner = WorkloadRunner::new(config);

        // Execute the workload
        runner
            .run()
            .await
            .map_err(|e| {
                error!("Workload execution failed: {}", e);
                Status::internal(format!("Workload execution failed: {}", e))
            })?;

        // Calculate duration
        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or(Duration::ZERO);
        let duration_s = duration.as_secs_f64();

        // Get metrics from the runner
        let metrics = runner.get_metrics();
        
        // === STORAGE METRICS ===
        let files_processed = metrics.files_processed();
        let bytes_read = metrics.bytes_read();
        let bytes_written = metrics.bytes_written();
        
        // Calculate storage throughput
        let total_ops = files_processed;
        let ops_per_s = if duration_s > 0.0 {
            total_ops as f64 / duration_s
        } else {
            0.0
        };

        let bytes_total = bytes_read + bytes_written;
        let mib_per_s = if duration_s > 0.0 {
            (bytes_total as f64 / (1024.0 * 1024.0)) / duration_s
        } else {
            0.0
        };

        // === AI/ML TRAINING METRICS ===
        // Sample-level calculations
        let total_samples = files_processed * samples_per_file;
        let samples_per_second = if duration_s > 0.0 {
            total_samples as f64 / duration_s
        } else {
            0.0
        };

        // Batch-level calculations
        let total_batches = if batch_size > 0 {
            (total_samples + batch_size - 1) / batch_size  // Ceiling division
        } else {
            0
        };
        
        let batches_per_second = if duration_s > 0.0 {
            total_batches as f64 / duration_s
        } else {
            0.0
        };

        // Calculate average batch time from metrics
        let batch_times = metrics.batch_times();
        let avg_batch_time_ms = if !batch_times.is_empty() {
            let total_batch_time: Duration = batch_times.iter().sum();
            total_batch_time.as_secs_f64() * 1000.0 / batch_times.len() as f64
        } else {
            0.0
        };

        // Epoch-level metrics
        let epoch_times = metrics.epoch_times();
        let epochs_completed = epoch_times.len() as u32;
        let avg_epoch_time_s = if !epoch_times.is_empty() {
            let total_epoch_time: Duration = epoch_times.iter().sum();
            total_epoch_time.as_secs_f64() / epoch_times.len() as f64
        } else {
            0.0
        };

        // Pipeline breakdown
        let data_loading_time_s = metrics.total_read_time().as_secs_f64();
        let compute_time_s = metrics.total_compute_time().as_secs_f64();
        let pipeline_efficiency = if duration_s > 0.0 {
            (data_loading_time_s + compute_time_s) / duration_s
        } else {
            0.0
        };

        // TODO: Get latency percentiles from metrics once available
        // For now, return zeros - these will be populated in future versions
        let (p50, p90, p95, p99) = (0.0, 0.0, 0.0, 0.0);
        let errors = 0u32; // TODO: Get error count from metrics

        info!(
            "Agent {} completed workload:",
            agent_id
        );
        info!(
            "  Training: {:.1} samples/s, {:.1} batches/s (batch_size={})",
            samples_per_second, batches_per_second, batch_size
        );
        info!(
            "  Storage:  {:.1} files/s, {:.1} MiB/s",
            ops_per_s, mib_per_s
        );
        info!(
            "  Totals:   {} samples, {} batches, {} files in {:.1}s",
            total_samples, total_batches, total_ops, duration_s
        );

        Ok(WorkloadSummary {
            agent_id: agent_id.to_string(),
            // Storage metrics
            ops_per_s,
            mib_per_s,
            p50_ms: p50,
            p90_ms: p90,
            p95_ms: p95,
            p99_ms: p99,
            errors,
            total_ops,
            duration_s,
            // AI/ML training metrics
            samples_per_second,
            total_samples,
            samples_per_batch: batch_size,
            batches_per_second,
            total_batches,
            avg_batch_time_ms,
            epochs_completed,
            avg_epoch_time_s,
            data_loading_time_s,
            compute_time_s,
            pipeline_efficiency,
            // Inline results (v0.8.1 enhancement - currently unused)
            console_log: String::new(),
            metadata_json: String::new(),
            storage_tsv_content: String::new(),
            aiml_tsv_content: String::new(),
            results_path: String::new(),
            // HDR histogram data (v0.8.1 enhancement - currently empty)
            histogram_read_latency: vec![],
            histogram_write_latency: vec![],
            histogram_batch_time: vec![],
        })
    }
}

#[tonic::async_trait]
impl DistAgent for AgentService {
    /// Execute a DLIO workload
    async fn run_workload(
        &self,
        request: Request<RunWorkloadRequest>,
    ) -> Result<Response<WorkloadSummary>, Status> {
        let req = request.into_inner();
        
        info!(
            "Received workload request for agent: {} (prefix: {})",
            req.agent_id, req.path_prefix
        );

        // Parse YAML config
        let mut config = DlioConfig::from_yaml(&req.config_yaml).map_err(|e| {
            error!("Failed to parse DLIO config: {}", e);
            Status::invalid_argument(format!("Invalid DLIO config: {}", e))
        })?;

        info!(
            "Parsed DLIO config: model={:?}, framework={:?}, data_folder={}",
            config.model.as_ref().map(|m| &m.name),
            config.framework,
            config.dataset.data_folder
        );

        // Apply agent path prefix for local storage isolation
        if !req.path_prefix.is_empty() {
            config
                .apply_agent_prefix(&req.agent_id, &req.path_prefix)
                .map_err(|e| {
                    error!("Failed to apply agent prefix: {}", e);
                    Status::internal(format!("Failed to apply path prefix: {}", e))
                })?;

            info!(
                "Applied path prefix '{}' to agent '{}', data_folder now: {}",
                req.path_prefix, req.agent_id, config.dataset.data_folder
            );
        }

        // Wait for coordinated start time
        Self::wait_for_start(req.start_unix_ms).await?;

        // Execute the workload and return metrics
        let summary = self.execute_workload(config, &req.agent_id).await?;

        Ok(Response::new(summary))
    }

    /// Health check endpoint
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        info!("Health check received for agent: {}", self.agent_id);

        Ok(Response::new(HealthCheckResponse {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_service_creation() {
        let service = AgentService::new("test-agent".to_string());
        assert_eq!(service.agent_id, "test-agent");
    }

    #[tokio::test]
    async fn test_wait_for_start_immediate() {
        // Start time of 0 means start immediately
        let result = AgentService::wait_for_start(0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_start_past() {
        // Start time in the past should begin immediately
        let past_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            - 5000; // 5 seconds ago

        let result = AgentService::wait_for_start(past_ms).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_start_future() {
        // Start time 100ms in the future
        let future_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 100;

        let start = std::time::Instant::now();
        let result = AgentService::wait_for_start(future_ms).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(elapsed.as_millis() >= 100);
    }
}
