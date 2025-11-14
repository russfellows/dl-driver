/// Rust wrapper types for distributed execution
/// 
/// Provides ergonomic Rust types that wrap the protobuf-generated structs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::proto;

/// Workload execution request with validated parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadRequest {
    pub config_yaml: String,
    pub agent_id: String,
    pub path_prefix: String,
    pub start_unix_ms: i64,
    // v0.8.8: Distributed rank information (Priority 0, Phase 1)
    pub global_rank: u32,
    pub global_world_size: u32,
    pub shard_strategy: String,
    // v0.8.8: Multi-rank per agent (Priority 0, Phase 2)
    pub ranks_per_agent: u32,
}

impl From<WorkloadRequest> for proto::RunWorkloadRequest {
    fn from(req: WorkloadRequest) -> Self {
        proto::RunWorkloadRequest {
            config_yaml: req.config_yaml,
            agent_id: req.agent_id,
            path_prefix: req.path_prefix,
            start_unix_ms: req.start_unix_ms,
            // v0.8.1 enhancement - per-agent config overrides (currently unused)
            agent_config: None,
            // v0.8.1 enhancement - shared storage flag (currently false)
            shared_storage: false,
            // v0.8.8: Distributed rank information
            global_rank: req.global_rank,
            global_world_size: req.global_world_size,
            shard_strategy: req.shard_strategy,
            // v0.8.8: Multi-rank per agent (Phase 2)
            ranks_per_agent: req.ranks_per_agent,
        }
    }
}

impl From<proto::RunWorkloadRequest> for WorkloadRequest {
    fn from(req: proto::RunWorkloadRequest) -> Self {
        WorkloadRequest {
            config_yaml: req.config_yaml,
            agent_id: req.agent_id,
            path_prefix: req.path_prefix,
            start_unix_ms: req.start_unix_ms,
            // v0.8.8: Distributed rank information
            global_rank: req.global_rank,
            global_world_size: req.global_world_size,
            shard_strategy: req.shard_strategy,
            // v0.8.8: Multi-rank per agent (Phase 2)
            ranks_per_agent: req.ranks_per_agent,
        }
    }
}

/// Workload execution results with performance metrics
/// 
/// Provides both storage-focused metrics (ops/s, MiB/s) and AI/ML training metrics
/// (samples/s, batches/s) for comprehensive performance analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadResult {
    pub agent_id: String,
    
    // Storage performance metrics
    pub ops_per_s: f64,
    pub mib_per_s: f64,
    pub p50_us: f64,  // v0.8.7: changed from ms to µs for consistency
    pub p90_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub errors: u32,
    pub total_ops: u64,
    pub duration_s: f64,
    
    // AI/ML training metrics
    pub samples_per_second: f64,
    pub total_samples: u64,
    pub samples_per_batch: u64,
    pub batches_per_second: f64,
    pub total_batches: u64,
    pub avg_batch_time_ms: f64,
    pub epochs_completed: u32,
    pub avg_epoch_time_s: f64,
    pub data_loading_time_s: f64,
    pub compute_time_s: f64,
    pub pipeline_efficiency: f64,
    pub accelerator_utilization: f64,  // v0.8.8: AU = compute / total (DLIO metric)
}

impl From<proto::WorkloadSummary> for WorkloadResult {
    fn from(summary: proto::WorkloadSummary) -> Self {
        WorkloadResult {
            agent_id: summary.agent_id,
            // Storage metrics
            ops_per_s: summary.ops_per_s,
            mib_per_s: summary.mib_per_s,
            p50_us: summary.p50_us,
            p90_us: summary.p90_us,
            p95_us: summary.p95_us,
            p99_us: summary.p99_us,
            errors: summary.errors,
            total_ops: summary.total_ops,
            duration_s: summary.duration_s,
            // AI/ML metrics
            samples_per_second: summary.samples_per_second,
            total_samples: summary.total_samples,
            samples_per_batch: summary.samples_per_batch,
            batches_per_second: summary.batches_per_second,
            total_batches: summary.total_batches,
            avg_batch_time_ms: summary.avg_batch_time_ms,
            epochs_completed: summary.epochs_completed,
            avg_epoch_time_s: summary.avg_epoch_time_s,
            data_loading_time_s: summary.data_loading_time_s,
            compute_time_s: summary.compute_time_s,
            pipeline_efficiency: summary.pipeline_efficiency,
            accelerator_utilization: summary.accelerator_utilization,
        }
    }
}

