use anyhow::Result;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use futures_util::StreamExt;

use s3dlio::data_loader::async_pool_dataloader::{AsyncPoolDataLoader, MultiBackendDataset, PoolConfig};
use s3dlio::data_loader::options::{LoaderOptions, LoadingMode, ReaderMode};
use s3dlio::object_store::store_for_uri;

/// Test large-scale file upload and async batch processing (100+ files)
#[test]
fn test_large_scale_upload_and_batch_processing() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let result = run_large_scale_test().await;
        assert!(result.is_ok(), "Large scale test failed: {:?}", result.err());
    });
}

async fn run_large_scale_test() -> Result<()> {
    println!("🎯 Starting LARGE-SCALE upload and batch processing test...");
    
    // Create temporary directory and generate 100+ test files
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();
    let num_files = 120; // Significantly more than 50
    let base_file_size = 2048; // 2KB base size
    
    println!("📁 Creating {} test files with varied sizes...", num_files);
    
    let upload_start = Instant::now();
    
    // Generate test files with varied sizes and content types
    for i in 0..num_files {
        let file_path = temp_path.join(format!("large_test_{:03}.dat", i));
        
        // Vary file sizes to simulate real-world data
        let size_multiplier = match i % 7 {
            0 => 1,   // 2KB
            1 => 2,   // 4KB  
            2 => 3,   // 6KB
            3 => 5,   // 10KB
            4 => 1,   // 2KB
            5 => 4,   // 8KB
            _ => 2,   // 4KB
        };
        
        let file_size = base_file_size * size_multiplier;
        
        // Create varied content to simulate real data
        let content = match i % 4 {
            0 => format!("IMAGE_DATA_{:03}:{}", i, "A".repeat(file_size - 50)),
            1 => format!("TEXT_DATA_{:03}:{}", i, "B".repeat(file_size - 50)),
            2 => format!("BINARY_DATA_{:03}:{}", i, "C".repeat(file_size - 50)),
            _ => format!("JSON_DATA_{:03}:{}", i, "D".repeat(file_size - 50)),
        };
        
        std::fs::write(&file_path, &content)?;
    }
    
    let upload_time = upload_start.elapsed();
    println!("✅ File creation completed in {:?}", upload_time);
    
    // Create store and verify all files
    let store_uri = format!("file://{}", temp_path.to_string_lossy());
    let store = store_for_uri(&store_uri)?;
    
    println!("📤 Verifying {} files via ObjectStore...", num_files);
    let file_list = store.list(&store_uri, true).await?;
    assert!(file_list.len() >= num_files, "Should have at least {} files, found {}", num_files, file_list.len());
    
    // Create MultiBackendDataset
    let dataset = MultiBackendDataset::from_prefix(&store_uri).await?;
    println!("✅ Dataset created with {} files", dataset.len());
    
    // Configure high-performance async pooling for large scale
    let pool_config = PoolConfig {
        pool_size: 32,           // High concurrency for large scale
        readahead_batches: 12,   // Aggressive read-ahead
        batch_timeout: Duration::from_secs(15),
        max_inflight: 128,       // High in-flight capacity
    };
    
    let loader_options = LoaderOptions {
        batch_size: 16,          // Larger batches for efficiency
        drop_last: false,
        shuffle: true,
        seed: 789,
        num_workers: 8,          // More workers for parallelism
        prefetch: 24,            // Large prefetch buffer
        auto_tune: true,
        reader_mode: ReaderMode::Sequential,
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        ..Default::default()
    };
    
    println!("⚙️ Configured HIGH-PERFORMANCE AsyncPoolDataLoader:");
    println!("   - Batch size: {} (large batches for efficiency)", loader_options.batch_size);
    println!("   - Pool size: {} concurrent requests", pool_config.pool_size);
    println!("   - Read-ahead batches: {}", pool_config.readahead_batches);
    println!("   - Max in-flight: {}", pool_config.max_inflight);
    println!("   - Workers: {}", loader_options.num_workers);
    println!("   - Prefetch buffer: {}", loader_options.prefetch);
    
    // Create AsyncPoolDataLoader
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_options);
    let mut stream = dataloader.stream_with_pool(pool_config);
    
    let processing_start = Instant::now();
    let mut total_batches = 0;
    let mut total_files_processed = 0;
    let mut total_bytes_processed = 0u64;
    let mut batch_times = Vec::new();
    let mut content_type_counts = std::collections::HashMap::new();
    
    println!("🚀 Starting HIGH-PERFORMANCE async batch processing...");
    
    // Process all batches with detailed metrics
    while let Some(batch_result) = stream.next().await {
        let batch_start = Instant::now();
        
        match batch_result {
            Ok(batch) => {
                total_batches += 1;
                let files_in_batch = batch.len();
                total_files_processed += files_in_batch;
                
                // Process and analyze batch contents
                for file_data in &batch {
                    total_bytes_processed += file_data.len() as u64;
                    
                    // Verify content integrity
                    assert!(!file_data.is_empty(), "File data should not be empty");
                    assert!(file_data.len() >= 100, "File should have substantial content");
                    
                    // Analyze content types
                    let content_str = String::from_utf8_lossy(&file_data[..50]);
                    if content_str.contains("IMAGE_DATA") {
                        *content_type_counts.entry("IMAGE").or_insert(0) += 1;
                    } else if content_str.contains("TEXT_DATA") {
                        *content_type_counts.entry("TEXT").or_insert(0) += 1;
                    } else if content_str.contains("BINARY_DATA") {
                        *content_type_counts.entry("BINARY").or_insert(0) += 1;
                    } else if content_str.contains("JSON_DATA") {
                        *content_type_counts.entry("JSON").or_insert(0) += 1;
                    }
                }
                
                let batch_time = batch_start.elapsed();
                batch_times.push(batch_time);
                
                println!("📦 Batch {} processed: {} files, {:.1}KB in {:?}", 
                        total_batches, files_in_batch, 
                        batch.iter().map(|f| f.len()).sum::<usize>() as f64 / 1024.0,
                        batch_time);
                
            }
            Err(e) => {
                panic!("Batch processing failed: {}", e);
            }
        }
    }
    
    let total_processing_time = processing_start.elapsed();
    let total_time = upload_start.elapsed();
    
    // Calculate comprehensive performance metrics
    let avg_batch_time = batch_times.iter().sum::<Duration>() / batch_times.len() as u32;
    let min_batch_time = batch_times.iter().min().unwrap();
    let max_batch_time = batch_times.iter().max().unwrap();
    let files_per_second = total_files_processed as f64 / total_processing_time.as_secs_f64();
    let mb_per_second = (total_bytes_processed as f64 / 1024.0 / 1024.0) / total_processing_time.as_secs_f64();
    
    println!("\n🎯 LARGE-SCALE PERFORMANCE RESULTS:");
    println!("   ✅ Total files processed: {}", total_files_processed);
    println!("   ✅ Total data processed: {:.2} MB", total_bytes_processed as f64 / 1024.0 / 1024.0);
    println!("   ✅ Total batches: {}", total_batches);
    println!("   ✅ Upload time: {:?}", upload_time);
    println!("   ✅ Processing time: {:?}", total_processing_time);
    println!("   ✅ Total time: {:?}", total_time);
    println!("\n📊 THROUGHPUT METRICS:");
    println!("   ⚡ Files per second: {:.2}", files_per_second);
    println!("   ⚡ MB per second: {:.2}", mb_per_second);
    println!("   ⚡ Average batch time: {:?}", avg_batch_time);
    println!("   ⚡ Min batch time: {:?}", min_batch_time);
    println!("   ⚡ Max batch time: {:?}", max_batch_time);
    
    println!("\n🔍 CONTENT TYPE ANALYSIS:");
    for (content_type, count) in &content_type_counts {
        println!("   📄 {}: {} files", content_type, count);
    }
    
    // Verify comprehensive processing
    assert!(total_files_processed >= num_files, 
           "Should have processed at least {} files, got {}", num_files, total_files_processed);
    assert!(total_batches >= 7, "Should have processed multiple batches");
    assert!(total_bytes_processed > 0, "Should have processed actual data");
    
    // Verify high-performance characteristics
    assert!(files_per_second > 100.0, 
           "Large-scale processing should achieve high throughput (got {:.2} files/sec)", files_per_second);
    assert!(mb_per_second > 1.0, 
           "Should achieve reasonable data throughput (got {:.2} MB/sec)", mb_per_second);
    assert!(total_processing_time < Duration::from_secs(10), 
           "Large-scale processing should complete efficiently");
    
    // Verify content type diversity was preserved
    assert!(content_type_counts.len() >= 3, "Should have processed diverse content types");
    
    println!("\n✅ LARGE-SCALE test completed successfully!");
    println!("✅ Processed {} files ({:.2} MB) in {:?} with {:.2} files/sec throughput", 
            total_files_processed, 
            total_bytes_processed as f64 / 1024.0 / 1024.0,
            total_processing_time,
            files_per_second);
    
    Ok(())
}

