/// Controller for orchestrating distributed DLIO workloads across multiple agents
/// 
/// Handles:
/// - Agent connection and health checking
/// - DLIO config distribution with path prefixes
/// - Coordinated start timing
/// - Result collection and aggregation

use anyhow::{Context, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::transport::Channel;
use tracing::{info, warn, error};

use crate::dlio_compat::DlioConfig;
use crate::dist::proto::dist_agent_client::DistAgentClient;
use crate::dist::proto::{RunWorkloadRequest, HealthCheckRequest, WorkloadSummary};
use crate::dist::types::{AggregateResults, WorkloadResult};
use crate::dist::path_utils::is_shared_storage;

/// Configuration for distributed execution
#[derive(Debug, Clone)]
pub struct DistributedConfig {
    pub agents: Vec<String>,
    pub path_template: String,
    pub start_delay_ms: u64,
    pub request_timeout_ms: u64,
    pub max_retries: u32,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        Self {
            agents: Vec::new(),
            path_template: "{id}/".to_string(),
            start_delay_ms: 1000,
            request_timeout_ms: 300_000, // 5 minutes
            max_retries: 3,
        }
    }
}

/// Controller orchestrates distributed DLIO workloads across multiple agents
pub struct Controller {
    config: DlioConfig,
    distributed: DistributedConfig,
}

impl Controller {
    /// Create a new controller with DLIO config and distributed settings
    pub fn new(config: DlioConfig, distributed: DistributedConfig) -> Self {
        Self { config, distributed }
    }

    /// Health check all agents before running workload
    /// 
    /// Returns Ok if all agents are healthy, Err otherwise
    pub async fn health_check_all(&self) -> Result<Vec<(String, bool)>> {
        info!("Health checking {} agents...", self.distributed.agents.len());
        
        let mut results = Vec::new();
        
        for agent_endpoint in &self.distributed.agents {
            match self.health_check_agent(agent_endpoint).await {
                Ok(healthy) => {
                    info!("✅ Agent {} - {}", agent_endpoint, 
                          if healthy { "Healthy" } else { "Unhealthy" });
                    results.push((agent_endpoint.clone(), healthy));
                }
                Err(e) => {
                    error!("❌ Agent {} - Connection failed: {}", agent_endpoint, e);
                    results.push((agent_endpoint.clone(), false));
                }
            }
        }
        
        let healthy_count = results.iter().filter(|(_, h)| *h).count();
        info!("Health check complete: {}/{} agents healthy", 
              healthy_count, results.len());
        
        if healthy_count != results.len() {
            anyhow::bail!("{} agents failed health check", results.len() - healthy_count);
        }
        
        Ok(results)
    }

    /// Health check a single agent
    async fn health_check_agent(&self, endpoint: &str) -> Result<bool> {
        let mut client = self.connect_agent(endpoint).await?;
        
        let request = tonic::Request::new(HealthCheckRequest {});
        
        let response = client
            .health_check(request)
            .await
            .context("Health check RPC failed")?;
        
        let health_response = response.into_inner();
        Ok(health_response.status == "healthy")
    }

    /// Connect to an agent endpoint
    async fn connect_agent(&self, endpoint: &str) -> Result<DistAgentClient<Channel>> {
        let url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            endpoint.to_string()
        } else {
            format!("http://{}", endpoint)
        };
        
        let channel = Channel::from_shared(url.clone())
            .context("Invalid agent URL")?
            .timeout(Duration::from_millis(self.distributed.request_timeout_ms))
            .connect()
            .await
            .context(format!("Failed to connect to agent: {}", endpoint))?;
        
