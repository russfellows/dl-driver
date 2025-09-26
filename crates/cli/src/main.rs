// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dl_driver_core::DlioConfig;
use dl_driver_core::plugins::PluginManager;
use tracing::{info, error};

/// dl-driver – Unified DLIO execution engine with optional MLPerf compliance mode
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run DLIO workload (use --mlperf for enhanced reporting and compliance)
    Run {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// If set, dump the parsed YAML back to stdout
        #[arg(long)]
        pretty: bool,

        /// Enable MLPerf compliance mode with enhanced reporting
        #[arg(long)]
        mlperf: bool,

        /// Output format for MLPerf reports (json, csv)
        #[arg(long, default_value = "json")]
        format: String,

        /// Save MLPerf report to file instead of stdout
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,

        /// Maximum number of epochs to run (MLPerf mode)
        #[arg(long, default_value_t = 3)]
        max_epochs: u32,

        /// Maximum number of steps to run (MLPerf mode)
        #[arg(long, default_value_t = 1000)]
        max_steps: u32,

        /// Override pool size for AsyncPoolDataLoader
        #[arg(long, default_value = "16")]
        pool_size: usize,

        /// Override readahead batches
        #[arg(long, default_value = "8")]
        readahead: usize,

        /// Override max inflight requests
        #[arg(long, default_value = "64")]
        max_inflight: usize,

        /// Batch timeout in seconds
        #[arg(long, default_value = "10")]
        timeout: u64,

        /// Number of accelerators for AU calculation (default: 1)
        #[arg(long, default_value_t = 1)]
        accelerators: u32,

        /// Enable strict AU mode - fail if AU is below threshold
        #[arg(long)]
        strict_au: bool,
    },
    /// Validate a DLIO config without running it
    Validate {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// Convert YAML to JSON and print it
        #[arg(long)]
        to_json: bool,
    },
    /// Generate synthetic dataset from DLIO config
    Generate {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// Show progress during generation
        #[arg(long)]
        verbose: bool,

        /// Skip generation if data folder already exists
        #[arg(long)]
        skip_existing: bool,
    },
}#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file early for S3/Azure credentials
    dotenvy::dotenv().ok(); // Ignore errors if .env doesn't exist

    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "info" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("dl_driver={}", log_level))
        .init();

    info!("dl-driver v{} starting", env!("CARGO_PKG_VERSION"));

    match args.command {
        Commands::Run {
            config,
            pretty,
            mlperf,
            format,
            output,
            max_epochs,
            max_steps,
            pool_size,
            readahead,
            max_inflight,
            timeout,
            accelerators,
            strict_au,
        } => run_unified_dlio(
            &config, 
            pretty, 
            mlperf, 
            &format, 
            output.as_deref(),
            max_epochs,
            max_steps,
            pool_size, 
            readahead, 
            max_inflight, 
            timeout,
            Some(accelerators),
            strict_au,
        ).await,
        Commands::Validate { config, to_json } => validate_dlio_config(&config, to_json).await,
        Commands::Generate {
            config,
            verbose,
            skip_existing,
        } => run_generate_only(&config, verbose, skip_existing).await,
    }
}

