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
use tracing::{debug, info, warn, error};

use crate::dlio_compat::DlioConfig;
use crate::dist::proto::dist_agent_client::DistAgentClient;
use crate::dist::proto::{RunWorkloadRequest, HealthCheckRequest, WorkloadSummary, LiveStats};
use crate::dist::types::{AggregateResults, WorkloadResult};
use crate::dist::path_utils::is_shared_storage;

/// Aggregated live statistics across all agents
#[derive(Debug, Clone)]
struct AggregateStats {
    num_agents: usize,
    elapsed_s: f64,
    
    // GET operations
    total_get_ops: u64,
    total_get_bytes: u64,
    get_mean_us: f64,
    get_p50_us: f64,
    get_p95_us: f64,
    
    // PUT operations
    total_put_ops: u64,
    total_put_bytes: u64,
    put_mean_us: f64,
    put_p50_us: f64,
    put_p95_us: f64,
    
    // AI/ML metrics
    samples_per_second: f64,
    total_samples: u64,
}

impl AggregateStats {
    /// Format progress message with intelligent phase detection
    /// 
    /// - >90% GET ops: Training phase (show samples/s)
    /// - >90% PUT ops: Data prep phase (show PUT only)
    /// - Mixed: Show both GET and PUT
    fn format_progress(&self) -> String {
        let total_ops = self.total_get_ops + self.total_put_ops;
        if total_ops == 0 {
            return "Waiting for operations...".to_string();
        }
        
        let get_ratio = self.total_get_ops as f64 / total_ops as f64;
        let put_ratio = self.total_put_ops as f64 / total_ops as f64;
        
        // Phase detection
        if get_ratio > 0.90 {
            // Training phase - emphasize samples/s
            format!(
                "Training: {} samples/s │ GET: {} ops, {} ({:.1}ms mean, {:.1}ms p95)",
                format_count(self.samples_per_second as u64),
                format_count(self.total_get_ops),
                format_bandwidth(self.total_get_bytes, self.elapsed_s),
                self.get_mean_us / 1000.0,
                self.get_p95_us / 1000.0
            )
        } else if put_ratio > 0.90 {
            // Data prep phase - show PUT only
            format!(
                "Data Prep: PUT {} ops, {} ({:.1}ms mean, {:.1}ms p95)",
                format_count(self.total_put_ops),
                format_bandwidth(self.total_put_bytes, self.elapsed_s),
                self.put_mean_us / 1000.0,
                self.put_p95_us / 1000.0
            )
        } else {
            // Mixed phase - show both (multi-line via newline)
            format!(
                "GET: {} ops, {} ({:.1}ms mean) │ PUT: {} ops, {} ({:.1}ms mean)",
                format_count(self.total_get_ops),
                format_bandwidth(self.total_get_bytes, self.elapsed_s),
                self.get_mean_us / 1000.0,
                format_count(self.total_put_ops),
                format_bandwidth(self.total_put_bytes, self.elapsed_s),
                self.put_mean_us / 1000.0
            )
        }
    }
}