impl From<WorkloadResult> for proto::WorkloadSummary {
    fn from(result: WorkloadResult) -> Self {
        proto::WorkloadSummary {
            agent_id: result.agent_id,
            // Storage metrics
            ops_per_s: result.ops_per_s,
            mib_per_s: result.mib_per_s,
            p50_us: result.p50_us,
            p90_us: result.p90_us,
            p95_us: result.p95_us,
            p99_us: result.p99_us,
            errors: result.errors,
            total_ops: result.total_ops,
            duration_s: result.duration_s,
            // AI/ML metrics
            samples_per_second: result.samples_per_second,
            total_samples: result.total_samples,
            samples_per_batch: result.samples_per_batch,
            batches_per_second: result.batches_per_second,
            total_batches: result.total_batches,
            avg_batch_time_ms: result.avg_batch_time_ms,
            epochs_completed: result.epochs_completed,
            avg_epoch_time_s: result.avg_epoch_time_s,
            data_loading_time_s: result.data_loading_time_s,
            compute_time_s: result.compute_time_s,
            pipeline_efficiency: result.pipeline_efficiency,
            accelerator_utilization: result.accelerator_utilization,
            // Inline results (v0.8.1 enhancement - currently unused)
            console_log: String::new(),
            metadata_json: String::new(),
            storage_tsv_content: String::new(),
            aiml_tsv_content: String::new(),
            results_path: String::new(),
            // HDR histogram data (v0.8.1 enhancement - currently empty)
            histogram_read: vec![],
            histogram_write: vec![],
            histogram_batch: vec![],
        }
    }
}

/// Aggregate results from multiple agents
/// 
/// Combines both storage metrics and AI/ML training metrics across all agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateResults {
    // Storage aggregate metrics
    pub total_ops_per_s: f64,
    pub total_mib_per_s: f64,
    pub avg_get_mean_us: f64,  // Average GET latency in µs
    pub avg_put_mean_us: f64,  // Average PUT latency in µs
    pub avg_p50_us: f64,  // v0.8.7: changed from ms to µs for consistency
    pub avg_p90_us: f64,
    pub avg_p95_us: f64,
    pub avg_p99_us: f64,
    pub total_errors: u32,
    pub total_ops: u64,
    
    // AI/ML aggregate metrics
    pub total_samples_per_second: f64,
    pub total_samples: u64,
    pub total_batches_per_second: f64,
    pub total_batches: u64,
    pub avg_batch_time_ms: f64,
    pub total_epochs_completed: u32,
    pub avg_epoch_time_s: f64,
    pub avg_data_loading_time_s: f64,
    pub avg_compute_time_s: f64,
    pub avg_pipeline_efficiency: f64,
    pub avg_accelerator_utilization: f64,  // v0.8.8: AU = compute / total (DLIO metric)
    
    pub agent_results: Vec<WorkloadResult>,
}

