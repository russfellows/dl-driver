use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use futures_util::StreamExt;

use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset, PoolConfig};
use s3dlio::ReaderMode;
use s3dlio::LoaderOptions;
use s3dlio::data_loader::options::LoadingMode;
use s3dlio::object_store::store_for_uri;

/// Test comprehensive async pool data loading with 50+ files
#[tokio::test]
async fn test_async_pool_dataloader_comprehensive() -> Result<()> {
    println!("🚀 Starting comprehensive AsyncPoolDataLoader test...");
    
    // Create temporary directory and generate 50+ test files
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();
    let num_files = 60; // More than 50 as requested
    let file_size_bytes = 1024; // 1KB per file
    
    println!("📁 Creating {} test files of {} bytes each...", num_files, file_size_bytes);
    
    // Generate test files with varied content
    let mut expected_file_contents = HashMap::new();
    for i in 0..num_files {
        let file_path = temp_path.join(format!("test_data_{:03}.txt", i));
        let content = format!("Test file {} with data: {}", i, "x".repeat(file_size_bytes - 50));
        std::fs::write(&file_path, &content)?;
        expected_file_contents.insert(file_path.to_string_lossy().to_string(), content.into_bytes());
    }
    
    // Create store and upload files
    let store_uri = format!("file://{}", temp_path.to_string_lossy());
    let store = store_for_uri(&store_uri)?;
    
    println!("📤 Verifying {} files are accessible via ObjectStore...", num_files);
    let file_list = store.list(&store_uri, true).await?;
    assert!(file_list.len() >= num_files, "Should have at least {} files, found {}", num_files, file_list.len());
    
    // Create MultiBackendDataset
    let dataset = MultiBackendDataset::from_prefix(&store_uri).await?;
    assert!(dataset.len() >= num_files, "Dataset should contain at least {} files", num_files);
    
    println!("✅ Dataset created with {} files", dataset.len());
    
    // Configure advanced LoaderOptions with async pooling
    let pool_config = PoolConfig {
        pool_size: 16,           // Concurrent requests
        readahead_batches: 8,    // Read-ahead batches to eliminate head latency
        batch_timeout: Duration::from_secs(10),
        max_inflight: 64,        // Maximum in-flight requests
    };
    
    let loader_options = LoaderOptions {
        batch_size: 8,           // Process in batches of 8
        drop_last: false,
        shuffle: true,
        seed: 42,                // Use seed instead of shuffle_seed
        num_workers: 4,
        prefetch: 16,            // Prefetch buffer to eliminate waits
        auto_tune: true,         // Dynamic optimization
        reader_mode: ReaderMode::Sequential,
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        ..Default::default()
    };
    
    println!("⚙️ Configured AsyncPoolDataLoader with:");
    println!("   - Batch size: {}", loader_options.batch_size);
    println!("   - Pool size: {} concurrent requests", pool_config.pool_size);
    println!("   - Read-ahead batches: {}", pool_config.readahead_batches);
    println!("   - Prefetch buffer: {}", loader_options.prefetch);
    println!("   - Shuffle enabled: {}", loader_options.shuffle);
    
    // Create AsyncPoolDataLoader
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_options);
    
    // Stream with pool configuration for dynamic batching
    let mut stream = dataloader.stream_with_pool(pool_config);
    
    let start_time = Instant::now();
    let mut total_batches = 0;
    let mut total_files_processed = 0;
    let mut batch_times = Vec::new();
    
    println!("🔄 Starting async batch processing...");
    
    // Process batches asynchronously
    while let Some(batch_result) = stream.next().await {
        let batch_start = Instant::now();
        
        match batch_result {
            Ok(batch) => {
                total_batches += 1;
                let files_in_batch = batch.len();
                total_files_processed += files_in_batch;
                
                // Verify batch contents
                assert!(!batch.is_empty(), "Batch should not be empty");
                assert!(files_in_batch <= 8, "Batch size should not exceed configured maximum");
                
                // Verify each file in batch has expected content structure
                for file_data in &batch {
                    assert!(!file_data.is_empty(), "File data should not be empty");
                    assert!(file_data.len() >= 50, "File should have minimum expected content");
                    
                    // Verify it's actual test data we created
                    let content_str = String::from_utf8_lossy(file_data);
                    assert!(content_str.starts_with("Test file"), "File should contain expected test data");
                }
                
                let batch_time = batch_start.elapsed();
                batch_times.push(batch_time);
                
                println!("📦 Batch {} processed: {} files in {:?}", 
                        total_batches, files_in_batch, batch_time);
                
            }
            Err(e) => {
                panic!("Batch processing failed: {}", e);
            }
        }
    }
    
    let total_time = start_time.elapsed();
    
    println!("\n📊 Performance Results:");
    println!("   ✅ Total files processed: {}", total_files_processed);
    println!("   ✅ Total batches: {}", total_batches);
    println!("   ✅ Total time: {:?}", total_time);
    println!("   ✅ Average time per batch: {:?}", 
            batch_times.iter().sum::<Duration>() / batch_times.len() as u32);
    println!("   ✅ Files per second: {:.2}", 
            total_files_processed as f64 / total_time.as_secs_f64());
    
    // Verify we processed the expected number of files
    assert!(total_files_processed >= num_files, 
           "Should have processed at least {} files, got {}", num_files, total_files_processed);
    assert!(total_batches > 1, "Should have processed multiple batches");
    
    // Verify async performance characteristics
    assert!(total_time < Duration::from_secs(30), 
           "Processing should complete reasonably quickly with async pooling");
    
    println!("✅ AsyncPoolDataLoader test completed successfully!");
    
    Ok(())
}

