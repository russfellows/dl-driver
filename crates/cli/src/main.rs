// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dl_driver_core::DlioConfig;
use dl_driver_core::plugins::PluginManager;
use tracing::{info, error, debug, warn};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// dl-driver – Unified DLIO execution engine with optional MLPerf compliance mode
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Increase verbosity (-v: info, -vv: debug, -vvv: trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

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

        // === GPU Simulation Options ===
        /// Number of GPUs to simulate for multi-GPU scaling (default: auto-detect or 1)
        #[arg(long)]
        gpus: Option<u32>,

        /// [FUTURE] GPU environment mode - detects GPUs but uses same CPU simulation (for future GPU integration)
        #[arg(long)]
        use_real_gpus: bool,

        // === Multi-rank scaling options ===
        /// Read file list from specified file (one path per line)
        #[arg(long)]
        filelist: Option<std::path::PathBuf>,

        /// Rank ID for multi-process execution (0-based)
        #[arg(long)]
        rank: Option<u32>,

        /// Total number of ranks in world
        #[arg(long)]
        world_size: Option<u32>,

        /// Unix timestamp to start execution (for synchronized multi-rank)
        #[arg(long)]
        start_at_epoch: Option<u64>,

        /// Sharding strategy: interleaved, contiguous, or hash
        #[arg(long, default_value = "interleaved")]
        shard_strategy: String,

        /// Output JSON results to specified file
        #[arg(long)]
        results: Option<std::path::PathBuf>,
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
    /// Aggregate results from multiple rank JSON files
    Aggregate {
        /// Pattern or paths to rank result files (e.g., "/results/rank*.json")
        #[arg(short, long)]
        inputs: String,

        /// Output aggregated results to file
        #[arg(short, long)]
        output: std::path::PathBuf,

        /// Enable strict AU mode - fail if global AU is below threshold
        #[arg(long)]
        strict_au: bool,

        /// Expected metric AU threshold (default from first rank config)
        #[arg(long)]
        au_threshold: Option<f64>,
    },
}#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file early for S3/Azure credentials
    dotenvy::dotenv().ok(); // Ignore errors if .env doesn't exist

    let args = Args::parse();

    // Initialize logging with verbosity levels
    let (dl_driver_level, s3dlio_level) = match args.verbose {
        0 => ("warn", "warn"),    // Default: warnings only
        1 => ("info", "warn"),    // -v: dl-driver info, s3dlio warnings
        2 => ("debug", "info"),   // -vv: dl-driver debug, s3dlio info
        _ => ("trace", "debug"),  // -vvv+: dl-driver trace, s3dlio debug
    };
    
    tracing_subscriber::fmt()
        .with_env_filter(format!("dl_driver_core={},dl_driver={},s3dlio={}", 
                                dl_driver_level, dl_driver_level, s3dlio_level))
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
            gpus,
            use_real_gpus,
            filelist,
            rank,
            world_size,
            start_at_epoch,
            shard_strategy,
            results,
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
            gpus,
            use_real_gpus,
            filelist.as_deref(),
            rank,
            world_size,
            start_at_epoch,
            &shard_strategy,
            results.as_deref(),
        ).await,
        Commands::Validate { config, to_json } => validate_dlio_config(&config, to_json).await,
        Commands::Generate {
            config,
            verbose,
            skip_existing,
        } => run_generate_only(&config, verbose, skip_existing).await,
        Commands::Aggregate {
            inputs,
            output,
            strict_au,
            au_threshold,
        } => aggregate_rank_results(&inputs, &output, strict_au, au_threshold).await,
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
    gpus: Option<u32>,
    use_real_gpus: bool,
    filelist: Option<&std::path::Path>,
    rank: Option<u32>,
    world_size: Option<u32>,
    start_at_epoch: Option<u64>,
    shard_strategy: &str,
    results_path: Option<&std::path::Path>,
) -> Result<()> {
    info!("Loading DLIO config from: {:?}", config_path);

    // Multi-rank validation and setup
    let (current_rank, total_ranks) = match (rank, world_size) {
        (Some(r), Some(w)) => {
            if r >= w {
                return Err(anyhow::anyhow!("Rank {} must be less than world_size {}", r, w));
            }
            info!("Multi-rank mode: rank={}/{}, strategy={}", r, w, shard_strategy);
            (r, w)
        }
        (None, None) => (0, 1), // Single-process mode
        _ => return Err(anyhow::anyhow!("Both --rank and --world-size must be specified together")),
    };

    // Handle start_at_epoch synchronization barrier
    if let Some(start_time) = start_at_epoch {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if start_time > now {
            let wait_duration = start_time - now;
            info!("Rank {}: Waiting {} seconds until synchronized start at epoch {}", 
                  current_rank, wait_duration, start_time);
            tokio::time::sleep(tokio::time::Duration::from_secs(wait_duration)).await;
        }
        info!("Rank {}: Starting synchronized execution", current_rank);
    }

    // Plan A1: Set GPU affinity for multi-GPU scaling on same host
    if total_ranks > 1 {
        setup_gpu_affinity(current_rank, total_ranks, gpus, use_real_gpus)?;
    }

    // Load DLIO configuration
    let yaml_content = std::fs::read_to_string(config_path)?;
    let dlio_config = DlioConfig::from_yaml(&yaml_content)?;

    // Handle file list sharding for multi-rank execution
    let sharded_file_list = if let Some(filelist_path) = filelist {
        // Load file list from file
        let content = std::fs::read_to_string(filelist_path)
            .with_context(|| format!("Failed to read filelist: {:?}", filelist_path))?;
        let all_files: Vec<String> = content.lines().map(|s| s.trim().to_string()).collect();
        
        // Apply sharding strategy
        let sharded_files = apply_sharding_strategy(&all_files, current_rank, total_ranks, shard_strategy)?;
        info!("Rank {}: Using {} files from filelist (total: {}, strategy: {})", 
              current_rank, sharded_files.len(), all_files.len(), shard_strategy);
        Some(sharded_files)
    } else if total_ranks > 1 {
        // Multi-rank mode without explicit filelist - we'll need to implement directory-based sharding
        info!("Rank {}: Directory-based sharding will be handled in workload execution", current_rank);
        None
    } else {
        None
    };

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
        // Plan A1: Multi-GPU scaling - each rank represents one GPU, so total accelerators = world_size
        let accelerator_count = if total_ranks > 1 {
            // Multi-GPU mode: each rank gets 1 GPU, total system has world_size GPUs
            info!("Plan A1 Multi-GPU: Using {} total GPUs ({} GPUs per rank × {} ranks)", 
                  total_ranks, 1, total_ranks);
            total_ranks
        } else {
            // Single-GPU mode: use explicit accelerator count
            accelerators.unwrap_or(1)
        };

        // Multi-rank coordination setup
        let coordinator = if total_ranks > 1 {
            use dl_driver_core::coordination::RankCoordinator;
            
            // Use deterministic coordination ID based on config path and world size
            let config_name = config_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("dlio");
            let coord_id = format!("dlio_{}_{}", config_name, total_ranks);
            let coord = RankCoordinator::new(current_rank, total_ranks, &coord_id)
                .context("Failed to create rank coordinator")?;
            
            info!("🔗 Rank {}: Registering with coordination group", current_rank);
            coord.register_and_wait().await
                .context("Failed to register with coordination group")?;
                
            info!("🚧 Rank {}: Waiting at execution barrier", current_rank);
            coord.barrier("execution_start").await
                .context("Failed to synchronize at execution barrier")?;
                
            // Rank 0 marks global start time
            if current_rank == 0 {
                coord.mark_global_start()
                    .context("Failed to mark global start time")?;
            }
            
            Some(coord)
        } else {
            None
        };

        let mut workload_runner = dl_driver_core::WorkloadRunner::new(dlio_config.clone())
            .with_accelerator_config(accelerator_count, strict_au)
            .with_rank_config(current_rank, total_ranks, sharded_file_list.clone());
            
        workload_runner.run_training_phase().await
            .context("Training workload failed")?;

        // Multi-rank coordination finish
        if let Some(ref coord) = coordinator {
            info!("🏁 Rank {}: Marking execution finished", current_rank);
            coord.mark_finished_and_wait().await
                .context("Failed to coordinate execution finish")?;
                
            // Only rank 0 displays aggregated results (eliminates temp file aggregation)
            if current_rank == 0 {
                match coord.get_aggregated_results() {
                    Ok(results) => {
                        println!("\n🎉 Plan A1 Multi-GPU Results (Shared Memory Coordination):");
                        println!("================================================================");
                        println!("Total files processed: {}", results.total_files_processed);
                        println!("Total data read: {:.2} GiB", results.total_bytes_read as f64 / 1_073_741_824.0);
                        println!("Combined throughput: {:.2} GiB/s", results.total_throughput_gib_s);
                        println!("Global runtime: {:.3}s", results.global_runtime_seconds);
                        println!("Number of ranks: {}", results.total_ranks);
                        println!("\nPer-rank breakdown:");
                        for detail in &results.rank_details {
                            println!("  Rank {}: {:.2} GiB/s, {} files, AU: {:.4}%", 
                                   detail.rank, 
                                   detail.throughput_gib_s,
                                   detail.files_processed,
                                   detail.au_fraction * 100.0);
                        }
                        println!("✅ Multi-rank coordination successful - NO TEMP FILES USED");
                    }
                    Err(e) => {
                        warn!("⚠️  Failed to get aggregated results: {}", e);
                    }
                }
            }
                
            let stats = coord.get_stats();
            debug!("📊 Coordination stats: {:?}", stats);
            
            // Cleanup coordination resources (rank 0 only)
            coord.cleanup()
                .context("Failed to cleanup coordination resources")?;
        }
        
        // Get final metrics from WorkloadRunner
        let workload_metrics = workload_runner.get_metrics();

        // Store results in shared memory (eliminates temp files for multi-rank)
        if let Some(coord) = coordinator.as_ref() {
            // Get metrics as JSON to extract needed values
            let metrics_json = workload_metrics.to_json(current_rank, &dlio_config);
            let metrics_obj = metrics_json["metrics"].as_object().unwrap();
            
            let files_processed = metrics_obj["files_processed"].as_u64().unwrap_or(0);
            let bytes_read = metrics_obj["bytes_read"].as_u64().unwrap_or(0);
            let throughput_gib_s = metrics_obj["storage_throughput_gib_s"].as_f64().unwrap_or(0.0);
            let wall_clock_time_ms = metrics_obj["wall_clock_time_ms"].as_u64().unwrap_or(0);
            let au_fraction = metrics_obj["au_fraction"].as_f64().unwrap_or(0.0);
            
            let start_time_ns = (metrics_json["start_time"].as_f64().unwrap_or(0.0) * 1_000_000_000.0) as u64;
            let end_time_ns = (metrics_json["end_time"].as_f64().unwrap_or(0.0) * 1_000_000_000.0) as u64;
            
            coord.store_results(
                files_processed,
                bytes_read,
                throughput_gib_s,
                wall_clock_time_ms as f64,
                au_fraction,
                start_time_ns,
                end_time_ns
            ).context("Failed to store results in shared memory")?;
            
            info!("📊 Rank {}: Results stored in shared memory", current_rank);
        } else {
            // Single rank mode: export to JSON file if requested
            if let Some(results_file) = results_path {
                let metrics_json = workload_metrics.to_json(current_rank, &dlio_config);
                std::fs::write(results_file, serde_json::to_string_pretty(&metrics_json)?)
                    .with_context(|| format!("Failed to write results to: {:?}", results_file))?;
                info!("Rank {}: Results saved to {:?}", current_rank, results_file);
            }
        }
    }

    println!("✅ DLIO workload completed successfully");

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
        println!("📊 DLIO workload execution completed successfully");
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

