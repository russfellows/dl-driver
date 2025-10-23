//! Results directory management for dl-driver distributed workloads
//!
//! Automatically creates structured output directories containing:
//! - TSV metrics results with histogram data
//! - Console output logs
//! - Configuration file copy
//! - Run metadata (JSON)
//! - Per-agent subdirectories with individual results
//!
//! Directory format: dlio-{YYYYMMDD}-{HHMM}-{test_name}/

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Metadata about a distributed workload run
#[derive(Debug, Serialize, Deserialize)]
pub struct RunMetadata {
    pub version: String,
    pub test_name: String,
    pub config_path: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_secs: Option<f64>,
    pub command_line: Vec<String>,
    pub hostname: String,
    pub distributed: bool,
    pub agents: Option<Vec<String>>,
    pub total_agents: usize,
    pub successful_agents: Option<usize>,
}

impl RunMetadata {
    pub fn new(test_name: String, config_path: String, num_agents: usize) -> Self {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let start_time = Local::now().to_rfc3339();
        let hostname = hostname::get()
            .unwrap_or_else(|_| "unknown".into())
            .to_string_lossy()
            .to_string();
        let command_line = std::env::args().collect();

        Self {
            version,
            test_name,
            config_path,
            start_time,
            end_time: None,
            duration_secs: None,
            command_line,
            hostname,
            distributed: num_agents > 1,
            agents: None,
            total_agents: num_agents,
            successful_agents: None,
        }
    }

    pub fn finalize(&mut self, duration_secs: f64, successful_agents: usize) {
        self.end_time = Some(Local::now().to_rfc3339());
        self.duration_secs = Some(duration_secs);
        self.successful_agents = Some(successful_agents);
    }
}

/// Results directory manager for distributed dl-driver workloads
pub struct ResultsDir {
    path: PathBuf,
    metadata: RunMetadata,
    console_log: Option<fs::File>,
}

impl ResultsDir {
    /// Create a new results directory with the standard naming convention
    /// 
    /// # Arguments
    /// * `config_path` - Path to the DLIO config file (used for default test name)
    /// * `custom_name` - Optional custom name to use instead of config filename
    /// * `base_dir` - Optional base directory (defaults to current directory)
    /// * `num_agents` - Number of agents in distributed run
    pub fn create(
        config_path: &Path,
        custom_name: Option<&str>,
        base_dir: Option<&Path>,
        num_agents: usize,
    ) -> Result<Self> {
        // Extract test name from config filename or use custom name
        let test_name = if let Some(name) = custom_name {
            name.to_string()
        } else {
            config_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("dlio_workload")
                .to_string()
        };

        // Generate directory name: dlio-{YYYYMMDD}-{HHMM}-{test_name}
        let now = Local::now();
        let dir_name = format!(
            "dlio-{}-{}",
            now.format("%Y%m%d-%H%M"),
            test_name
        );

        // Determine base directory
        let base = base_dir.unwrap_or_else(|| Path::new("."));
        let dir_path = base.join(&dir_name);

        // Create directory
        fs::create_dir_all(&dir_path)
            .with_context(|| format!("Failed to create results directory: {}", dir_path.display()))?;

        // Copy config file
        let config_dest = dir_path.join("config.yaml");
        fs::copy(config_path, &config_dest)
            .with_context(|| "Failed to copy config to results directory")?;

        // Create metadata
        let metadata = RunMetadata::new(
            test_name,
            config_path.to_string_lossy().to_string(),
            num_agents,
        );

        // Create console log file
        let console_log_path = dir_path.join("console.log");
        let console_log = fs::File::create(&console_log_path)
            .with_context(|| "Failed to create console.log")?;

        tracing::info!("Created results directory: {}", dir_path.display());

        Ok(Self {
            path: dir_path,
            metadata,
            console_log: Some(console_log),
        })
    }

    /// Get the path to the results directory
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the path for the storage TSV results file
    pub fn storage_tsv_path(&self) -> PathBuf {
        self.path.join("storage_results.tsv")
    }

    /// Get the path for the AI/ML TSV results file
    pub fn aiml_tsv_path(&self) -> PathBuf {
        self.path.join("aiml_results.tsv")
    }

    /// Get the path for console log
    pub fn console_log_path(&self) -> PathBuf {
        self.path.join("console.log")
    }

    /// Write a line to the console log
    pub fn write_console(&mut self, line: &str) -> Result<()> {
        if let Some(ref mut log) = self.console_log {
            writeln!(log, "{}", line)
                .with_context(|| "Failed to write to console.log")?;
        }
        Ok(())
    }