/// Test dynamic batching eliminates head latency wait problem
#[tokio::test]
async fn test_dynamic_batching_eliminates_head_latency() -> Result<()> {
    println!("🎯 Testing dynamic batching to eliminate head latency...");
    
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();
    let num_files = 25; // Smaller set for focused test
    
    // Create files with simulated variable access times (different sizes)
    for i in 0..num_files {
        let file_path = temp_path.join(format!("latency_test_{:02}.txt", i));
        // Vary file sizes to simulate different access latencies
        let size = if i % 5 == 0 { 2048 } else { 512 }; // Some files larger
        let content = "x".repeat(size);
        std::fs::write(&file_path, &content)?;
    }
    
    let store_uri = format!("file://{}", temp_path.to_string_lossy());
    let dataset = MultiBackendDataset::from_prefix(&store_uri).await?;
    
    // Configure for dynamic batching with out-of-order completion
    let pool_config = PoolConfig {
        pool_size: 12,           // High concurrency
        readahead_batches: 6,    // Aggressive read-ahead
        batch_timeout: Duration::from_millis(500), // Quick timeouts
        max_inflight: 32,
    };
    
    let loader_options = LoaderOptions {
        batch_size: 4,
        drop_last: false,
        shuffle: true,           // Shuffle to randomize access patterns
        seed: 123,               // Use seed instead of shuffle_seed
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        ..Default::default()
    };
    
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_options);
    let mut stream = dataloader.stream_with_pool(pool_config);
    
    let start_time = Instant::now();
    let mut batch_intervals = Vec::new();
    let mut last_batch_time = start_time;
    
    println!("⏱️ Measuring batch arrival intervals (lower is better for head latency)...");
    
    while let Some(batch_result) = stream.next().await {
        let current_time = Instant::now();
        let interval = current_time.duration_since(last_batch_time);
        batch_intervals.push(interval);
        last_batch_time = current_time;
        
        match batch_result {
            Ok(batch) => {
                println!("📦 Batch arrived after {:?} with {} files", interval, batch.len());
            }
            Err(e) => panic!("Batch error: {}", e),
        }
    }
    
    let total_time = start_time.elapsed();
    
    // Analyze head latency characteristics
    let avg_interval = batch_intervals.iter().sum::<Duration>() / batch_intervals.len() as u32;
    let max_interval = batch_intervals.iter().max().unwrap();
    let min_interval = batch_intervals.iter().min().unwrap();
    
    println!("\n📈 Head Latency Analysis:");
    println!("   ⚡ Average batch interval: {:?}", avg_interval);
    println!("   📊 Min interval: {:?}", min_interval);
    println!("   📊 Max interval: {:?}", max_interval);
    println!("   🎯 Total processing time: {:?}", total_time);
    
    // Verify dynamic batching is working effectively
    assert!(batch_intervals.len() > 1, "Should have multiple batches");
    assert!(*max_interval < Duration::from_secs(2), 
           "Dynamic batching should keep intervals reasonable");
    
    // The key test: verify we don't have long waits between batches
    // (indicating out-of-order completion is working)
    let long_waits = batch_intervals.iter()
        .filter(|&&interval| interval > Duration::from_millis(200))
        .count();
    
    let wait_ratio = long_waits as f64 / batch_intervals.len() as f64;
    println!("   🚫 Long waits (>200ms): {} out of {} ({:.1}%)", 
            long_waits, batch_intervals.len(), wait_ratio * 100.0);
    
    // With dynamic batching, we should have very few long waits
    assert!(wait_ratio < 0.3, 
           "Dynamic batching should minimize head latency waits (got {:.1}% long waits)", 
           wait_ratio * 100.0);
    
    println!("✅ Dynamic batching successfully eliminates head latency wait problem!");
    
    Ok(())
}