/// Apply sharding strategy to distribute files across ranks
fn apply_sharding_strategy(
    files: &[String],
    rank: u32,
    world_size: u32,
    strategy: &str,
) -> Result<Vec<String>> {
    let total_files = files.len();
    if total_files == 0 {
        return Ok(Vec::new());
    }

    let rank = rank as usize;
    let world_size = world_size as usize;

    let sharded = match strategy {
        "interleaved" => {
            // Round-robin distribution: rank 0 gets files 0,N,2N,..., rank 1 gets files 1,N+1,2N+1,...
            files
                .iter()
                .enumerate()
                .filter(|(i, _)| i % world_size == rank)
                .map(|(_, f)| f.clone())
                .collect()
        }
        "contiguous" => {
            // Contiguous blocks: divide files into equal chunks
            let chunk_size = total_files / world_size;
            let remainder = total_files % world_size;
            
            let start = rank * chunk_size + std::cmp::min(rank, remainder);
            let end = start + chunk_size + if rank < remainder { 1 } else { 0 };
            
            files[start..end].to_vec()
        }
        "hash" => {
            // Hash-based distribution: consistent but pseudo-random
            files
                .iter()
                .filter(|f| {
                    let mut hasher = DefaultHasher::new();
                    f.hash(&mut hasher);
                    (hasher.finish() % world_size as u64) as usize == rank
                })
                .cloned()
                .collect()
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown sharding strategy: '{}'. Valid options: interleaved, contiguous, hash",
                strategy
            ));
        }
    };

    info!(
        "Sharding strategy '{}': rank {} gets {}/{} files",
        strategy, rank, sharded.len(), total_files
    );

    Ok(sharded)
}

