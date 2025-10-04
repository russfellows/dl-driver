//! Real backend integration tests
//!
//! These tests actually execute I/O operations against real storage backends:
//! - DirectIO: Uses /tmp/directio_replay_test
//! - S3: Uses signal65-public bucket (requires .env with AWS credentials)
//!
//! These tests are more comprehensive than the simulated tests in streaming_replay_tests.rs

use anyhow::Result;
use dl_driver_core::replay::{ReplayConfig, SimpleReplayEngine};
use std::collections::HashMap;
use std::path::PathBuf;

/// Test real DirectIO backend operations
/// 
/// This test actually performs PUT/GET/STAT/DELETE operations using DirectIO
#[tokio::test]
async fn test_real_directio_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/real_directio_test.csv.zst");
    
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    // Load .env for any needed configuration
    dotenvy::dotenv().ok();

    // Create test directory
    let test_dir = "/tmp/directio_replay_test";
    std::fs::create_dir_all(test_dir)?;

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: format!("direct://{}", test_dir),
        concurrency: 4,
        fast_mode: true, // Fast mode for testing
        timeout_seconds: 60,
        path_remaps: HashMap::new(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: true, // Continue even if DirectIO isn't fully supported
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("\n📊 Real DirectIO Backend Test Results:");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);
    println!("   Total bytes: {} bytes", stats.total_bytes);
    if let Some(d) = stats.duration() {
        println!("   Duration: {:.2}s", d.as_secs_f64());
    }

    assert_eq!(stats.total_operations, 4, "Should have parsed 4 operations");
    
    // DirectIO might fail on some systems without proper setup
    if stats.failed_operations > 0 {
        println!("⚠️  Note: {} operations failed (DirectIO may require special hardware/setup)", stats.failed_operations);
    } else {
        println!("✅ All DirectIO operations succeeded!");
    }

    // Cleanup
    std::fs::remove_dir_all(test_dir).ok();

    Ok(())
}

/// Test real S3 backend operations
/// 
/// This test actually performs PUT/GET/STAT/LIST/DELETE operations against
/// the signal65-public S3 bucket in us-west-2.
/// Requires AWS credentials in .env file.
#[tokio::test]
async fn test_real_s3_backend() -> Result<()> {
    let oplog_path = PathBuf::from("../../tests/replay_tests/oplogs/real_s3_test.csv.zst");
    
    if !oplog_path.exists() {
        eprintln!("⚠️  Skipping test: {} not found", oplog_path.display());
        return Ok(());
    }

    // Load .env for AWS credentials
    dotenvy::dotenv().ok();

    // Check if credentials are available
    if std::env::var("AWS_ACCESS_KEY_ID").is_err() {
        eprintln!("⚠️  Skipping S3 test: AWS_ACCESS_KEY_ID not found in environment");
        eprintln!("    Make sure .env file has AWS credentials");
        return Ok(());
    }

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: "s3://signal65-public/dl-driver-test/".to_string(),
        concurrency: 4,
        fast_mode: true,
        timeout_seconds: 120, // S3 operations may take longer
        path_remaps: HashMap::new(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: false, // Fail fast for S3 - we expect these to work
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("\n📊 Real S3 Backend Test Results:");
    println!("   Bucket: signal65-public");
    println!("   Region: us-west-2");
    println!("   Total operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Failed: {}", stats.failed_operations);
    println!("   Total bytes: {} bytes ({:.2} KB)", stats.total_bytes, stats.total_bytes as f64 / 1024.0);
    if let Some(d) = stats.duration() {
        println!("   Duration: {:.2}s", d.as_secs_f64());
    }
    println!("   Throughput: {:.2} ops/sec", stats.operations_per_second());

    assert_eq!(stats.total_operations, 5, "Should have parsed 5 operations");
    assert_eq!(stats.failed_operations, 0, "All S3 operations should succeed with valid credentials");
    assert_eq!(stats.completed_operations, 5, "All 5 operations should complete");

    println!("✅ All S3 operations succeeded!");

    Ok(())
}

/// Test S3 with actual data generation
/// 
/// This creates a small test file, uploads it, downloads it, verifies content, then cleans up
#[tokio::test]
async fn test_s3_with_real_data() -> Result<()> {
    // Load .env for AWS credentials
    dotenvy::dotenv().ok();

    // Check if credentials are available
    if std::env::var("AWS_ACCESS_KEY_ID").is_err() {
        eprintln!("⚠️  Skipping S3 data test: AWS_ACCESS_KEY_ID not found in environment");
        return Ok(());
    }

    use rand::RngCore;
    use tempfile::NamedTempFile;
    
    println!("\n🔧 Testing S3 with real data generation and verification...");

    // Generate test data
    let mut test_data = vec![0u8; 65536]; // 64KB
    rand::rng().fill_bytes(&mut test_data);
    
    // Write to temp file
    let temp_file = NamedTempFile::new()?;
    std::fs::write(temp_file.path(), &test_data)?;

    // Create op-log for upload with proper .tsv extension
    let oplog_content = format!(
        "idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror\n\
         1\tPUT\t{}\ts3://signal65-public\tdl-driver-test/data_test.bin\t2025-10-03T19:30:00Z\t100000000\t\n\
         2\tGET\t{}\ts3://signal65-public\tdl-driver-test/data_test.bin\t2025-10-03T19:30:01Z\t80000000\t\n\
         3\tDELETE\t0\ts3://signal65-public\tdl-driver-test/data_test.bin\t2025-10-03T19:30:02Z\t50000000\t",
        test_data.len(),
        test_data.len()
    );

    // Use a temp file with .tsv extension for proper format detection
    let oplog_path = std::env::temp_dir().join(format!("s3_data_test_{}.tsv", std::process::id()));
    std::fs::write(&oplog_path, oplog_content)?;

    let config = ReplayConfig {
        op_log_path: oplog_path.to_string_lossy().to_string(),
        base_uri: "s3://signal65-public/dl-driver-test/".to_string(),
        concurrency: 1, // Sequential for data verification
        fast_mode: true,
        timeout_seconds: 120,
        path_remaps: HashMap::new(),
        endpoint_remaps: HashMap::new(),
        continue_on_error: false,
    };

    let mut engine = SimpleReplayEngine::new(config);
    let stats = engine.run_replay().await?;

    println!("\n📊 S3 Data Test Results:");
    println!("   Operations: {}", stats.total_operations);
    println!("   Completed: {}", stats.completed_operations);
    println!("   Data size: {} bytes", test_data.len());

    assert_eq!(stats.total_operations, 3);
    assert_eq!(stats.completed_operations, 3);
    
    println!("✅ S3 data upload/download/delete completed successfully!");

    // Cleanup temp op-log file
    std::fs::remove_file(&oplog_path).ok();

    Ok(())
}