        Ok(DistAgentClient::new(channel))
    }

    /// Run distributed workload across all agents (without results directory)
    /// 
    /// Returns aggregated results from all agents
    pub async fn run_distributed(&self) -> Result<AggregateResults> {
        // Simple version without results directory - just run the workload
        use std::path::PathBuf;
        let dummy_results_dir = PathBuf::from("/tmp/dummy_results");
        std::fs::create_dir_all(&dummy_results_dir)?;
        
        let mut results_dir = crate::results_dir::ResultsDir::create(
            std::path::Path::new("config.yaml"),
            Some("temp"),
            Some(&dummy_results_dir),
            self.distributed.agents.len(),
        )?;
        
        let agents_dir = results_dir.create_agents_dir()?;
        
        self.run_distributed_internal(&mut results_dir, &agents_dir).await
    }

    /// Run distributed workload across all agents with results directory
    /// 
    /// # Arguments
    /// * `config_path` - Optional path to config file (for results directory naming)
    /// * `output_dir` - Optional output directory for results (defaults to current directory)
    /// 
    /// Returns aggregated results from all agents
    pub async fn run_distributed_with_results(
        &self,
        config_path: Option<&std::path::Path>,
        output_dir: Option<&std::path::Path>,
    ) -> Result<AggregateResults> {
        use crate::results_dir::ResultsDir;
        use std::time::Instant;
        
        let start_time = Instant::now();
        
        // Create results directory
        let config_path = config_path.unwrap_or_else(|| std::path::Path::new("dlio_config.yaml"));
        let mut results_dir = ResultsDir::create(
            config_path,
            None,
            output_dir,
            self.distributed.agents.len(),
        )?;
        
        // Create agents subdirectory
        let agents_dir = results_dir.create_agents_dir()?;
        
        results_dir.write_console(&format!("🚀 Starting distributed workload execution"))?;
        results_dir.write_console(&format!("   Agents: {}", self.distributed.agents.len()))?;
        results_dir.write_console(&format!("   Start delay: {}ms", self.distributed.start_delay_ms))?;
        results_dir.write_console("")?;
        
        // Run the actual workload
        let aggregate = self.run_distributed_internal(&mut results_dir, &agents_dir).await?;
        
        // Calculate duration
        let duration_secs = start_time.elapsed().as_secs_f64();
        
        // Write consolidated TSV files at top level
        let storage_tsv_path = results_dir.storage_tsv_path();
        let aiml_tsv_path = results_dir.aiml_tsv_path();
        
        // Write storage results TSV
        let storage_tsv_content = aggregate.to_storage_tsv();
        std::fs::write(&storage_tsv_path, &storage_tsv_content)
            .with_context(|| format!("Failed to write storage TSV: {}", storage_tsv_path.display()))?;
        
        // Write AI/ML results TSV
        let aiml_tsv_content = aggregate.to_aiml_tsv();
        std::fs::write(&aiml_tsv_path, &aiml_tsv_content)
            .with_context(|| format!("Failed to write AI/ML TSV: {}", aiml_tsv_path.display()))?;
        
        // Finalize results directory
        results_dir.finalize(duration_secs, aggregate.agent_results.len())?;
        
        info!("\n✅ Results saved to: {}", results_dir.path().display());
        info!("   - config.yaml (copy of input config)");
        info!("   - storage_results.tsv (consolidated storage metrics)");
        info!("   - aiml_results.tsv (consolidated AI/ML metrics)");
        info!("   - metadata.json (run metadata)");
        info!("   - console.log (execution log)");
        info!("   - agents/ (per-agent results)");
        
        Ok(aggregate)
    }

    /// Run distributed workload across all agents (internal implementation)
    /// 
    /// Returns aggregated results from all agents
    async fn run_distributed_internal(
        &self,
        results_dir: &mut crate::results_dir::ResultsDir,
        agents_dir: &std::path::Path,
    ) -> Result<AggregateResults> {
        info!("🚀 Starting distributed workload execution");
        info!("   Agents: {}", self.distributed.agents.len());
        info!("   Start delay: {}ms", self.distributed.start_delay_ms);
        
        // Health check all agents first
        self.health_check_all().await?;
        
        // Calculate coordinated start time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let start_unix_ms = now + self.distributed.start_delay_ms as i64;
        
        info!("   Coordinated start time: {}ms from now", self.distributed.start_delay_ms);
        
        // Detect if storage backend is shared
        let is_shared = is_shared_storage(&self.config.dataset.data_folder);
        if is_shared {
            info!("   Storage: Shared ({})", self.config.dataset.data_folder);
            info!("   Path strategy: No agent-specific prefixes");
        } else {
            info!("   Storage: Local ({})", self.config.dataset.data_folder);
            info!("   Path strategy: Agent-specific prefixes ({})", 
                  self.distributed.path_template);
        }
        
        // Send workload to all agents in parallel
        let mut tasks = Vec::new();
        
        for (idx, agent_endpoint) in self.distributed.agents.iter().enumerate() {
            let agent_id = format!("agent-{}", idx);
            let agent_id_task = agent_id.clone(); // Clone for task
            let config = self.config.clone();
            let endpoint = agent_endpoint.clone();
            let path_template = self.distributed.path_template.clone();
            let timeout_ms = self.distributed.request_timeout_ms;
            
            let task = tokio::spawn(async move {
                Self::send_workload_to_agent(
                    &endpoint,
                    &agent_id_task,
                    config,
                    &path_template,
                    start_unix_ms,
                    timeout_ms,
                    is_shared,
                )
                .await
            });
            
            tasks.push((agent_id, task));
        }
        
        info!("📤 Workload sent to all {} agents", tasks.len());
        results_dir.write_console("📤 Workload sent to all agents")?;
        info!("⏳ Waiting for agents to complete...");
        results_dir.write_console("⏳ Waiting for agents to complete...")?;
        results_dir.write_console("")?;
        
        // Collect results and proto summaries (for histogram data)
        let mut results = Vec::new();
        let mut summaries = Vec::new();
        for (agent_id, task) in tasks {
            match task.await {
                Ok(Ok((result, summary))) => {
                    info!("✅ Agent {} completed successfully", agent_id);
                    results_dir.write_console(&format!("✅ Agent {} completed successfully", agent_id))?;
                    results_dir.add_agent(agent_id.clone());
                    
                    // Write per-agent results to agents/ subdirectory
                    self.write_agent_results(agents_dir, &agent_id, &result, &summary)?;
                    
                    results.push(result);
                    summaries.push(summary);
                }
                Ok(Err(e)) => {
                    error!("❌ Agent {} failed: {}", agent_id, e);
                    results_dir.write_console(&format!("❌ Agent {} failed: {}", agent_id, e))?;
                    // Continue collecting other results
                }
                Err(e) => {
                    error!("❌ Agent {} task panicked: {}", agent_id, e);
                    results_dir.write_console(&format!("❌ Agent {} task panicked: {}", agent_id, e))?;
                }
            }
        }
        
        if results.is_empty() {
            anyhow::bail!("All agents failed - no results to aggregate");
        }
        
        if results.len() < self.distributed.agents.len() {
            let msg = format!("⚠️  Only {}/{} agents succeeded", 
                  results.len(), self.distributed.agents.len());
            warn!("{}", msg);
            results_dir.write_console(&msg)?;
        }
        
        results_dir.write_console("")?;
        info!("📊 Aggregating results from {} agents...", results.len());
        results_dir.write_console(&format!("📊 Aggregating results from {} agents...", results.len()))?;
        
        // Calculate wall time (max duration across all agents)
        let wall_seconds = results.iter()
            .map(|r| r.duration_s)
            .fold(0.0f64, f64::max);
        
        // Write consolidated bucket-level histogram TSV (sai3-bench pattern)
        Self::write_consolidated_histogram_tsv(results_dir, &summaries, wall_seconds)?;
        
        // Aggregate results using histogram merging for accurate percentiles
        let aggregate = AggregateResults::from_results_with_histograms(results, &summaries)?;
        
        info!("🎉 Distributed workload complete!");
        results_dir.write_console("🎉 Distributed workload complete!")?;
        results_dir.write_console("")?;
        
        Ok(aggregate)
    }

    /// Write per-agent results to agents subdirectory
    fn write_agent_results(
        &self,
        agents_dir: &std::path::Path,
        agent_id: &str,
        result: &WorkloadResult,
        summary: &WorkloadSummary,
    ) -> Result<()> {
        
        // Create agent subdirectory
        let agent_dir = agents_dir.join(agent_id);
        std::fs::create_dir_all(&agent_dir)
            .with_context(|| format!("Failed to create agent directory: {}", agent_dir.display()))?;
        
        // Write metadata.json
        let metadata = serde_json::json!({
            "agent_id": agent_id,
            "ops_per_s": result.ops_per_s,
            "mib_per_s": result.mib_per_s,
            "total_ops": result.total_ops,
            "total_samples": result.total_samples,
            "epochs_completed": result.epochs_completed,
            "p50_ms": result.p50_ms,
            "p90_ms": result.p90_ms,
            "p95_ms": result.p95_ms,
            "p99_ms": result.p99_ms,
            "duration_s": result.duration_s,
        });
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(agent_dir.join("metadata.json"), metadata_json)?;
        
        // Write bucket-level storage TSV (from agent)
        if !summary.storage_tsv_content.is_empty() {
            std::fs::write(agent_dir.join("storage_results.tsv"), &summary.storage_tsv_content)?;
        }
        
        // Write AI/ML TSV (if provided)
        if !summary.aiml_tsv_content.is_empty() {
            std::fs::write(agent_dir.join("aiml_results.tsv"), &summary.aiml_tsv_content)?;
        }
        
        info!("Wrote agent {} results to: {}", agent_id, agent_dir.display());
        Ok(())
    }

    /// Send workload to a single agent
    /// 
    /// Returns both WorkloadResult and the raw proto WorkloadSummary (which contains histogram data)
    async fn send_workload_to_agent(
        endpoint: &str,
        agent_id: &str,
        config: DlioConfig,
        path_template: &str,
        start_unix_ms: i64,
        timeout_ms: u64,
        is_shared: bool,
    ) -> Result<(WorkloadResult, WorkloadSummary)> {
        // Connect to agent
        let url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            endpoint.to_string()
        } else {
            format!("http://{}", endpoint)
        };
        
        let channel = Channel::from_shared(url.clone())
            .context("Invalid agent URL")?
            .timeout(Duration::from_millis(timeout_ms))
            .connect()
            .await
            .context(format!("Failed to connect to agent: {}", endpoint))?;
        
        let mut client = DistAgentClient::new(channel);
        
        // Apply agent-specific path prefix if local storage
        let path_prefix = if is_shared {
            String::new()
        } else {
            path_template.replace("{id}", agent_id)
        };
        
        if !path_prefix.is_empty() {
            info!("Agent {} using path prefix: {}", agent_id, path_prefix);
        }
        
        // Serialize config to YAML
        let config_yaml = serde_yaml::to_string(&config)
            .context("Failed to serialize DLIO config to YAML")?;
        
        // Send RunWorkload request
        let request = tonic::Request::new(RunWorkloadRequest {
            config_yaml,
            agent_id: agent_id.to_string(),
            path_prefix,
            start_unix_ms,
            // v0.8.1 enhancement - per-agent config overrides (currently unused)
            agent_config: None,
            // v0.8.1 enhancement - shared storage flag (currently false)
            shared_storage: false,
        });
        
        let response = client
            .run_workload(request)
            .await
            .context(format!("RunWorkload RPC failed for agent {}", agent_id))?;
        
        let summary = response.into_inner();
        
        // Return both WorkloadResult and the proto summary (which contains histogram bytes)
        let result = WorkloadResult::from(summary.clone());
        Ok((result, summary))
    }
    
    /// Write consolidated bucket-level histogram TSV (sai3-bench pattern)
    /// 
    /// Deserializes and merges histograms from all agents, then exports bucket-level
    /// percentiles to consolidated_storage_results.tsv
    fn write_consolidated_histogram_tsv(
        results_dir: &crate::results_dir::ResultsDir,
        summaries: &[WorkloadSummary],
        wall_seconds: f64,
    ) -> Result<()> {
        use hdrhistogram::{Histogram, serialization::Deserializer};
        use crate::metrics::{NUM_SIZE_BUCKETS, SIZE_BUCKET_LABELS};
        use std::io::Write;
        
        const NUM_BUCKETS: usize = NUM_SIZE_BUCKETS;
        
        if summaries.is_empty() {
            return Ok(());
        }
        
        // Create accumulators for read and write operations
        let mut read_accumulators: Vec<Histogram<u64>> = Vec::new();
        let mut write_accumulators: Vec<Histogram<u64>> = Vec::new();
        
        for _ in 0..NUM_BUCKETS {
            read_accumulators.push(Histogram::new(3)?);
            write_accumulators.push(Histogram::new(3)?);
        }
        
        // Deserialize and merge histograms from all agents
        let mut deserializer = Deserializer::new();
        
        for (agent_idx, summary) in summaries.iter().enumerate() {
            // Deserialize READ histograms
            if !summary.histogram_read.is_empty() {
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
            
            // Deserialize WRITE histograms
            if !summary.histogram_write.is_empty() {
                let mut cursor = &summary.histogram_write[..];
                for bucket_idx in 0..NUM_BUCKETS {
                    let hist: Histogram<u64> = deserializer.deserialize(&mut cursor)
                        .with_context(|| format!(
                            "Failed to deserialize WRITE histogram bucket {} from agent {}",
                            bucket_idx, agent_idx
                        ))?;
                    write_accumulators[bucket_idx].add(hist)
                        .with_context(|| format!(
                            "Failed to merge WRITE histogram bucket {} from agent {}",
                            bucket_idx, agent_idx
                        ))?;
                }
            }
        }
        
        // Write consolidated TSV with bucket-level detail
        let tsv_path = results_dir.path().join("consolidated_storage_results.tsv");
        let mut f = std::fs::File::create(&tsv_path)
            .with_context(|| format!("Failed to create consolidated TSV: {}", tsv_path.display()))?;
        
        // Write header (matching sai3-bench format)
        writeln!(f, "operation\tsize_bucket\tbucket_idx\tmean_us\tp50_us\tp90_us\tp95_us\tp99_us\tmax_us\tops_per_sec\tcount")?;
        
        // Collect rows for sorting
        let mut rows: Vec<(usize, String)> = Vec::new();
        
        // Collect READ bucket rows
        for (bucket_idx, hist) in read_accumulators.iter().enumerate() {
            let count = hist.len();
            if count == 0 {
                continue;
            }
            
            let mean_us = hist.mean();
            let p50_us = hist.value_at_quantile(0.50) as f64;
            let p90_us = hist.value_at_quantile(0.90) as f64;
            let p95_us = hist.value_at_quantile(0.95) as f64;
            let p99_us = hist.value_at_quantile(0.99) as f64;
            let max_us = hist.max() as f64;
            
            let ops_per_sec = count as f64 / wall_seconds;
            
            let row = format!(
                "READ\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}",
                SIZE_BUCKET_LABELS[bucket_idx],
                bucket_idx,
                mean_us, p50_us, p90_us, p95_us, p99_us, max_us,
                ops_per_sec,
                count
            );
            
            rows.push((bucket_idx, row));
        }
        
        // Collect WRITE bucket rows
        for (bucket_idx, hist) in write_accumulators.iter().enumerate() {
            let count = hist.len();
            if count == 0 {
                continue;
            }
            
            let mean_us = hist.mean();
            let p50_us = hist.value_at_quantile(0.50) as f64;
            let p90_us = hist.value_at_quantile(0.90) as f64;
            let p95_us = hist.value_at_quantile(0.95) as f64;
            let p99_us = hist.value_at_quantile(0.99) as f64;
            let max_us = hist.max() as f64;
            
            let ops_per_sec = count as f64 / wall_seconds;
            
            let row = format!(
                "WRITE\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}",
                SIZE_BUCKET_LABELS[bucket_idx],
                bucket_idx,
                mean_us, p50_us, p90_us, p95_us, p99_us, max_us,
                ops_per_sec,
                count
            );
            
            rows.push((bucket_idx + NUM_BUCKETS, row)); // Offset for WRITE to sort after READ
        }
        
        // Add aggregate rows (combine all buckets)
        let mut read_combined = Histogram::new(3)?;
        for hist in read_accumulators.iter() {
            if hist.len() > 0 {
                read_combined.add(hist)?;
            }
        }
        
        if read_combined.len() > 0 {
            let count = read_combined.len();
            let mean_us = read_combined.mean();
            let p50_us = read_combined.value_at_quantile(0.50) as f64;
            let p90_us = read_combined.value_at_quantile(0.90) as f64;
            let p95_us = read_combined.value_at_quantile(0.95) as f64;
            let p99_us = read_combined.value_at_quantile(0.99) as f64;
            let max_us = read_combined.max() as f64;
            let ops_per_sec = count as f64 / wall_seconds;
            
            let row = format!(
                "READ\tALL\t98\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}",
                mean_us, p50_us, p90_us, p95_us, p99_us, max_us,
                ops_per_sec,
                count
            );
            rows.push((98, row));
        }
        
        let mut write_combined = Histogram::new(3)?;
        for hist in write_accumulators.iter() {
            if hist.len() > 0 {
                write_combined.add(hist)?;
            }
        }
        
        if write_combined.len() > 0 {
            let count = write_combined.len();
            let mean_us = write_combined.mean();
            let p50_us = write_combined.value_at_quantile(0.50) as f64;
            let p90_us = write_combined.value_at_quantile(0.90) as f64;
            let p95_us = write_combined.value_at_quantile(0.95) as f64;
            let p99_us = write_combined.value_at_quantile(0.99) as f64;
            let max_us = write_combined.max() as f64;
            let ops_per_sec = count as f64 / wall_seconds;
            
            let row = format!(
                "WRITE\tALL\t99\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}",
                mean_us, p50_us, p90_us, p95_us, p99_us, max_us,
                ops_per_sec,
                count
            );
            rows.push((99, row));
        }
        
        // Sort by bucket index
        rows.sort_by_key(|(idx, _)| *idx);
        
        // Write all rows
        for (_, row) in rows {
            writeln!(f, "{}", row)?;
        }
        
        info!("Consolidated bucket-level TSV written to: {}", tsv_path.display());
        Ok(())
    }
}

