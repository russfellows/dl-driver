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
use crate::dist::proto::{RunWorkloadRequest, HealthCheckRequest};
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

    /// Run distributed workload across all agents
    /// 
    /// Returns aggregated results from all agents
    pub async fn run_distributed(&self) -> Result<AggregateResults> {
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
        info!("⏳ Waiting for agents to complete...");
        
        // Collect results
        let mut results = Vec::new();
        for (agent_id, task) in tasks {
            match task.await {
                Ok(Ok(result)) => {
                    info!("✅ Agent {} completed successfully", agent_id);
                    results.push(result);
                }
                Ok(Err(e)) => {
                    error!("❌ Agent {} failed: {}", agent_id, e);
                    // Continue collecting other results
                }
                Err(e) => {
                    error!("❌ Agent {} task panicked: {}", agent_id, e);
                }
            }
        }
        
        if results.is_empty() {
            anyhow::bail!("All agents failed - no results to aggregate");
        }
        
        if results.len() < self.distributed.agents.len() {
            warn!("⚠️  Only {}/{} agents succeeded", 
                  results.len(), self.distributed.agents.len());
        }
        
        info!("📊 Aggregating results from {} agents...", results.len());
        
        // Aggregate results
        let aggregate = AggregateResults::from_results(results)?;
        
        info!("🎉 Distributed workload complete!");
        
        Ok(aggregate)
    }

    /// Send workload to a single agent
    async fn send_workload_to_agent(
        endpoint: &str,
        agent_id: &str,
        config: DlioConfig,
        path_template: &str,
        start_unix_ms: i64,
        timeout_ms: u64,
        is_shared: bool,
    ) -> Result<WorkloadResult> {
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
        
        // Convert to WorkloadResult
        Ok(WorkloadResult::from(summary))
    }
}
