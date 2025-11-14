/// Distributed execution configuration
/// 
/// Configuration for coordinating DLIO workloads across multiple hosts.
/// Can be embedded in DLIO config YAML under a `distributed:` key.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Configuration for distributed multi-host execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    /// List of agent addresses in "host:port" format
    /// Example: ["host1:50051", "host2:50051", "host3:50051"]
    pub agents: Vec<String>,

    /// Path prefix template for agent isolation on local storage
    /// Use `{id}` placeholder which will be replaced with agent_id
    /// Example: "agent-{id}/" → "agent-0/", "agent-1/", etc.
    /// 
    /// Only applied to local storage (file://, direct://). 
    /// Shared storage (s3://, az://) is unchanged.
    #[serde(default = "default_path_template")]
    pub path_template: String,

    /// Coordinated start delay in milliseconds
    /// All agents will wait until this time has elapsed before starting
    /// to ensure synchronized execution
    #[serde(default = "default_start_delay_ms")]
    pub start_delay_ms: u64,

    /// Request timeout in milliseconds
    /// How long to wait for each agent RPC to complete
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,

    /// Maximum number of retries for failed agents
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Sharding strategy for distributing files across ranks
    /// Options: "interleaved", "contiguous", "hash"
    #[serde(default = "default_shard_strategy")]
    pub shard_strategy: String,

    /// Number of ranks per agent (Phase 2: enables multi-rank per agent)
    /// Default: 1 (one rank per agent, Phase 1 behavior)
    #[serde(default = "default_ranks_per_agent")]
    pub ranks_per_agent: usize,

    /// Backend types considered "shared" (don't need path prefixes)
    /// Default: ["s3", "azure", "gcs"]
    #[serde(default = "default_shared_backends")]
    pub shared_backends: Vec<String>,
}

fn default_path_template() -> String {
    "agent-{id}/".to_string()
}

fn default_start_delay_ms() -> u64 {
    1000 // 1 second
}

fn default_request_timeout_ms() -> u64 {
    300_000 // 5 minutes
}

fn default_max_retries() -> u32 {
    3
}

fn default_shard_strategy() -> String {
    "interleaved".to_string()
}

fn default_ranks_per_agent() -> usize {
    1
}

fn default_shared_backends() -> Vec<String> {
    vec![
        "s3".to_string(),
        "azure".to_string(),
        "gcs".to_string(),
    ]
}

impl Default for DistributedConfig {
    fn default() -> Self {
        DistributedConfig {
            agents: Vec::new(),
            path_template: default_path_template(),
            start_delay_ms: default_start_delay_ms(),
            request_timeout_ms: default_request_timeout_ms(),
            max_retries: default_max_retries(),
            shard_strategy: default_shard_strategy(),
            ranks_per_agent: default_ranks_per_agent(),
            shared_backends: default_shared_backends(),
        }
    }
}

impl DistributedConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.agents.is_empty() {
            anyhow::bail!("At least one agent must be specified");
        }

        for agent in &self.agents {
            if !agent.contains(':') {
                anyhow::bail!("Agent address must be in 'host:port' format: {}", agent);
            }
        }

        if self.start_delay_ms == 0 {
            anyhow::bail!("start_delay_ms must be > 0");
        }

        if self.request_timeout_ms < 1000 {
            anyhow::bail!("request_timeout_ms must be >= 1000 (1 second)");
        }

        Ok(())
    }

    /// Generate agent IDs from agent addresses
    /// For "host1:50051" → "host1:50051", or custom ID format
    pub fn agent_ids(&self) -> Vec<String> {
        self.agents
            .iter()
            .enumerate()
            .map(|(i, _addr)| format!("agent-{}", i))
            .collect()
    }

    /// Check if a backend type is considered shared storage
    pub fn is_shared_backend(&self, backend: &str) -> bool {
        self.shared_backends.contains(&backend.to_string())
    }

    /// Parse from YAML string
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        serde_yaml::from_str(yaml_str).with_context(|| "Failed to parse distributed config")
    }

    /// Parse from JSON string
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str).with_context(|| "Failed to parse distributed config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DistributedConfig::default();
        assert_eq!(config.path_template, "agent-{id}/");
        assert_eq!(config.start_delay_ms, 1000);
        assert_eq!(config.request_timeout_ms, 300_000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.shared_backends.len(), 3);
    }

    #[test]
    fn test_validate_empty_agents() {
        let config = DistributedConfig::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_agent_format() {
        let mut config = DistributedConfig::default();
        config.agents = vec!["localhost".to_string()];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_config() {
        let mut config = DistributedConfig::default();
        config.agents = vec![
            "host1:50051".to_string(),
            "host2:50051".to_string(),
        ];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_agent_ids() {
        let mut config = DistributedConfig::default();
        config.agents = vec![
            "host1:50051".to_string(),
            "host2:50051".to_string(),
            "host3:50051".to_string(),
        ];
        
        let ids = config.agent_ids();
        assert_eq!(ids, vec!["agent-0", "agent-1", "agent-2"]);
    }

    #[test]
    fn test_is_shared_backend() {
        let config = DistributedConfig::default();
        assert!(config.is_shared_backend("s3"));
        assert!(config.is_shared_backend("azure"));
        assert!(config.is_shared_backend("gcs"));
        assert!(!config.is_shared_backend("file"));
        assert!(!config.is_shared_backend("directio"));
    }

    #[test]
    fn test_parse_yaml() {
        let yaml = r#"
agents:
  - "host1:50051"
  - "host2:50051"
path_template: "run-{id}/"
start_delay_ms: 2000
request_timeout_ms: 600000
max_retries: 5
shared_backends:
  - "s3"
  - "azure"
"#;

        let config = DistributedConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.path_template, "run-{id}/");
        assert_eq!(config.start_delay_ms, 2000);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_parse_yaml_with_defaults() {
        let yaml = r#"
agents:
  - "host1:50051"
"#;

        let config = DistributedConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.path_template, "agent-{id}/");
        assert_eq!(config.start_delay_ms, 1000);
    }
}
