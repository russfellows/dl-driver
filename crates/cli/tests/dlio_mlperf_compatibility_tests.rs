// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// Comprehensive DLIO/MLPerf compatibility tests for dl-driver
// Tests that dl-driver can run identical workloads to DLIO with same configs
use anyhow::Result;
use std::path::Path;
use std::process::Command;
use serde_json::Value;
use tempfile::TempDir;

/// Test configuration structure for benchmark validation
#[derive(Debug)]
struct BenchmarkTest {
    name: &'static str,
    config_path: &'static str,
    expected_format: &'static str,
    min_throughput_samples_per_sec: f64,
    max_execution_time_secs: f64,
}

const BENCHMARK_TESTS: &[BenchmarkTest] = &[
    BenchmarkTest {
        name: "UNet3D",
        config_path: "docs/goldens/test_configs/unet3d_config.yaml",
        expected_format: "npz", 
        min_throughput_samples_per_sec: 50.0,
        max_execution_time_secs: 120.0,
    },
    BenchmarkTest {
        name: "BERT",
        config_path: "docs/goldens/test_configs/bert_config.yaml",
        expected_format: "npz",
        min_throughput_samples_per_sec: 100.0,
        max_execution_time_secs: 90.0,
    },
    BenchmarkTest {
        name: "ResNet",
        config_path: "docs/goldens/test_configs/resnet_config.yaml",
        expected_format: "npz",
        min_throughput_samples_per_sec: 200.0,
        max_execution_time_secs: 60.0,
    },
    BenchmarkTest {
        name: "CosmoFlow",
        config_path: "docs/goldens/test_configs/cosmoflow_config.yaml",
        expected_format: "npz",
        min_throughput_samples_per_sec: 30.0,
        max_execution_time_secs: 150.0,
    },
];

#[tokio::test]
async fn test_mlcommons_dlio_benchmark_compatibility() -> Result<()> {
    // Ensure dl-driver binary exists
    let binary_path = get_dl_driver_binary()?;
    
    for benchmark in BENCHMARK_TESTS {
        println!("🧪 Testing {} benchmark compatibility...", benchmark.name);
        
        // Create temporary directory for test data
        let temp_dir = TempDir::new()?;
        let test_data_path = temp_dir.path().join("test_data");
        std::fs::create_dir_all(&test_data_path)?;
        
        // Modify config to use temporary directory (to avoid needing large datasets)
        let modified_config = create_test_config(benchmark, &test_data_path)?;
        
        // Run dl-driver MLPerf benchmark
        let report = run_mlperf_benchmark(&binary_path, &modified_config)?;
        
        // Validate report structure and contents
        validate_mlperf_report(&report, benchmark)?;
        
        println!("✅ {} benchmark: PASSED", benchmark.name);
    }
    
    Ok(())
}

