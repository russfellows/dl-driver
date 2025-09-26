// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{PyTorchDataLoader, FrameworkConfig};
use crate::framework_config::PyTorchConfig;
use dl_driver_core::dlio_compat::{DlioConfig, DatasetConfig, ReaderConfig};
use anyhow::Result;

/// Create a minimal test DlioConfig for testing
fn create_test_dlio_config() -> DlioConfig {
    DlioConfig {
        model: None,
        framework: Some("pytorch".to_string()),
        workflow: None,
        dataset: DatasetConfig {
            data_folder: "file:///tmp/test".to_string(),
            format: Some("npz".to_string()),
            num_files_train: Some(100),
            num_files_eval: None,
            record_length_bytes: Some(1024),
            num_samples_per_file: Some(10),
            compression: None,
        },
        reader: ReaderConfig {
            data_loader: Some("pytorch".to_string()),
            batch_size: Some(32),
            prefetch: Some(4),
            shuffle: Some(true),
            read_threads: Some(2),
            compute_threads: Some(4),
            transfer_size: None,
            file_access_type: None,
            seed: Some(42),
        },
        checkpointing: None,
        profiling: None,
        pytorch_config: None,
        tensorflow_config: None,
        jax_config: None,
        framework_profiles: None,
    }
}

#[test]
fn test_pytorch_config_validation() -> Result<()> {
    // Test valid PyTorch configuration
    let pytorch_config = PyTorchConfig::default();
    
    // Test validation of valid config
    let framework_config = FrameworkConfig {
        pytorch: Some(pytorch_config.clone()),
        tensorflow: None,
        dlio: create_test_dlio_config(),
    };
    
    assert!(framework_config.validate().is_ok());
    
    // Test invalid batch size
    let mut invalid_pytorch_config = pytorch_config.clone();
    invalid_pytorch_config.batch_size = 0;
    let invalid_framework_config = FrameworkConfig {
        pytorch: Some(invalid_pytorch_config),
        tensorflow: None,
        dlio: create_test_dlio_config(),
    };
    assert!(invalid_framework_config.validate().is_err());
    
    // Test invalid prefetch factor
    let mut invalid_pytorch_config2 = pytorch_config;
    invalid_pytorch_config2.prefetch_factor = Some(0);
    let invalid_framework_config2 = FrameworkConfig {
        pytorch: Some(invalid_pytorch_config2),
        tensorflow: None,
        dlio: create_test_dlio_config(),
    };
    assert!(invalid_framework_config2.validate().is_err());
    
    Ok(())
}

#[test]
fn test_pytorch_config_serialization() -> Result<()> {
    let pytorch_config = PyTorchConfig::default();
    
    // Test JSON serialization
    let json = serde_json::to_string(&pytorch_config)?;
    println!("PyTorch config JSON: {}", json);
    
    // Test deserialization
    let deserialized: PyTorchConfig = serde_json::from_str(&json)?;
    assert_eq!(pytorch_config.batch_size, deserialized.batch_size);
    assert_eq!(pytorch_config.num_workers, deserialized.num_workers);
    assert_eq!(pytorch_config.shuffle, deserialized.shuffle);
    
    Ok(())
}

#[test]
fn test_pytorch_dataloader_config_conversion() -> Result<()> {
    // Create a DLIO config for testing
    let dlio_config = create_test_dlio_config();
    let pytorch_config = PyTorchConfig {
        batch_size: 64,
        shuffle: true,
        seed: Some(42),
        ..PyTorchConfig::default()
    };
    
    // Create PyTorchDataLoader (this tests the config validation path)
    let dataloader = PyTorchDataLoader::from_dlio_config(
        &dlio_config,
        pytorch_config.clone(),
        "file:///tmp/test_data".to_string()
    )?;
    
    // Test loader options conversion
    let loader_options = dataloader.to_loader_options(&dlio_config);
    assert_eq!(loader_options.batch_size, 64);
    assert_eq!(loader_options.shuffle, true);
    
    // Test configuration access
    let retrieved_pytorch_config = dataloader.pytorch_config();
    assert_eq!(retrieved_pytorch_config.batch_size, 64);
    assert_eq!(retrieved_pytorch_config.shuffle, true);
    assert_eq!(retrieved_pytorch_config.seed, Some(42));
    
    Ok(())
}

#[test]
fn test_pytorch_format_detection() -> Result<()> {
    // Test format detection logic
    let mut dlio_config = create_test_dlio_config();
    
    // Test NPZ format detection
    dlio_config.dataset.format = Some("npz".to_string());
    let dataloader = PyTorchDataLoader::from_dlio_config(
        &dlio_config,
        PyTorchConfig::default(),
        "file:///tmp/test.npz".to_string()
    )?;
    
    // Test that data folder is correctly set
    assert_eq!(dataloader.data_folder(), "file:///tmp/test.npz");
    
    Ok(())
}

#[test]
fn test_pytorch_epoch_management() -> Result<()> {
    let dlio_config = create_test_dlio_config();
    let mut dataloader = PyTorchDataLoader::from_dlio_config(
        &dlio_config,
        PyTorchConfig::default(),
        "file:///tmp/test".to_string()
    )?;
    
    // Test initial epoch
    assert_eq!(dataloader.current_epoch(), 0);
    
    // Test next epoch
    let next = dataloader.next_epoch();
    assert_eq!(next, 1);
    assert_eq!(dataloader.current_epoch(), 1);
    
    // Test reset
    dataloader.reset_epoch();
    assert_eq!(dataloader.current_epoch(), 0);
    
    Ok(())
}

#[test]
fn test_framework_config_from_dlio() -> Result<()> {
    // Test creating FrameworkConfig from DLIO config
    let dlio_config = create_test_dlio_config();
    
    let pytorch_framework_config = FrameworkConfig::from_dlio_with_pytorch(dlio_config.clone());
    assert!(pytorch_framework_config.pytorch.is_some());
    assert!(pytorch_framework_config.tensorflow.is_none());
    
    let tensorflow_framework_config = FrameworkConfig::from_dlio_with_tensorflow(dlio_config);
    assert!(tensorflow_framework_config.pytorch.is_none());
    assert!(tensorflow_framework_config.tensorflow.is_some());
    
    Ok(())
}

#[test] 
fn test_seed_state_management() -> Result<()> {
    let dlio_config = create_test_dlio_config();
    let pytorch_config = PyTorchConfig {
        seed: Some(123),
        ..PyTorchConfig::default()
    };
    
    let mut dataloader = PyTorchDataLoader::from_dlio_config(
        &dlio_config,
        pytorch_config,
        "file:///tmp/test".to_string()
    )?;
    
    // Test seed state retrieval
    assert_eq!(dataloader.seed_state(), Some(123));
    
    // Test seed state update
    dataloader.update_seed_state(Some(456));
    assert_eq!(dataloader.seed_state(), Some(456));
    
    // Test clearing seed
    dataloader.update_seed_state(None);
    assert_eq!(dataloader.seed_state(), None);
    
    Ok(())
}