impl AggregateResults {
    /// Compute aggregate statistics from multiple agent results
    /// 
    /// NOTE: This method uses simple averaging for percentiles, which is mathematically
    /// incorrect for unbalanced workloads. Use `from_results_with_histograms()` for
    /// accurate percentile aggregation when histogram data is available.
    pub fn from_results(results: Vec<WorkloadResult>) -> Result<Self> {
        if results.is_empty() {
            anyhow::bail!("Cannot aggregate empty results");
        }

        let count = results.len() as f64;
        
        // Storage metric aggregation
        let total_ops_per_s: f64 = results.iter().map(|r| r.ops_per_s).sum();
        let total_mib_per_s: f64 = results.iter().map(|r| r.mib_per_s).sum();
        let total_errors: u32 = results.iter().map(|r| r.errors).sum();
        let total_ops: u64 = results.iter().map(|r| r.total_ops).sum();

        // WARNING: Averaging percentiles is statistically incorrect for unbalanced workloads
        // This can cause significant errors (30%+) when agents have different operation counts
        let avg_p50_us = results.iter().map(|r| r.p50_us).sum::<f64>() / count;
        let avg_p90_us = results.iter().map(|r| r.p90_us).sum::<f64>() / count;
        let avg_p95_us = results.iter().map(|r| r.p95_us).sum::<f64>() / count;
        let avg_p99_us = results.iter().map(|r| r.p99_us).sum::<f64>() / count;

        // AI/ML metric aggregation
        let total_samples_per_second: f64 = results.iter().map(|r| r.samples_per_second).sum();
        let total_samples: u64 = results.iter().map(|r| r.total_samples).sum();
        let total_batches_per_second: f64 = results.iter().map(|r| r.batches_per_second).sum();
        let total_batches: u64 = results.iter().map(|r| r.total_batches).sum();
        let total_epochs_completed: u32 = results.iter().map(|r| r.epochs_completed).sum();
        
        let avg_batch_time_ms = results.iter().map(|r| r.avg_batch_time_ms).sum::<f64>() / count;
        let avg_epoch_time_s = results.iter().map(|r| r.avg_epoch_time_s).sum::<f64>() / count;
        let avg_data_loading_time_s = results.iter().map(|r| r.data_loading_time_s).sum::<f64>() / count;
        let avg_compute_time_s = results.iter().map(|r| r.compute_time_s).sum::<f64>() / count;
        let avg_pipeline_efficiency = results.iter().map(|r| r.pipeline_efficiency).sum::<f64>() / count;
        let avg_accelerator_utilization = results.iter().map(|r| r.accelerator_utilization).sum::<f64>() / count;

        Ok(AggregateResults {
            // Storage metrics
            total_ops_per_s,
            total_mib_per_s,
            avg_get_mean_us: 0.0,  // Not available from WorkloadResult (uses LiveStats)
            avg_put_mean_us: 0.0,  // Not available from WorkloadResult (uses LiveStats)
            avg_p50_us,
            avg_p90_us,
            avg_p95_us,
            avg_p99_us,
            total_errors,
            total_ops,
            // AI/ML metrics
            total_samples_per_second,
            total_samples,
            total_batches_per_second,
            total_batches,
            avg_batch_time_ms,
            total_epochs_completed,
            avg_epoch_time_s,
            avg_data_loading_time_s,
            avg_compute_time_s,
            avg_pipeline_efficiency,
            avg_accelerator_utilization,
            agent_results: results,
        })
    }

