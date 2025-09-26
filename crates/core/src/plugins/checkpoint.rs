// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/core/src/plugins/checkpoint.rs
use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::{DlioConfig, Checkpoint as CheckpointConfig};
use super::Plugin;
use s3dlio::object_store::{store_for_uri, ObjectStore};

/// Checkpoint data structure that gets serialized and written
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub run_id: String,
    pub step: u32,
    pub epoch: Option<u32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub dl_driver_version: String,
    pub config_snapshot: String,  // JSON representation of config at checkpoint time
    pub metadata: CheckpointMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub total_samples_processed: u64,
    pub total_bytes_read: u64,
    pub elapsed_time_secs: f64,
    pub compression_enabled: bool,
    pub compressed_size_bytes: Option<usize>,
    pub uncompressed_size_bytes: usize,
}

/// CheckpointPlugin handles writing checkpoint artifacts to any supported backend
/// Supports multi-backend storage via s3dlio ObjectStore and optional zstd compression
pub struct CheckpointPlugin {
    cfg: CheckpointConfig,
    store: Box<dyn ObjectStore>,
    run_id: String,
    config_snapshot: String,
    next_checkpoint_step: u32,
    base_uri: String,
}

impl std::fmt::Debug for CheckpointPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointPlugin")
            .field("run_id", &self.run_id)
            .field("step_interval", &self.step_interval())
            .field("compression_enabled", &self.compression_enabled())
            .field("next_checkpoint_step", &self.next_checkpoint_step)
            .finish()
    }
}

impl CheckpointPlugin {
    /// Get the step interval from config
    pub fn step_interval(&self) -> u32 {
        self.cfg.steps_between_checkpoints.unwrap_or(100)
    }

    /// Check if compression is enabled
    pub fn compression_enabled(&self) -> bool {
        self.cfg.compression.as_deref() == Some("zstd")
    }

    /// Get compression level
    pub fn compression_level(&self) -> i32 {
        self.cfg.compression_level.unwrap_or(3)
    }

    /// Get checkpoint URI, falling back to data folder if not specified
    pub fn checkpoint_uri<'a>(&'a self, fallback_data_folder: &'a str) -> &'a str {
        self.cfg.uri.as_deref().unwrap_or(fallback_data_folder)
    }

    /// Create a new CheckpointPlugin from DlioConfig if checkpointing is enabled
    pub async fn new(config: &DlioConfig) -> Result<Option<Self>> {
        println!("DEBUG: CheckpointPlugin::new() called");
        println!("DEBUG: config.checkpoint = {:?}", config.checkpoint);
        
        let checkpoint_cfg = match config.checkpoint.as_ref() {
            Some(cfg) => {
                println!("DEBUG: Found checkpoint config: enabled = {:?}", cfg.enabled);
                if cfg.enabled.unwrap_or(false) {
                    println!("DEBUG: Checkpointing is enabled!");
                    cfg
                } else {
                    println!("DEBUG: Checkpointing disabled in config (enabled = false)");
                    debug!("Checkpointing not enabled in config");
                    return Ok(None);
                }
            },
            None => {
                println!("DEBUG: No checkpoint config found");
                debug!("Checkpointing not enabled in config");
                return Ok(None);
            }
        };

        let step_interval = checkpoint_cfg.steps_between_checkpoints.unwrap_or(100);
        if step_interval == 0 {
            warn!("steps_between_checkpoints is 0, checkpointing disabled");
            return Ok(None);
        }

        // Use checkpoint URI if specified, otherwise fall back to data_folder
        let raw_uri = checkpoint_cfg.uri.as_ref()
            .unwrap_or(&config.dataset.data_folder);
        
        // Normalize the URI to handle file:// schemes properly
        let checkpoint_uri = crate::config::dlio_config::normalize_uri(raw_uri);

        info!("Initializing CheckpointPlugin with URI: {}", checkpoint_uri);
        
        // Create object store for the checkpoint URI
        let store = store_for_uri(&checkpoint_uri)
            .with_context(|| format!("Failed to create object store for URI: {}", checkpoint_uri))?;

        let run_id = Uuid::new_v4().to_string();

        // Serialize config for checkpoint metadata  
        let config_snapshot = serde_json::to_string_pretty(config)
            .context("Failed to serialize config for checkpoint metadata")?;

        info!(
            "CheckpointPlugin initialized: run_id={}, interval={}, compression={}, uri={}", 
            run_id, step_interval, 
            checkpoint_cfg.compression.as_deref() == Some("zstd"),
            checkpoint_uri
        );

        Ok(Some(Self {
            cfg: checkpoint_cfg.clone(),
            store,
            run_id,
            config_snapshot,
            next_checkpoint_step: step_interval,
            base_uri: checkpoint_uri,
        }))
    }