    /// Write metadata to metadata.json
    pub fn write_metadata(&self) -> Result<()> {
        let metadata_path = self.path.join("metadata.json");
        let json = serde_json::to_string_pretty(&self.metadata)
            .with_context(|| "Failed to serialize metadata")?;
        fs::write(&metadata_path, json)
            .with_context(|| "Failed to write metadata.json")?;
        Ok(())
    }

    /// Finalize the results directory (write final metadata)
    pub fn finalize(&mut self, duration_secs: f64, successful_agents: usize) -> Result<()> {
        self.metadata.finalize(duration_secs, successful_agents);
        self.write_metadata()?;
        
        // Flush and close console log
        if let Some(mut log) = self.console_log.take() {
            log.flush()?;
        }

        tracing::info!("Results saved to: {}", self.path.display());
        Ok(())
    }

    /// Create agents subdirectory for distributed runs
    pub fn create_agents_dir(&mut self) -> Result<PathBuf> {
        self.metadata.distributed = true;
        let agents_dir = self.path.join("agents");
        fs::create_dir_all(&agents_dir)
            .with_context(|| "Failed to create agents subdirectory")?;
        Ok(agents_dir)
    }

    /// Add an agent to the metadata
    pub fn add_agent(&mut self, agent_name: String) {
        if self.metadata.agents.is_none() {
            self.metadata.agents = Some(Vec::new());
        }
        if let Some(ref mut agents) = self.metadata.agents {
            agents.push(agent_name);
        }
    }

    /// Write per-agent results to agents/ subdirectory
    pub fn write_agent_results(
        &self,
        agents_dir: &Path,
        agent_id: &str,
        storage_tsv: &str,
        aiml_tsv: &str,
        metadata_json: &str,
    ) -> Result<()> {
        // Create subdirectory for this agent
        let agent_dir = agents_dir.join(agent_id);
        fs::create_dir_all(&agent_dir)
            .with_context(|| format!("Failed to create agent directory: {}", agent_dir.display()))?;
        
        // Write storage results TSV
        if !storage_tsv.is_empty() {
            let tsv_path = agent_dir.join("storage_results.tsv");
            fs::write(&tsv_path, storage_tsv)
                .with_context(|| format!("Failed to write agent storage TSV: {}", tsv_path.display()))?;
        }
        
        // Write AI/ML results TSV
        if !aiml_tsv.is_empty() {
            let tsv_path = agent_dir.join("aiml_results.tsv");
            fs::write(&tsv_path, aiml_tsv)
                .with_context(|| format!("Failed to write agent AI/ML TSV: {}", tsv_path.display()))?;
        }
        
        // Write metadata.json
        if !metadata_json.is_empty() {
            let metadata_path = agent_dir.join("metadata.json");
            fs::write(&metadata_path, metadata_json)
                .with_context(|| format!("Failed to write agent metadata: {}", metadata_path.display()))?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_results_dir_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");
        fs::write(&config_path, "# test config").unwrap();

        let results_dir = ResultsDir::create(&config_path, None, Some(temp_dir.path()), 2).unwrap();

        assert!(results_dir.path().exists());
        assert!(results_dir.path().join("config.yaml").exists());
        assert!(results_dir.path().join("console.log").exists());
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = RunMetadata::new("test".to_string(), "config.yaml".to_string(), 2);
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"version\""));
        assert!(json.contains("\"test_name\":\"test\""));
        assert!(json.contains("\"total_agents\":2"));
    }

    #[test]
    fn test_agents_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");
        fs::write(&config_path, "# test config").unwrap();

        let mut results_dir = ResultsDir::create(&config_path, None, Some(temp_dir.path()), 3).unwrap();
        let agents_dir = results_dir.create_agents_dir().unwrap();

        assert!(agents_dir.exists());
        assert!(results_dir.metadata.distributed);
    }

    #[test]
    fn test_write_agent_results() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");
        fs::write(&config_path, "# test config").unwrap();

        let mut results_dir = ResultsDir::create(&config_path, None, Some(temp_dir.path()), 2).unwrap();
        let agents_dir = results_dir.create_agents_dir().unwrap();

        results_dir.write_agent_results(
            &agents_dir,
            "agent-0",
            "storage\tdata\n",
            "aiml\tdata\n",
            "{\"agent_id\": \"agent-0\"}",
        ).unwrap();

        let agent_dir = agents_dir.join("agent-0");
        assert!(agent_dir.exists());
        assert!(agent_dir.join("storage_results.tsv").exists());
        assert!(agent_dir.join("aiml_results.tsv").exists());
        assert!(agent_dir.join("metadata.json").exists());
    }
}