    /// Compute aggregate statistics using HDR histogram merging for accurate percentiles
    /// 
    /// This method correctly handles unbalanced workloads by merging histograms before
    /// calculating percentiles, avoiding the statistical errors of naive averaging.
    /// 
    /// Following sai3-bench pattern: Deserialize all 9 size-bucketed histograms from each
    /// agent, merge them using accumulator.add(), then calculate percentiles from merged data.
    /// 
    /// # Arguments
    /// * `results` - Vector of agent results
    /// * `summaries` - Vector of proto summaries containing histogram data (9 buckets each)
    /// 
    /// # Returns
    /// * `AggregateResults` with correctly computed percentiles from merged histograms
    pub fn from_results_with_histograms(
        results: Vec<WorkloadResult>,
        summaries: &[proto::WorkloadSummary],
    ) -> Result<Self> {
        use hdrhistogram::{Histogram, serialization::Deserializer};
        
        if results.is_empty() {
            anyhow::bail!("Cannot aggregate empty results");
        }

        const NUM_BUCKETS: usize = 9;
        let count = results.len() as f64;
        
        // Storage metric aggregation (sums and counts)
        let total_ops_per_s: f64 = results.iter().map(|r| r.ops_per_s).sum();
        let total_mib_per_s: f64 = results.iter().map(|r| r.mib_per_s).sum();
        let total_errors: u32 = results.iter().map(|r| r.errors).sum();
        let total_ops: u64 = results.iter().map(|r| r.total_ops).sum();

        // Percentile aggregation using HDR histogram merging (sai3-bench pattern)
        let (avg_p50_us, avg_p90_us, avg_p95_us, avg_p99_us) = if summaries.is_empty() {
            // Fallback to naive averaging if no histogram data available
            (
                results.iter().map(|r| r.p50_us).sum::<f64>() / count,
                results.iter().map(|r| r.p90_us).sum::<f64>() / count,
                results.iter().map(|r| r.p95_us).sum::<f64>() / count,
                results.iter().map(|r| r.p99_us).sum::<f64>() / count,
            )
        } else {
            // Create accumulators for 9 size buckets (read operations)
            let mut read_accumulators: Vec<Histogram<u64>> = Vec::new();
            for _ in 0..NUM_BUCKETS {
                read_accumulators.push(
                    Histogram::new(3).context("Failed to create read histogram accumulator")?
                );
            }
            
            // Deserialize and merge read histograms from all agents
            let mut deserializer = Deserializer::new();
            let mut any_read_data = false;
            
            for (agent_idx, summary) in summaries.iter().enumerate() {
                if summary.histogram_read.is_empty() {
                    continue;
                }
                
                any_read_data = true;
                
                // Deserialize all 9 bucket histograms for read operations
                let mut cursor = &summary.histogram_read[..];
                for bucket_idx in 0..NUM_BUCKETS {
                    let hist: Histogram<u64> = deserializer.deserialize(&mut cursor)
                        .with_context(|| format!(
                            "Failed to deserialize READ histogram bucket {} from agent {}",
                            bucket_idx, agent_idx
                        ))?;
                    
                    read_accumulators[bucket_idx].add(hist)
                        .with_context(|| format!(
                            "Failed to merge READ histogram bucket {} from agent {}",
                            bucket_idx, agent_idx
                        ))?;
                }
            }
            
            if any_read_data {
                // Calculate combined percentiles across all buckets
                // Start with first bucket's histogram
                let mut combined = read_accumulators[0].clone();
                for bucket_accumulator in read_accumulators.iter().skip(1) {
                    combined.add(bucket_accumulator)
                        .context("Failed to combine read bucket histograms")?;
                }
                
                // Extract percentiles (values are in microseconds)
                let p50_us = combined.value_at_quantile(0.50) as f64;
                let p90_us = combined.value_at_quantile(0.90) as f64;
                let p95_us = combined.value_at_quantile(0.95) as f64;
                let p99_us = combined.value_at_quantile(0.99) as f64;
                
                // v0.8.7: Keep values in microseconds (no conversion needed)
                (p50_us, p90_us, p95_us, p99_us)
            } else {
                // Fallback if no histogram data available
                (
                    results.iter().map(|r| r.p50_us).sum::<f64>() / count,
                    results.iter().map(|r| r.p90_us).sum::<f64>() / count,
                    results.iter().map(|r| r.p95_us).sum::<f64>() / count,
                    results.iter().map(|r| r.p99_us).sum::<f64>() / count,
                )
            }
        };

        // AI/ML metric aggregation
        let total_samples_per_second: f64 = results.iter().map(|r| r.samples_per_second).sum();
        let total_samples: u64 = results.iter().map(|r| r.total_samples).sum();
        let total_batches_per_second: f64 = results.iter().map(|r| r.batches_per_second).sum();
        let total_batches: u64 = results.iter().map(|r| r.total_batches).sum();
        let total_epochs_completed: u32 = results.iter().map(|r| r.epochs_completed).sum();
        
        let avg_batch_time_ms = results.iter().map(|r| r.avg_batch_time_ms).sum::<f64>() / count;
        let avg_epoch_time_s = results.iter().map(|r| r.avg_epoch_time_s).sum::<f64>() / count;
        let avg_data_loading_time_s = results.iter().map(|r| r.data_loading_time_s).sum::<f64>() / count;
        let avg_compute_time_s = results.iter().map(|r| r.compute_time_s).sum::<f64>() / count;
        let avg_pipeline_efficiency = results.iter().map(|r| r.pipeline_efficiency).sum::<f64>() / count;
        let avg_accelerator_utilization = results.iter().map(|r| r.accelerator_utilization).sum::<f64>() / count;

        Ok(AggregateResults {
            // Storage metrics with correctly merged percentiles
            total_ops_per_s,
            total_mib_per_s,
            avg_get_mean_us: 0.0,  // Not available from WorkloadResult (uses LiveStats)
            avg_put_mean_us: 0.0,  // Not available from WorkloadResult (uses LiveStats)
            avg_p50_us,
            avg_p90_us,
            avg_p95_us,
            avg_p99_us,
            total_errors,
            total_ops,
            // AI/ML metrics
            total_samples_per_second,
            total_samples,
            total_batches_per_second,
            total_batches,
            avg_batch_time_ms,
            total_epochs_completed,
            avg_epoch_time_s,
            avg_data_loading_time_s,
            avg_compute_time_s,
            avg_pipeline_efficiency,
            avg_accelerator_utilization,
            agent_results: results,
        })
    }