    /// Write checkpoint for the given step
    async fn write_checkpoint(&self, step: u32) -> Result<()> {
        println!("DEBUG: write_checkpoint() started for step {}", step);
        
        let checkpoint_data = CheckpointData {
            run_id: self.run_id.clone(),
            step,
            epoch: None, // TODO: Add epoch tracking when available
            timestamp: chrono::Utc::now(),
            dl_driver_version: env!("CARGO_PKG_VERSION").to_string(),
            config_snapshot: self.config_snapshot.clone(),
            metadata: CheckpointMetadata {
                total_samples_processed: 0, // TODO: Get from metrics when available
                total_bytes_read: 0,        // TODO: Get from metrics when available
                elapsed_time_secs: 0.0,     // TODO: Get from metrics when available
                compression_enabled: self.compression_enabled(),
                compressed_size_bytes: None,
                uncompressed_size_bytes: 0,
            },
        };

        // Serialize checkpoint data to JSON
        let json_data = serde_json::to_vec_pretty(&checkpoint_data)
            .context("Failed to serialize checkpoint data")?;

        let uncompressed_size = json_data.len();
        
        // Apply compression if enabled
        let (final_data, compressed_size) = if self.compression_enabled() {
            let compressed = zstd::encode_all(json_data.as_slice(), self.compression_level())
                .context("Failed to compress checkpoint data with zstd")?;
            let size = compressed.len();
            (Bytes::from(compressed), Some(size))
        } else {
            (Bytes::from(json_data), None)
        };

        // Create checkpoint file path: {run_id}/step_{step:08}.ckpt
        let checkpoint_relative_path = format!("{}/step_{:08}.ckpt", self.run_id, step);
        
        // Construct full URI by appending relative path to base URI
        let checkpoint_full_uri = if self.base_uri.ends_with('/') {
            format!("{}{}", self.base_uri, checkpoint_relative_path)
        } else {
            format!("{}/{}", self.base_uri, checkpoint_relative_path)
        };
        
        println!("DEBUG: base_uri = {}", self.base_uri);
        println!("DEBUG: checkpoint_relative_path = {}", checkpoint_relative_path);
        println!("DEBUG: checkpoint_full_uri = {}", checkpoint_full_uri);
        println!("DEBUG: final_data.len() = {}", final_data.len());
        
        // Write to object store using full URI
        println!("DEBUG: About to call store.put()...");
        let result = self.store
            .put(&checkpoint_full_uri, &final_data)
            .await;
            
        match &result {
            Ok(_) => println!("DEBUG: store.put() succeeded!"),
            Err(e) => println!("DEBUG: store.put() failed: {}", e),
        }
        
        result.with_context(|| format!("Failed to write checkpoint to {}", checkpoint_relative_path))?;

        let compression_info = if let Some(compressed) = compressed_size {
            format!(" (compressed {} -> {} bytes, {:.1}% reduction)", 
                uncompressed_size, compressed,
                (1.0 - (compressed as f64 / uncompressed_size as f64)) * 100.0)
        } else {
            format!(" ({} bytes uncompressed)", uncompressed_size)
        };

        info!(
            "Checkpoint written: step={}, path={}{}", 
            step, checkpoint_relative_path, compression_info
        );

        Ok(())
    }

    /// Check if a checkpoint should be written at this step
    fn should_checkpoint(&self, step: u32) -> bool {
        step >= self.next_checkpoint_step
    }

    /// Update next checkpoint step after writing
    fn update_next_checkpoint(&mut self, step: u32) {
        // Calculate next checkpoint step based on interval
        let interval = self.step_interval();
        self.next_checkpoint_step = ((step / interval) + 1) * interval;
    }
}

