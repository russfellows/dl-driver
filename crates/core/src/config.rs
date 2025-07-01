use anyhow::Result;
use serde::Deserialize;
use std::{fs, path::Path};

/// For M0 we just keep the raw YAML ⇢ serde_yaml::Value.
/// Later milestones will map this into a strongly-typed struct.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub raw: serde_yaml::Value,
}

/// Load a DLIO workload YAML file.
impl Config {
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let text = fs::read_to_string(&path)?;
        let raw: serde_yaml::Value = serde_yaml::from_str(&text)?;
        Ok(Self { raw })
    }
}