    /// Format storage results as TSV with per-agent and aggregate rows
    /// 
    /// Returns storage performance metrics (ops/s, MiB/s, latency)
    pub fn to_storage_tsv(&self) -> String {
        let mut output = String::new();
        output.push_str("agent_id\tops_s\tmib_s\tp50_us\tp90_us\tp95_us\tp99_us\terrors\ttotal_ops\tduration_s\n");

        for result in &self.agent_results {
            output.push_str(&format!(
                "{}\t{:.1}\t{:.1}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{}\t{}\t{:.2}\n",
                result.agent_id,
                result.ops_per_s,
                result.mib_per_s,
                result.p50_us,
                result.p90_us,
                result.p95_us,
                result.p99_us,
                result.errors,
                result.total_ops,
                result.duration_s
            ));
        }

        output.push_str(&format!(
            "AGGREGATE\t{:.1}\t{:.1}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{}\t{}\t-\n",
            self.total_ops_per_s,
            self.total_mib_per_s,
            self.avg_p50_us,
            self.avg_p90_us,
            self.avg_p95_us,
            self.avg_p99_us,
            self.total_errors,
            self.total_ops
        ));

        output
    }

    /// Format AI/ML training results as TSV with per-agent and aggregate rows
    /// 
    /// Returns AI/ML training metrics (samples/s, batches/s, epochs)
    pub fn to_aiml_tsv(&self) -> String {
        let mut output = String::new();
        output.push_str("agent_id\tsamples_s\ttotal_samples\tbatches_s\ttotal_batches\tsamples_per_batch\tavg_batch_ms\tepochs\tavg_epoch_s\tdata_load_s\tcompute_s\tpipeline_eff\n");

        for result in &self.agent_results {
            output.push_str(&format!(
                "{}\t{:.1}\t{}\t{:.1}\t{}\t{}\t{:.2}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\n",
                result.agent_id,
                result.samples_per_second,
                result.total_samples,
                result.batches_per_second,
                result.total_batches,
                result.samples_per_batch,
                result.avg_batch_time_ms,
                result.epochs_completed,
                result.avg_epoch_time_s,
                result.data_loading_time_s,
                result.compute_time_s,
                result.pipeline_efficiency,
            ));
        }

        output.push_str(&format!(
            "AGGREGATE\t{:.1}\t{}\t{:.1}\t{}\t-\t{:.2}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\n",
            self.total_samples_per_second,
            self.total_samples,
            self.total_batches_per_second,
            self.total_batches,
            self.avg_batch_time_ms,
            self.total_epochs_completed,
            self.avg_epoch_time_s,
            self.avg_data_loading_time_s,
            self.avg_compute_time_s,
            self.avg_pipeline_efficiency,
        ));

        output
    }