#[async_trait]
impl Plugin for CheckpointPlugin {
    async fn initialize(&mut self, _cfg: &DlioConfig) -> Result<()> {
        info!("CheckpointPlugin initialized for run_id: {}", self.run_id);
        Ok(())
    }

    async fn after_step(&mut self, step: u32) -> Result<()> {
        println!("DEBUG: CheckpointPlugin::after_step() called with step = {}", step);
        println!("DEBUG: should_checkpoint({}) = {}", step, self.should_checkpoint(step));
        println!("DEBUG: next_checkpoint_step = {}", self.next_checkpoint_step);
        
        if self.should_checkpoint(step) {
            println!("DEBUG: Writing checkpoint at step {}", step);
            debug!("Writing checkpoint at step {}", step);
            self.write_checkpoint(step).await?;
            self.update_next_checkpoint(step);
        }
        Ok(())
    }

    async fn after_epoch(&mut self, epoch: u32) -> Result<()> {
        // Optionally write checkpoint at end of each epoch
        debug!("Epoch {} completed", epoch);
        Ok(())
    }

    async fn finalize(&mut self) -> Result<()> {
        info!("CheckpointPlugin finalized for run_id: {}", self.run_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DlioConfig, Dataset, Reader};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_checkpoint_plugin_creation() {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let mut config = DlioConfig {
            model: None,
            framework: None,
            workflow: None,
            dataset: Dataset {
                data_folder: format!("file://{}", temp_path),
                format: "npz".to_string(),
                num_files_train: Some(10),
                num_files_eval: None,
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(100),
                compression: None,
            },
            reader: Reader {
                batch_size: Some(32),
                prefetch: Some(2),
                shuffle: Some(true),
                read_threads: Some(4),
                compute_threads: Some(4),
                drop_last: Some(true),
                seed: Some(42),
                data_loader: None,
            },
            checkpoint: None,
        };

        // Test disabled checkpointing
        let plugin = CheckpointPlugin::new(&config).await.unwrap();
        assert!(plugin.is_none());

        // Test enabled checkpointing
        config.checkpoint = Some(CheckpointConfig {
            enabled: Some(true),
            uri: None, // Use data_folder
            steps_between_checkpoints: Some(50),
            compression: Some("zstd".to_string()),
            compression_level: Some(5),
        });

        let plugin = CheckpointPlugin::new(&config).await.unwrap();
        assert!(plugin.is_some());

        let plugin = plugin.unwrap();
        assert_eq!(plugin.step_interval(), 50);
        assert!(plugin.compression_enabled());
        assert!(!plugin.run_id.is_empty());
    }

    #[tokio::test]
    async fn test_checkpoint_interval_logic() {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let config = DlioConfig {
            model: None,
            framework: None,
            workflow: None,
            dataset: Dataset {
                data_folder: format!("file://{}", temp_path),
                format: "npz".to_string(),
                num_files_train: Some(10),
                num_files_eval: None,
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(100),
                compression: None,
            },
            reader: Reader {
                batch_size: Some(32),
                prefetch: Some(2),
                shuffle: Some(true),
                read_threads: Some(4),
                compute_threads: Some(4),
                drop_last: Some(true),
                seed: Some(42),
                data_loader: None,
            },
            checkpoint: Some(CheckpointConfig {
                enabled: Some(true),
                uri: None,
                steps_between_checkpoints: Some(10),
                compression: None,
                compression_level: None,
            }),
        };

        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        
        // Test checkpoint decision logic
        assert!(!plugin.should_checkpoint(5));   // Before first checkpoint
        assert!(plugin.should_checkpoint(10));   // At first checkpoint
        assert!(plugin.should_checkpoint(15));   // After first checkpoint
        
        // Test next checkpoint calculation
        let mut plugin = plugin;
        assert_eq!(plugin.next_checkpoint_step, 10);
        plugin.update_next_checkpoint(10);
        assert_eq!(plugin.next_checkpoint_step, 20);
        plugin.update_next_checkpoint(15);
        assert_eq!(plugin.next_checkpoint_step, 20);
    }
}