/// Helper to format large counts with K/M/B suffixes
fn format_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Helper to format bandwidth (bytes/sec)
fn format_bandwidth(bytes: u64, seconds: f64) -> String {
    if seconds == 0.0 {
        return "0 B/s".to_string();
    }
    
    let bytes_per_sec = bytes as f64 / seconds;
    if bytes_per_sec >= 1_073_741_824.0 {
        format!("{:.2} GiB/s", bytes_per_sec / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576.0 {
        format!("{:.2} MiB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.2} KiB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// Aggregates live statistics from multiple agents
struct LiveStatsAggregator {
    latest_stats: std::collections::HashMap<String, LiveStats>,
    completed_agents: std::collections::HashSet<String>,
}

impl LiveStatsAggregator {
    fn new() -> Self {
        Self {
            latest_stats: std::collections::HashMap::new(),
            completed_agents: std::collections::HashSet::new(),
        }
    }
    
    fn update(&mut self, stats: LiveStats) {
        self.latest_stats.insert(stats.agent_id.clone(), stats);
    }
    
    fn mark_completed(&mut self, agent_id: &str) {
        self.completed_agents.insert(agent_id.to_string());
    }
    
    fn all_completed(&self) -> bool {
        if self.latest_stats.is_empty() {
            return false;
        }
        self.latest_stats.len() == self.completed_agents.len()
    }
    
    /// Aggregate statistics across all agents
    /// 
    /// Uses weighted averaging for latencies (weight = operation count)
    fn aggregate(&self) -> AggregateStats {
        let mut agg = AggregateStats {
            num_agents: self.latest_stats.len(),
            elapsed_s: 0.0,
            total_get_ops: 0,
            total_get_bytes: 0,
            get_mean_us: 0.0,
            get_p50_us: 0.0,
            get_p95_us: 0.0,
            total_put_ops: 0,
            total_put_bytes: 0,
            put_mean_us: 0.0,
            put_p50_us: 0.0,
            put_p95_us: 0.0,
            samples_per_second: 0.0,
            total_samples: 0,
        };
        
        if self.latest_stats.is_empty() {
            return agg;
        }
        
        // Accumulate weighted latencies and totals
        for stats in self.latest_stats.values() {
            agg.elapsed_s = agg.elapsed_s.max(stats.elapsed_s);
            
            // GET operations
            agg.total_get_ops += stats.get_ops;
            agg.total_get_bytes += stats.get_bytes;
            agg.get_mean_us += stats.get_mean_us * stats.get_ops as f64;
            agg.get_p50_us += stats.get_p50_us * stats.get_ops as f64;
            agg.get_p95_us += stats.get_p95_us * stats.get_ops as f64;
            
            // PUT operations
            agg.total_put_ops += stats.put_ops;
            agg.total_put_bytes += stats.put_bytes;
            agg.put_mean_us += stats.put_mean_us * stats.put_ops as f64;
            agg.put_p50_us += stats.put_p50_us * stats.put_ops as f64;
            agg.put_p95_us += stats.put_p95_us * stats.put_ops as f64;
            
            // AI/ML metrics
            agg.samples_per_second += stats.samples_per_second;
            agg.total_samples += stats.total_samples;
        }
        
        // Normalize weighted averages
        if agg.total_get_ops > 0 {
            agg.get_mean_us /= agg.total_get_ops as f64;
            agg.get_p50_us /= agg.total_get_ops as f64;
            agg.get_p95_us /= agg.total_get_ops as f64;
        }
        if agg.total_put_ops > 0 {
            agg.put_mean_us /= agg.total_put_ops as f64;
            agg.put_p50_us /= agg.total_put_ops as f64;
            agg.put_p95_us /= agg.total_put_ops as f64;
        }
        
        agg
    }
}

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
    /// Uses streaming RPC to collect live stats from all agents and display
    /// real-time progress with intelligent phase detection.
    /// 
    /// Returns aggregated results from all agents
    async fn run_distributed_internal(
        &self,
        results_dir: &mut crate::results_dir::ResultsDir,
        agents_dir: &std::path::Path,  // WARNING: Currently unused - will be used in Phase 5 for per-agent results writing
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
        
        // Create channel for live stats aggregation
        let (tx_stats, mut rx_stats) = tokio::sync::mpsc::channel::<LiveStats>(100);
        
        // Spawn streaming tasks for all agents
        let mut stream_tasks = Vec::new();
        let agents: Vec<_> = self.distributed.agents.iter().enumerate().collect();
        
        for (idx, agent_endpoint) in agents.iter() {
            let agent_id = format!("agent-{}", idx);
            let agent_id_task = agent_id.clone();  // Clone for task
            let config = self.config.clone();
            let endpoint = agent_endpoint.to_string();
            let path_template = self.distributed.path_template.clone();
            let timeout_ms = self.distributed.request_timeout_ms;
            let tx = tx_stats.clone();
            
            let task = tokio::spawn(async move {
                Self::stream_workload_from_agent(
                    &endpoint,
                    &agent_id_task,
                    config,
                    &path_template,
                    start_unix_ms,
                    timeout_ms,
                    is_shared,
                    tx,
                )
                .await
            });
            
            stream_tasks.push((agent_id, task));
        }
        
        // Drop our copy of tx so rx will close when all tasks complete
        drop(tx_stats);
        
        info!("📤 Workload sent to all {} agents", stream_tasks.len());
        results_dir.write_console("📤 Workload sent to all agents")?;
        info!("⏳ Live stats streaming enabled...");
        results_dir.write_console("⏳ Live stats streaming enabled...")?;
        results_dir.write_console("")?;
        
        // Setup progress bar
        use indicatif::{ProgressBar, ProgressStyle};
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap()
        );
        progress_bar.enable_steady_tick(std::time::Duration::from_millis(100));
        
        // Setup Ctrl+C handler
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        
        // Aggregator for live stats
        let mut aggregator = LiveStatsAggregator::new();
        let mut last_update = std::time::Instant::now();
        
        // v0.8.7: Resilience - track per-agent activity for timeout detection
        let mut agent_last_seen: std::collections::HashMap<String, std::time::Instant> = std::collections::HashMap::new();
        let mut dead_agents = std::collections::HashSet::new();
        let timeout_warn_secs = 5.0;
        let timeout_dead_secs = 10.0;
        
        // v0.8.7: Collect final summaries for persistence (extracted from completed LiveStats messages)
        let mut agent_summaries: Vec<WorkloadSummary> = Vec::new();
        
        // v0.8.7: Track last console.log write time (write every 1 second)
        let mut last_console_log = std::time::Instant::now();
        
        // Process live stats stream
        loop {
            // v0.8.7: Check for stalled agents (timeout detection)
            let now = std::time::Instant::now();
            for (agent_id, last_seen) in &agent_last_seen {
                if dead_agents.contains(agent_id) {
                    continue;  // Skip already dead agents
                }
                
                // Check if agent is in completed set (via aggregator)
                let agg = aggregator.aggregate();
                if agg.num_agents > 0 && aggregator.all_completed() {
                    continue;  // Skip completed agents
                }
                
                let elapsed = now.duration_since(*last_seen).as_secs_f64();
                if elapsed >= timeout_dead_secs {
                    if !dead_agents.contains(agent_id) {
                        error!("❌ Agent {} STALLED (no updates for {:.1}s) - marking as DEAD", agent_id, elapsed);
                        dead_agents.insert(agent_id.clone());
                        aggregator.mark_completed(agent_id);  // Remove from active count
                        progress_bar.set_message(format!("{} (⚠️ {} dead)", agg.format_progress(), dead_agents.len()));
                    }
                } else if elapsed >= timeout_warn_secs {
                    warn!("⚠️  Agent {} delayed: no updates for {:.1}s", agent_id, elapsed);
                }
            }
            
            tokio::select! {
                Some(stats) = rx_stats.recv() => {
                    // v0.8.7: Update last seen timestamp for resilience
                    agent_last_seen.insert(stats.agent_id.clone(), std::time::Instant::now());
                    
                    // v0.8.7: Extract final summary if completed
                    if stats.completed {
                        aggregator.mark_completed(&stats.agent_id);
                        
                        // Extract and store final summary for persistence
                        if let Some(summary) = stats.final_summary {
                            debug!("Collected final summary from agent {}", summary.agent_id);
                            agent_summaries.push(summary);
                        } else {
                            warn!("Agent {} completed but did not provide final summary", stats.agent_id);
                        }
                    } else {
                        // Regular update (not completed yet)
                        aggregator.update(stats);
                    }
                    
                    // Update display every 100ms (rate limiting)
                    if last_update.elapsed() > std::time::Duration::from_millis(100) {
                        let agg = aggregator.aggregate();
                        let msg = if dead_agents.is_empty() {
                            agg.format_progress()
                        } else {
                            format!("{} (⚠️ {} dead)", agg.format_progress(), dead_agents.len())
                        };
                        progress_bar.set_message(msg.clone());
                        last_update = std::time::Instant::now();
                        
                        // v0.8.7: Write live stats to console.log every 1 second
                        if last_console_log.elapsed() >= std::time::Duration::from_secs(1) {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let log_line = format!("[{}] {}", timestamp, msg);
                            if let Err(e) = results_dir.write_console(&log_line) {
                                warn!("Failed to write live stats to console.log: {}", e);
                            }
                            last_console_log = std::time::Instant::now();
                        }
                    }
                    
                    // v0.8.7: Check if all agents completed or dead (graceful degradation)
                    if aggregator.all_completed() {
                        break;
                    }
                }
                
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    // v0.8.7: Periodic timeout check (every 1 second)
                    // Check logic is at top of loop
                }
                
                _ = &mut ctrl_c => {
                    warn!("Ctrl+C received, interrupting workload");
                    progress_bar.finish_with_message("Interrupted by user");
                    anyhow::bail!("Interrupted by Ctrl+C");
                }
            }
        }
        
        // Final aggregation
        let final_stats = aggregator.aggregate();
        progress_bar.finish_with_message(format!("✓ All {} agents completed", final_stats.num_agents));
        println!();  // Blank line after progress
        
        // Print final aggregate results
        println!("=== Final Aggregate Results ===");
        let total_ops = final_stats.total_get_ops + final_stats.total_put_ops;
        println!("Total operations: {} READ, {} WRITE", 
                 format_count(final_stats.total_get_ops), 
                 format_count(final_stats.total_put_ops));
        
        if final_stats.total_get_ops > 0 {
            println!("READ: {:.0} ops/s, {} (mean: {:.1}ms, p50: {:.1}ms, p95: {:.1}ms)",
                     final_stats.total_get_ops as f64 / final_stats.elapsed_s,
                     format_bandwidth(final_stats.total_get_bytes, final_stats.elapsed_s),
                     final_stats.get_mean_us / 1000.0,
                     final_stats.get_p50_us / 1000.0,
                     final_stats.get_p95_us / 1000.0);
        }
        
        if final_stats.total_put_ops > 0 {
            println!("WRITE: {:.0} ops/s, {} (mean: {:.1}ms, p50: {:.1}ms, p95: {:.1}ms)",
                     final_stats.total_put_ops as f64 / final_stats.elapsed_s,
                     format_bandwidth(final_stats.total_put_bytes, final_stats.elapsed_s),
                     final_stats.put_mean_us / 1000.0,
                     final_stats.put_p50_us / 1000.0,
                     final_stats.put_p95_us / 1000.0);
        }
        
        if final_stats.total_samples > 0 {
            println!("AI/ML: {} samples, {:.0} samples/s",
                     format_count(final_stats.total_samples),
                     final_stats.samples_per_second);
        }
        
        println!("Elapsed: {:.2}s", final_stats.elapsed_s);
        println!();
        
        // v0.8.7: Write per-agent results and create consolidated TSV with histogram aggregation
        if !agent_summaries.is_empty() {
            info!("Writing results for {} agents", agent_summaries.len());
            
            // Write per-agent results to agents/{agent-id}/ subdirectory
            for summary in &agent_summaries {
                let result = WorkloadResult::from(summary.clone());
                if let Err(e) = self.write_agent_results(&agents_dir, &summary.agent_id, &result, summary) {
                    error!("Failed to write results for agent {}: {}", summary.agent_id, e);
                }
            }
            
            // Create consolidated histogram TSV with bucket-level aggregation
            if let Err(e) = Self::write_consolidated_histogram_tsv(results_dir, &agent_summaries, final_stats.elapsed_s) {
                error!("Failed to create consolidated histogram TSV: {}", e);
            } else {
                info!("✓ Consolidated storage_results.tsv created with accurate histogram aggregation");
            }
        } else {
            warn!("No agent summaries collected - per-agent results and consolidated TSV not available");
            warn!("This may indicate agents failed to return final_summary in completed LiveStats messages");
        }
        
        // Collect task results (errors only)
        for (agent_id, task) in stream_tasks {
            match task.await {
                Ok(Ok(())) => {
                    info!("✅ Agent {} stream completed successfully", agent_id);
                }
                Ok(Err(e)) => {
                    error!("❌ Agent {} stream failed: {}", agent_id, e);
                }
                Err(e) => {
                    error!("❌ Agent {} task panicked: {}", agent_id, e);
                }
            }
        }
        
        // v0.8.7: Build aggregate results from collected summaries using From trait
        let agent_results: Vec<WorkloadResult> = agent_summaries.iter()
            .map(|s| WorkloadResult::from(s.clone()))
            .collect();
        
        let aggregate = AggregateResults {
            agent_results,
            total_ops: total_ops,
            total_samples: final_stats.total_samples,
            total_ops_per_s: total_ops as f64 / final_stats.elapsed_s,
            total_mib_per_s: (final_stats.total_get_bytes + final_stats.total_put_bytes) as f64 
                             / 1_048_576.0 / final_stats.elapsed_s,
            avg_p50_ms: (final_stats.get_p50_us + final_stats.put_p50_us) / 2000.0,  // Avg of GET/PUT
            avg_p90_ms: 0.0,  // Not available from live stats
            avg_p95_ms: (final_stats.get_p95_us + final_stats.put_p95_us) / 2000.0,  // Avg of GET/PUT
            avg_p99_ms: 0.0,  // Not available from live stats
            total_errors: 0,
            total_samples_per_second: final_stats.samples_per_second,
            total_batches_per_second: 0.0,
            total_batches: 0,
            avg_batch_time_ms: 0.0,
            total_epochs_completed: 0,
            avg_epoch_time_s: 0.0,
            avg_data_loading_time_s: 0.0,
            avg_compute_time_s: 0.0,
            avg_pipeline_efficiency: 0.0,
        };
        
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
    
    /// Stream workload from a single agent using streaming RPC
    /// 
    /// Consumes live stats stream and forwards to aggregator via channel.
    /// Returns () on success (live stats are sent via channel).
    async fn stream_workload_from_agent(
        endpoint: &str,
        agent_id: &str,
        config: DlioConfig,
        path_template: &str,
        start_unix_ms: i64,
        timeout_ms: u64,
        is_shared: bool,
        tx: tokio::sync::mpsc::Sender<LiveStats>,
    ) -> Result<()> {
        use futures::stream::StreamExt;
        
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
        
        // Send RunWorkloadWithLiveStats request (streaming)
        let request = tonic::Request::new(RunWorkloadRequest {
            config_yaml,
            agent_id: agent_id.to_string(),
            path_prefix,
            start_unix_ms,
            agent_config: None,
            shared_storage: false,
        });
        
        let mut stream = client
            .run_workload_with_live_stats(request)
            .await
            .context(format!("RunWorkloadWithLiveStats RPC failed for agent {}", agent_id))?
            .into_inner();
        
        // Consume stream and forward to aggregator
        while let Some(result) = stream.next().await {
            match result {
                Ok(stats) => {
                    if tx.send(stats).await.is_err() {
                        // Receiver dropped (probably Ctrl+C)
                        break;
                    }
                }
                Err(e) => {
                    error!("Agent {} stream error: {}", agent_id, e);
                    return Err(e.into());
                }
            }
        }
        
        Ok(())
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

