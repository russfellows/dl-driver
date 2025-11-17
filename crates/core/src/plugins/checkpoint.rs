// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/core/src/plugins/checkpoint.rs
use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::dlio_compat::{DlioConfig, CheckpointingConfig};
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

/// State loaded from a checkpoint for resuming training
#[derive(Debug, Clone)]
pub struct CheckpointState {
    pub run_id: String,
    pub step: u32,
    pub epoch: Option<u32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub checkpoint_version: String,
    pub config_snapshot: DlioConfig,
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
    cfg: CheckpointingConfig,
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
            .field("compression_enabled", &self.use_compression())
            .field("next_checkpoint_step", &self.next_checkpoint_step)
            .finish()
    }
}

impl CheckpointPlugin {
    /// Get the step interval from config
    pub fn step_interval(&self) -> u32 {
        self.cfg.steps_between_checkpoints.unwrap_or(100) as u32
    }

    /// Check if compression is enabled
    pub fn use_compression(&self) -> bool {
        // Compression not configurable in dlio_compat CheckpointingConfig
        false
    }

    /// Get compression level
    pub fn compression_level(&self) -> i32 {
        // Compression level not configurable in dlio_compat CheckpointingConfig
        3
    }

    /// Get checkpoint URI, falling back to data folder if not specified
    pub fn checkpoint_uri(&self, fallback_data_folder: &str) -> String {
        self.cfg.checkpoint_folder.clone().unwrap_or_else(|| fallback_data_folder.to_string())
    }

    /// Create a new CheckpointPlugin from DlioConfig if checkpointing is enabled
    pub async fn new(config: &DlioConfig) -> Result<Option<Self>> {
        debug!("CheckpointPlugin::new() called");
        debug!("config.checkpointing = {:?}", config.checkpointing);

        let checkpoint_cfg = match config.checkpointing.as_ref() {
            Some(cfg) => {
                debug!("Found checkpoint config: folder = {:?}", cfg.checkpoint_folder);
                if cfg.checkpoint_folder.is_some() {
                    debug!("Checkpointing is enabled!");
                    cfg
                } else {
                    debug!("Checkpointing disabled in config (enabled = false)");
                    return Ok(None);
                }
            },
            None => {
                debug!("No checkpoint config found");
                return Ok(None);
            }
        };

        let step_interval = checkpoint_cfg.steps_between_checkpoints.unwrap_or(100);
        if step_interval == 0 {
            warn!("steps_between_checkpoints is 0, checkpointing disabled");
            return Ok(None);
        }

        // Use checkpoint URI if specified, otherwise fall back to data_folder
        let raw_uri = checkpoint_cfg.checkpoint_folder.as_ref()
            .unwrap_or(&config.dataset.data_folder);
        
        // Normalize the URI to handle file:// schemes properly
        let checkpoint_uri = crate::dlio_compat::normalize_uri(raw_uri);

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
            false, // Compression not configurable in dlio_compat
            checkpoint_uri
        );

