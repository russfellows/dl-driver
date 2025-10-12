/// Rust wrapper types for distributed execution
/// 
/// Provides ergonomic Rust types that wrap the protobuf-generated structs

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::proto;

/// Workload execution request with validated parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadRequest {
    pub config_yaml: String,
    pub agent_id: String,
    pub path_prefix: String,
    pub start_unix_ms: i64,
}

impl From<WorkloadRequest> for proto::RunWorkloadRequest {
    fn from(req: WorkloadRequest) -> Self {
        proto::RunWorkloadRequest {
            config_yaml: req.config_yaml,
            agent_id: req.agent_id,
            path_prefix: req.path_prefix,
            start_unix_ms: req.start_unix_ms,
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
        }
    }
}

/// Workload execution results with performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadResult {
    pub agent_id: String,
    pub ops_per_s: f64,
    pub mib_per_s: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub errors: u32,
    pub total_ops: u64,
    pub duration_s: f64,
}

impl From<proto::WorkloadSummary> for WorkloadResult {
    fn from(summary: proto::WorkloadSummary) -> Self {
        WorkloadResult {
            agent_id: summary.agent_id,
            ops_per_s: summary.ops_per_s,
            mib_per_s: summary.mib_per_s,
            p50_ms: summary.p50_ms,
            p90_ms: summary.p90_ms,
            p95_ms: summary.p95_ms,
            p99_ms: summary.p99_ms,
            errors: summary.errors,
            total_ops: summary.total_ops,
            duration_s: summary.duration_s,
        }
    }
}

impl From<WorkloadResult> for proto::WorkloadSummary {
    fn from(result: WorkloadResult) -> Self {
        proto::WorkloadSummary {
            agent_id: result.agent_id,
            ops_per_s: result.ops_per_s,
            mib_per_s: result.mib_per_s,
            p50_ms: result.p50_ms,
            p90_ms: result.p90_ms,
            p95_ms: result.p95_ms,
            p99_ms: result.p99_ms,
            errors: result.errors,
            total_ops: result.total_ops,
            duration_s: result.duration_s,
        }
    }
}

/// Aggregate results from multiple agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateResults {
    pub total_ops_per_s: f64,
    pub total_mib_per_s: f64,
    pub avg_p50_ms: f64,
    pub avg_p90_ms: f64,
    pub avg_p95_ms: f64,
    pub avg_p99_ms: f64,
    pub total_errors: u32,
    pub total_ops: u64,
    pub agent_results: Vec<WorkloadResult>,
}

impl AggregateResults {
    /// Compute aggregate statistics from multiple agent results
    pub fn from_results(results: Vec<WorkloadResult>) -> Result<Self> {
        if results.is_empty() {
            anyhow::bail!("Cannot aggregate empty results");
        }

        let total_ops_per_s: f64 = results.iter().map(|r| r.ops_per_s).sum();
        let total_mib_per_s: f64 = results.iter().map(|r| r.mib_per_s).sum();
        let total_errors: u32 = results.iter().map(|r| r.errors).sum();
        let total_ops: u64 = results.iter().map(|r| r.total_ops).sum();

        let count = results.len() as f64;
        let avg_p50_ms = results.iter().map(|r| r.p50_ms).sum::<f64>() / count;
        let avg_p90_ms = results.iter().map(|r| r.p90_ms).sum::<f64>() / count;
        let avg_p95_ms = results.iter().map(|r| r.p95_ms).sum::<f64>() / count;
        let avg_p99_ms = results.iter().map(|r| r.p99_ms).sum::<f64>() / count;

        Ok(AggregateResults {
            total_ops_per_s,
            total_mib_per_s,
            avg_p50_ms,
            avg_p90_ms,
            avg_p95_ms,
            avg_p99_ms,
            total_errors,
            total_ops,
            agent_results: results,
        })
    }

    /// Format results as TSV with per-agent and aggregate rows
    pub fn to_tsv(&self) -> String {
        let mut output = String::new();
        output.push_str("agent_id\tops_s\tmib_s\tp50_ms\tp90_ms\tp95_ms\tp99_ms\terrors\ttotal_ops\tduration_s\n");

        for result in &self.agent_results {
            output.push_str(&format!(
                "{}\t{:.1}\t{:.1}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t{:.2}\n",
                result.agent_id,
                result.ops_per_s,
                result.mib_per_s,
                result.p50_ms,
                result.p90_ms,
                result.p95_ms,
                result.p99_ms,
                result.errors,
                result.total_ops,
                result.duration_s
            ));
        }

        output.push_str(&format!(
            "AGGREGATE\t{:.1}\t{:.1}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t-\n",
            self.total_ops_per_s,
            self.total_mib_per_s,
            self.avg_p50_ms,
            self.avg_p90_ms,
            self.avg_p95_ms,
            self.avg_p99_ms,
            self.total_errors,
            self.total_ops
        ));

        output
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
                ops_per_s: 1000.0,
                mib_per_s: 500.0,
                p50_ms: 10.0,
                p90_ms: 20.0,
                p95_ms: 25.0,
                p99_ms: 30.0,
                errors: 0,
                total_ops: 10000,
                duration_s: 10.0,
            },
            WorkloadResult {
                agent_id: "agent2".to_string(),
                ops_per_s: 1200.0,
                mib_per_s: 600.0,
                p50_ms: 12.0,
                p90_ms: 22.0,
                p95_ms: 27.0,
                p99_ms: 32.0,
                errors: 1,
                total_ops: 12000,
                duration_s: 10.0,
            },
        ];

        let agg = AggregateResults::from_results(results).unwrap();
        
        assert_eq!(agg.total_ops_per_s, 2200.0);
        assert_eq!(agg.total_mib_per_s, 1100.0);
        assert_eq!(agg.avg_p50_ms, 11.0);
        assert_eq!(agg.total_errors, 1);
        assert_eq!(agg.total_ops, 22000);
    }

    #[test]
    fn test_tsv_output() {
        let results = vec![
            WorkloadResult {
                agent_id: "agent1".to_string(),
                ops_per_s: 1000.0,
                mib_per_s: 500.0,
                p50_ms: 10.0,
                p90_ms: 20.0,
                p95_ms: 25.0,
                p99_ms: 30.0,
                errors: 0,
                total_ops: 10000,
                duration_s: 10.0,
            },
        ];

        let agg = AggregateResults::from_results(results).unwrap();
        let tsv = agg.to_tsv();
        
        assert!(tsv.contains("agent_id\tops_s"));
        assert!(tsv.contains("agent1\t1000.0"));
        assert!(tsv.contains("AGGREGATE\t1000.0"));
    }
}