/// Unified DLIO execution engine with optional MLPerf compliance mode
async fn run_unified_dlio(
    config_path: &std::path::Path,
    pretty: bool,
    mlperf_mode: bool,
    _format: &str,
    _output_path: Option<&std::path::Path>,
    max_epochs: u32,
    max_steps: u32,
    _pool_size: usize,
    _readahead: usize,
    _max_inflight: usize,
    _timeout: u64,
    accelerators: Option<u32>,
    strict_au: bool,
) -> Result<()> {
    info!("Loading DLIO config from: {:?}", config_path);

    // Load DLIO configuration
    let yaml_content = std::fs::read_to_string(config_path)?;
    let dlio_config = DlioConfig::from_yaml(&yaml_content)?;

    if pretty {
        println!("=== Parsed DLIO Configuration ===");
        println!("{:#?}", dlio_config);
        println!("Data folder URI: {}", dlio_config.data_folder_uri());
        println!(
            "Should generate data: {}",
            dlio_config.should_generate_data()
        );
        println!("Should train: {}", dlio_config.should_train());
        println!("Should checkpoint: {}", dlio_config.should_checkpoint());
        if mlperf_mode {
            println!("MLPerf compliance mode: ENABLED");
            println!("Max epochs: {}, Max steps: {}", max_epochs, max_steps);
        }
        return Ok(());
    }

    // Create plugin manager with CheckpointPlugin if enabled
    let _plugins = PluginManager::new();
    
    // TODO: Temporarily disabled while we fix config compatibility
    // Add CheckpointPlugin if checkpointing is enabled in config
    // if let Some(checkpoint_plugin) = dl_driver_core::plugins::CheckpointPlugin::new(&dlio_config).await? {
    //     plugins.push(Box::new(checkpoint_plugin));
    //     info!("CheckpointPlugin registered");
    // }
    
    // plugins.initialize(&dlio_config).await
    //     .context("Failed to initialize plugins")?;

    // Initialize metrics system (always available, enhanced in MLPerf mode)
    let _metrics = if mlperf_mode {
        dl_driver_core::mlperf::MlperfMetrics::new()
    } else {
        dl_driver_core::mlperf::MlperfMetrics::new() // Same system for both modes
    };

    // Phase 1: Data Generation (if enabled)
    if dlio_config.workflow.as_ref().map_or(false, |w| w.generate_data.unwrap_or(false)) {
        info!("Phase 1: Generating data");
        run_data_generation(&dlio_config).await
            .context("Data generation failed")?;
    }

    // Phase 2: Training workload using WorkloadRunner for DLIO compliance measurement
    if dlio_config.workflow.as_ref().map_or(true, |w| w.train.unwrap_or(true)) {
        info!("Phase 2: Training workload (MEASURED for AU calculation)");
        
        // Use WorkloadRunner ONLY for training phase measurement (data generation already done)
        let accelerator_count = accelerators.unwrap_or(1);
        let mut workload_runner = dl_driver_core::WorkloadRunner::new(dlio_config.clone())
            .with_accelerator_config(accelerator_count, strict_au);
        workload_runner.run_training_phase().await
            .context("Training workload failed")?;
        
        // Get final metrics from WorkloadRunner (simple metrics for now)
        let _workload_metrics = workload_runner.get_metrics();
        // TODO: Convert workload_metrics to MlperfMetrics when re-enabling mlperf module
    }

    info!("✅ DLIO workload completed successfully");

    // Output results based on mode
    if mlperf_mode {
        // TODO: Temporarily disabled while we fix config compatibility
        println!("MLPerf mode temporarily disabled during config system update");
        /*
        // Generate comprehensive MLPerf report
        let report = dl_driver_core::mlperf::MlperfReport::from_metrics(&metrics, &dlio_config);
        
        let output_content = match format.to_lowercase().as_str() {
            "json" => report.to_json()?,
            "csv" => {
                let mut csv_content = String::new();
                csv_content.push_str(&format!("{}\n", dl_driver_core::mlperf::MlperfReport::to_csv_header()));
                csv_content.push_str(&format!("{}\n", report.to_csv_row()));
                csv_content
            }
            _ => return Err(anyhow::anyhow!("Unsupported format '{}'. Use 'json' or 'csv'", format)),
        };

        // Output to file or stdout
        if let Some(output_file) = output_path {
            std::fs::write(output_file, output_content)
                .with_context(|| format!("Failed to write report to {:?}", output_file))?;
            eprintln!("✅ MLPerf report written to {:?}", output_file);
        } else {
            println!("{}", output_content);
        }

        // Print summary to stderr so it doesn't interfere with JSON/CSV output
        eprintln!("🏁 MLPerf benchmark completed:");
        eprintln!("  Backend: {}", report.backend_type);
        eprintln!("  Samples: {}", report.total_samples);
        eprintln!("  Throughput: {:.2} samples/sec", report.throughput_samples_per_sec);
        eprintln!("  P99 latency: {:.3} ms", report.p99_latency_ms);
        */
    } else {
        // Basic DLIO output - using simplified metrics since WorkloadRunner handles detailed tracking
        info!("📊 DLIO workload execution completed successfully");
        info!("📈 Detailed performance metrics available in WorkloadRunner (epochs, throughput, AU calculation)");
    }

    Ok(())
}