/// Test concurrent processing with multiple datasets
#[test]
fn test_concurrent_multi_dataset_processing() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let result = run_concurrent_test().await;
        assert!(result.is_ok(), "Concurrent test failed: {:?}", result.err());
    });
}

async fn run_concurrent_test() -> Result<()> {
    println!("🔄 Testing concurrent multi-dataset processing...");
    
    // Create multiple datasets
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;
    let num_files_per_dataset = 30;
    
    // Create dataset 1 - "training" data
    for i in 0..num_files_per_dataset {
        let file_path = temp_dir1.path().join(format!("train_{:02}.txt", i));
        let content = format!("TRAINING_DATA_{}: {}", i, "X".repeat(1000));
        std::fs::write(&file_path, &content)?;
    }
    
    // Create dataset 2 - "validation" data  
    for i in 0..num_files_per_dataset {
        let file_path = temp_dir2.path().join(format!("val_{:02}.txt", i));
        let content = format!("VALIDATION_DATA_{}: {}", i, "Y".repeat(800));
        std::fs::write(&file_path, &content)?;
    }
    
    let store_uri1 = format!("file://{}", temp_dir1.path().to_string_lossy());
    let store_uri2 = format!("file://{}", temp_dir2.path().to_string_lossy());
    
    // Create datasets
    let dataset1 = MultiBackendDataset::from_prefix(&store_uri1).await?;
    let dataset2 = MultiBackendDataset::from_prefix(&store_uri2).await?;
    
    println!("✅ Created two datasets: {} and {} files", dataset1.len(), dataset2.len());
    
    // Configure for concurrent processing
    let pool_config = PoolConfig {
        pool_size: 20,
        readahead_batches: 6,
        batch_timeout: Duration::from_secs(5),
        max_inflight: 40,
    };
    
    let loader_options = LoaderOptions {
        batch_size: 6,
        shuffle: true,
        seed: 456,
        loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
        ..Default::default()
    };
    
    // Create concurrent dataloaders
    let dataloader1 = AsyncPoolDataLoader::new(dataset1, loader_options.clone());
    let dataloader2 = AsyncPoolDataLoader::new(dataset2, loader_options);
    
    let mut stream1 = dataloader1.stream_with_pool(pool_config.clone());
    let mut stream2 = dataloader2.stream_with_pool(pool_config);
    
    let start_time = Instant::now();
    let mut total_files = 0;
    let mut training_files = 0;
    let mut validation_files = 0;
    
    println!("🔄 Processing both datasets sequentially (simulating concurrent workload)...");
    
    // Process first dataset
    println!("📚 Processing training dataset...");
    while let Some(batch_result) = stream1.next().await {
        match batch_result {
            Ok(batch) => {
                training_files += batch.len();
                total_files += batch.len();
                println!("📚 Training batch: {} files", batch.len());
                
                // Verify training data
                for file_data in &batch {
                    let content = String::from_utf8_lossy(file_data);
                    assert!(content.contains("TRAINING_DATA"), "Should be training data");
                }
            }
            Err(e) => panic!("Training batch error: {}", e),
        }
    }
    
    // Process second dataset  
    println!("� Processing validation dataset...");
    while let Some(batch_result) = stream2.next().await {
        match batch_result {
            Ok(batch) => {
                validation_files += batch.len();
                total_files += batch.len();
                println!("🔍 Validation batch: {} files", batch.len());
                
                // Verify validation data
                for file_data in &batch {
                    let content = String::from_utf8_lossy(file_data);
                    assert!(content.contains("VALIDATION_DATA"), "Should be validation data");
                }
            }
            Err(e) => panic!("Validation batch error: {}", e),
        }
    }
    
    let total_time = start_time.elapsed();
    
    println!("\n🎯 MULTI-DATASET PROCESSING RESULTS:");
    println!("   ✅ Training files processed: {}", training_files);
    println!("   ✅ Validation files processed: {}", validation_files);
    println!("   ✅ Total files processed: {}", total_files);
    println!("   ✅ Total time: {:?}", total_time);
    println!("   ✅ Files per second: {:.2}", total_files as f64 / total_time.as_secs_f64());
    
    // Verify multi-dataset processing worked
    assert!(training_files >= num_files_per_dataset, "Should process all training files");
    assert!(validation_files >= num_files_per_dataset, "Should process all validation files");
    assert!(total_time < Duration::from_secs(10), "Multi-dataset processing should be efficient");
    
    println!("✅ Multi-dataset processing test completed successfully!");
    
    Ok(())
}
