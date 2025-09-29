// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use dl_driver_core::DlioConfig;

/// Test configuration parsing and backend detection
#[test]
fn test_config_parsing() -> Result<()> {
    // Test File backend detection
    let file_config = DlioConfig::from_yaml_file("tests/configs/test_file_config.yaml")?;
    assert_eq!(file_config.detect_storage_backend(), "file");
    assert!(file_config.data_folder_uri().starts_with("file://"));

    // Test S3 backend detection
    let s3_config = DlioConfig::from_yaml_file("tests/configs/test_s3_large_config.yaml")?;
    assert_eq!(s3_config.detect_storage_backend(), "s3");
    assert!(s3_config.data_folder_uri().starts_with("s3://"));

    // Test Azure backend detection
    let azure_config = DlioConfig::from_yaml_file("tests/configs/test_azure_config.yaml")?;
    assert_eq!(azure_config.detect_storage_backend(), "azure");
    assert!(azure_config.data_folder_uri().starts_with("az://"));

    // Test DirectIO backend detection
    let directio_config = DlioConfig::from_yaml_file("tests/configs/test_directio_config.yaml")?;
    assert_eq!(directio_config.detect_storage_backend(), "direct");
    assert!(directio_config.data_folder_uri().starts_with("direct://"));

    println!("✅ All backend detection tests passed");
    Ok(())
}

#[test]
fn test_config_validation() -> Result<()> {
    let config = DlioConfig::from_yaml_file("tests/configs/test_file_config.yaml")?;

    // Validate required fields
    assert!(config.dataset.num_files_train.unwrap_or(0) > 0);
    assert!(!config.dataset.format.as_deref().unwrap_or("").is_empty());
    assert!(!config.dataset.data_folder.is_empty());
    assert!(config.reader.batch_size.unwrap_or(0) > 0);

    println!("✅ Config validation tests passed");
    Ok(())
}