        Ok(Some(Self {
            cfg: checkpoint_cfg.clone(),
            store,
            run_id,
            config_snapshot,
            next_checkpoint_step: step_interval as u32,
            base_uri: checkpoint_uri,
        }))
    }

    /// Generate random checkpoint data to simulate model weights
    /// 
    /// Thin wrapper over s3dlio's generate_random_data() which creates non-compressible
    /// random bytes to accurately simulate storage I/O load. Uses s3dlio's optimized
    /// random data generation with minimal CPU overhead.
    fn generate_checkpoint_data(&self, size_mb: usize) -> Vec<u8> {
        let size_bytes = size_mb * 1024 * 1024;
        
        // Use s3dlio's optimized random data generation
        // dedup=1, compress=1 means NO deduplication or compression (1:1 ratio)
        s3dlio::generate_controlled_data(size_bytes, 1, 1)
    }

    /// Write checkpoint for the given step
    async fn write_checkpoint(&self, step: u32) -> Result<()> {
        debug!("write_checkpoint() started for step {}", step);
        
        // Generate checkpoint data based on configured size
        let checkpoint_size_mb = self.cfg.checkpoint_size_mb;
        let checkpoint_binary_data = self.generate_checkpoint_data(checkpoint_size_mb);
        
        // First pass: Create checkpoint with placeholder metadata to calculate size
        let mut checkpoint_data = CheckpointData {
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
                compression_enabled: self.use_compression(),
                compressed_size_bytes: None, // Will be updated in second pass if compression enabled
                uncompressed_size_bytes: checkpoint_binary_data.len(),
            },
        };

        // Serialize checkpoint metadata to JSON (small header)
        let json_metadata = serde_json::to_vec_pretty(&checkpoint_data)
            .context("Failed to serialize checkpoint metadata")?;
        
        // Calculate total size: metadata + binary checkpoint data
        let total_uncompressed_size = json_metadata.len() + checkpoint_binary_data.len();
        checkpoint_data.metadata.uncompressed_size_bytes = total_uncompressed_size;
        
        // Create final checkpoint file: [metadata_len: u32][metadata: JSON][checkpoint_data: binary]
        let mut final_data = Vec::with_capacity(4 + json_metadata.len() + checkpoint_binary_data.len());
        
        // Write metadata length prefix (4 bytes)
        final_data.extend_from_slice(&(json_metadata.len() as u32).to_le_bytes());
        
        // Write metadata (JSON)
        final_data.extend_from_slice(&json_metadata);
        
        // Write checkpoint binary data
        final_data.extend_from_slice(&checkpoint_binary_data);
        
        let _compressed_size: Option<usize> = None; // Compression disabled (random data doesn't compress)
        let final_data = Bytes::from(final_data);

        // Create checkpoint file path: {run_id}/step_{step:08}.ckpt
        let checkpoint_relative_path = format!("{}/step_{:08}.ckpt", self.run_id, step);
        
        // Construct full URI by appending relative path to base URI
        let checkpoint_full_uri = if self.base_uri.ends_with('/') {
            format!("{}{}", self.base_uri, checkpoint_relative_path)
        } else {
            format!("{}/{}", self.base_uri, checkpoint_relative_path)
        };
        
        debug!("base_uri = {}", self.base_uri);
        debug!("checkpoint_relative_path = {}", checkpoint_relative_path);
        debug!("checkpoint_full_uri = {}", checkpoint_full_uri);
        debug!("final_data.len() = {}", final_data.len());
        
        // Write to object store using full URI
        debug!("About to call store.put()...");
        let result = self.store
            .put(&checkpoint_full_uri, &final_data)
            .await;
            
        if let Err(e) = &result {
            debug!("store.put() failed: {}", e);
        } else {
            debug!("store.put() succeeded!");
        }
        
        result.with_context(|| format!("Failed to write checkpoint to {}", checkpoint_relative_path))?;

        let size_info = format!(
            " ({:.2} MB, {} bytes)",
            final_data.len() as f64 / (1024.0 * 1024.0),
            final_data.len()
        );

        info!(
            "Checkpoint written: step={}, path={}{}", 
            step, checkpoint_relative_path, size_info
        );

        Ok(())
    }

    /// Write checkpoint for the given epoch
    async fn write_epoch_checkpoint(&self, epoch: u32) -> Result<()> {
        debug!("write_epoch_checkpoint() started for epoch {}", epoch);
        
        // Generate checkpoint data based on configured size
        let checkpoint_size_mb = self.cfg.checkpoint_size_mb;
        let checkpoint_binary_data = self.generate_checkpoint_data(checkpoint_size_mb);
        
        // First pass: Create checkpoint with placeholder metadata to calculate size
        let mut checkpoint_data = CheckpointData {
            run_id: self.run_id.clone(),
            step: 0, // Step not meaningful for epoch-based checkpointing
            epoch: Some(epoch),
            timestamp: chrono::Utc::now(),
            dl_driver_version: env!("CARGO_PKG_VERSION").to_string(),
            config_snapshot: self.config_snapshot.clone(),
            metadata: CheckpointMetadata {
                total_samples_processed: 0, // TODO: Get from metrics when available
                total_bytes_read: 0,        // TODO: Get from metrics when available
                elapsed_time_secs: 0.0,     // TODO: Get from metrics when available
                compression_enabled: self.use_compression(),
                compressed_size_bytes: None, // Will be updated in second pass if compression enabled
                uncompressed_size_bytes: checkpoint_binary_data.len(),
            },
        };

        // Serialize checkpoint metadata to JSON (small header)
        let json_metadata = serde_json::to_vec_pretty(&checkpoint_data)
            .context("Failed to serialize checkpoint metadata")?;
        
        // Calculate total size: metadata + binary checkpoint data
        let total_uncompressed_size = json_metadata.len() + checkpoint_binary_data.len();
        checkpoint_data.metadata.uncompressed_size_bytes = total_uncompressed_size;
        
        // Create final checkpoint file: [metadata_len: u32][metadata: JSON][checkpoint_data: binary]
        let mut final_data = Vec::with_capacity(4 + json_metadata.len() + checkpoint_binary_data.len());
        
        // Write metadata length prefix (4 bytes)
        final_data.extend_from_slice(&(json_metadata.len() as u32).to_le_bytes());
        
        // Write metadata (JSON)
        final_data.extend_from_slice(&json_metadata);
        
        // Write checkpoint binary data
        final_data.extend_from_slice(&checkpoint_binary_data);
        
        let final_data = Bytes::from(final_data);

        // Create checkpoint file path: {run_id}/epoch_{epoch:04}.ckpt
        let checkpoint_relative_path = format!("{}/epoch_{:04}.ckpt", self.run_id, epoch);
        
        // Construct full URI by appending relative path to base URI
        let checkpoint_full_uri = if self.base_uri.ends_with('/') {
            format!("{}{}", self.base_uri, checkpoint_relative_path)
        } else {
            format!("{}/{}", self.base_uri, checkpoint_relative_path)
        };
        
        debug!("Epoch checkpoint - base_uri = {}", self.base_uri);
        debug!("Epoch checkpoint - checkpoint_relative_path = {}", checkpoint_relative_path);
        debug!("Epoch checkpoint - checkpoint_full_uri = {}", checkpoint_full_uri);
        
        // Write to object store using full URI
        self.store
            .put(&checkpoint_full_uri, &final_data)
            .await
            .with_context(|| format!("Failed to write epoch checkpoint to {}", checkpoint_relative_path))?;

        let size_info = format!(
            " ({:.2} MB, {} bytes)",
            final_data.len() as f64 / (1024.0 * 1024.0),
            final_data.len()
        );

        info!(
            "Epoch checkpoint written: epoch={}, path={}{}", 
            epoch, checkpoint_relative_path, size_info
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

    /// Load checkpoint from a specific URI
    /// 
    /// Supports both step-based and epoch-based checkpoints:
    /// - Step: checkpoint_folder/run_id/step_00000100.ckpt
    /// - Epoch: checkpoint_folder/run_id/epoch_0001.ckpt
    /// 
    /// Returns CheckpointState with restored training state
    pub async fn load_checkpoint(checkpoint_uri: &str) -> Result<CheckpointState> {
        info!("Loading checkpoint from: {}", checkpoint_uri);

        // Create object store for the checkpoint URI
        let store = store_for_uri(checkpoint_uri)
            .with_context(|| format!("Failed to create object store for checkpoint URI: {}", checkpoint_uri))?;

        // Read checkpoint file
        let data = store.get(checkpoint_uri)
            .await
            .with_context(|| format!("Failed to read checkpoint file from: {}", checkpoint_uri))?;

        // Parse checkpoint format: [4-byte length][JSON metadata][binary data]
        if data.len() < 4 {
            anyhow::bail!("Checkpoint file too small: {} bytes", data.len());
        }

        // Read metadata length (first 4 bytes, little-endian)
        let metadata_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        
        if data.len() < 4 + metadata_len {
            anyhow::bail!(
                "Checkpoint file truncated: expected {} bytes of metadata, file has {} total bytes",
                metadata_len, data.len()
            );
        }

        // Extract JSON metadata (bytes 4 to 4+metadata_len)
        let json_data = &data[4..4 + metadata_len];

        // Deserialize checkpoint data
        let checkpoint_data: CheckpointData = serde_json::from_slice(json_data)
            .context("Failed to deserialize checkpoint JSON")?;

        // Parse config snapshot
        let config_snapshot: DlioConfig = serde_json::from_str(&checkpoint_data.config_snapshot)
            .context("Failed to deserialize config snapshot from checkpoint")?;

        // Validate checkpoint version compatibility
        let current_version = env!("CARGO_PKG_VERSION");
        if checkpoint_data.dl_driver_version != current_version {
            warn!(
                "Checkpoint version mismatch: checkpoint={}, current={}",
                checkpoint_data.dl_driver_version, current_version
            );
            // Continue anyway - minor version differences should be compatible
        }

        info!(
            "Checkpoint loaded: run_id={}, step={}, epoch={:?}, timestamp={}",
            checkpoint_data.run_id,
            checkpoint_data.step,
            checkpoint_data.epoch,
            checkpoint_data.timestamp
        );

        Ok(CheckpointState {
            run_id: checkpoint_data.run_id,
            step: checkpoint_data.step,
            epoch: checkpoint_data.epoch,
            timestamp: checkpoint_data.timestamp,
            checkpoint_version: checkpoint_data.dl_driver_version,
            config_snapshot,
            metadata: checkpoint_data.metadata,
        })
    }

    /// Restore plugin state from loaded checkpoint
    /// 
    /// Used when resuming training to restore the checkpoint counter
    pub fn restore_from_checkpoint(&mut self, state: &CheckpointState) {
        info!(
            "Restoring CheckpointPlugin state: step={}, epoch={:?}",
            state.step, state.epoch
        );
        
        // Update next checkpoint step to be after the resumed step
        let interval = self.step_interval();
        self.next_checkpoint_step = ((state.step / interval) + 1) * interval;
        
        info!(
            "Next checkpoint will be at step: {}",
            self.next_checkpoint_step
        );
    }
}

#[async_trait]
impl Plugin for CheckpointPlugin {
    async fn initialize(&mut self, _cfg: &DlioConfig) -> Result<()> {
        info!("CheckpointPlugin initialized for run_id: {}", self.run_id);
        Ok(())
    }

    async fn after_step(&mut self, step: u32) -> Result<()> {
        debug!("CheckpointPlugin::after_step() called with step = {}", step);
        debug!("should_checkpoint({}) = {}", step, self.should_checkpoint(step));
        debug!("next_checkpoint_step = {}", self.next_checkpoint_step);
        
        if self.should_checkpoint(step) {
            debug!("Writing checkpoint at step {}", step);
            self.write_checkpoint(step).await?;
            self.update_next_checkpoint(step);
        }
        Ok(())
    }

    async fn after_epoch(&mut self, epoch: u32) -> Result<()> {
        // Check if epoch-based checkpointing is configured
        let checkpoint_after = self.cfg.checkpoint_after_epoch.map(|e| e as u32).unwrap_or(u32::MAX);
        let epochs_between = self.cfg.epochs_between_checkpoints.map(|e| e as u32).unwrap_or(u32::MAX);
        
        // Don't checkpoint if we haven't reached checkpoint_after_epoch yet
        if epoch < checkpoint_after {
            debug!("Epoch {}: Before checkpoint_after_epoch ({})", epoch, checkpoint_after);
            return Ok(());
        }
        
        // Check if we should checkpoint this epoch based on interval
        let epochs_since_start = epoch - checkpoint_after;
        if epochs_since_start % epochs_between == 0 {
            info!("Epoch-based checkpoint triggered at epoch {}", epoch);
            
            // Write checkpoint with epoch information
            self.write_epoch_checkpoint(epoch).await?;
        }
        
        Ok(())
    }

    async fn finalize(&mut self) -> Result<()> {
        info!("CheckpointPlugin finalized for run_id: {}", self.run_id);
        Ok(())
    }
    
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlio_compat::{DlioConfig, DatasetConfig, ReaderConfig, CheckpointingConfig, TrainConfig};
    use tempfile::tempdir;
    use std::fs;

    /// Helper to create a minimal test config
    fn create_test_config(checkpoint_folder: &str, steps_between: usize) -> DlioConfig {
        // Ensure checkpoint_folder has file:// scheme
        let checkpoint_uri = if checkpoint_folder.starts_with("file://") {
            checkpoint_folder.to_string()
        } else {
            format!("file://{}", checkpoint_folder)
        };
        
        DlioConfig {
            model: None,
            framework: None,
            workflow: None,
            dataset: DatasetConfig {
                data_folder: "file:///tmp/test_data".to_string(),
                format: Some("npz".to_string()),
                num_files_train: Some(10),
                num_files_eval: None,
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(100),
                compression: None,
                num_subfolders_train: None,
                directory_tree: None,
                endpoint_uris: None,
                load_balance_strategy: "round_robin".to_string(),
            },
            reader: ReaderConfig {
                data_loader: None,
                batch_size: Some(32),
                prefetch: Some(2),
                shuffle: Some(false),
                read_threads: Some(4),
                compute_threads: Some(4),
                transfer_size: None,
                file_access_type: None,
                seed: Some(42),
            },
            train: Some(TrainConfig {
                epochs: Some(3),
                computation_time: Some(0.1),
                computation_time_stdev: None,
                total_training_steps: None,
            }),
            metric: None,
            checkpointing: Some(CheckpointingConfig {
                checkpoint_folder: Some(checkpoint_uri),
                checkpoint_after_epoch: None,
                epochs_between_checkpoints: None,
                steps_between_checkpoints: Some(steps_between),
                endpoint_uris: None,
                load_balance_strategy: "round_robin".to_string(),
            }),
            profiling: None,
            resume: None,
            pytorch_config: None,
            tensorflow_config: None,
            jax_config: None,
            framework_profiles: None,
        }
    }

    #[tokio::test]
    async fn test_checkpoint_save_and_load_basic() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 100);
        
        // Create plugin and write a checkpoint
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_checkpoint(100).await.unwrap();
        
        // Construct the actual checkpoint path (includes run_id subdirectory)
        let checkpoint_path = temp_dir.path().join(format!("{}/step_{:08}.ckpt", plugin.run_id, 100));
        let checkpoint_uri = format!("file://{}", checkpoint_path.display());
        
        // Load the checkpoint
        let loaded_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await.unwrap();
        
        // Verify loaded state
        assert_eq!(loaded_state.step, 100);
        assert_eq!(loaded_state.epoch, None);
        assert_eq!(loaded_state.run_id, plugin.run_id);
        assert_eq!(loaded_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_checkpoint_epoch_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 100);
        
        // Create plugin and write an epoch checkpoint
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_epoch_checkpoint(2).await.unwrap();
        
        // Construct epoch checkpoint path
        let epoch_checkpoint_path = temp_dir.path().join(format!("{}/epoch_{:04}.ckpt", plugin.run_id, 2));
        let epoch_checkpoint_uri = format!("file://{}", epoch_checkpoint_path.display());
        
        // Load the checkpoint
        let loaded_state = CheckpointPlugin::load_checkpoint(&epoch_checkpoint_uri).await.unwrap();
        
        // Verify loaded state
        assert_eq!(loaded_state.epoch, Some(2));
        assert_eq!(loaded_state.run_id, plugin.run_id);
    }

    #[tokio::test]
    async fn test_checkpoint_restore_updates_next_step() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 50);
        
        // Create plugin and write checkpoint at step 125
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_checkpoint(125).await.unwrap();
        
        // Load checkpoint (construct path with run_id)
        let checkpoint_path = temp_dir.path().join(format!("{}/step_{:08}.ckpt", plugin.run_id, 125));
        let checkpoint_uri = format!("file://{}", checkpoint_path.display());
        let loaded_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await.unwrap();
        
        // Create new plugin and restore from checkpoint
        let mut new_plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        assert_eq!(new_plugin.next_checkpoint_step, 50); // Initial value
        
        new_plugin.restore_from_checkpoint(&loaded_state);
        
        // Verify next_checkpoint_step is updated correctly
        // Step 125 with interval 50: next should be 150 (125/50 = 2, (2+1)*50 = 150)
        assert_eq!(new_plugin.next_checkpoint_step, 150);
    }

    #[tokio::test]
    async fn test_checkpoint_load_nonexistent_fails() {
        let result = CheckpointPlugin::load_checkpoint("file:///nonexistent/checkpoint.ckpt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_load_invalid_json_fails() {
        let temp_dir = tempdir().unwrap();
        let bad_checkpoint = temp_dir.path().join("bad.ckpt");
        fs::write(&bad_checkpoint, b"not valid json").unwrap();
        
        let checkpoint_uri = format!("file://{}", bad_checkpoint.display());
        let result = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_version_mismatch_warning() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 100);
        
        // Create and write checkpoint
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_checkpoint(100).await.unwrap();
        
        // Manually modify checkpoint to have different version (construct path with run_id)
        let checkpoint_path = temp_dir.path().join(format!("{}/step_{:08}.ckpt", plugin.run_id, 100));
        let checkpoint_data = fs::read(&checkpoint_path).unwrap();
        let mut checkpoint_json: serde_json::Value = serde_json::from_slice(&checkpoint_data).unwrap();
        checkpoint_json["checkpoint_version"] = serde_json::Value::String("99.99.99".to_string());
        fs::write(&checkpoint_path, serde_json::to_vec(&checkpoint_json).unwrap()).unwrap();
        
        // Load should succeed but log warning
        let checkpoint_uri = format!("file://{}", checkpoint_path.display());
        let result = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await;
        assert!(result.is_ok()); // Should still succeed despite version mismatch
    }

    #[tokio::test]
    async fn test_checkpoint_config_snapshot_preserved() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 100);
        
        // Create plugin and write checkpoint
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_checkpoint(100).await.unwrap();
        
        // Load checkpoint (construct path with run_id)
        let checkpoint_path = temp_dir.path().join(format!("{}/step_{:08}.ckpt", plugin.run_id, 100));
        let checkpoint_uri = format!("file://{}", checkpoint_path.display());
        let loaded_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await.unwrap();
        
        // Verify config snapshot matches original
        assert_eq!(loaded_state.config_snapshot.dataset.data_folder, config.dataset.data_folder);
        assert_eq!(loaded_state.config_snapshot.reader.batch_size, config.reader.batch_size);
        assert_eq!(loaded_state.config_snapshot.checkpointing.as_ref().unwrap().steps_between_checkpoints,
                   config.checkpointing.as_ref().unwrap().steps_between_checkpoints);
    }

    #[tokio::test]
    async fn test_checkpoint_metadata_preserved() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap(), 100);
        
        // Create plugin and write checkpoint
        let plugin = CheckpointPlugin::new(&config).await.unwrap().unwrap();
        plugin.write_checkpoint(100).await.unwrap();
        
        // Load checkpoint (construct path with run_id)
        let checkpoint_path = temp_dir.path().join(format!("{}/step_{:08}.ckpt", plugin.run_id, 100));
        let checkpoint_uri = format!("file://{}", checkpoint_path.display());
        let loaded_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await.unwrap();
        
        // Verify metadata exists and has correct values
        assert!(loaded_state.metadata.uncompressed_size_bytes > 0, 
                "Expected uncompressed_size_bytes > 0, got {}", 
                loaded_state.metadata.uncompressed_size_bytes);
        assert_eq!(loaded_state.metadata.compression_enabled, false); // Default is disabled
        assert!(loaded_state.metadata.compressed_size_bytes.is_none());
    }
    
    // Tests temporarily disabled during config unification
    // TODO: Update tests to use dlio_compat::DlioConfig structure
    /*
    use super::*;
    use crate::dlio_compat::{DlioConfig, DatasetConfig, ReaderConfig};
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
    */
}