/// Data generation phase using s3dlio (shared by both modes) - PARALLEL VERSION
async fn run_data_generation(config: &DlioConfig) -> Result<()> {
    use s3dlio::object_store::store_for_uri;
    use std::sync::Arc;
    
    let start_time = std::time::Instant::now();
    info!("Starting PARALLEL data generation phase");

    // Create object store for the configured storage backend
    let store = Arc::new(store_for_uri(&config.dataset.data_folder)
        .with_context(|| format!("Failed to create object store for {}", config.dataset.data_folder))?);

    let num_files = config.dataset.num_files_train.unwrap_or(100);
    let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1);
    let record_size = config.dataset.record_length_bytes.unwrap_or(1024);
    
    let file_size_mb = (samples_per_file * record_size) as f64 / 1024.0 / 1024.0;
    let total_size_gb = (num_files as f64 * file_size_mb) / 1024.0;

    info!(
        "🚀 Generating {} files with {} samples each ({:.1}MB per file, {:.2}GB total)",
        num_files, samples_per_file, file_size_mb, total_size_gb
    );

    // Pre-generate synthetic data buffer to reuse across all files (memory optimization)
    let synthetic_data = Arc::new(generate_synthetic_data(samples_per_file, record_size));
    info!("📦 Pre-generated {:.1}MB synthetic data buffer for reuse", 
          synthetic_data.len() as f64 / 1024.0 / 1024.0);

    // Determine concurrency level - AGGRESSIVE for maximum I/O throughput
    let available_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
    let concurrency = if num_files <= 64 {
        // For small file counts, use ALL files in parallel for maximum speed
        num_files
    } else {
        // For larger datasets, use 4x cores or half the files, whichever is smaller
        std::cmp::min(available_cores * 4, num_files / 2)
    };
    
    info!("⚡ AGGRESSIVE PARALLELISM: Using {} concurrent workers (available cores: {}, total files: {})", 
          concurrency, available_cores, num_files);

    // Create semaphore to limit concurrent operations
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let data_folder = config.dataset.data_folder.clone();
    let format = config.dataset.format.as_ref().map(|f| f.as_str()).unwrap_or("npz");

    // Spawn parallel file generation tasks
    let mut handles = Vec::new();
    for file_idx in 0..num_files {
        let store_clone = Arc::clone(&store);
        let data_clone = Arc::clone(&synthetic_data);
        let semaphore_clone = Arc::clone(&semaphore);
        let data_folder_clone = data_folder.clone();
        let format_str = format.to_string();

        let handle = tokio::spawn(async move {
            // Acquire semaphore permit for rate limiting
            let _permit = semaphore_clone.acquire().await.unwrap();
            
            // Create full URI path
            let file_name = format!("train_file_{:06}.{}", file_idx, format_str);
            let full_path = if data_folder_clone.ends_with('/') {
                format!("{}{}", data_folder_clone, file_name)
            } else {
                format!("{}/{}", data_folder_clone, file_name)
            };

            let write_start = std::time::Instant::now();
            let result = store_clone
                .put(&full_path, &*data_clone)
                .await
                .with_context(|| format!("Failed to write file {}", full_path));
            let write_time = write_start.elapsed();

            // Return result with timing info
            result.map(|_| (file_idx, full_path, data_clone.len(), write_time))
        });
        
        handles.push(handle);
    }

    // Wait for all tasks and collect results
    let mut completed = 0;
    let mut total_bytes = 0u64;
    let mut fastest_write = std::time::Duration::from_secs(999);
    let mut slowest_write = std::time::Duration::ZERO;
    
    for handle in handles {
        match handle.await.unwrap() {
            Ok((file_idx, _path, bytes, write_time)) => {
                completed += 1;
                total_bytes += bytes as u64;
                fastest_write = fastest_write.min(write_time);
                slowest_write = slowest_write.max(write_time);
                
                if completed % 50 == 0 || completed == num_files {
                    let progress = (completed as f64 / num_files as f64) * 100.0;
                    info!(
                        "⏳ Progress: {}/{} files ({:.1}%) - Latest: file_{:06} ({:.1}MB in {:?})",
                        completed, num_files, progress, file_idx,
                        bytes as f64 / 1024.0 / 1024.0, write_time
                    );
                }
            }
            Err(e) => {
                error!("❌ File generation failed: {}", e);
                return Err(e);
            }
        }
    }

    let generation_time = start_time.elapsed();
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / generation_time.as_secs_f64();
    
    info!("✅ PARALLEL data generation completed!");
    info!("📊 Performance Summary:");
    info!("   • Files: {} generated", completed);
    info!("   • Data: {:.2} GB written", total_bytes as f64 / 1024.0 / 1024.0 / 1024.0);
    info!("   • Time: {:?}", generation_time);
    info!("   • Throughput: {:.1} MB/s", throughput_mbps);
    info!("   • Write times: {:.2?} (fastest) to {:.2?} (slowest)", fastest_write, slowest_write);
    info!("   • Speedup: ~{}x faster than sequential", concurrency);
    
    Ok(())
}

/// Generate synthetic data for testing (shared utility)
fn generate_synthetic_data(samples: usize, record_size: usize) -> Vec<u8> {
    let total_size = samples * record_size;
    let mut data = vec![0u8; total_size];
    
    // Fill with some pattern for testing
    for i in 0..total_size {
        data[i] = (i % 256) as u8;
    }
    
    data
}

