/// gRPC Agent Service for distributed DLIO workload execution
/// 
/// This module implements the agent server that receives workload requests,
/// applies path prefixes, coordinates start times, and executes DLIO workloads.

use anyhow::{Result, Context};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::{Request, Response, Status};
use tracing::{info, warn, error};

use crate::dlio_compat::DlioConfig;
use crate::workload::WorkloadRunner;
use crate::dist::proto::{
    dist_agent_server::DistAgent, 
    HealthCheckRequest, 
    HealthCheckResponse,
    LiveStats,
    RunWorkloadRequest, 
    WorkloadSummary,
    live_stats::Status as LiveStatsStatus,  // v0.8.7: Import Status enum
};

/// Agent service implementation for distributed execution
#[derive(Clone)]
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

    /// UNUSED: Create temporary results directory and export TSV content (file-based approach)
    /// 
    /// This was an alternative implementation that writes to temp files (like sai3-bench does).
    /// Currently NOT USED - we generate TSV content in-memory instead (see export_to_string).
    /// Kept for reference in case we need file-based approach later.
    #[allow(dead_code)]
    fn _create_agent_tsv_content_via_file(
        agent_id: &str,
        read_hists: &crate::metrics::StorageOpHists,
        write_hists: &crate::metrics::StorageOpHists,
        bytes_read: u64,
        bytes_written: u64,
        duration_s: f64,
    ) -> Result<String, Status> {
        use std::fs;
        use crate::tsv_export::StorageTsvExporter;
        
        // Create temp directory for agent results (with PID for uniqueness)
        let pid = std::process::id();
        let agent_results_dir = std::env::temp_dir()
            .join(format!("dl-driver-agent-{}-{}", agent_id, pid));
        
        fs::create_dir_all(&agent_results_dir)
            .map_err(|e| Status::internal(format!("Failed to create temp results dir: {}", e)))?;
        
        // Export storage TSV with bucket-level histograms
        let tsv_path = agent_results_dir.join("storage_results.tsv");
        let exporter = StorageTsvExporter::new(&tsv_path);
        
        exporter.export_results(read_hists, write_hists, bytes_read, bytes_written, duration_s)
            .map_err(|e| Status::internal(format!("Failed to export results: {}", e)))?;
        
        // Read back the TSV content
        let tsv_content = fs::read_to_string(&tsv_path)
            .map_err(|e| Status::internal(format!("Failed to read TSV content: {}", e)))?;
        
        // Cleanup temp directory (optional - /tmp will be cleaned eventually)
        let _ = fs::remove_dir_all(&agent_results_dir);
        
        Ok(tsv_content)
    }

    /// Execute a DLIO workload and return metrics
    async fn execute_workload(
        &self,
        config: DlioConfig,
        agent_id: &str,
        live_stats_tracker: Option<Arc<crate::live_stats::LiveStatsTracker>>,
        rank_start: usize,
        ranks_per_agent: usize,
        global_world_size: usize,
        shard_strategy: &str,
    ) -> Result<WorkloadSummary, Status> {
        // v0.8.8 Phase 2: Multi-rank per agent support
        // Each agent spawns multiple WorkloadRunners in parallel (tokio tasks)
        // ranks_per_agent = 1: Phase 1 behavior (single rank)
        // ranks_per_agent > 1: Phase 2 behavior (multiple concurrent ranks per agent)
        
        info!(
            "Agent {} starting workload execution: rank_start={}, ranks_per_agent={}, world_size={}, strategy={}",
            agent_id, rank_start, ranks_per_agent, global_world_size, shard_strategy
        );

        if ranks_per_agent == 1 {
            // Phase 1 path: Single rank per agent (optimized, no spawning overhead)
            Self::execute_single_rank(
                config,
                agent_id,
                live_stats_tracker,
                rank_start,
                global_world_size,
                shard_strategy,
            ).await
        } else {
            // Phase 2 path: Multiple ranks per agent (spawn concurrent tasks)
            Self::execute_multi_rank(
                config,
                agent_id,
                live_stats_tracker,
                rank_start,
                ranks_per_agent,
                global_world_size,
                shard_strategy,
            ).await
        }
    }

    /// Execute single rank workload (Phase 1: optimized path)
    async fn execute_single_rank(
        config: DlioConfig,
        agent_id: &str,
        live_stats_tracker: Option<Arc<crate::live_stats::LiveStatsTracker>>,
        global_rank: usize,
        global_world_size: usize,
        shard_strategy: &str,
    ) -> Result<WorkloadSummary, Status> {
        info!("Agent {} executing single rank {} of {}", agent_id, global_rank, global_world_size);

        // Extract config values for AI/ML metrics calculation
        let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1) as u64;
        let batch_size = config.reader.batch_size.unwrap_or(1) as u64;

        // Create and run the workload
        let start_time = SystemTime::now();
        
        let mut runner = WorkloadRunner::new(config.clone());
        
        // Apply data sharding if in multi-rank mode
        if global_world_size > 1 {
            info!("Discovering files for sharding from: {}", config.dataset.data_folder);
            
            // Discover files from data_folder using s3dlio
            let file_list = Self::discover_files(&config.dataset.data_folder).await
                .map_err(|e| {
                    error!("Failed to discover files: {}", e);
                    Status::internal(format!("File discovery failed: {}", e))
                })?;
            
            info!("Discovered {} total files before sharding", file_list.len());
            
            // Apply sharding strategy to get this rank's subset
            let sharded_files = Self::apply_sharding_strategy(
                &file_list,
                global_world_size,
                global_rank,
                shard_strategy,
            ).map_err(|e| {
                error!("Failed to apply sharding strategy: {}", e);
                Status::internal(format!("Sharding failed: {}", e))
            })?;
            
            info!("After sharding: rank {} gets {}/{} files", 
                  global_rank, sharded_files.len(), file_list.len());
            
            // Configure runner with rank-specific file list
            runner = runner.with_rank_config(
                global_rank as u32,
                global_world_size as u32,
                Some(sharded_files),
            );
        }
        
        // Wire live stats tracker for distributed operation recording (v0.8.7+)
        if let Some(tracker) = live_stats_tracker {
            runner = runner.with_live_stats_tracker(tracker);
        }

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

        // v0.8.1: Extract histograms and calculate accurate percentiles
        let read_hists = metrics.get_read_histograms();
        let write_hists = metrics.get_write_histograms();
        let batch_hists = metrics.get_batch_histograms();

        // Calculate percentiles from combined read histogram (across all size buckets)
        // v0.8.7: Values in microseconds (no conversion needed - histograms store µs)
        let combined_read = read_hists.combined_histogram();
        let (p50, p90, p95, p99) = if combined_read.len() > 0 {
            (
                combined_read.value_at_quantile(0.50) as f64,
                combined_read.value_at_quantile(0.90) as f64,
                combined_read.value_at_quantile(0.95) as f64,
                combined_read.value_at_quantile(0.99) as f64,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let errors = 0u32; // TODO: Get error count from metrics

        // v0.8.6: Serialize ALL bucket histograms for accurate aggregation (like sai3-bench)
        // This allows controller to properly merge histograms across agents
        use crate::dist::histogram::serialize_histogram;
        
        // Serialize read histograms (all 9 size buckets)
        let mut histogram_read = Vec::new();
        for bucket_hist in read_hists.buckets.iter() {
            let hist = bucket_hist.lock().unwrap();
            serialize_histogram(&*hist)
                .and_then(|bytes| {
                    histogram_read.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Failed to serialize read histogram bucket: {}", e);
                });
        }

        // Serialize write histograms (all 9 size buckets)
        let mut histogram_write = Vec::new();
        for bucket_hist in write_hists.buckets.iter() {
            let hist = bucket_hist.lock().unwrap();
            serialize_histogram(&*hist)
                .and_then(|bytes| {
                    histogram_write.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Failed to serialize write histogram bucket: {}", e);
                });
        }

        // Serialize batch time histogram (single histogram)
        let histogram_batch = if let Some(batch_hist) = batch_hists.get_histogram() {
            serialize_histogram(&batch_hist)
                .unwrap_or_else(|e| {
                    warn!("Failed to serialize batch histogram: {}", e);
                    vec![]
                })
        } else {
            vec![]
        };

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

        // v0.8.6: Generate bucket-level TSV content in memory (like sai3-bench)
        // This allows controller to write per-agent TSV files
        use crate::tsv_export::StorageTsvExporter;
        
        let storage_tsv_content = StorageTsvExporter::export_to_string(
            &read_hists,
            &write_hists,
            bytes_read,
            bytes_written,
            duration_s,
        ).unwrap_or_else(|e| {
            warn!("Failed to generate storage TSV content: {}", e);
            String::new()
        });

        Ok(WorkloadSummary {
            agent_id: agent_id.to_string(),
            // Storage metrics
            ops_per_s,
            mib_per_s,
            p50_us: p50,
            p90_us: p90,
            p95_us: p95,
            p99_us: p99,
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
            // Inline results (v0.8.6 enhancement - bucket-level TSV content)
            console_log: String::new(),
            metadata_json: String::new(),
            storage_tsv_content,
            aiml_tsv_content: String::new(),
            results_path: String::new(),
            // HDR histogram data (v0.8.6) - serialized for accurate aggregation
            // Each histogram field contains 9 serialized bucket histograms (except batch which has 1)
            histogram_read,
            histogram_write,
            histogram_batch,
        })
    }

    /// Execute multi-rank workload (Phase 2: spawn multiple concurrent runners)
    /// 
    /// Each agent spawns `ranks_per_agent` WorkloadRunner instances as tokio tasks.
    /// Each runner gets its own global_rank, applies sharding, and creates independent
    /// storage clients to simulate separate processes hitting storage concurrently.
    async fn execute_multi_rank(
        config: DlioConfig,
        agent_id: &str,
        live_stats_tracker: Option<Arc<crate::live_stats::LiveStatsTracker>>,
        rank_start: usize,
        ranks_per_agent: usize,
        global_world_size: usize,
        shard_strategy: &str,
    ) -> Result<WorkloadSummary, Status> {
        info!(
            "Agent {} spawning {} concurrent ranks: ranks [{}, {})",
            agent_id, ranks_per_agent, rank_start, rank_start + ranks_per_agent
        );

        let start_time = SystemTime::now();
        
        // Discover files once (shared across all local ranks)
        let file_list = if global_world_size > 1 {
            info!("Discovering files for sharding from: {}", config.dataset.data_folder);
            
            let files = Self::discover_files(&config.dataset.data_folder).await
                .map_err(|e| {
                    error!("Failed to discover files: {}", e);
                    Status::internal(format!("File discovery failed: {}", e))
                })?;
            
            info!("Discovered {} total files before sharding", files.len());
            files
        } else {
            Vec::new()
        };

        // Spawn concurrent tasks for each local rank
        let mut handles = Vec::new();
        
        for local_rank in 0..ranks_per_agent {
            let global_rank = rank_start + local_rank;
            let config_clone = config.clone();
            let agent_id_clone = agent_id.to_string();
            let file_list_clone = file_list.clone();
            let shard_strategy_owned = shard_strategy.to_string();
            let tracker_clone = live_stats_tracker.clone();
            
            info!("Spawning runner for rank {} (local rank {})", global_rank, local_rank);
            
            let handle = tokio::spawn(async move {
                Self::run_single_rank_task(
                    config_clone,
                    &agent_id_clone,
                    tracker_clone,
                    global_rank,
                    global_world_size,
                    &shard_strategy_owned,
                    &file_list_clone,
                )
                .await
            });
            
            handles.push((global_rank, handle));
        }

        // Wait for all ranks to complete and collect summaries
        info!("Waiting for {} rank tasks to complete...", ranks_per_agent);
        let mut rank_summaries = Vec::new();
        
        for (global_rank, handle) in handles {
            match handle.await {
                Ok(Ok(summary)) => {
                    info!("Rank {} completed successfully", global_rank);
                    rank_summaries.push(summary);
                }
                Ok(Err(e)) => {
                    error!("Rank {} failed: {}", global_rank, e);
                    return Err(Status::internal(format!("Rank {} failed: {}", global_rank, e)));
                }
                Err(e) => {
                    error!("Rank {} task panicked: {}", global_rank, e);
                    return Err(Status::internal(format!("Rank {} task panicked: {}", global_rank, e)));
                }
            }
        }

        // Calculate total duration
        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or(Duration::ZERO);
        let duration_s = duration.as_secs_f64();

        info!("All {} ranks completed, aggregating results...", ranks_per_agent);

        // Aggregate results from all local ranks
        Self::aggregate_rank_summaries(agent_id, rank_summaries, duration_s)
            .map_err(|e| {
                error!("Failed to aggregate rank summaries: {}", e);
                Status::internal(format!("Aggregation failed: {}", e))
            })
    }

    /// Run a single rank task (called by execute_multi_rank via tokio::spawn)
    /// 
    /// This function is similar to execute_single_rank but returns a plain Result
    /// without Status wrapper (Status doesn't implement Send easily across threads).
    async fn run_single_rank_task(
        config: DlioConfig,
        agent_id: &str,
        live_stats_tracker: Option<Arc<crate::live_stats::LiveStatsTracker>>,
        global_rank: usize,
        global_world_size: usize,
        shard_strategy: &str,
        file_list: &[String],
    ) -> anyhow::Result<WorkloadSummary> {
        use anyhow::Context;

        let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1) as u64;
        let batch_size = config.reader.batch_size.unwrap_or(1) as u64;

        let start_time = SystemTime::now();
        
        // Create runner (each rank gets its own WorkloadRunner with independent clients)
        let mut runner = WorkloadRunner::new(config.clone());
        
        // Apply sharding for this rank
        info!("Rank {}: global_world_size={}, file_list.len()={}", 
              global_rank, global_world_size, file_list.len());
        
        if global_world_size > 1 && !file_list.is_empty() {
            let sharded_files = Self::apply_sharding_strategy(
                file_list,
                global_world_size,
                global_rank,
                shard_strategy,
            ).context("Failed to apply sharding strategy")?;
            
            info!("Rank {}: sharded to {}/{} files", global_rank, sharded_files.len(), file_list.len());
            
            runner = runner.with_rank_config(
                global_rank as u32,
                global_world_size as u32,
                Some(sharded_files),
            );
        } else {
            info!("Rank {}: NO SHARDING (world_size={}, files={})", 
                  global_rank, global_world_size, file_list.len());
        }
        
        // Wire live stats tracker
        if let Some(tracker) = live_stats_tracker {
            runner = runner.with_live_stats_tracker(tracker);
        }

        // Execute workload
        runner.run().await.context("Workload execution failed")?;

        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or(Duration::ZERO);
        let duration_s = duration.as_secs_f64();

        let metrics = runner.get_metrics();
        
        // Calculate metrics (same as execute_single_rank)
        let files_processed = metrics.files_processed();
        let bytes_read = metrics.bytes_read();
        let bytes_written = metrics.bytes_written();
        
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

        // AI/ML metrics
        let total_samples = files_processed * samples_per_file;
        let samples_per_second = if duration_s > 0.0 {
            total_samples as f64 / duration_s
        } else {
            0.0
        };

        let total_batches = if batch_size > 0 {
            (total_samples + batch_size - 1) / batch_size
        } else {
            0
        };
        
        let batches_per_second = if duration_s > 0.0 {
            total_batches as f64 / duration_s
        } else {
            0.0
        };

        let batch_times = metrics.batch_times();
        let avg_batch_time_ms = if !batch_times.is_empty() {
            let total_batch_time: Duration = batch_times.iter().sum();
            total_batch_time.as_secs_f64() * 1000.0 / batch_times.len() as f64
        } else {
            0.0
        };

        let epoch_times = metrics.epoch_times();
        let epochs_completed = epoch_times.len() as u32;
        let avg_epoch_time_s = if !epoch_times.is_empty() {
            let total_epoch_time: Duration = epoch_times.iter().sum();
            total_epoch_time.as_secs_f64() / epoch_times.len() as f64
        } else {
            0.0
        };

        let data_loading_time_s = metrics.total_read_time().as_secs_f64();
        let compute_time_s = metrics.total_compute_time().as_secs_f64();
        let pipeline_efficiency = if duration_s > 0.0 {
            (data_loading_time_s + compute_time_s) / duration_s
        } else {
            0.0
        };

        // Extract and serialize histograms
        let read_hists = metrics.get_read_histograms();
        let write_hists = metrics.get_write_histograms();
        let batch_hists = metrics.get_batch_histograms();

        let combined_read = read_hists.combined_histogram();
        let (p50, p90, p95, p99) = if combined_read.len() > 0 {
            (
                combined_read.value_at_quantile(0.50) as f64,
                combined_read.value_at_quantile(0.90) as f64,
                combined_read.value_at_quantile(0.95) as f64,
                combined_read.value_at_quantile(0.99) as f64,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let errors = 0u32;

        // Serialize histograms
        use crate::dist::histogram::serialize_histogram;
        
        let mut histogram_read = Vec::new();
        for bucket_hist in read_hists.buckets.iter() {
            let hist = bucket_hist.lock().unwrap();
            serialize_histogram(&*hist)
                .and_then(|bytes| {
                    histogram_read.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Rank {}: Failed to serialize read histogram bucket: {}", global_rank, e);
                });
        }

        let mut histogram_write = Vec::new();
        for bucket_hist in write_hists.buckets.iter() {
            let hist = bucket_hist.lock().unwrap();
            serialize_histogram(&*hist)
                .and_then(|bytes| {
                    histogram_write.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Rank {}: Failed to serialize write histogram bucket: {}", global_rank, e);
                });
        }

        let histogram_batch = if let Some(batch_hist) = batch_hists.get_histogram() {
            serialize_histogram(&batch_hist)
                .unwrap_or_else(|e| {
                    warn!("Rank {}: Failed to serialize batch histogram: {}", global_rank, e);
                    vec![]
                })
        } else {
            vec![]
        };

        // Generate TSV content (per-rank, will be aggregated later)
        use crate::tsv_export::StorageTsvExporter;
        let storage_tsv_content = StorageTsvExporter::export_to_string(
            &read_hists,
            &write_hists,
            bytes_read,
            bytes_written,
            duration_s,
        ).unwrap_or_default();

        info!(
            "Rank {} completed: {:.1} samples/s, {:.1} MiB/s, {} files in {:.1}s",
            global_rank, samples_per_second, mib_per_s, total_ops, duration_s
        );

        Ok(WorkloadSummary {
            agent_id: format!("{}-rank{}", agent_id, global_rank),
            ops_per_s,
            mib_per_s,
            p50_us: p50,
            p90_us: p90,
            p95_us: p95,
            p99_us: p99,
            errors,
            total_ops,
            duration_s,
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
            console_log: String::new(),
            metadata_json: String::new(),
            storage_tsv_content,
            aiml_tsv_content: String::new(),
            results_path: String::new(),
            histogram_read,
            histogram_write,
            histogram_batch,
        })
    }

    /// Aggregate WorkloadSummary results from multiple local ranks
    /// 
    /// Uses HDR histogram merging for accurate percentiles (following sai3-bench pattern)
    /// and sums counters (ops, bytes, samples, etc.).
    fn aggregate_rank_summaries(
        agent_id: &str,
        summaries: Vec<WorkloadSummary>,
        total_duration_s: f64,
    ) -> anyhow::Result<WorkloadSummary> {
        use hdrhistogram::{Histogram, serialization::Deserializer};
        use anyhow::Context;

        if summaries.is_empty() {
            anyhow::bail!("No rank summaries to aggregate");
        }

        let num_ranks = summaries.len();
        info!("Aggregating {} rank summaries for agent {}", num_ranks, agent_id);

        // Sum counters across ranks
        let total_ops: u64 = summaries.iter().map(|s| s.total_ops).sum();
        let total_samples: u64 = summaries.iter().map(|s| s.total_samples).sum();
        let total_batches: u64 = summaries.iter().map(|s| s.total_batches).sum();
        let total_errors: u32 = summaries.iter().map(|s| s.errors).sum();
        
        // Sum throughput rates (each rank's rate contributes to total aggregate throughput)
        let total_ops_per_s: f64 = summaries.iter().map(|s| s.ops_per_s).sum();
        let total_mib_per_s: f64 = summaries.iter().map(|s| s.mib_per_s).sum();
        let total_samples_per_second: f64 = summaries.iter().map(|s| s.samples_per_second).sum();
        let total_batches_per_second: f64 = summaries.iter().map(|s| s.batches_per_second).sum();

        // Average time-based metrics
        let avg_batch_time_ms: f64 = summaries.iter().map(|s| s.avg_batch_time_ms).sum::<f64>() / num_ranks as f64;
        let avg_epoch_time_s: f64 = summaries.iter().map(|s| s.avg_epoch_time_s).sum::<f64>() / num_ranks as f64;
        let data_loading_time_s: f64 = summaries.iter().map(|s| s.data_loading_time_s).sum::<f64>() / num_ranks as f64;
        let compute_time_s: f64 = summaries.iter().map(|s| s.compute_time_s).sum::<f64>() / num_ranks as f64;
        let pipeline_efficiency: f64 = summaries.iter().map(|s| s.pipeline_efficiency).sum::<f64>() / num_ranks as f64;
        
        let epochs_completed: u32 = summaries.iter().map(|s| s.epochs_completed).sum();
        let samples_per_batch = summaries[0].samples_per_batch;  // Should be same for all ranks

        // Merge HDR histograms for accurate percentiles
        const NUM_BUCKETS: usize = 9;
        let mut deserializer = Deserializer::new();

        // Create accumulators for read histograms (9 size buckets)
        let mut read_accumulators: Vec<Histogram<u64>> = Vec::new();
        for _ in 0..NUM_BUCKETS {
            read_accumulators.push(
                Histogram::new(3).context("Failed to create read histogram accumulator")?
            );
        }

        // Deserialize and merge read histograms from all ranks
        for (rank_idx, summary) in summaries.iter().enumerate() {
            if summary.histogram_read.is_empty() {
                continue;
            }

            let mut cursor = &summary.histogram_read[..];
            for bucket_idx in 0..NUM_BUCKETS {
                let hist: Histogram<u64> = deserializer.deserialize(&mut cursor)
                    .with_context(|| format!(
                        "Failed to deserialize READ histogram bucket {} from rank {}",
                        bucket_idx, rank_idx
                    ))?;
                
                read_accumulators[bucket_idx].add(hist)
                    .with_context(|| format!(
                        "Failed to merge READ histogram bucket {} from rank {}",
                        bucket_idx, rank_idx
                    ))?;
            }
        }

        // Combine read buckets and calculate percentiles
        let mut combined_read = read_accumulators[0].clone();
        for bucket_accumulator in read_accumulators.iter().skip(1) {
            combined_read.add(bucket_accumulator)
                .context("Failed to combine read bucket histograms")?;
        }

        let (p50_us, p90_us, p95_us, p99_us) = if combined_read.len() > 0 {
            (
                combined_read.value_at_quantile(0.50) as f64,
                combined_read.value_at_quantile(0.90) as f64,
                combined_read.value_at_quantile(0.95) as f64,
                combined_read.value_at_quantile(0.99) as f64,
            )
        } else {
            // Fallback: average percentiles (statistically incorrect but better than nothing)
            (
                summaries.iter().map(|s| s.p50_us).sum::<f64>() / num_ranks as f64,
                summaries.iter().map(|s| s.p90_us).sum::<f64>() / num_ranks as f64,
                summaries.iter().map(|s| s.p95_us).sum::<f64>() / num_ranks as f64,
                summaries.iter().map(|s| s.p99_us).sum::<f64>() / num_ranks as f64,
            )
        };

        // Re-serialize merged histograms for controller aggregation
        use crate::dist::histogram::serialize_histogram;
        let mut histogram_read = Vec::new();
        for bucket_accumulator in read_accumulators.iter() {
            serialize_histogram(bucket_accumulator)
                .and_then(|bytes| {
                    histogram_read.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Failed to serialize merged read histogram bucket: {}", e);
                });
        }

        // Write histograms (same pattern)
        let mut write_accumulators: Vec<Histogram<u64>> = Vec::new();
        for _ in 0..NUM_BUCKETS {
            write_accumulators.push(
                Histogram::new(3).context("Failed to create write histogram accumulator")?
            );
        }

        for (rank_idx, summary) in summaries.iter().enumerate() {
            if summary.histogram_write.is_empty() {
                continue;
            }

            let mut cursor = &summary.histogram_write[..];
            for bucket_idx in 0..NUM_BUCKETS {
                let hist: Histogram<u64> = deserializer.deserialize(&mut cursor)
                    .with_context(|| format!(
                        "Failed to deserialize WRITE histogram bucket {} from rank {}",
                        bucket_idx, rank_idx
                    ))?;
                
                write_accumulators[bucket_idx].add(hist)
                    .with_context(|| format!(
                        "Failed to merge WRITE histogram bucket {} from rank {}",
                        bucket_idx, rank_idx
                    ))?;
            }
        }

        let mut histogram_write = Vec::new();
        for bucket_accumulator in write_accumulators.iter() {
            serialize_histogram(bucket_accumulator)
                .and_then(|bytes| {
                    histogram_write.extend_from_slice(&bytes);
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    warn!("Failed to serialize merged write histogram bucket: {}", e);
                });
        }

        // Batch histogram (single histogram)
        let mut batch_accumulator = Histogram::new(3)
            .context("Failed to create batch histogram accumulator")?;
        
        for (rank_idx, summary) in summaries.iter().enumerate() {
            if summary.histogram_batch.is_empty() {
                continue;
            }

            let mut cursor = &summary.histogram_batch[..];
            let hist: Histogram<u64> = deserializer.deserialize(&mut cursor)
                .with_context(|| format!(
                    "Failed to deserialize BATCH histogram from rank {}",
                    rank_idx
                ))?;
            
            batch_accumulator.add(hist)
                .with_context(|| format!(
                    "Failed to merge BATCH histogram from rank {}",
                    rank_idx
                ))?;
        }

        let histogram_batch = serialize_histogram(&batch_accumulator)
            .unwrap_or_else(|e| {
                warn!("Failed to serialize merged batch histogram: {}", e);
                vec![]
            });

        // Aggregate TSV content (concatenate per-rank TSV data)
        let storage_tsv_content = summaries
            .iter()
            .map(|s| s.storage_tsv_content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        info!(
            "Agent {} aggregated {} ranks: {:.1} samples/s, {:.1} MiB/s, {} total ops",
            agent_id, num_ranks, total_samples_per_second, total_mib_per_s, total_ops
        );

        Ok(WorkloadSummary {
            agent_id: agent_id.to_string(),
            ops_per_s: total_ops_per_s,
            mib_per_s: total_mib_per_s,
            p50_us,
            p90_us,
            p95_us,
            p99_us,
            errors: total_errors,
            total_ops,
            duration_s: total_duration_s,
            samples_per_second: total_samples_per_second,
            total_samples,
            samples_per_batch,
            batches_per_second: total_batches_per_second,
            total_batches,
            avg_batch_time_ms,
            epochs_completed,
            avg_epoch_time_s,
            data_loading_time_s,
            compute_time_s,
            pipeline_efficiency,
            console_log: String::new(),
            metadata_json: String::new(),
            storage_tsv_content,
            aiml_tsv_content: String::new(),
            results_path: String::new(),
            histogram_read,
            histogram_write,
            histogram_batch,
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

        // Extract rank information (v0.8.8)
        // Phase 1: global_rank field contains agent's first rank (rank_start)
        // Phase 2: ranks_per_agent specifies how many concurrent ranks to spawn
        let rank_start = req.global_rank as usize;
        let ranks_per_agent = req.ranks_per_agent as usize;
        let global_world_size = req.global_world_size as usize;
        let shard_strategy = req.shard_strategy.as_str();

        // Execute the workload and return metrics (no live stats for blocking RPC)
        let summary = self.execute_workload(
            config,
            &req.agent_id,
            None,
            rank_start,
            ranks_per_agent,
            global_world_size,
            shard_strategy,
        ).await?;

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

    /// v0.8.7: Server streaming RPC for live progress updates during distributed execution
    type RunWorkloadWithLiveStatsStream = 
        std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<LiveStats, Status>> + Send>>;

    async fn run_workload_with_live_stats(
        &self,
        request: Request<RunWorkloadRequest>,
    ) -> Result<Response<Self::RunWorkloadWithLiveStatsStream>, Status> {
        info!("Received run_workload_with_live_stats request (streaming mode)");
        
        let req = request.into_inner();
        
        // Parse and apply config BEFORE stream (v0.8.7)
        let mut config = DlioConfig::from_yaml(&req.config_yaml).map_err(|e| {
            error!("Failed to parse DLIO config: {}", e);
            Status::invalid_argument(format!("Invalid DLIO config: {}", e))
        })?;

        if !req.path_prefix.is_empty() {
            config
                .apply_agent_prefix(&req.agent_id, &req.path_prefix)
                .map_err(|e| {
                    error!("Failed to apply agent prefix: {}", e);
                    Status::internal(format!("Failed to apply path prefix: {}", e))
                })?;
        }

        // Calculate wait duration for coordinated start (v0.8.7)
        let wait_duration = if req.start_unix_ms > 0 {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let wait_ms = req.start_unix_ms - now_ms;
            if wait_ms > 0 {
                Some(Duration::from_millis(wait_ms as u64))
            } else {
                None
            }
        } else {
            None
        };

        // Clone data for stream (v0.8.7)
        let agent_id_stream = req.agent_id.clone();
        let config_stream = config.clone();
        let self_stream = self.clone();
        
        // Clone rank information for stream (v0.8.8)
        // Phase 2: global_rank field = rank_start, ranks_per_agent specifies concurrent ranks
        let rank_start_stream = req.global_rank as usize;
        let ranks_per_agent_stream = req.ranks_per_agent as usize;
        let global_world_size_stream = req.global_world_size as usize;
        let shard_strategy_stream = req.shard_strategy.clone();

        // Create stream with startup handshake (v0.8.7)
        let stream = async_stream::stream! {
            // Step 1: Validate configuration and send READY or ERROR
            match validate_workload_config(&config_stream).await {
                Ok(_) => {
                    info!("Configuration validated successfully for agent {}", agent_id_stream);
                    // Send READY status immediately
                    let ready_msg = LiveStats {
                        agent_id: agent_id_stream.clone(),
                        timestamp_s: 0.0,
                        get_ops: 0,
                        get_bytes: 0,
                        get_mean_us: 0.0,
                        get_p50_us: 0.0,
                        get_p90_us: 0.0,
                        get_p95_us: 0.0,
                        get_p99_us: 0.0,
                        put_ops: 0,
                        put_bytes: 0,
                        put_mean_us: 0.0,
                        put_p50_us: 0.0,
                        put_p90_us: 0.0,
                        put_p95_us: 0.0,
                        put_p99_us: 0.0,
                        samples_per_second: 0.0,
                        total_samples: 0,
                        elapsed_s: 0.0,
                        completed: false,
                        final_summary: None,
                        status: LiveStatsStatus::Ready as i32,
                        error_message: String::new(),
                    };
                    yield Ok(ready_msg);
                }
                Err(e) => {
                    let error_msg = format!("Configuration validation failed: {}", e);
                    error!("{}", error_msg);
                    // Send ERROR status and exit
                    let error_stats = LiveStats {
                        agent_id: agent_id_stream.clone(),
                        timestamp_s: 0.0,
                        get_ops: 0,
                        get_bytes: 0,
                        get_mean_us: 0.0,
                        get_p50_us: 0.0,
                        get_p90_us: 0.0,
                        get_p95_us: 0.0,
                        get_p99_us: 0.0,
                        put_ops: 0,
                        put_bytes: 0,
                        put_mean_us: 0.0,
                        put_p50_us: 0.0,
                        put_p90_us: 0.0,
                        put_p95_us: 0.0,
                        put_p99_us: 0.0,
                        samples_per_second: 0.0,
                        total_samples: 0,
                        elapsed_s: 0.0,
                        completed: false,
                        final_summary: None,
                        status: LiveStatsStatus::Error as i32,
                        error_message: error_msg,
                    };
                    yield Ok(error_stats);
                    return; // Exit stream on validation failure
                }
            }

            // Step 2: Wait for coordinated start time (INSIDE stream, after READY sent)
            if let Some(wait_dur) = wait_duration {
                info!("Agent {} waiting {:?} for coordinated start", agent_id_stream, wait_dur);
                tokio::time::sleep(wait_dur).await;
            }

            info!("Starting workload execution with live stats for agent {}", agent_id_stream);

            // Step 3: Create live stats tracker and spawn workload (INSIDE stream)
            let tracker = Arc::new(crate::live_stats::LiveStatsTracker::new());
            let (tx_done, mut rx_done) = tokio::sync::mpsc::channel::<Result<WorkloadSummary, String>>(1);
            
            let tracker_exec = tracker.clone();
            let config_exec = config_stream.clone();
            let agent_id_exec = agent_id_stream.clone();
            let self_exec = self_stream.clone();
            
            // Clone rank info for spawned task (v0.8.8)
            let rank_start_exec = rank_start_stream;
            let ranks_per_agent_exec = ranks_per_agent_stream;
            let global_world_size_exec = global_world_size_stream;
            let shard_strategy_exec = shard_strategy_stream.clone();
            
            tokio::spawn(async move {
                match self_exec.execute_workload(
                    config_exec,
                    &agent_id_exec,
                    Some(tracker_exec),
                    rank_start_exec,
                    ranks_per_agent_exec,
                    global_world_size_exec,
                    &shard_strategy_exec,
                ).await {
                    Ok(summary) => {
                        info!("Workload completed successfully for agent {}", agent_id_exec);
                        let _ = tx_done.send(Ok(summary)).await;
                    }
                    Err(e) => {
                        error!("Workload execution failed for agent {}: {:?}", agent_id_exec, e);
                        let _ = tx_done.send(Err(format!("Workload execution failed: {:?}", e))).await;
                    }
                }
            });

            // Step 4: Stream live stats every 1 second
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Send live stats snapshot
                        let snapshot = tracker.snapshot();
                        let stats = LiveStats {
                            agent_id: agent_id_stream.clone(),
                            timestamp_s: snapshot.timestamp_secs() as f64,
                            get_ops: snapshot.get_ops,
                            get_bytes: snapshot.get_bytes,
                            get_mean_us: snapshot.get_mean_us as f64,
                            get_p50_us: snapshot.get_p50_us as f64,
                            get_p90_us: snapshot.get_p90_us as f64,
                            get_p95_us: snapshot.get_p95_us as f64,
                            get_p99_us: snapshot.get_p99_us as f64,
                            put_ops: snapshot.put_ops,
                            put_bytes: snapshot.put_bytes,
                            put_mean_us: snapshot.put_mean_us as f64,
                            put_p50_us: snapshot.put_p50_us as f64,
                            put_p90_us: snapshot.put_p90_us as f64,
                            put_p95_us: snapshot.put_p95_us as f64,
                            put_p99_us: snapshot.put_p99_us as f64,
                            samples_per_second: snapshot.samples_per_second(),
                            total_samples: snapshot.total_samples,
                            elapsed_s: snapshot.elapsed_secs(),
                            completed: false,
                            final_summary: None,  // v0.8.7: Only in final message
                            status: LiveStatsStatus::Running as i32,  // v0.8.7: Agent is executing
                            error_message: String::new(),  // v0.8.7: No error
                        };
                        yield Ok(stats);
                    }
                    
                    result = rx_done.recv() => {
                        // Workload completed (or failed)
                        match result {
                            Some(Ok(summary)) => {
                                // v0.8.7: Send final stats with completed=true and complete summary
                                let snapshot = tracker.snapshot();
                                let final_stats = LiveStats {
                                    agent_id: agent_id_stream.clone(),
                                    timestamp_s: snapshot.timestamp_secs() as f64,
                                    get_ops: snapshot.get_ops,
                                    get_bytes: snapshot.get_bytes,
                                    get_mean_us: snapshot.get_mean_us as f64,
                                    get_p50_us: snapshot.get_p50_us as f64,
                                    get_p90_us: snapshot.get_p90_us as f64,
                                    get_p95_us: snapshot.get_p95_us as f64,
                                    get_p99_us: snapshot.get_p99_us as f64,
                                    put_ops: snapshot.put_ops,
                                    put_bytes: snapshot.put_bytes,
                                    put_mean_us: snapshot.put_mean_us as f64,
                                    put_p50_us: snapshot.put_p50_us as f64,
                                    put_p90_us: snapshot.put_p90_us as f64,
                                    put_p95_us: snapshot.put_p95_us as f64,
                                    put_p99_us: snapshot.put_p99_us as f64,
                                    samples_per_second: snapshot.samples_per_second(),
                                    total_samples: snapshot.total_samples,
                                    elapsed_s: snapshot.elapsed_secs(),
                                    completed: true,
                                    final_summary: Some(summary),  // v0.8.7: Include complete results
                                    status: LiveStatsStatus::Completed as i32,  // v0.8.7: Agent finished
                                    error_message: String::new(),  // v0.8.7: No error
                                };
                                yield Ok(final_stats);
                                break;
                            }
                            Some(Err(e)) => {
                                yield Err(Status::internal(e));
                                break;
                            }
                            None => {
                                yield Err(Status::internal("Workload task terminated unexpectedly"));
                                break;
                            }
                        }
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }
}

impl AgentService {
    /// Discover files from data_folder using s3dlio (v0.8.8 Priority 0, Phase 1)
    async fn discover_files(data_folder: &str) -> anyhow::Result<Vec<String>> {
        use s3dlio::object_store::store_for_uri;
        
        let store = store_for_uri(data_folder)
            .context(format!("Failed to create store for {}", data_folder))?;
        
        // List all objects in the data_folder (recursive)
        let files = store.list(data_folder, true).await
            .context(format!("Failed to list files in {}", data_folder))?;
        
        Ok(files)
    }
    
    /// Apply sharding strategy to distribute files across ranks (v0.8.8 Priority 0, Phase 1)
    /// 
    /// Strategies:
    /// - "interleaved": Round-robin (rank 0 gets files 0,N,2N,...)
    /// - "contiguous": Equal chunks (rank 0 gets files 0..N/world_size)
    /// - "hash": Hash-based pseudo-random distribution
    fn apply_sharding_strategy(
        files: &[String],
        world_size: usize,
        rank: usize,
        strategy: &str,
    ) -> anyhow::Result<Vec<String>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let total_files = files.len();
        if total_files == 0 {
            return Ok(Vec::new());
        }

        let sharded = match strategy {
            "interleaved" => {
                // Round-robin distribution: rank 0 gets files 0,N,2N,..., rank 1 gets files 1,N+1,2N+1,...
                files
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % world_size == rank)
                    .map(|(_, f)| f.clone())
                    .collect()
            }
            "contiguous" => {
                // Contiguous blocks: divide files into equal chunks
                let chunk_size = total_files / world_size;
                let remainder = total_files % world_size;
                
                let start = rank * chunk_size + std::cmp::min(rank, remainder);
                let end = start + chunk_size + if rank < remainder { 1 } else { 0 };
                
                files[start..end].to_vec()
            }
            "hash" => {
                // Hash-based distribution: consistent but pseudo-random
                files
                    .iter()
                    .filter(|f| {
                        let mut hasher = DefaultHasher::new();
                        f.hash(&mut hasher);
                        (hasher.finish() % world_size as u64) as usize == rank
                    })
                    .cloned()
                    .collect()
            }
            _ => {
                anyhow::bail!(
                    "Unknown sharding strategy: '{}'. Valid options: interleaved, contiguous, hash",
                    strategy
                );
            }
        };

        info!(
            "Sharding strategy '{}': rank {} gets {}/{} files",
            strategy, rank, sharded.len(), total_files
        );

        Ok(sharded)
    }
}

/// Validate DLIO workload configuration after path prefixing (v0.8.7)
/// 
/// Performs pre-flight validation to catch configuration errors before workload starts.
/// This validation happens AFTER agent path prefix is applied, so file:// paths are
/// checked with the correct agent-specific prefix.
async fn validate_workload_config(config: &DlioConfig) -> Result<()> {
    // Check that data_folder is not empty
    if config.dataset.data_folder.is_empty() {
        anyhow::bail!("dataset.data_folder is required");
    }
    
    // For file:// URIs, verify that files/directories exist (if NOT generating data)
    let should_generate = config.workflow
        .as_ref()
        .and_then(|w| w.generate_data)
        .unwrap_or(false);
    
    if !should_generate && config.dataset.data_folder.starts_with("file://") {
        let file_path = config.dataset.data_folder.replace("file://", "");
        
        // Check if path contains glob patterns
        if file_path.contains('*') || file_path.contains('?') {
            // Validate glob pattern
            let paths: Vec<_> = glob::glob(&file_path)
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern in data_folder '{}': {}", file_path, e))?
                .collect();
            
            if paths.is_empty() {
                anyhow::bail!("No files/directories found matching pattern: {}", config.dataset.data_folder);
            }
        } else {
            // Check if directory exists
            if !tokio::fs::try_exists(&file_path).await.unwrap_or(false) {
                anyhow::bail!("data_folder does not exist: {}", config.dataset.data_folder);
            }
        }
    }
    
    // Validate that batch_size is configured for training phase
    if let Some(workflow) = &config.workflow {
        if workflow.train.unwrap_or(false) {
            if config.reader.batch_size.is_none() {
                anyhow::bail!("batch_size must be configured in reader section for training phase");
            }
        }
    }
    
    // Validate checkpoint configuration if checkpointing is enabled
    if let Some(workflow) = &config.workflow {
        if workflow.checkpoint.unwrap_or(false) {
            if config.checkpointing.is_none() {
                anyhow::bail!("checkpointing configuration required when workflow.checkpoint=true");
            }
        }
    }
    
    Ok(())
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
