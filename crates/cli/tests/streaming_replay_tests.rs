//! Streaming replay integration tests
//!
//! Tests the new streaming replay functionality with s3dlio-oplog across
//! all 5 supported storage backends: File, S3, Azure Blob, GCS, DirectIO

use anyhow::Result;
use dl_driver_core::replay::{ReplayConfig, SimpleReplayEngine};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test streaming replay with File backend op-log
#[tokio::test]
async fn test_streaming_replay_file_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_file_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let base_uri = format!("file://{}", temp_dir.path().display());

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri,
        concurrency: 4,
        fast_mode: true, // Run without timing delays for faster test
        timeout_seconds: 30,
        path_remaps: vec![("/tmp/replay_test/data/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 File Backend Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    // Verify we parsed the expected number of operations
    assert_eq!(stats.total_operations, 10, "Should have parsed 10 operations");
    assert!(stats.completed_operations > 0, "Should have completed some operations");

    Ok(())
}

/// Test streaming replay with S3 backend op-log (simulated)
#[tokio::test]
async fn test_streaming_replay_s3_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_s3_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    // For this test, we'll just verify parsing and statistics
    // Actual S3 operations would require credentials
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: "s3://test-replay-bucket/data/".to_string(),
        concurrency: 8,
        fast_mode: true,
        timeout_seconds: 60,
        path_remaps: vec![("test-bucket/replay/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 S3 Backend Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    assert_eq!(stats.total_operations, 9, "Should have parsed 9 operations");
    Ok(())
}

/// Test streaming replay with Azure Blob backend op-log (simulated)
#[tokio::test]
async fn test_streaming_replay_azure_backend() -> Result<()> {
    let oplog_path =
        PathBuf::from("../../tests/replay_tests/oplogs/sample_azure_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: "az://test-replay-container/data/".to_string(),
        concurrency: 6,
        fast_mode: true,
        timeout_seconds: 60,
        path_remaps: vec![("testcontainer/replay/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 Azure Backend Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    assert_eq!(stats.total_operations, 7, "Should have parsed 7 operations");
    Ok(())
}

/// Test streaming replay with GCS (Google Cloud Storage) backend op-log (simulated)
/// GCS support is new in s3dlio v0.8.19
#[tokio::test]
async fn test_streaming_replay_gcs_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_gcs_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: "gs://test-replay-bucket/data/".to_string(),
        concurrency: 8,
        fast_mode: true,
        timeout_seconds: 60,
        path_remaps: vec![("test-gcs-bucket/replay/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 GCS Backend Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    assert_eq!(stats.total_operations, 9, "Should have parsed 9 operations");
    Ok(())
}

/// Test streaming replay with DirectIO backend op-log (simulated)
#[tokio::test]
async fn test_streaming_replay_directio_backend() -> Result<()> {
    let oplog_path =
        PathBuf::from("../../tests/replay_tests/oplogs/sample_directio_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let base_uri = format!("direct://{}", temp_dir.path().display());

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri,
        concurrency: 4,
        fast_mode: true,
        timeout_seconds: 30,
        path_remaps: vec![("/mnt/nvme/replay_test/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 DirectIO Backend Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    assert_eq!(stats.total_operations, 7, "Should have parsed 7 operations");
    Ok(())
}

/// Test streaming with concurrent execution
#[tokio::test]
async fn test_streaming_replay_concurrent() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_file_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("file://{}", temp_dir.path().display()),
        concurrency: 16, // High concurrency
        fast_mode: true,
        timeout_seconds: 30,
        path_remaps: vec![("/tmp/replay_test/data/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let start = std::time::Instant::now();
    let stats = engine.run_replay().await?;
    let duration = start.elapsed();

    println!("📊 Concurrent Replay (16 workers) Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);
    println!("   Wall-clock duration: {:?}", duration);

    assert_eq!(stats.total_operations, 10);
    assert!(stats.completed_operations > 0);

    Ok(())
}

/// Test streaming with sequential execution (concurrency = 1)
#[tokio::test]
async fn test_streaming_replay_sequential() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_file_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("file://{}", temp_dir.path().display()),
        concurrency: 1, // Sequential execution
        fast_mode: true,
        timeout_seconds: 30,
        path_remaps: vec![("/tmp/replay_test/data/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 Sequential Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);

    assert_eq!(stats.total_operations, 10);
    Ok(())
}

/// Test path remapping functionality
#[tokio::test]
async fn test_streaming_replay_with_remapping() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_file_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("file://{}/remapped", temp_dir.path().display()),
        concurrency: 4,
        fast_mode: true,
        timeout_seconds: 30,
        path_remaps: vec![
            ("/tmp/replay_test/data/".to_string(), "".to_string()),
            ("file_".to_string(), "remapped_file_".to_string()),
        ]
        .into_iter()
        .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 Remapped Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Path remaps applied: /tmp/replay_test/data/ -> ''");
    println!("                         file_ -> remapped_file_");

    assert_eq!(stats.total_operations, 10);
    Ok(())
}

/// Test endpoint remapping (cross-backend replay)
#[tokio::test]
async fn test_streaming_replay_cross_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_s3_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    // Replay S3 operations to local file system
    let temp_dir = TempDir::new()?;
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("file://{}", temp_dir.path().display()),
        concurrency: 4,
        fast_mode: true,
        timeout_seconds: 30,
        path_remaps: vec![("test-bucket/replay/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: vec![("s3://".to_string(), "file://".to_string())]
            .into_iter()
            .collect(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("📊 Cross-Backend Replay (S3 -> File) Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Endpoint remap: s3:// -> file://");

    assert_eq!(stats.total_operations, 9);
    Ok(())
}

/// Test with timing delays (non-fast mode)
#[tokio::test]
async fn test_streaming_replay_with_timing() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/sample_file_backend.csv.zst");
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    let temp_dir = TempDir::new()?;
    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("file://{}", temp_dir.path().display()),
        concurrency: 4,
        fast_mode: false, // Preserve timing delays
        timeout_seconds: 30,
        path_remaps: vec![("/tmp/replay_test/data/".to_string(), "".to_string())]
            .into_iter()
            .collect(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let start = std::time::Instant::now();
    let stats = engine.run_replay().await?;
    let duration = start.elapsed();

    println!("📊 Timed Replay Stats:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Wall-clock duration: {:?}", duration);
    println!("   (Should respect inter-arrival delays from op-log)");

    assert_eq!(stats.total_operations, 10);
    // With timing, should take noticeable time (at least 500ms)
    assert!(duration.as_millis() >= 500, "Should take time with delays, took {:?}", duration);

    Ok(())
}