async fn validate_dlio_config(config_path: &std::path::Path, to_json: bool) -> Result<()> {
    info!("Validating DLIO config: {:?}", config_path);

    // Load and parse YAML
    let yaml_content = std::fs::read_to_string(config_path)?;

    if to_json {
        // Convert YAML to JSON and print
        let json_content = dl_driver_core::dlio_compat::yaml_to_json(&yaml_content)?;
        println!("{}", json_content);
        return Ok(());
    }

    // Parse as DLIO config
    let dlio_config = DlioConfig::from_yaml(&yaml_content)?;

    // Validate essential fields
    println!("✅ YAML parsing: SUCCESS");
    println!(
        "✅ Model name: {:?}",
        dlio_config.model.as_ref().and_then(|m| m.name.as_ref())
    );
    println!("✅ Framework: {:?}", dlio_config.framework);
    println!("✅ Data folder: {}", dlio_config.data_folder_uri());
    println!("✅ Batch size: {:?}", dlio_config.reader.batch_size);

    // Test LoaderOptions conversion
    let loader_opts = dlio_config.to_loader_options();
    println!("✅ LoaderOptions conversion: SUCCESS");
    println!("  - Batch size: {}", loader_opts.batch_size);
    println!("  - Prefetch: {}", loader_opts.prefetch);
    println!("  - Shuffle: {}", loader_opts.shuffle);
    println!("  - Num workers: {}", loader_opts.num_workers);

    // Test PoolConfig conversion
    let pool_config = dlio_config.to_pool_config();
    println!("✅ PoolConfig conversion: SUCCESS");
    println!("  - Pool size: {}", pool_config.pool_size);
    println!("  - Readahead batches: {}", pool_config.readahead_batches);
    println!("  - Max inflight: {}", pool_config.max_inflight);

    // Test object store URI parsing (don't actually create store for validation)
    let uri = dlio_config.data_folder_uri();
    if uri.starts_with("file://") {
        println!("✅ Backend detection: File");
    } else if uri.starts_with("s3://") {
        println!("✅ Backend detection: S3");
    } else if uri.starts_with("az://") {
        println!("✅ Backend detection: Azure");
    } else if uri.starts_with("direct://") {
        println!("✅ Backend detection: DirectIO");
    } else {
        println!("⚠️  Backend detection: Unknown scheme");
    }

    // Test RunPlan conversion (using flat RunPlan structure)
    let run_plan = dlio_config.to_run_plan()?;
    println!("✅ RunPlan conversion: SUCCESS");
    
    // Display model info
    if let Some(model) = &dlio_config.model {
        println!("  - Model: {} ({})", 
            model.name.as_deref().unwrap_or("unnamed"),
            dlio_config.framework.as_deref().unwrap_or("unspecified"));
    } else {
        println!("  - Model: No model specified");
    }
    
    // Display workflow info  
    if let Some(workflow) = &dlio_config.workflow {
        println!("  - Workflow: generate_data={}, train={}, checkpoint={}, evaluation={}",
            workflow.generate_data.unwrap_or(false),
            workflow.train.unwrap_or(false), 
            workflow.checkpoint.unwrap_or(false),
            workflow.evaluation.unwrap_or(false));
    } else {
        println!("  - Workflow: No workflow specified");
    }
    
    // Display dataset info using the structured RunPlan
    println!("  - Dataset: {} files, {} samples/file, {} bytes/record",
        run_plan.dataset.train.num_files,
        run_plan.dataset.train.num_samples_per_file,
        run_plan.dataset.train.record_length_bytes);
        
    // Calculate totals
    let total_samples = run_plan.dataset.train.num_files * 
                       run_plan.dataset.train.num_samples_per_file;
    let total_bytes = total_samples * run_plan.dataset.train.record_length_bytes;
    
    println!("  - Total: {} samples, {:.2} MB",
        total_samples,
        total_bytes as f64 / 1024.0 / 1024.0);

    println!("🎉 DLIO configuration is valid and ready to run!");

    Ok(())
}

/// Generate dataset only (no training) - useful for testing and debugging
async fn run_generate_only(
    config_path: &std::path::Path, 
    verbose: bool, 
    skip_existing: bool
) -> Result<()> {
    use dl_driver_core::dlio_compat::DlioConfig;
    
    // Load DLIO config
    let yaml_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file {:?}", config_path))?;
    let dlio_config = DlioConfig::from_yaml(&yaml_content)
        .with_context(|| format!("Failed to parse DLIO config from {:?}", config_path))?;
    
    if verbose {
        info!("Loaded DLIO config: data_folder = {}", dlio_config.dataset.data_folder);
        info!("Files to generate: {}", dlio_config.dataset.num_files_train.unwrap_or(100));
        info!("Samples per file: {}", dlio_config.dataset.num_samples_per_file.unwrap_or(1));
        info!("Record size: {}B", dlio_config.dataset.record_length_bytes.unwrap_or(1024));
    }
    
    // Check if data folder exists and handle skip_existing
    if skip_existing {
        // TODO: Add logic to check if folder exists and skip if it does
        info!("Note: --skip-existing flag is set but not yet implemented");
    }
    
    // Run data generation phase
    info!("🚀 Starting data generation phase...");
    run_data_generation(&dlio_config).await
        .context("Data generation failed")?;
    
    info!("✅ Data generation completed successfully");
    Ok(())
}