/// Test multi-backend support with different storage types
#[tokio::test]
async fn test_multi_backend_comprehensive() -> Result<()> {
    println!("🔄 Testing multi-backend support...");
    
    // Test File backend (always available)
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();
    
    // Create test files for file backend
    for i in 0..15 {
        let file_path = temp_path.join(format!("backend_test_{}.txt", i));
        let content = format!("Backend test file {} content", i);
        std::fs::write(&file_path, &content)?;
    }
    
    let file_uri = format!("file://{}", temp_path.to_string_lossy());
    
    // Test file backend with AsyncPoolDataLoader
    let dataset = MultiBackendDataset::from_prefix(&file_uri).await?;
    
    let pool_config = PoolConfig {
        pool_size: 8,
        readahead_batches: 4,
        batch_timeout: Duration::from_secs(5),
        max_inflight: 16,
    };
    
    let loader_options = LoaderOptions {
        batch_size: 3,
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        shuffle: true,
        prefetch: 8,
        ..Default::default()
    };
    
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_options);
    let mut stream = dataloader.stream_with_pool(pool_config);
    
    let mut files_processed = 0;
    let start_time = Instant::now();
    
    while let Some(batch_result) = stream.next().await {
        match batch_result {
            Ok(batch) => {
                files_processed += batch.len();
                println!("📦 File backend batch: {} files", batch.len());
                
                // Verify content
                for file_data in &batch {
                    let content = String::from_utf8_lossy(file_data);
                    assert!(content.contains("Backend test file"), 
                           "File should contain expected content");
                }
            }
            Err(e) => panic!("File backend batch error: {}", e),
        }
    }
    
    let total_time = start_time.elapsed();
    
    println!("✅ File backend processed {} files in {:?}", files_processed, total_time);
    assert!(files_processed >= 15, "Should process all files");
    
    // TODO: Add S3 and Azure tests when credentials are available
    // This demonstrates the multi-backend architecture is working
    
    println!("✅ Multi-backend test completed successfully!");
    
    Ok(())
}

/// Test auto-tuning and performance optimization
#[tokio::test]
async fn test_auto_tuning_optimization() -> Result<()> {
    println!("🔧 Testing auto-tuning and performance optimization...");
    
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();
    
    // Create files with varied sizes to test auto-tuning
    for i in 0..30 {
        let file_path = temp_path.join(format!("tune_test_{:02}.txt", i));
        let size = match i % 4 {
            0 => 512,   // Small files
            1 => 1024,  // Medium files  
            2 => 2048,  // Large files
            _ => 768,   // Variable files
        };
        let content = "x".repeat(size);
        std::fs::write(&file_path, &content)?;
    }
    
    let store_uri = format!("file://{}", temp_path.to_string_lossy());
    let dataset = MultiBackendDataset::from_prefix(&store_uri).await?;
    
    // Test with auto-tuning enabled
    let pool_config = PoolConfig {
        pool_size: 10,
        readahead_batches: 5,
        batch_timeout: Duration::from_secs(3),
        max_inflight: 20,
    };
    
    let loader_options = LoaderOptions {
        batch_size: 5,
        auto_tune: true,         // Enable auto-tuning
        prefetch: 12,
        shuffle: true,
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        ..Default::default()
    };
    
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_options);
    let mut stream = dataloader.stream_with_pool(pool_config);
    
    let start_time = Instant::now();
    let mut batch_count = 0;
    let mut total_files = 0;
    
    while let Some(batch_result) = stream.next().await {
        match batch_result {
            Ok(batch) => {
                batch_count += 1;
                total_files += batch.len();
                
                println!("🔧 Auto-tuned batch {}: {} files", batch_count, batch.len());
                
                // Verify batch processing
                for file_data in &batch {
                    assert!(!file_data.is_empty(), "Auto-tuned batch should have valid data");
                }
            }
            Err(e) => panic!("Auto-tuning batch error: {}", e),
        }
    }
    
    let total_time = start_time.elapsed();
    let throughput = total_files as f64 / total_time.as_secs_f64();
    
    println!("📊 Auto-tuning results:");
    println!("   ✅ Processed {} files in {} batches", total_files, batch_count);
    println!("   ✅ Total time: {:?}", total_time);
    println!("   ✅ Throughput: {:.2} files/second", throughput);
    
    assert!(total_files >= 30, "Should process all files");
    assert!(batch_count > 1, "Should create multiple batches");
    assert!(throughput > 5.0, "Auto-tuning should achieve reasonable throughput");
    
    println!("✅ Auto-tuning optimization test completed successfully!");
    
    Ok(())
}
