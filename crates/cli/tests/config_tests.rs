use anyhow::Result;
use real_dlio_core::config::{Config, StorageBackend};

/// Test configuration parsing and backend detection
#[test]
fn test_config_parsing() -> Result<()> {
    // Test File backend detection
    let file_config = Config::from_yaml_file("tests/configs/test_file_config.yaml")?;
    assert!(matches!(file_config.storage_backend(), StorageBackend::File));
    assert!(file_config.storage_uri().starts_with("file://"));
    
    // Test S3 backend detection
    let s3_config = Config::from_yaml_file("tests/configs/test_s3_large_config.yaml")?;
    assert!(matches!(s3_config.storage_backend(), StorageBackend::S3));
    assert!(s3_config.storage_uri().starts_with("s3://"));
    
    // Test Azure backend detection
    let azure_config = Config::from_yaml_file("tests/configs/test_azure_config.yaml")?;
    assert!(matches!(azure_config.storage_backend(), StorageBackend::Azure));
    assert!(azure_config.storage_uri().starts_with("az://"));
    
    // Test DirectIO backend detection
    let directio_config = Config::from_yaml_file("tests/configs/test_directio_config.yaml")?;
    assert!(matches!(directio_config.storage_backend(), StorageBackend::DirectIO));
    assert!(directio_config.storage_uri().starts_with("direct://"));
    
    println!("✅ All backend detection tests passed");
    Ok(())
}

#[test]
fn test_config_validation() -> Result<()> {
    let config = Config::from_yaml_file("tests/configs/test_file_config.yaml")?;
    
    // Validate required fields
    assert!(config.dataset.num_files_train > 0);
    assert!(!config.dataset.format.is_empty());
    assert!(!config.dataset.data_folder.is_empty());
    assert!(config.reader.batch_size > 0);
    
    println!("✅ Config validation tests passed");
    Ok(())
}
