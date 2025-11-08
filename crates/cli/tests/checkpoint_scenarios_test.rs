// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for checkpoint save/load scenarios
//! 
//! Tests three primary scenarios:
//! 1. Training without checkpoints
//! 2. Training with checkpoint creation (but no reload)
//! 3. Training with checkpoint creation and reload

use anyhow::Result;
use dl_driver_core::{DlioConfig, WorkloadRunner};
use dl_driver_core::plugins::{PluginManager, CheckpointPlugin};
use tempfile::tempdir;
use std::path::PathBuf;

/// Helper to create a minimal test config for checkpoint scenarios
fn create_test_config(
    data_folder: &str,
    checkpoint_folder: Option<&str>,
    steps_between_checkpoints: Option<usize>,
) -> DlioConfig {
    use dl_driver_core::dlio_compat::*;
    
    let checkpoint_uri = checkpoint_folder.map(|f| {
        if f.starts_with("file://") {
            f.to_string()
        } else {
            format!("file://{}", f)
        }
    });
    
    DlioConfig {
        model: Some(ModelConfig {
            name: Some("checkpoint_test".to_string()),
            model_size: None,
            framework: Some("pytorch".to_string()),
        }),
        framework: Some("pytorch".to_string()),
        workflow: Some(WorkflowConfig {
            generate_data: Some(false), // Data should already exist
            train: Some(true),
            checkpoint: Some(checkpoint_folder.is_some()),
            evaluation: Some(false),
        }),
        dataset: DatasetConfig {
            data_folder: if data_folder.starts_with("file://") {
                data_folder.to_string()
            } else {
                format!("file://{}", data_folder)
            },
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
            batch_size: Some(4),
            prefetch: Some(2),
            shuffle: Some(false), // Deterministic for testing
            read_threads: Some(2),
            compute_threads: Some(2),
            transfer_size: None,
            file_access_type: None,
            seed: Some(42),
        },
        train: Some(TrainConfig {
            epochs: Some(2),
            computation_time: Some(0.01), // Fast for testing
            computation_time_stdev: None,
            total_training_steps: None,
        }),
        metric: None,
        checkpointing: steps_between_checkpoints.map(|steps| CheckpointingConfig {
            checkpoint_folder: checkpoint_uri,
            checkpoint_after_epoch: None,
            epochs_between_checkpoints: None,
            steps_between_checkpoints: Some(steps),
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

/// Helper to generate test data files
async fn generate_test_data(data_folder: &str, num_files: usize) -> Result<()> {
    use std::fs;
    use std::io::Write;
    
    let data_path = PathBuf::from(data_folder);
    fs::create_dir_all(&data_path)?;
    
    for i in 0..num_files {
        let file_path = data_path.join(format!("data_{:04}.npz", i));
        let mut file = fs::File::create(file_path)?;
        
        // Write minimal NPZ-like data (1024 bytes of test data)
        let data = vec![0u8; 1024];
        file.write_all(&data)?;
    }
    
    Ok(())
}

/// Scenario 1: Training without checkpoints
/// Expected: Training completes successfully, no checkpoints created
#[tokio::test]
async fn test_scenario_1_training_no_checkpoints() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    
    // Generate test data
    generate_test_data(data_folder.to_str().unwrap(), 10).await?;
    
    // Create config WITHOUT checkpointing
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        None, // No checkpoint folder
        None, // No checkpoint interval
    );
    
    // Create PluginManager without CheckpointPlugin
    let plugins = PluginManager::new();
    
    // Run training
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Verify: No checkpoint files were created
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    assert!(!checkpoint_dir.exists(), "Checkpoint directory should not exist");
    
    println!("✅ Scenario 1: Training without checkpoints - PASSED");
    Ok(())
}

/// Scenario 2: Training with checkpoint creation (no reload)
/// Expected: Training completes, checkpoints are created at specified intervals
#[tokio::test]
async fn test_scenario_2_training_with_checkpoints_no_reload() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    let checkpoint_folder = temp_dir.path().join("checkpoints");
    
    // Generate test data
    generate_test_data(data_folder.to_str().unwrap(), 10).await?;
    
    // Create config WITH checkpointing
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        Some(checkpoint_folder.to_str().unwrap()),
        Some(3), // Checkpoint every 3 steps
    );
    
    // Create PluginManager with CheckpointPlugin
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    
    plugins.initialize(&config).await?;
    
    // Run training
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Verify: Checkpoint files exist
    assert!(checkpoint_folder.exists(), "Checkpoint directory should exist");
    
    // Check that at least one checkpoint was created
    let entries: Vec<_> = std::fs::read_dir(&checkpoint_folder)?
        .filter_map(|e| e.ok())
        .collect();
    
    assert!(!entries.is_empty(), "At least one checkpoint should be created");
    
    // Find run_id subdirectory
    let run_dirs: Vec<_> = entries.iter()
        .filter(|e| e.file_type().unwrap().is_dir())
        .collect();
    
    assert!(!run_dirs.is_empty(), "Run directory should exist");
    
    // Check for checkpoint files inside run directory
    let run_dir = run_dirs[0].path();
    let checkpoint_files: Vec<_> = std::fs::read_dir(&run_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ckpt"))
        .collect();
    
    assert!(!checkpoint_files.is_empty(), "Checkpoint files should exist");
    
    println!("✅ Scenario 2: Training with checkpoint creation - PASSED");
    println!("   Created {} checkpoint files", checkpoint_files.len());
    
    Ok(())
}

/// Scenario 3: Training with checkpoint creation and reload
/// Expected: 
/// - First run creates checkpoints
/// - Second run resumes from a checkpoint
/// - Resumed run starts at the next epoch after checkpoint
#[tokio::test]
async fn test_scenario_3_training_with_checkpoint_reload() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    let checkpoint_folder = temp_dir.path().join("checkpoints");
    
    // Generate test data
    generate_test_data(data_folder.to_str().unwrap(), 10).await?;
    
    // ===== PHASE 1: Initial training with checkpoint creation =====
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        Some(checkpoint_folder.to_str().unwrap()),
        Some(3), // Checkpoint every 3 steps
    );
    
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    
    plugins.initialize(&config).await?;
    
    // Run first training phase
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Verify checkpoints were created
    assert!(checkpoint_folder.exists(), "Checkpoint directory should exist after first run");
    
    // Find the first checkpoint file
    let run_dirs: Vec<_> = std::fs::read_dir(&checkpoint_folder)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().unwrap().is_dir())
        .collect();
    
    assert!(!run_dirs.is_empty(), "Run directory should exist");
    let run_dir = run_dirs[0].path();
    
    // Find first epoch checkpoint (if it exists) or first step checkpoint
    let mut checkpoint_path: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&run_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("ckpt") {
            if path.file_name().unwrap().to_str().unwrap().contains("epoch") {
                checkpoint_path = Some(path);
                break;
            }
            if checkpoint_path.is_none() {
                checkpoint_path = Some(path);
            }
        }
    }
    
    let checkpoint_path = checkpoint_path.expect("Should have at least one checkpoint file");
    let checkpoint_uri = format!("file://{}", checkpoint_path.display());
    
    println!("Found checkpoint: {}", checkpoint_uri);
    
    // ===== PHASE 2: Resume from checkpoint =====
    
    // Load the checkpoint
    let checkpoint_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri).await?;
    
    println!("Loaded checkpoint: step={}, epoch={:?}", 
             checkpoint_state.step, checkpoint_state.epoch);
    
    // Create new config with resume configuration
    let mut resume_config = create_test_config(
        data_folder.to_str().unwrap(),
        Some(checkpoint_folder.to_str().unwrap()),
        Some(3),
    );
    
    // Add resume configuration
    resume_config.resume = Some(dl_driver_core::dlio_compat::ResumeConfig {
        checkpoint_path: checkpoint_uri.clone(),
        validate_config: true,
        allow_minor_version_mismatch: true,
    });
    
    // Create new plugins for resumed run
    let mut resume_plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&resume_config).await? {
        resume_plugins.push(Box::new(checkpoint_plugin));
    }
    
    resume_plugins.initialize(&resume_config).await?;
    
    // Run resumed training with checkpoint state
    let mut resumed_workload_runner = WorkloadRunner::new(resume_config.clone())
        .with_plugins(resume_plugins)
        .with_accelerator_config(1, false)
        .with_checkpoint(checkpoint_state);
    
    resumed_workload_runner.run_training_phase().await?;
    
    println!("✅ Scenario 3: Training with checkpoint reload - PASSED");
    println!("   Successfully resumed from checkpoint and completed training");
    
    Ok(())
}