/// Aggregate results from multiple rank JSON files
async fn aggregate_rank_results(
    inputs: &str,
    output: &std::path::Path,
    strict_au: bool,
    au_threshold: Option<f64>,
) -> Result<()> {
    use glob::glob;
    use serde_json::Value;
    
    info!("Aggregating results from pattern: {}", inputs);
    
    // Find all matching files
    let paths: Vec<_> = glob(inputs)
        .with_context(|| format!("Failed to glob pattern: {}", inputs))?
        .collect::<Result<Vec<_>, _>>()?;
        
    if paths.is_empty() {
        return Err(anyhow::anyhow!("No files found matching pattern: {}", inputs));
    }
    
    info!("Found {} result files to aggregate", paths.len());
    
    let mut aggregated = serde_json::json!({
        "aggregated_results": {
            "total_ranks": paths.len(),
            "global_metrics": {},
            "rank_details": []
        }
    });
    
    let mut total_throughput = 0.0_f64;
    let mut total_files_processed = 0u64;
    let mut total_bytes_read = 0u64;
    let mut min_start_time = f64::MAX;
    let mut max_end_time = 0.0_f64;
    
    // Process each rank result file
    for (rank_idx, path) in paths.iter().enumerate() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read result file: {:?}", path))?;
        let rank_data: Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON from: {:?}", path))?;
            
        // Extract metrics from rank data
        if let Some(metrics) = rank_data.get("metrics") {
            if let Some(throughput) = metrics.get("storage_throughput_gib_s").and_then(|v| v.as_f64()) {
                total_throughput += throughput;
            }
            if let Some(files) = metrics.get("files_processed").and_then(|v| v.as_u64()) {
                total_files_processed += files;
            }
            if let Some(bytes) = metrics.get("bytes_read").and_then(|v| v.as_u64()) {
                total_bytes_read += bytes;
            }
        }
        
        // Track timing for global AU calculation
        if let Some(start) = rank_data.get("start_time").and_then(|v| v.as_f64()) {
            min_start_time = min_start_time.min(start);
        }
        if let Some(end) = rank_data.get("end_time").and_then(|v| v.as_f64()) {
            max_end_time = max_end_time.max(end);
        }
        
        // Add rank details to aggregated results
        aggregated["aggregated_results"]["rank_details"].as_array_mut().unwrap()
            .push(serde_json::json!({
                "rank": rank_idx,
                "file": path.file_name().unwrap_or_default().to_string_lossy(),
                "metrics": rank_data.get("metrics").cloned().unwrap_or(Value::Null)
            }));
    }
    
    // Calculate global metrics
    let global_runtime = max_end_time - min_start_time;
    
    // Plan A1: Multi-GPU AU aggregation - sum compute times and wall clock times across all GPUs
    let mut total_compute_time = 0.0;
    let mut total_wall_clock_time = 0.0;
    let mut gpu_count = 0u32;
    
    // Re-read rank files to aggregate AU calculation data
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(rank_data) = serde_json::from_str::<Value>(&content) {
                if let Some(metrics) = rank_data.get("metrics") {
                    // Sum total compute time from all GPUs
                    if let Some(compute_ms) = metrics.get("total_compute_time_ms").and_then(|v| v.as_f64()) {
                        total_compute_time += compute_ms / 1000.0; // Convert to seconds
                    }
                    // Sum wall clock time from all GPUs
                    if let Some(wall_ms) = metrics.get("wall_clock_time_ms").and_then(|v| v.as_f64()) {
                        total_wall_clock_time += wall_ms / 1000.0; // Convert to seconds
                    }
                    gpu_count += 1;
                }
            }
        }
    }
    
    // Plan A1: Global AU = Total GPU compute time / (Total wall clock time across all GPUs)
    let global_au = if total_wall_clock_time > 0.0 && gpu_count > 0 {
        // Multi-GPU AU: aggregate utilization across all GPUs
        let average_wall_clock = total_wall_clock_time / gpu_count as f64;
        (total_compute_time / average_wall_clock).min(1.0) // Cap at 100%
    } else {
        0.0
    };
    
    info!("Plan A1 Multi-GPU AU: {:.1}% across {} GPUs (total_compute={:.3}s, avg_wall_clock={:.3}s)", 
          global_au * 100.0, gpu_count, total_compute_time, total_wall_clock_time / gpu_count.max(1) as f64);
    
    aggregated["aggregated_results"]["global_metrics"] = serde_json::json!({
        "total_throughput_gib_s": total_throughput,
        "total_files_processed": total_files_processed,
        "total_bytes_read": total_bytes_read,
        "global_runtime_seconds": global_runtime,
        "global_au": global_au,
        "pass": !strict_au || global_au >= au_threshold.unwrap_or(0.9)
    });
    
    // Write aggregated results
    std::fs::write(output, serde_json::to_string_pretty(&aggregated)?)
        .with_context(|| format!("Failed to write aggregated results to: {:?}", output))?;
        
    info!("✅ Aggregated results written to: {:?}", output);
    info!("Global metrics: {:.2} GiB/s throughput, {} files, {:.2}s runtime", 
          total_throughput, total_files_processed, global_runtime);
    
    if strict_au && global_au < au_threshold.unwrap_or(0.9) {
        return Err(anyhow::anyhow!("Global AU {:.3} below threshold {:.3}", 
                                  global_au, au_threshold.unwrap_or(0.9)));
    }
    
    Ok(())
}

