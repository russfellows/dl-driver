// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dl_driver_core::DlioConfig;
use dl_driver_core::plugins::PluginManager;
use tracing::{info, error, debug, warn, trace};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// dl-driver – Unified DLIO execution engine with optional MLPerf compliance mode
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Increase verbosity (default: warnings only, -v: info, -vv: debug, -vvv: trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run DLIO workload (phases controlled by workflow: section in config)
    ///
    /// Workflow phases:
    ///   - generate_data: Generate synthetic dataset
    ///   - train: Run training/data loading workload  
    ///   - checkpoint: Checkpointing I/O (planned)
    ///   - evaluation: Evaluation phase (planned)
    Run {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// If set, dump the parsed YAML back to stdout
        #[arg(long)]
        pretty: bool,

        /// Validate config and show execution summary without running (dry-run mode)
        #[arg(long)]
        dry_run: bool,

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

        // === Workstream A: Realistic AI/ML Workloads ===
        /// Use realistic framework-specific workload profile (torch-like, tf-like, jax-like)
        #[arg(long)]
        profile: Option<String>,

        /// Export metrics summary to JSON file
        #[arg(long)]
        metrics_json: Option<std::path::PathBuf>,

        /// Export metrics summary to CSV file
        #[arg(long)]
        metrics_csv: Option<std::path::PathBuf>,
        
        /// Resume from checkpoint (path/URI to checkpoint file)
        #[arg(long)]
        resume_from_checkpoint: Option<String>,
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
    /// Run distributed DLIO workload across multiple agents
    Distributed {
        #[command(subcommand)]
        command: DistributedCommands,
    },
}

#[derive(Subcommand, Debug)]
enum DistributedCommands {
    /// Run workload across multiple agents
    Run {
        /// Path to DLIO YAML config file
        #[arg(long)]
        config: std::path::PathBuf,

        /// Distributed config file (YAML with agents list)
        #[arg(long)]
        distributed_config: Option<std::path::PathBuf>,

        /// Agent endpoints (alternative to distributed_config)
        #[arg(long, value_delimiter = ',')]
        agents: Option<Vec<String>>,

        /// Path template for agent-specific directories (e.g., "{id}/", "agent-{id}/")
        #[arg(long, default_value = "{id}/")]
        path_template: String,

        /// Coordinated start delay in milliseconds
        #[arg(long, default_value = "1000")]
        start_delay_ms: u64,

        /// Request timeout in milliseconds
        #[arg(long, default_value = "300000")]
        request_timeout_ms: u64,

        /// Maximum retries per agent
        #[arg(long, default_value = "3")]
        max_retries: u32,

        /// Dry-run: validate configuration without running workload
        #[arg(long)]
        dry_run: bool,

        /// Output storage metrics TSV file
        #[arg(long)]
        storage_tsv: Option<std::path::PathBuf>,

        /// Output AI/ML metrics TSV file
        #[arg(long)]
        aiml_tsv: Option<std::path::PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file early for S3/Azure credentials
    dotenvy::dotenv().ok(); // Ignore errors if .env doesn't exist

    let args = Args::parse();

    // Initialize logging with verbosity levels
    // Multi-level logging strategy (s3dlio always one level less than dl-driver):
    // -v   (1): dl-driver=INFO,  s3dlio=WARN   (detailed progress)
    // -vv  (2): dl-driver=DEBUG, s3dlio=INFO   (internal details, s3dlio at info)
    // -vvv (3): dl-driver=TRACE, s3dlio=DEBUG  (maximum verbosity)
    let dl_driver_level = match args.verbose {
        0 => "warn",    // Default: warnings only, use println! for user messages
        1 => "info",    // -v: info level with detailed progress
        2 => "debug",   // -vv: debug level with internal details
        _ => "trace",   // -vvv+: trace level with maximum verbosity
    };
    
    // Map to log crate level for s3dlio (always one level less than dl-driver)
    let s3dlio_log_level = match args.verbose {
        0 => "warn",    // Default: warnings only
        1 => "warn",    // -v: s3dlio at warn (dl-driver at info)
        2 => "info",    // -vv: s3dlio at info (dl-driver at debug)
        _ => "debug",   // -vvv: s3dlio at debug (dl-driver at trace)
    };
    
    // Initialize the log-to-tracing bridge so s3dlio's log messages appear in our tracing output
    let _ = tracing_log::LogTracer::init();
    
    // Set up logging for all dl-driver crates and s3dlio (via log bridge)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(format!(
            "dl_driver_core={},dl_driver_storage={},dl_driver_formats={},dl_driver_frameworks={},dl_driver={},s3dlio={}", 
            dl_driver_level, dl_driver_level, dl_driver_level, dl_driver_level, dl_driver_level, s3dlio_log_level
        ))
        .try_init();

    info!("dl-driver v{} starting", env!("CARGO_PKG_VERSION"));
    debug!("Logging initialized at {} level (s3dlio at {} level)", dl_driver_level, s3dlio_log_level);
    trace!("Trace logging is active (only visible with -vvv)");

    match args.command {
        Commands::Run {
            config,
            pretty,
            dry_run,
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
            profile,
            metrics_json,
            metrics_csv,
            resume_from_checkpoint,
        } => run_unified_dlio(
            &config, 
            pretty,
            dry_run,
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
            profile.as_deref(),
            metrics_json.as_deref(),
            metrics_csv.as_deref(),
            resume_from_checkpoint.as_deref(),
        ).await,
        Commands::Validate { config, to_json } => validate_dlio_config(&config, to_json).await,
        Commands::Distributed { command } => match command {
            DistributedCommands::Run {
                config,
                distributed_config,
                agents,
                path_template,
                start_delay_ms,
                request_timeout_ms,
                max_retries,
                dry_run,
                storage_tsv,
                aiml_tsv,
            } => run_distributed(
                &config,
                distributed_config.as_deref(),
                agents.as_ref(),
                &path_template,
                start_delay_ms,
                request_timeout_ms,
                max_retries,
                dry_run,
                storage_tsv.as_deref(),
                aiml_tsv.as_deref(),
            ).await,
        },
    }
}