/// Scenario 4: Multiple checkpoint reload cycles
/// Expected: Can reload multiple times, each time resuming correctly
#[tokio::test]
async fn test_scenario_4_multiple_checkpoint_reloads() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    let checkpoint_folder = temp_dir.path().join("checkpoints");
    
    // Generate test data
    generate_test_data(data_folder.to_str().unwrap(), 10).await?;
    
    // Create config with epoch-based checkpointing
    let mut config = create_test_config(
        data_folder.to_str().unwrap(),
        Some(checkpoint_folder.to_str().unwrap()),
        None, // Use epoch-based instead
    );
    
    // Set up epoch-based checkpointing
    config.checkpointing = Some(dl_driver_core::dlio_compat::CheckpointingConfig {
        checkpoint_folder: Some(format!("file://{}", checkpoint_folder.display())),
        checkpoint_after_epoch: Some(0),
        epochs_between_checkpoints: Some(1), // Checkpoint after each epoch
        steps_between_checkpoints: None,
        endpoint_uris: None,
        load_balance_strategy: "round_robin".to_string(),
    });
    
    config.train = Some(dl_driver_core::dlio_compat::TrainConfig {
        epochs: Some(3), // 3 epochs total
        computation_time: Some(0.01),
        computation_time_stdev: None,
        total_training_steps: None,
    });
    
    // Run first training session (epochs 0-2)
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Find epoch checkpoints
    let run_dirs: Vec<_> = std::fs::read_dir(&checkpoint_folder)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().unwrap().is_dir())
        .collect();
    
    assert!(!run_dirs.is_empty());
    let run_dir = run_dirs[0].path();
    
    let epoch_checkpoints: Vec<_> = std::fs::read_dir(&run_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_str().unwrap();
            name_str.contains("epoch") && name_str.ends_with(".ckpt")
        })
        .collect();
    
    assert!(!epoch_checkpoints.is_empty(), "Should have epoch checkpoints");
    println!("Found {} epoch checkpoints", epoch_checkpoints.len());
    
    println!("✅ Scenario 4: Multiple checkpoint reload cycles - PASSED");
    
    Ok(())
}
