// crates/core/src/config/mod.rs
pub mod dlio_config;

pub use dlio_config::{DlioConfig, StorageBackend, Model, Workflow, Dataset, Reader, Checkpoint};

// Legacy Config type removed - use DlioConfig directly

/// Convert YAML to JSON - utility for CLI validation
pub fn yaml_to_json(yaml_str: &str) -> anyhow::Result<String> {
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse YAML: {}", e))?;
    serde_json::to_string_pretty(&yaml_value)
        .map_err(|e| anyhow::anyhow!("Failed to convert to JSON: {}", e))
}