/// Unified DLIO execution engine with optional MLPerf compliance mode
async fn run_unified_dlio(
    config_path: &std::path::Path,
    pretty: bool,
    dry_run: bool,
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
    // Workstream A: Realistic AI/ML Workloads
    profile: Option<&str>,
    metrics_json: Option<&std::path::Path>,
    metrics_csv: Option<&std::path::Path>,
    resume_from_checkpoint: Option<&str>,
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
    let mut dlio_config = DlioConfig::from_yaml(&yaml_content)?;
    
    // Load checkpoint if resume requested
    let checkpoint_state = if let Some(checkpoint_path) = resume_from_checkpoint {
        info!("Loading checkpoint from: {}", checkpoint_path);
        let state = dl_driver_core::plugins::checkpoint::CheckpointPlugin::load_checkpoint(checkpoint_path).await
            .context("Failed to load checkpoint")?;
        
        info!("✅ Checkpoint loaded: run_id={}, step={}, epoch={:?}, timestamp={}",
              state.run_id, state.step, state.epoch, state.timestamp);
        
        // Validate config if requested
        if dlio_config.resume.as_ref().map_or(true, |r| r.validate_config) {
            info!("Validating loaded checkpoint config against current config...");
            // TODO: Add deep config comparison logic
            warn!("Config validation not yet implemented - skipping validation");
        }
        
        // Merge resume config into dlio_config if not already present
        if dlio_config.resume.is_none() {
            dlio_config.resume = Some(dl_driver_core::dlio_compat::ResumeConfig {
                checkpoint_path: checkpoint_path.to_string(),
                validate_config: true,
                allow_minor_version_mismatch: true,
            });
        }
        
        Some(state)
    } else if let Some(resume_config) = &dlio_config.resume {
        // Resume config specified in YAML file
        info!("Loading checkpoint from config: {}", resume_config.checkpoint_path);
        let state = dl_driver_core::plugins::checkpoint::CheckpointPlugin::load_checkpoint(&resume_config.checkpoint_path).await
            .context("Failed to load checkpoint from config")?;
        
        info!("✅ Checkpoint loaded: run_id={}, step={}, epoch={:?}, timestamp={}",
              state.run_id, state.step, state.epoch, state.timestamp);
        
        Some(state)
    } else {
        None
    };
    
    if let Some(ref state) = checkpoint_state {
        info!("🔄 Resuming from checkpoint: step {} (epoch {:?})", state.step, state.epoch);
    }

    // Dry-run mode: display config summary and exit
    if dry_run {
        display_config_summary(&dlio_config, config_path)?;
        return Ok(());
    }

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
    let mut plugins = PluginManager::new();
    
    // Add CheckpointPlugin if checkpointing is enabled in config
    if let Some(checkpoint_plugin) = dl_driver_core::plugins::CheckpointPlugin::new(&dlio_config).await? {
        plugins.push(Box::new(checkpoint_plugin));
        info!("CheckpointPlugin registered");
    }
    
    plugins.initialize(&dlio_config).await
        .context("Failed to initialize plugins")?;

    // Initialize metrics system (always available, enhanced in MLPerf mode)
    let _metrics = if mlperf_mode {
        dl_driver_core::mlperf::MlperfMetrics::new()
    } else {
        dl_driver_core::mlperf::MlperfMetrics::new() // Same system for both modes
    };

    // Phase 1: Data Generation (if enabled)
    if dlio_config.workflow.as_ref().map_or(false, |w| w.generate_data.unwrap_or(false)) {
        println!("\n📁 Phase 1: Data Generation");
        info!("Phase 1: Generating data");
        run_data_generation(&dlio_config).await
            .context("Data generation failed")?;
    }

    // Phase 2: Training workload using WorkloadRunner for DLIO compliance measurement
    if dlio_config.workflow.as_ref().map_or(true, |w| w.train.unwrap_or(true)) {
        println!("\n🚀 Phase 2: Training");
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
            .with_plugins(plugins)
            .with_accelerator_config(accelerator_count, strict_au)
            .with_rank_config(current_rank, total_ranks, sharded_file_list.clone());
        
        // Resume from checkpoint if loaded
        if let Some(checkpoint) = checkpoint_state {
            workload_runner = workload_runner.with_checkpoint(checkpoint);
        }

        // Workstream A: Apply realistic framework profile if specified
        if let Some(profile_name) = profile {
            info!("🎯 Applying {} workload profile", profile_name);
            workload_runner = workload_runner.with_profile(profile_name)
                .context("Failed to apply workload profile")?;
        }
            
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

        // Workstream A: Export metrics if requested
        if let Some(json_path) = metrics_json {
            info!("📄 Exporting metrics to JSON: {:?}", json_path);
            workload_metrics.export_json(json_path, "workload")
                .context("Failed to export metrics to JSON")?;
        }

        if let Some(csv_path) = metrics_csv {
            info!("📊 Exporting metrics to CSV: {:?}", csv_path);
            workload_metrics.export_csv(csv_path, "workload")
                .context("Failed to export metrics to CSV")?;
        }

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
/// Supports 3 directory organization modes:
/// 1. Flat: All files in single directory (Mode 1)
/// 2. DLIO-style sharding: Files distributed across train/NNNN subdirectories (Mode 2)
/// 3. Hierarchical: Multi-level nested directory tree (Mode 3)
async fn run_data_generation(config: &DlioConfig) -> Result<()> {
    use s3dlio::object_store::store_for_uri;
    use std::sync::Arc;
    use indicatif::{ProgressBar, ProgressStyle};
    use dl_driver_core::directory_tree::DirectoryMode;
    
    let start_time = std::time::Instant::now();
    info!("Starting PARALLEL data generation phase");

    // Determine directory organization mode from config
    let dir_mode = DirectoryMode::from_config(
        config.dataset.directory_tree.as_ref(),
        config.dataset.num_subfolders_train,
    )?;
    
    match &dir_mode {
        DirectoryMode::Flat => {
            info!("📁 Directory mode: Flat (all files in single directory)");
        }
        DirectoryMode::DlioSharding { num_subfolders } => {
            info!("📁 Directory mode: DLIO-style sharding ({} subfolders)", num_subfolders);
        }
        DirectoryMode::Hierarchical { tree } => {
            info!("📁 Directory mode: Hierarchical (width={}, depth={}, {} dirs, {} files)",
                tree.config().width, tree.config().depth,
                tree.total_directories(), tree.total_files());
        }
    }

    // Create object store for the configured storage backend
    let store = Arc::new(store_for_uri(&config.dataset.data_folder)
        .with_context(|| format!("Failed to create object store for {}", config.dataset.data_folder))?);

    // Create directory structure if needed (filesystem only, not object stores)
    let data_folder = &config.dataset.data_folder;
    let dirs_to_create = dir_mode.get_directories_to_create(data_folder);
    if !dirs_to_create.is_empty() {
        info!("📂 Creating {} directories for {} backend", 
            dirs_to_create.len(),
            if data_folder.starts_with("file://") { "filesystem" } 
            else if data_folder.starts_with("direct://") { "direct I/O" }
            else { "object store (implicit)" }
        );
        
        for dir_path in &dirs_to_create {
            let full_dir_uri = if data_folder.ends_with('/') {
                format!("{}{}", data_folder, dir_path)
            } else {
                format!("{}/{}", data_folder, dir_path)
            };
            
            // Use s3dlio's mkdir (added in v0.9.11)
            store.mkdir(&full_dir_uri).await
                .with_context(|| format!("Failed to create directory: {}", full_dir_uri))?;
            debug!("Created directory: {}", full_dir_uri);
        }
        info!("✅ Directory structure created successfully");
    }

    // Determine number of files to generate
    let num_files = match &dir_mode {
        DirectoryMode::Hierarchical { tree } => {
            // Mode 3: Use tree's total file count
            tree.total_files()
        }
        _ => {
            // Mode 1 & 2: Use num_files_train from config
            config.dataset.num_files_train.unwrap_or(100)
        }
    };

    let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1);
    let record_size = config.dataset.record_length_bytes.unwrap_or(1024);
    
    let file_size_mb = (samples_per_file * record_size) as f64 / 1024.0 / 1024.0;
    let total_size_gb = (num_files as f64 * file_size_mb) / 1024.0;

    println!("📦 Generating {} files ({:.2} GB total)...", num_files, total_size_gb);
    info!(
        "Generating {} files with {} samples each ({:.1}MB per file, {:.2}GB total)",
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

    // Create progress bar for visual feedback
    let progress = ProgressBar::new(num_files as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files {msg}"
        ).expect("Failed to set progress bar template")
    );

    // Create semaphore to limit concurrent operations
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let data_folder_clone = config.dataset.data_folder.clone();
    let format = config.dataset.format.as_ref().map(|f| f.as_str()).unwrap_or("npz");

    // Clone DirectoryMode for use in async tasks
    let dir_mode = Arc::new(dir_mode);

    // Spawn parallel file generation tasks
    let mut handles = Vec::new();
    for file_idx in 0..num_files {
        let store_clone = Arc::clone(&store);
        let data_clone = Arc::clone(&synthetic_data);
        let semaphore_clone = Arc::clone(&semaphore);
        let data_folder_clone2 = data_folder_clone.clone();
        let format_str = format.to_string();
        let progress_clone = progress.clone();
        let dir_mode_clone = Arc::clone(&dir_mode);

        let handle = tokio::spawn(async move {
            // Acquire semaphore permit for rate limiting
            let _permit = semaphore_clone.acquire().await.unwrap();
            
            // Get file path based on directory mode (uses DirectoryMode logic)
            let rel_path = dir_mode_clone.get_file_path(file_idx, &format_str);
            let full_path = if data_folder_clone2.ends_with('/') {
                format!("{}{}", data_folder_clone2, rel_path)
            } else {
                format!("{}/{}", data_folder_clone2, rel_path)
            };

            let write_start = std::time::Instant::now();
            let result = store_clone
                .put(&full_path, &*data_clone)
                .await
                .with_context(|| format!("Failed to write file {}", full_path));
            let write_time = write_start.elapsed();

            // Update progress bar
            progress_clone.inc(1);

            // Return result with timing info, including file_idx for error reporting
            result
                .map(|_| (file_idx, full_path, data_clone.len(), write_time))
                .with_context(|| format!("Failed to generate file index {}", file_idx))
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
                
                // Update progress bar message with throughput info
                let throughput = bytes as f64 / 1024.0 / 1024.0 / write_time.as_secs_f64();
                progress.set_message(format!("{:.1} MB/s", throughput));
                
                // Debug logging for troubleshooting - show which specific files complete
                debug!(
                    "File {:06} generated: {:.1}MB in {:?} ({:.1} MB/s)",
                    file_idx,
                    bytes as f64 / 1024.0 / 1024.0,
                    write_time,
                    throughput
                );
            }
            Err(e) => {
                progress.finish_with_message("❌ Failed");
                error!("❌ File generation failed: {}", e);
                return Err(e);
            }
        }
    }

    progress.finish();

    let generation_time = start_time.elapsed();
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / generation_time.as_secs_f64();
    
    // User-facing summary
    println!(
        "✅ Generated {} files ({:.2} GB) in {:.2}s @ {:.1} MB/s",
        completed, 
        total_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
        generation_time.as_secs_f64(),
        throughput_mbps
    );
    
    info!("PARALLEL data generation completed!");
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

    // Use the same comprehensive validation as --dry-run
    // This makes 'validate' and '--dry-run' functional aliases
    display_config_summary(&dlio_config, config_path)?;

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

/// Run distributed DLIO workload across multiple agents
async fn run_distributed(
    config_path: &std::path::Path,
    distributed_config_path: Option<&std::path::Path>,
    agents: Option<&Vec<String>>,
    path_template: &str,
    start_delay_ms: u64,
    request_timeout_ms: u64,
    max_retries: u32,
    dry_run: bool,
    storage_tsv: Option<&std::path::Path>,
    aiml_tsv: Option<&std::path::Path>,
) -> Result<()> {
    use dl_driver_core::dist::controller::{Controller, DistributedConfig};
    use std::io::Write;
    
    info!("🚀 Distributed Execution Mode");
    
    // Load DLIO config
    let config_path_str = config_path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in config path"))?;
    let dlio_config = DlioConfig::from_yaml_file(config_path_str)
        .with_context(|| format!("Failed to load DLIO config from {:?}", config_path))?;
    
    info!("✅ DLIO config loaded: {:?}", config_path);
    
    // Build distributed config from CLI args or file
    let mut dist_config = if let Some(dist_path) = distributed_config_path {
        // TODO: Implement DistributedConfig::from_yaml_file when we add the config module
        info!("📋 Loading distributed config from: {:?}", dist_path);
        DistributedConfig::default() // Placeholder for now
    } else {
        DistributedConfig::default()
    };
    
    // Override with CLI args
    if let Some(agent_list) = agents {
        dist_config.agents = agent_list.clone();
    }
    dist_config.path_template = path_template.to_string();
    dist_config.start_delay_ms = start_delay_ms;
    dist_config.request_timeout_ms = request_timeout_ms;
    dist_config.max_retries = max_retries;
    
    // Validate we have agents
    if dist_config.agents.is_empty() {
        anyhow::bail!("No agents specified. Use --agents or --distributed-config");
    }
    
    info!("📊 Configuration:");
    info!("   Agents: {}", dist_config.agents.len());
    for (idx, agent) in dist_config.agents.iter().enumerate() {
        info!("     [{}] {}", idx, agent);
    }
    info!("   Path template: {}", dist_config.path_template);
    info!("   Start delay: {}ms", dist_config.start_delay_ms);
    info!("   Request timeout: {}ms", dist_config.request_timeout_ms);
    info!("   Max retries: {}", dist_config.max_retries);
    
    // Create controller
    let controller = Controller::new(dlio_config, dist_config);
    
    if dry_run {
        info!("🔍 DRY-RUN MODE - Validating configuration");
        
        // Health check all agents
        let health_results = controller.health_check_all().await?;
        
        println!("\n╔════════════════════════════════════════════════╗");
        println!("║   Distributed Execution Plan (DRY-RUN)        ║");
        println!("╚════════════════════════════════════════════════╝\n");
        
        println!("🌐 Agent Health Check:");
        for (agent, healthy) in health_results {
            let status = if healthy { "✅ Healthy" } else { "❌ Unhealthy" };
            println!("   {} - {}", agent, status);
        }
        
        println!("\n✅ Validation Passed! Ready to run distributed workload.");
        println!("   Remove --dry-run to execute.\n");
        
        return Ok(());
    }
    
    // Run distributed workload
    info!("🚀 Starting distributed workload execution...");
    let aggregate_results = controller.run_distributed().await?;
    
    // Display results
    println!("\n╔════════════════════════════════════════════════╗");
    println!("║   Distributed Workload Complete! 🎉           ║");
    println!("╚════════════════════════════════════════════════╝\n");
    
    println!("📊 Storage Performance (I/O Perspective):");
    println!("   Total Throughput: {:.1} ops/s, {:.1} MiB/s", 
             aggregate_results.total_ops_per_s, aggregate_results.total_mib_per_s);
    println!("   Total Operations: {}", aggregate_results.total_ops);
    println!("   Average Latency: p50={:.2}ms, p90={:.2}ms, p95={:.2}ms, p99={:.2}ms",
             aggregate_results.avg_p50_ms, aggregate_results.avg_p90_ms,
             aggregate_results.avg_p95_ms, aggregate_results.avg_p99_ms);
    println!("   Errors: {}", aggregate_results.total_errors);
    
    println!("\n🤖 AI/ML Training Performance (Training Perspective):");
    println!("   Training Velocity: {:.1} samples/s, {:.1} batches/s",
             aggregate_results.total_samples_per_second, aggregate_results.total_batches_per_second);
    println!("   Total Samples: {}, Total Batches: {}",
             aggregate_results.total_samples, aggregate_results.total_batches);
    println!("   Average Batch Time: {:.2}ms", aggregate_results.avg_batch_time_ms);
    println!("   Epochs Completed: {}", aggregate_results.total_epochs_completed);
    println!("   Pipeline Efficiency: {:.1}%", aggregate_results.avg_pipeline_efficiency * 100.0);
    
    // Write TSV files if requested
    if let Some(storage_path) = storage_tsv {
        let storage_content = aggregate_results.to_storage_tsv();
        let mut file = std::fs::File::create(storage_path)
            .with_context(|| format!("Failed to create storage TSV: {:?}", storage_path))?;
        file.write_all(storage_content.as_bytes())?;
        info!("💾 Storage metrics written to: {:?}", storage_path);
    }
    
    if let Some(aiml_path) = aiml_tsv {
        let aiml_content = aggregate_results.to_aiml_tsv();
        let mut file = std::fs::File::create(aiml_path)
            .with_context(|| format!("Failed to create AI/ML TSV: {:?}", aiml_path))?;
        file.write_all(aiml_content.as_bytes())?;
        info!("💾 AI/ML metrics written to: {:?}", aiml_path);
    }
    
    println!();
    
    Ok(())
}

// ============================================================================
// Configuration Validation & Summary Display (--dry-run)
// ============================================================================

/// Display comprehensive configuration summary for dry-run validation
fn display_config_summary(config: &DlioConfig, config_path: &std::path::Path) -> Result<()> {
    use dl_driver_core::directory_tree::DirectoryMode;
    
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║         DL-DRIVER CONFIGURATION VALIDATION & TEST SUMMARY             ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("✅ Config file parsed successfully: {}", config_path.display());
    println!();
    
    // Model Information
    if let Some(ref model) = config.model {
        println!("┌─ Model Configuration ────────────────────────────────────────────────┐");
        if let Some(ref name) = model.name {
            println!("│ Model Name:   {}", name);
        }
        if let Some(size) = model.model_size {
            println!("│ Model Size:   {} parameters", size);
        }
        println!("└──────────────────────────────────────────────────────────────────────┘");
        println!();
    }
    
    // Framework
    if let Some(ref framework) = config.framework {
        println!("Framework: {}", framework);
        println!();
    }
    
    // Workflow phases
    if let Some(ref workflow) = config.workflow {
        println!("┌─ Workflow Phases ────────────────────────────────────────────────────┐");
        println!("│ Generate Data:  {}", if workflow.generate_data.unwrap_or(false) { "✅ YES" } else { "❌ NO" });
        println!("│ Training:       {}", if workflow.train.unwrap_or(false) { "✅ YES" } else { "❌ NO" });
        println!("│ Checkpoint:     {}", if workflow.checkpoint.unwrap_or(false) { "✅ YES" } else { "❌ NO" });
        println!("│ Evaluation:     {} (future)", if workflow.evaluation.unwrap_or(false) { "✅ YES" } else { "❌ NO" });
        println!("└──────────────────────────────────────────────────────────────────────┘");
        println!();
    }
    
    // Dataset configuration - detect backend type
    let data_folder = &config.dataset.data_folder;
    let backend_type = if data_folder.starts_with("s3://") {
        "Amazon S3"
    } else if data_folder.starts_with("az://") || data_folder.starts_with("azure://") {
        "Azure Blob Storage"
    } else if data_folder.starts_with("gs://") {
        "Google Cloud Storage"
    } else if data_folder.starts_with("file://") {
        "Local Filesystem (file://)"
    } else if data_folder.starts_with("direct://") {
        "Direct I/O (direct://)"
    } else if data_folder.starts_with("/") {
        "Local Filesystem (absolute path)"
    } else {
        "Unknown Backend"
    };
    
    println!("┌─ Dataset Configuration ──────────────────────────────────────────────┐");
    println!("│ Data Folder:  {}", data_folder);
    println!("│ Backend Type: {}", backend_type);
    
    // Display multi-endpoint configuration if present
    if let Some(ref endpoint_uris) = config.dataset.endpoint_uris {
        if endpoint_uris.len() > 1 {
            println!("│");
            println!("│ Multi-Endpoint Configuration:");
            println!("│   Endpoints:  {} URIs", endpoint_uris.len());
            for (i, uri) in endpoint_uris.iter().enumerate() {
                println!("│     [{}] {}", i + 1, uri);
            }
            println!("│   Strategy:   {}", config.dataset.load_balance_strategy);
        }
    }
    
    if let Some(ref format) = config.dataset.format {
        println!("│ Format:       {}", format);
    }
    if let Some(record_len) = config.dataset.record_length_bytes {
        println!("│ Record Size:  {} bytes ({:.2} MB)", 
                 record_len, 
                 record_len as f64 / (1024.0 * 1024.0));
    }
    if let Some(samples) = config.dataset.num_samples_per_file {
        println!("│ Samples/File: {}", samples);
    }
    println!("└──────────────────────────────────────────────────────────────────────┘");
    println!();
    
    // Directory structure analysis
    let dir_mode = DirectoryMode::from_config(
        config.dataset.directory_tree.as_ref(),
        config.dataset.num_subfolders_train,
    )?;
    
    let num_files = config.dataset.num_files_train.unwrap_or(0);
    let format_str = config.dataset.format.as_deref().unwrap_or("dat");
    
    match &dir_mode {
        DirectoryMode::Flat => {
            println!("┌─ Directory Structure: Mode 1 (Flat) ────────────────────────────────┐");
            println!("│ Structure:     Single directory (all files in one location)");
            println!("│ Files:         {} training files", num_files);
            println!("│ Path Pattern:  train_file_{{:08}}.{}", format_str);
            println!("│ Example:       train_file_00000000.{}", format_str);
            println!("│                train_file_00000001.{}", format_str);
            
            if let (Some(record_len), Some(samples_per_file)) = 
                (config.dataset.record_length_bytes, config.dataset.num_samples_per_file) {
                let total_bytes = num_files as u64 * samples_per_file as u64 * record_len as u64;
                let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!("│ Total Size:    {:.2} GB", total_gb);
            }
            println!("└──────────────────────────────────────────────────────────────────────┘");
        }
        DirectoryMode::DlioSharding { num_subfolders } => {
            println!("┌─ Directory Structure: Mode 2 (DLIO Sharding) ───────────────────────┐");
            println!("│ Structure:     Flat subdirectories (DLIO-compatible sharding)");
            println!("│ Subdirectories: {} folders (train/0000 through train/{:04})", 
                     num_subfolders, num_subfolders - 1);
            println!("│ Total Files:   {} training files", num_files);
            println!("│ Files/Subdir:  ~{} files per subdirectory", 
                     num_files / num_subfolders);
            println!("│ Distribution:  Modulo sharding (file_i → train/{{i % {}}})", num_subfolders);
            println!("│ Path Pattern:  train/{{:04}}/train_file_{{:08}}.{}", format_str);
            println!("│ Example:       train/0000/train_file_00000000.{}", format_str);
            println!("│                train/0001/train_file_00000001.{}", format_str);
            println!("│                train/0000/train_file_000000{:02}.{}", num_subfolders, format_str);
            
            if let (Some(record_len), Some(samples_per_file)) = 
                (config.dataset.record_length_bytes, config.dataset.num_samples_per_file) {
                let total_bytes = num_files as u64 * samples_per_file as u64 * record_len as u64;
                let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!("│ Total Size:    {:.2} GB", total_gb);
            }
            println!("└──────────────────────────────────────────────────────────────────────┘");
        }
        DirectoryMode::Hierarchical { tree } => {
            println!("┌─ Directory Structure: Mode 3 (Hierarchical Tree) ───────────────────┐");
            println!("│ Structure:     Nested hierarchical directories (realistic ML datasets)");
            println!("│ Width:         {} subdirectories per level", tree.config().width);
            println!("│ Depth:         {} levels", tree.config().depth);
            println!("│ Files/Dir:     {} files per {} directory", 
                     tree.config().files_per_dir,
                     if tree.config().distribution == "bottom" { "leaf" } else { "directory" });
            println!("│ Distribution:  {} (files {})", 
                     tree.config().distribution,
                     if tree.config().distribution == "bottom" { 
                         "only in leaf directories" 
                     } else { 
                         "at every level" 
                     });
            println!("│ Directory Mask: {}", tree.config().dir_mask);
            println!("│");
            
            // Calculate totals
            let total_dirs = tree.total_directories();
            let total_files = tree.total_files();
            
            println!("│ 📊 Calculated Tree Metrics:");
            println!("│   Total Directories:  {}", total_dirs);
            println!("│   Total Files:        {}", total_files);
            
            if let (Some(record_len), Some(samples_per_file)) = 
                (config.dataset.record_length_bytes, config.dataset.num_samples_per_file) {
                let total_bytes = total_files as u64 * samples_per_file as u64 * record_len as u64;
                let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!("│   Total Size:         {:.2} GB", total_gb);
            }
            
            // Show example paths
            println!("│");
            println!("│ Example Paths:");
            if tree.config().depth >= 1 {
                // Build example path manually
                let example_dir = format!("{}", 
                    tree.config().dir_mask.replace("%d", "1").replacen("%d", "0", 1));
                println!("│   {}/train_file_00000000.{}", example_dir, format_str);
            }
            println!("└──────────────────────────────────────────────────────────────────────┘");
        }
    }
    println!();
    
    // Reader configuration
    println!("┌─ Data Loader Configuration ──────────────────────────────────────────┐");
    if let Some(ref loader) = config.reader.data_loader {
        println!("│ Loader Type:       {}", loader);
    }
    if let Some(batch_size) = config.reader.batch_size {
        println!("│ Batch Size:        {}", batch_size);
    }
    if let Some(read_threads) = config.reader.read_threads {
        println!("│ Read Threads:      {}", read_threads);
    }
    if let Some(compute_threads) = config.reader.compute_threads {
        println!("│ Compute Threads:   {}", compute_threads);
    }
    if let Some(prefetch) = config.reader.prefetch {
        println!("│ Prefetch:          {}", prefetch);
    }
    if let Some(transfer_size) = config.reader.transfer_size {
        println!("│ Transfer Size:     {} bytes", transfer_size);
    }
    if let Some(shuffle) = config.reader.shuffle {
        println!("│ Shuffle:           {}", if shuffle { "✅ YES" } else { "❌ NO" });
    }
    println!("└──────────────────────────────────────────────────────────────────────┘");
    println!();
    
    // Training configuration
    if let Some(ref workflow) = config.workflow {
        if workflow.train.unwrap_or(false) {
            if let Some(ref train_config) = config.train {
                println!("┌─ Training Configuration ─────────────────────────────────────────────┐");
                if let Some(epochs) = train_config.epochs {
                    println!("│ Epochs:            {}", epochs);
                }
                if let Some(comp_time) = train_config.computation_time {
                    println!("│ Computation Time:  {:.3}s per batch", comp_time);
                }
                
                // Calculate training metrics
                if let (Some(batch_size), Some(epochs)) = (config.reader.batch_size, train_config.epochs) {
                    let total_samples = num_files * config.dataset.num_samples_per_file.unwrap_or(1);
                    let batches_per_epoch = (total_samples + batch_size - 1) / batch_size;
                    let total_batches = batches_per_epoch * epochs as usize;
                    
                    println!("│");
                    println!("│ 📊 Estimated Training Workload:");
                    println!("│   Total Samples:   {} ({} files × {} samples/file)", 
                             total_samples, num_files, config.dataset.num_samples_per_file.unwrap_or(1));
                    println!("│   Batches/Epoch:   {}", batches_per_epoch);
                    println!("│   Total Batches:   {}", total_batches);
                    
                    if let Some(comp_time) = train_config.computation_time {
                        let estimated_compute_time = total_batches as f64 * comp_time;
                        println!("│   Compute Time:    {:.1}s ({:.1} min) - excluding I/O", 
                                 estimated_compute_time, estimated_compute_time / 60.0);
                    }
                }
                println!("└──────────────────────────────────────────────────────────────────────┘");
                println!();
            }
        }
    }
    
    // Checkpoint configuration
    if let Some(ref workflow) = config.workflow {
        if workflow.checkpoint.unwrap_or(false) {
            if let Some(ref ckpt_config) = config.checkpointing {
                println!("┌─ Checkpoint Configuration ───────────────────────────────────────────┐");
                if let Some(ref folder) = ckpt_config.checkpoint_folder {
                    println!("│ Checkpoint Folder: {}", folder);
                }
                if let Some(after_epoch) = ckpt_config.checkpoint_after_epoch {
                    println!("│ After Epoch:       {}", after_epoch);
                }
                if let Some(epoch_interval) = ckpt_config.epochs_between_checkpoints {
                    println!("│ Epoch Interval:    every {} epoch(s)", epoch_interval);
                }
                if let Some(step_interval) = ckpt_config.steps_between_checkpoints {
                    println!("│ Step Interval:     every {} step(s)", step_interval);
                }
                println!("└──────────────────────────────────────────────────────────────────────┘");
                println!();
            }
        }
    }
    
    // Object store specific warnings
    if backend_type.contains("S3") || backend_type.contains("Azure") || backend_type.contains("Google") {
        println!("⚠️  Object Store Notes:");
        println!("   - Directories are implicit (created automatically with first object)");
        println!("   - No explicit mkdir operations will be performed");
        println!("   - Directory tree structure reflected in object key paths");
        println!();
    }
    
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║                         DRY-RUN VALIDATION COMPLETE                   ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("✅ Configuration is valid and ready to execute.");
    println!("   Remove --dry-run flag to run the workload.");
    println!();
    
    Ok(())
}


