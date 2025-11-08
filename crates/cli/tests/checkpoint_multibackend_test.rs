// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Multi-backend checkpoint integration tests
//! 
//! Tests checkpoint save/load/reload functionality across different storage backends:
//! - file:// (local filesystem)
//! - s3:// (AWS S3)
//! - az:// (Azure Blob Storage)
//! 
//! These tests require proper backend configuration:
//! - S3: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION
//! - Azure: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCOUNT_KEY

use anyhow::Result;
use dl_driver_core::{DlioConfig, WorkloadRunner};
use dl_driver_core::plugins::{PluginManager, CheckpointPlugin};
use dl_driver_core::dlio_compat::*;
use tempfile::tempdir;

/// Helper to create a minimal test config with configurable backend
fn create_test_config(
    data_folder: &str,
    checkpoint_uri: &str,
    steps_between_checkpoints: Option<usize>,
) -> DlioConfig {
    DlioConfig {
        model: Some(ModelConfig {
            name: Some("multibackend_test".to_string()),
            model_size: None,
            framework: Some("pytorch".to_string()),
        }),
        framework: Some("pytorch".to_string()),
        workflow: Some(WorkflowConfig {
            generate_data: Some(false),
            train: Some(true),
            checkpoint: Some(true),
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
            shuffle: Some(false),
            read_threads: Some(2),
            compute_threads: Some(2),
            transfer_size: None,
            file_access_type: None,
            seed: Some(42),
        },
        train: Some(TrainConfig {
            epochs: Some(2),
            computation_time: Some(0.01),
            computation_time_stdev: None,
            total_training_steps: None,
        }),
        metric: None,
        checkpointing: Some(CheckpointingConfig {
            checkpoint_folder: Some(checkpoint_uri.to_string()),
            checkpoint_after_epoch: None,
            epochs_between_checkpoints: None,
            steps_between_checkpoints,
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

/// Test checkpoint save/load with file:// backend (baseline)
#[tokio::test(flavor = "multi_thread")]
async fn test_file_backend_checkpoint_roundtrip() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    let checkpoint_folder = temp_dir.path().join("checkpoints");
    
    // Create data folder and generate simple test data using standard tools
    std::fs::create_dir_all(&data_folder)?;
    for i in 0..10 {
        let file_path = data_folder.join(format!("data_{:04}.npz", i));
        std::fs::write(&file_path, vec![0u8; 1024])?; // Dummy data
    }
    
    // Create config with file:// checkpoint URI
    let checkpoint_uri = format!("file://{}", checkpoint_folder.display());
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        &checkpoint_uri,
        Some(3),
    );
    
    // Phase 1: Create checkpoints
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Verify checkpoint directory exists
    assert!(checkpoint_folder.exists(), "Checkpoint directory should exist");
    
    // Find checkpoint file
    let run_dirs: Vec<_> = std::fs::read_dir(&checkpoint_folder)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().unwrap().is_dir())
        .collect();
    assert!(!run_dirs.is_empty(), "Run directory should exist");
    
    let run_dir = run_dirs[0].path();
    let checkpoint_files: Vec<_> = std::fs::read_dir(&run_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ckpt"))
        .collect();
    assert!(!checkpoint_files.is_empty(), "At least one checkpoint should exist");
    
    let checkpoint_path = checkpoint_files[0].path();
    let checkpoint_load_uri = format!("file://{}", checkpoint_path.display());
    
    println!("Testing file:// backend: {}", checkpoint_load_uri);
    
    // Phase 2: Load checkpoint
    let checkpoint_state = CheckpointPlugin::load_checkpoint(&checkpoint_load_uri).await?;
    
    assert_eq!(checkpoint_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    assert!(checkpoint_state.step > 0);
    assert!(checkpoint_state.metadata.uncompressed_size_bytes > 0);
    
    println!("✓ file:// backend checkpoint roundtrip successful");
    
    Ok(())
}

/// Test checkpoint save/load with S3 backend
/// 
/// Requires: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION
/// Set S3_TEST_BUCKET environment variable or skip test
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Only run with --ignored flag when S3 credentials are available
async fn test_s3_backend_checkpoint_roundtrip() -> Result<()> {
    use uuid::Uuid;
    
    // Check if S3 test bucket is configured
    let bucket = match std::env::var("S3_TEST_BUCKET") {
        Ok(b) => b,
        Err(_) => {
            println!("Skipping S3 test: S3_TEST_BUCKET not set");
            return Ok(());
        }
    };
    
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    
    // Generate test data locally
    std::fs::create_dir_all(&data_folder)?;
    for i in 0..10 {
        let file_path = data_folder.join(format!("data_{:04}.npz", i));
        std::fs::write(&file_path, vec![0u8; 1024])?;
    }
    
    // Create config with s3:// checkpoint URI
    let test_prefix = format!("dl-driver-test-{}", Uuid::new_v4());
    let checkpoint_uri = format!("s3://{}/{}", bucket, test_prefix);
    
    println!("Testing S3 backend: {}", checkpoint_uri);
    
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        &checkpoint_uri,
        Some(3),
    );
    
    // Phase 1: Create checkpoints on S3
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // List checkpoint files using s3dlio
    use s3dlio::object_store::store_for_uri;
    let store = store_for_uri(&checkpoint_uri)?;
    let checkpoint_files = store.list(&checkpoint_uri, true).await?;
    
    assert!(!checkpoint_files.is_empty(), "At least one checkpoint should be created on S3");
    
    // Find a .ckpt file
    let checkpoint_uri = checkpoint_files.iter()
        .find(|f| f.ends_with(".ckpt"))
        .expect("Should have at least one .ckpt file");
    
    println!("Found S3 checkpoint: {}", checkpoint_uri);
    
    // Phase 2: Load checkpoint from S3
    let checkpoint_state = CheckpointPlugin::load_checkpoint(checkpoint_uri).await?;
    
    assert_eq!(checkpoint_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    assert!(checkpoint_state.step > 0);
    assert!(checkpoint_state.metadata.uncompressed_size_bytes > 0);
    
    // Cleanup: Delete test objects
    store.delete_prefix(&format!("s3://{}/{}", bucket, test_prefix)).await?;
    
    println!("✓ S3 backend checkpoint roundtrip successful");
    
    Ok(())
}

/// Test checkpoint save/load with Azure Blob Storage backend
/// 
/// Requires: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCOUNT_KEY
/// Set AZURE_TEST_CONTAINER environment variable or skip test
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Only run with --ignored flag when Azure credentials are available
async fn test_azure_backend_checkpoint_roundtrip() -> Result<()> {
    use uuid::Uuid;
    
    // Check if Azure test container is configured
    let container = match std::env::var("AZURE_TEST_CONTAINER") {
        Ok(c) => c,
        Err(_) => {
            println!("Skipping Azure test: AZURE_TEST_CONTAINER not set");
            return Ok(());
        }
    };
    
    // Get Azure storage account name
    let account = match std::env::var("AZURE_STORAGE_ACCOUNT_NAME") {
        Ok(a) => a,
        Err(_) => {
            println!("Skipping Azure test: AZURE_STORAGE_ACCOUNT_NAME not set");
            return Ok(());
        }
    };
    
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    
    // Generate test data locally
    std::fs::create_dir_all(&data_folder)?;
    for i in 0..10 {
        let file_path = data_folder.join(format!("data_{:04}.npz", i));
        std::fs::write(&file_path, vec![0u8; 1024])?;
    }
    
    // Create config with az:// checkpoint URI - format: az://<account>/<container>/<path>
    let test_prefix = format!("dl-driver-test-{}", Uuid::new_v4());
    let checkpoint_uri = format!("az://{}/{}/{}", account, container, test_prefix);
    
    println!("Testing Azure backend: az://<redacted>/{}/{}", container, test_prefix);
    
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        &checkpoint_uri,
        Some(3),
    );
    
    // Phase 1: Create checkpoints on Azure
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // List checkpoint files using s3dlio
    use s3dlio::object_store::store_for_uri;
    let store = store_for_uri(&checkpoint_uri)?;
    let checkpoint_files = store.list(&checkpoint_uri, true).await?;
    
    assert!(!checkpoint_files.is_empty(), "At least one checkpoint should be created on Azure");
    
    // Find a .ckpt file
    let checkpoint_uri = checkpoint_files.iter()
        .find(|f| f.ends_with(".ckpt"))
        .expect("Should have at least one .ckpt file");
    
    println!("Found Azure checkpoint: {}", checkpoint_uri);
    
    // Phase 2: Load checkpoint from Azure
    let checkpoint_state = CheckpointPlugin::load_checkpoint(checkpoint_uri).await?;
    
    assert_eq!(checkpoint_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    assert!(checkpoint_state.step > 0);
    assert!(checkpoint_state.metadata.uncompressed_size_bytes > 0);
    
    // Cleanup: Delete test objects
    store.delete_prefix(&format!("az://{}/{}/{}", account, container, test_prefix)).await?;
    
    println!("✓ Azure backend checkpoint roundtrip successful");
    
    Ok(())
}

/// Test checkpoint save/load with Google Cloud Storage backend
/// 
/// Requires: GOOGLE_APPLICATION_CREDENTIALS or gcloud auth application-default login
/// Set GCS_TEST_BUCKET environment variable or skip test
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Only run with --ignored flag when GCS credentials are available
async fn test_gcs_backend_checkpoint_roundtrip() -> Result<()> {
    use uuid::Uuid;
    
    // Check if GCS test bucket is configured
    let bucket = match std::env::var("GCS_TEST_BUCKET") {
        Ok(b) => b,
        Err(_) => {
            println!("Skipping GCS test: GCS_TEST_BUCKET not set");
            return Ok(());
        }
    };
    
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    
    // Generate test data locally
    std::fs::create_dir_all(&data_folder)?;
    for i in 0..10 {
        let file_path = data_folder.join(format!("data_{:04}.npz", i));
        std::fs::write(&file_path, vec![0u8; 1024])?;
    }
    
    // Create config with gs:// checkpoint URI
    let test_prefix = format!("dl-driver-test-{}", Uuid::new_v4());
    let checkpoint_uri = format!("gs://{}/{}", bucket, test_prefix);
    
    println!("Testing GCS backend: {}", checkpoint_uri);
    
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        &checkpoint_uri,
        Some(3),
    );
    
    // Phase 1: Create checkpoints on GCS
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // List checkpoint files using s3dlio
    use s3dlio::object_store::store_for_uri;
    let store = store_for_uri(&checkpoint_uri)?;
    let checkpoint_files = store.list(&checkpoint_uri, true).await?;
    
    assert!(!checkpoint_files.is_empty(), "At least one checkpoint should be created on GCS");
    
    // Find a .ckpt file
    let checkpoint_uri = checkpoint_files.iter()
        .find(|f| f.ends_with(".ckpt"))
        .expect("Should have at least one .ckpt file");
    
    println!("Found GCS checkpoint: {}", checkpoint_uri);
    
    // Phase 2: Load checkpoint from GCS
    let checkpoint_state = CheckpointPlugin::load_checkpoint(checkpoint_uri).await?;
    
    assert_eq!(checkpoint_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    assert!(checkpoint_state.step > 0);
    assert!(checkpoint_state.metadata.uncompressed_size_bytes > 0);
    
    // Cleanup: Delete test objects
    store.delete_prefix(&format!("gs://{}/{}", bucket, test_prefix)).await?;
    
    println!("✓ GCS backend checkpoint roundtrip successful");
    
    Ok(())
}

/// Test mixed backends: save to file://, load from different file:// location
/// This simulates copying checkpoint files between storage systems
#[tokio::test(flavor = "multi_thread")]
async fn test_mixed_backend_file_to_file() -> Result<()> {
    let temp_dir = tempdir()?;
    let data_folder = temp_dir.path().join("data");
    let checkpoint_folder_1 = temp_dir.path().join("checkpoints_source");
    let checkpoint_folder_2 = temp_dir.path().join("checkpoints_dest");
    
    // Generate test data
    std::fs::create_dir_all(&data_folder)?;
    for i in 0..10 {
        let file_path = data_folder.join(format!("data_{:04}.npz", i));
        std::fs::write(&file_path, vec![0u8; 1024])?;
    }
    
    // Phase 1: Save to first location
    let checkpoint_uri_1 = format!("file://{}", checkpoint_folder_1.display());
    let config = create_test_config(
        data_folder.to_str().unwrap(),
        &checkpoint_uri_1,
        Some(3),
    );
    
    let mut plugins = PluginManager::new();
    if let Some(checkpoint_plugin) = CheckpointPlugin::new(&config).await? {
        plugins.push(Box::new(checkpoint_plugin));
    }
    plugins.initialize(&config).await?;
    
    let mut workload_runner = WorkloadRunner::new(config.clone())
        .with_plugins(plugins)
        .with_accelerator_config(1, false);
    
    workload_runner.run_training_phase().await?;
    
    // Find checkpoint file in source
    let run_dirs: Vec<_> = std::fs::read_dir(&checkpoint_folder_1)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().unwrap().is_dir())
        .collect();
    let run_dir = run_dirs[0].path();
    let checkpoint_files: Vec<_> = std::fs::read_dir(&run_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ckpt"))
        .collect();
    let source_checkpoint = checkpoint_files[0].path();
    
    // Copy checkpoint to second location (simulate cross-storage migration)
    std::fs::create_dir_all(&checkpoint_folder_2)?;
    let dest_checkpoint = checkpoint_folder_2.join(source_checkpoint.file_name().unwrap());
    std::fs::copy(&source_checkpoint, &dest_checkpoint)?;
    
    let checkpoint_uri_2 = format!("file://{}", dest_checkpoint.display());
    
    println!("Testing mixed backend: {} → {}", checkpoint_uri_1, checkpoint_uri_2);
    
    // Phase 2: Load from second location
    let checkpoint_state = CheckpointPlugin::load_checkpoint(&checkpoint_uri_2).await?;
    
    assert_eq!(checkpoint_state.checkpoint_version, env!("CARGO_PKG_VERSION"));
    assert!(checkpoint_state.step > 0);
    assert!(checkpoint_state.metadata.uncompressed_size_bytes > 0);
    
    println!("✓ Mixed backend (file→file) checkpoint load successful");
    
    Ok(())
}