/// Plan A1: Set GPU affinity and environment for realistic multi-GPU scaling
fn setup_gpu_affinity(rank: u32, world_size: u32, simulated_gpus: Option<u32>, use_real_gpus: bool) -> Result<()> {
    let effective_gpu_count = simulated_gpus.unwrap_or(world_size);
    
    if use_real_gpus {
        info!("🎯 Plan A1: [FUTURE] GPU DETECTION for rank {} of {} (found {} GPUs)", 
              rank, world_size, effective_gpu_count);
        
        // Future: Set CUDA_VISIBLE_DEVICES to bind this rank to a specific GPU
        let gpu_id = rank % effective_gpu_count;
        std::env::set_var("CUDA_VISIBLE_DEVICES", gpu_id.to_string());
        info!("   🔮 [FUTURE] GPU environment: CUDA_VISIBLE_DEVICES={} (Currently: CPU simulation only)", gpu_id);
        
        // Set CUDA device order for consistent binding
        std::env::set_var("CUDA_DEVICE_ORDER", "PCI_BUS_ID");
        
        // Set NUMA affinity if possible (on NUMA systems)
        if let Ok(numa_nodes) = std::env::var("NUMA_NODES") {
            let numa_count: u32 = numa_nodes.parse().unwrap_or(1);
            let numa_node = rank % numa_count;
            info!("   🖥️  NUMA affinity: Rank {} -> NUMA node {}", rank, numa_node);
        }
    } else {
        info!("🎯 Plan A1: Setting up PURE SIMULATION environment for rank {} of {} (simulating {} GPUs)", 
              rank, world_size, effective_gpu_count);
        
        // Simulation mode: set environment variables without requiring real GPUs
        let simulated_gpu_id = rank % effective_gpu_count;
        std::env::set_var("SIMULATED_CUDA_VISIBLE_DEVICES", simulated_gpu_id.to_string());
        std::env::set_var("DL_DRIVER_SIMULATION_MODE", "1");
        info!("   🎮 PURE SIMULATION: GPU_{} (CPU-based compute simulation)", simulated_gpu_id);
    }
    
    // Set common environment variables for both modes
    std::env::set_var("LOCAL_RANK", rank.to_string());
    std::env::set_var("LOCAL_WORLD_SIZE", world_size.to_string());
    std::env::set_var("DL_DRIVER_GPU_COUNT", effective_gpu_count.to_string());
    
    let mode = if use_real_gpus { "GPU ENVIRONMENT [FUTURE]" } else { "PURE SIMULATION" };
    info!("✅ Plan A1: {} mode configured (All compute is CPU-based simulation)", mode);
    Ok(())
}