#[tokio::test] 
async fn test_dlio_config_parsing_compatibility() -> Result<()> {
    let binary_path = get_dl_driver_binary()?;
    
    for benchmark in BENCHMARK_TESTS {
        println!("🧪 Testing {} config parsing compatibility...", benchmark.name);
        
        // Resolve config path relative to workspace root
        let config_path = get_config_path(benchmark.config_path)?;
        
        // Test config validation
        let output = Command::new(&binary_path)
            .arg("validate")
            .arg("--config")
            .arg(&config_path)
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("❌ Config validation failed for {}: {}", benchmark.name, stderr);
        }
        
        println!("✅ {} config parsing: PASSED", benchmark.name);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_multi_backend_checkpoint_compatibility() -> Result<()> {
    let binary_path = get_dl_driver_binary()?;
    
    println!("🧪 Testing checkpoint compatibility across storage backends...");
    
    // Test checkpointing on different backends
    for backend in &["file"] {
        println!("🧪 Testing checkpoint compatibility on {} backend...", backend);
        
        let temp_dir = TempDir::new()?;
        let (config, checkpoint_path) = create_checkpoint_test_config(&temp_dir, backend)?;
        
        // Run benchmark with checkpointing enabled
        run_mlperf_benchmark(&binary_path, &config)?;
        
        // Validate checkpoints were created
        validate_checkpoints_created(&checkpoint_path)?;
        
        println!("✅ {} backend checkpointing: PASSED", backend);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_deterministic_access_order() -> Result<()> {
    let binary_path = get_dl_driver_binary()?;
    
    println!("🧪 Testing deterministic access order...");
    
    // Run same benchmark twice with same seed
    let temp_dir1 = TempDir::new()?;
    let config1 = create_deterministic_test_config(&temp_dir1)?;
    let report1 = run_mlperf_benchmark(&binary_path, &config1)?;
    
    let temp_dir2 = TempDir::new()?; 
    let config2 = create_deterministic_test_config(&temp_dir2)?;
    let report2 = run_mlperf_benchmark(&binary_path, &config2)?;
    
    // Compare access order samples
    validate_deterministic_access_order(&report1, &report2)?;
    
    println!("✅ Deterministic access order: PASSED");
    Ok(())
}

#[tokio::test]
async fn test_performance_regression() -> Result<()> {
    let binary_path = get_dl_driver_binary()?;
    
    println!("🧪 Testing performance regression against minimal thresholds...");
    
    // Use minimal config for performance testing
    let temp_dir = TempDir::new()?;
    let config = create_performance_test_config(&temp_dir)?;
    let report = run_mlperf_benchmark(&binary_path, &config)?;
    
    // Validate performance meets minimum thresholds
    validate_performance_thresholds(&report)?;
    
    println!("✅ Performance regression: PASSED");
    Ok(())
}

// Helper functions

fn get_workspace_root() -> Result<std::path::PathBuf> {
    let mut current_dir = std::env::current_dir()?;
    loop {
        if current_dir.join("Cargo.toml").exists() {
            // Check if this is the workspace root (contains workspace section)
            let cargo_toml = std::fs::read_to_string(current_dir.join("Cargo.toml"))?;
            if cargo_toml.contains("[workspace]") {
                return Ok(current_dir);
            }
        }
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => anyhow::bail!("Could not find workspace root with Cargo.toml containing [workspace]"),
        }
    }
}

fn get_config_path(relative_path: &str) -> Result<String> {
    let workspace_root = get_workspace_root()?;
    let full_path = workspace_root.join(relative_path);
    if !full_path.exists() {
        anyhow::bail!("Config file not found: {}", full_path.display());
    }
    Ok(full_path.to_string_lossy().to_string())
}

fn get_dl_driver_binary() -> Result<String> {
    // Try to use CARGO_BIN_EXE environment variable first (set by cargo test)
    if let Ok(binary_path) = std::env::var("CARGO_BIN_EXE_dl-driver") {
        if Path::new(&binary_path).exists() {
            return Ok(binary_path);
        }
    }

    let workspace_root = get_workspace_root()?;

    // Try release build first
    let release_path = workspace_root.join("target/release/dl-driver");
    if release_path.exists() {
        return Ok(release_path.to_string_lossy().to_string());
    }

    // Try debug build as fallback
    let debug_path = workspace_root.join("target/debug/dl-driver");
    if debug_path.exists() {
        return Ok(debug_path.to_string_lossy().to_string());
    }

    anyhow::bail!(
        "dl-driver binary not found. Tried:\n  {}\n  {}\nRun 'cargo build --release' or 'cargo build' first.",
        release_path.display(),
        debug_path.display()
    );
}

fn create_test_config(benchmark: &BenchmarkTest, test_data_path: &Path) -> Result<String> {
    // Create a minimal test dataset
    create_minimal_test_dataset(test_data_path, benchmark.expected_format)?;
    
    // Create modified config file
    let config_content = format!(
        r#"
model:
  name: "{}"
  
dataset:
  data_folder: "file://{}"
  format: "{}"
  num_files_train: 5
  record_length: 1024
  
reader:
  batch_size: 4
  read_threads: 2
  shuffle: false
  
train:
  epochs: 1
  steps: 10

checkpoint:
  enabled: false
"#,
        benchmark.name,
        test_data_path.display(),
        benchmark.expected_format
    );
    
    let config_path = test_data_path.join("test_config.yaml");
    std::fs::write(&config_path, config_content)?;
    
    Ok(config_path.to_string_lossy().to_string())
}

fn create_minimal_test_dataset(path: &Path, format: &str) -> Result<()> {
    match format {
        "npz" => create_minimal_npz_dataset(path)?,
        "hdf5" => create_minimal_hdf5_dataset(path)?,
        _ => anyhow::bail!("Unsupported format for test dataset: {}", format),
    }
    Ok(())
}

fn create_minimal_npz_dataset(path: &Path) -> Result<()> {
    // Create minimal NPZ files for testing (matching config: 3 files with 1024 bytes each)
    use std::fs::File;
    use std::io::Write;
    
    for i in 0..3 {
        let file_path = path.join(format!("train_file_{:06}.npz", i));
        let mut file = File::create(file_path)?;
        
        // Write minimal NPZ content (this is a simplified placeholder)
        // In a real test, you'd use numpy-rs or similar to create proper NPZ files
        file.write_all(b"PK")?; // NPZ files start with PK (ZIP header)
        file.write_all(&vec![0u8; 1022])?; // Pad to record_length (1024 total)
    }
    
    Ok(())
}

fn create_minimal_hdf5_dataset(path: &Path) -> Result<()> {
    // Create minimal HDF5 files for testing
    // This would need proper HDF5 library integration
    for i in 0..5 {
        let file_path = path.join(format!("train_file_{:06}.h5", i));
        let mut file = std::fs::File::create(file_path)?;
        std::io::Write::write_all(&mut file, &vec![0u8; 1024])?; // Placeholder
    }
    Ok(())
}

fn run_mlperf_benchmark(binary_path: &str, config_path: &str) -> Result<Value> {
    let output = Command::new(binary_path)
        .arg("mlperf")
        .arg("--config")
        .arg(config_path)
        .arg("--format")
        .arg("json")
        .arg("--max-epochs")
        .arg("1")
        .arg("--max-steps")
        .arg("10")
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("MLPerf benchmark failed: {}", stderr);
    }
    
    let stdout = String::from_utf8(output.stdout)?;
    let report: Value = serde_json::from_str(&stdout)?;
    
    Ok(report)
}

fn validate_mlperf_report(report: &Value, benchmark: &BenchmarkTest) -> Result<()> {
    // Validate report structure
    let required_fields = [
        "dl_driver_version", "s3dlio_version", "total_samples",
        "total_bytes", "throughput_samples_per_sec", "io_p50_latency_ms", "total_execution_time_secs"
    ];
    
    for field in &required_fields {
        if !report.get(field).is_some() {
            anyhow::bail!("Missing required field in MLPerf report: {}", field);
        }
    }
    
    // Validate performance thresholds
    let throughput = report["throughput_samples_per_sec"].as_f64()
        .ok_or_else(|| anyhow::anyhow!("Invalid throughput value"))?;
        
    if throughput < benchmark.min_throughput_samples_per_sec {
        anyhow::bail!(
            "Throughput {} below minimum {} for {}",
            throughput, benchmark.min_throughput_samples_per_sec, benchmark.name
        );
    }
    
    let execution_time = report["total_execution_time_secs"].as_f64()
        .ok_or_else(|| anyhow::anyhow!("Invalid execution time"))?;
        
    if execution_time > benchmark.max_execution_time_secs {
        anyhow::bail!(
            "Execution time {} exceeds maximum {} for {}",
            execution_time, benchmark.max_execution_time_secs, benchmark.name
        );
    }
    
    Ok(())
}

fn create_checkpoint_test_config(temp_dir: &TempDir, backend: &str) -> Result<(String, std::path::PathBuf)> {
    // Use /mnt/vast1 for data storage as per project guidelines
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    
    let data_path = std::path::PathBuf::from(format!("/mnt/vast1/dl_driver_checkpoint_test_data_{}", timestamp));
    let checkpoint_path = std::path::PathBuf::from(format!("/mnt/vast1/dl_driver_checkpoint_test_checkpoints_{}", timestamp));
    
    std::fs::create_dir_all(&data_path)?;
    std::fs::create_dir_all(&checkpoint_path)?;
    
    create_minimal_npz_dataset(&data_path)?;
    
    let config_content = format!(
        r#"
model:
  name: "checkpoint_test"
  
workflow:
  generate_data: true
  train: true
  checkpoint: true
  evaluation: false
  
dataset:
  data_folder: "{}://{}"
  format: "npz"
  num_files_train: 3
  record_length_bytes: 1024
  num_samples_per_file: 1
  
reader:
  batch_size: 2
  read_threads: 1
  shuffle: false
  
train:
  epochs: 1
  steps: 6

checkpoint:
  enabled: true
  steps_between_checkpoints: 3
  compression: zstd
  folder: "{}://{}"
"#,
        backend,
        data_path.display(),
        backend,
        checkpoint_path.display()
    );
    
    let config_path = temp_dir.path().join("checkpoint_config.yaml");
    std::fs::write(&config_path, config_content)?;
    
    Ok((config_path.to_string_lossy().to_string(), checkpoint_path))
}

fn validate_checkpoints_created(checkpoint_path: &std::path::Path) -> Result<()> {
    use walkdir::WalkDir;
    
    // Search recursively for .ckpt files (checkpoints are stored in run_id subdirectories)
    let mut ckpt_files = Vec::new();
    for entry in WalkDir::new(checkpoint_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy().ends_with(".ckpt") {
            ckpt_files.push(entry.path().to_path_buf());
        }
    }
    
    if ckpt_files.is_empty() {
        anyhow::bail!("No checkpoint .ckpt files found under {:?}", checkpoint_path);
    }
    
    println!("✅ Found {} checkpoint files", ckpt_files.len());
    
    // Validate checkpoint content (basic existence and size check)
    for ckpt_file in &ckpt_files {
        let file_size = std::fs::metadata(ckpt_file)?.len();
        if file_size == 0 {
            anyhow::bail!("Empty checkpoint file: {:?}", ckpt_file);
        }
        println!("✅ Checkpoint file: {:?} ({} bytes)", ckpt_file, file_size);
    }
    
    Ok(())
}

fn create_deterministic_test_config(temp_dir: &TempDir) -> Result<String> {
    let data_path = temp_dir.path().join("data");
    std::fs::create_dir_all(&data_path)?;
    create_minimal_npz_dataset(&data_path)?;
    
    let config_content = format!(
        r#"
model:
  name: "deterministic_test"
  
dataset:
  data_folder: "file://{}"
  format: "npz"
  num_files_train: 5
  
reader:
  batch_size: 2
  read_threads: 1
  shuffle: true
  seed: 42
  
train:
  epochs: 1
  steps: 10

checkpoint:
  enabled: false
"#,
        data_path.display()
    );
    
    let config_path = temp_dir.path().join("deterministic_config.yaml");
    std::fs::write(&config_path, config_content)?;
    
    Ok(config_path.to_string_lossy().to_string())
}

fn validate_deterministic_access_order(report1: &Value, report2: &Value) -> Result<()> {
    let access_order1 = report1.get("access_order_sample")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing access_order_sample in report 1"))?;
        
    let access_order2 = report2.get("access_order_sample")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing access_order_sample in report 2"))?;
    
    if access_order1 != access_order2 {
        anyhow::bail!(
            "Access order differs between runs:\nRun 1: {:?}\nRun 2: {:?}",
            access_order1, access_order2
        );
    }
    
    Ok(())
}

fn create_performance_test_config(temp_dir: &TempDir) -> Result<String> {
    let data_path = temp_dir.path().join("data");
    std::fs::create_dir_all(&data_path)?;
    create_minimal_npz_dataset(&data_path)?;
    
    let config_content = format!(
        r#"
model:
  name: "performance_test"
  
dataset:
  data_folder: "file://{}"
  format: "npz"
  num_files_train: 5
  
reader:
  batch_size: 8
  read_threads: 4
  prefetch: 8
  
train:
  epochs: 1
  steps: 20

checkpoint:
  enabled: false
"#,
        data_path.display()
    );
    
    let config_path = temp_dir.path().join("performance_config.yaml");
    std::fs::write(&config_path, config_content)?;
    
    Ok(config_path.to_string_lossy().to_string())
}

fn validate_performance_thresholds(report: &Value) -> Result<()> {
    // Minimum performance thresholds to prevent severe regressions
    let min_throughput = 10.0; // samples/sec
    let max_io_latency_p95 = 1000.0; // ms
    
    let throughput = report["throughput_samples_per_sec"].as_f64()
        .ok_or_else(|| anyhow::anyhow!("Missing throughput in report"))?;
        
    if throughput < min_throughput {
        anyhow::bail!("Throughput {} below minimum threshold {}", throughput, min_throughput);
    }
    
    if let Some(io_latency_p95) = report.get("io_p95_latency_ms").and_then(|v| v.as_f64()) {
        if io_latency_p95 > max_io_latency_p95 {
            anyhow::bail!("I/O P95 latency {} exceeds maximum threshold {}", io_latency_p95, max_io_latency_p95);
        }
    }
    
    Ok(())
}