    /// Format results as legacy TSV (for backward compatibility)
    /// 
    /// Alias for to_storage_tsv()
    pub fn to_tsv(&self) -> String {
        self.to_storage_tsv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_results() {
        let results = vec![
            WorkloadResult {
                agent_id: "agent1".to_string(),
                // Storage metrics
                ops_per_s: 1000.0,
                mib_per_s: 500.0,
                p50_ms: 10.0,
                p90_ms: 20.0,
                p95_ms: 25.0,
                p99_ms: 30.0,
                errors: 0,
                total_ops: 10000,
                duration_s: 10.0,
                // AI/ML metrics
                samples_per_second: 5000.0,
                total_samples: 50000,
                samples_per_batch: 64,
                batches_per_second: 78.125,
                total_batches: 781,
                avg_batch_time_ms: 12.8,
                epochs_completed: 1,
                avg_epoch_time_s: 10.0,
                data_loading_time_s: 6.0,
                compute_time_s: 3.5,
                pipeline_efficiency: 0.95,
            },
            WorkloadResult {
                agent_id: "agent2".to_string(),
                // Storage metrics
                ops_per_s: 1200.0,
                mib_per_s: 600.0,
                p50_ms: 12.0,
                p90_ms: 22.0,
                p95_ms: 27.0,
                p99_ms: 32.0,
                errors: 1,
                total_ops: 12000,
                duration_s: 10.0,
                // AI/ML metrics
                samples_per_second: 6000.0,
                total_samples: 60000,
                samples_per_batch: 64,
                batches_per_second: 93.75,
                total_batches: 938,
                avg_batch_time_ms: 10.7,
                epochs_completed: 1,
                avg_epoch_time_s: 10.0,
                data_loading_time_s: 5.5,
                compute_time_s: 4.0,
                pipeline_efficiency: 0.95,
            },
        ];

        let agg = AggregateResults::from_results(results).unwrap();
        
        // Storage aggregates
        assert_eq!(agg.total_ops_per_s, 2200.0);
        assert_eq!(agg.total_mib_per_s, 1100.0);
        assert_eq!(agg.avg_p50_ms, 11.0);
        assert_eq!(agg.total_errors, 1);
        assert_eq!(agg.total_ops, 22000);
        
        // AI/ML aggregates
        assert_eq!(agg.total_samples_per_second, 11000.0);
        assert_eq!(agg.total_samples, 110000);
        assert_eq!(agg.total_batches, 1719);
    }

    #[test]
    fn test_tsv_output() {
        let results = vec![
            WorkloadResult {
                agent_id: "agent1".to_string(),
                // Storage metrics
                ops_per_s: 1000.0,
                mib_per_s: 500.0,
                p50_ms: 10.0,
                p90_ms: 20.0,
                p95_ms: 25.0,
                p99_ms: 30.0,
                errors: 0,
                total_ops: 10000,
                duration_s: 10.0,
                // AI/ML metrics
                samples_per_second: 5000.0,
                total_samples: 50000,
                samples_per_batch: 64,
                batches_per_second: 78.125,
                total_batches: 781,
                avg_batch_time_ms: 12.8,
                epochs_completed: 1,
                avg_epoch_time_s: 10.0,
                data_loading_time_s: 6.0,
                compute_time_s: 3.5,
                pipeline_efficiency: 0.95,
            },
        ];

        let agg = AggregateResults::from_results(results).unwrap();
        
        // Test storage TSV
        let storage_tsv = agg.to_storage_tsv();
        assert!(storage_tsv.contains("agent_id\tops_s"));
        assert!(storage_tsv.contains("agent1\t1000.0"));
        assert!(storage_tsv.contains("AGGREGATE\t1000.0"));
        
        // Test AI/ML TSV
        let aiml_tsv = agg.to_aiml_tsv();
        assert!(aiml_tsv.contains("agent_id\tsamples_s"));
        assert!(aiml_tsv.contains("agent1\t5000.0"));
        assert!(aiml_tsv.contains("AGGREGATE\t5000.0"));
        
        // Test legacy to_tsv() still works
        let legacy_tsv = agg.to_tsv();
        assert!(legacy_tsv.contains("agent_id\tops_s"));
    }
}
