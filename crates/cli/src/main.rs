use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dl_driver_core::DlioConfig;
use dl_driver_core::plugins::PluginManager;
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset};
use tracing::{error, info};

use futures_util::StreamExt;

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
            timeout
        ).await,
        Commands::Validate { config, to_json } => validate_dlio_config(&config, to_json).await,
        Commands::Generate {
            config: _,
            verbose: _,
            skip_existing: _,
        } => {
            eprintln!("Generate command temporarily disabled during refactor");
            Ok(())
        },
    }
}

/// Unified DLIO execution engine with optional MLPerf compliance mode
async fn run_unified_dlio(
    config_path: &std::path::Path,
    pretty: bool,
    mlperf_mode: bool,
    format: &str,
    output_path: Option<&std::path::Path>,
    max_epochs: u32,
    max_steps: u32,
    pool_size: usize,
    readahead: usize,
    max_inflight: usize,
    timeout: u64,
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
    let mut plugins = PluginManager::new();
    
    // Add CheckpointPlugin if checkpointing is enabled in config
    if let Some(checkpoint_plugin) = dl_driver_core::plugins::CheckpointPlugin::new(&dlio_config).await? {
        plugins.push(Box::new(checkpoint_plugin));
        info!("CheckpointPlugin registered");
    }
    
    plugins.initialize(&dlio_config).await
        .context("Failed to initialize plugins")?;

    // Initialize metrics system (always available, enhanced in MLPerf mode)
    let mut metrics = if mlperf_mode {
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

    // Convert to LoaderOptions
    let mut loader_opts = dlio_config.to_loader_options();

    // Override pool configuration with CLI arguments
    let mut pool_config = dlio_config.to_pool_config();
    pool_config.pool_size = pool_size;
    pool_config.readahead_batches = readahead;
    pool_config.max_inflight = max_inflight;
    pool_config.batch_timeout = std::time::Duration::from_secs(timeout);

    loader_opts.loading_mode =
        s3dlio::data_loader::options::LoadingMode::AsyncPool(pool_config.clone());

    info!(
        "Creating dataset for URI: {}",
        dlio_config.data_folder_uri()
    );

    // Create dataset from DLIO config
    let dataset = MultiBackendDataset::from_prefix(dlio_config.data_folder_uri()).await?;
    info!("✅ Dataset created with {} files", dataset.len());

    // Create AsyncPoolDataLoader
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_opts);
    info!(
        "✅ AsyncPoolDataLoader created with pool_size={}, readahead={}",
        pool_size, readahead
    );

    // Start metrics tracking
    metrics.begin_run();
    let start_time = std::time::Instant::now();

    // Run the unified data loading loop
    let mut stream = dataloader.stream_with_pool(pool_config);
    let mut batch_count = 0;
    let mut step: u32 = 0;
    let mut epoch: u32 = 0;
    let samples_per_epoch = dlio_config.dataset.num_files_train.unwrap_or(100) 
        * dlio_config.dataset.num_samples_per_file.unwrap_or(1);
    let mut samples_this_epoch = 0;

    info!("🚀 Starting DLIO workload execution...");
    if mlperf_mode {
        info!("MLPerf mode: Enhanced metrics and reporting enabled");
        info!("Limits: {} epochs, {} steps", max_epochs, max_steps);
    }

    while let Some(batch_result) = stream.next().await {
        match batch_result {
            Ok(batch) => {
                batch_count += 1;
                samples_this_epoch += batch.len();

                // Record metrics (enhanced in MLPerf mode)
                metrics.on_batch(&batch);
                
                // Record access order for deterministic validation (MLPerf compliance)
                if mlperf_mode {
                    metrics.record_item_access(format!("step_{:08}", step));
                }

                if batch_count % 10 == 0 {
                    let total_samples = metrics.total_samples;
                    info!(
                        "Processed {} batches, {} samples total",
                        batch_count, total_samples
                    );
                }

                // Simulate some compute work based on DLIO config
                if let Some(_compute_threads) = dlio_config.reader.compute_threads {
                    tokio::task::yield_now().await; // Simulate compute delay
                }

                // Plugin hook after each step (batch)
                plugins.after_step(step).await
                    .context("Plugin after_step failed")?;
                
                step += 1;

                // Check if we've completed an epoch (MLPerf tracking)
                if mlperf_mode && samples_this_epoch >= samples_per_epoch {
                    epoch += 1;
                    samples_this_epoch = 0;
                    
                    info!("Completed epoch {} after {} steps", epoch, step);
                    
                    // Plugin hook after each epoch
                    plugins.after_epoch(epoch).await
                        .context("Plugin after_epoch failed")?;

                    // Check configurable epoch limit (MLPerf mode)
                    if epoch >= max_epochs {
                        info!("Completed {} epochs (limit: {}), ending benchmark", epoch, max_epochs);
                        break;
                    }
                }

                // Check configurable step limit (MLPerf mode)
                if mlperf_mode && step >= max_steps {
                    info!("Reached step limit ({} steps), ending benchmark", step);
                    break;
                }
            }
            Err(e) => {
                error!("Batch processing error: {}", e);
                break;
            }
        }
    }

    // Finalize plugins
    plugins.finalize().await
        .context("Failed to finalize plugins")?;

    let total_time = start_time.elapsed();
    metrics.complete_run(total_time);

    info!("✅ DLIO workload completed successfully");

    // Output results based on mode
    if mlperf_mode {
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
    } else {
        // Basic DLIO output
        info!(
            "📊 Final Stats: {} batches, {} samples in {:?}",
            batch_count, metrics.total_samples, total_time
        );

        if metrics.total_samples > 0 {
            let throughput = metrics.throughput_samples_per_sec();
            info!("📈 Throughput: {:.2} samples/sec", throughput);
        }
    }

    Ok(())
}

/// Data generation phase using s3dlio (shared by both modes)
async fn run_data_generation(config: &DlioConfig) -> Result<()> {
    use s3dlio::object_store::store_for_uri;
    
    let start_time = std::time::Instant::now();
    info!("Starting data generation phase");

    // Create object store for the configured storage backend
    let store = store_for_uri(&config.dataset.data_folder)
        .with_context(|| format!("Failed to create object store for {}", config.dataset.data_folder))?;

    let num_files = config.dataset.num_files_train.unwrap_or(100);
    let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1);
    let record_size = config.dataset.record_length_bytes.unwrap_or(1024);

    info!(
        "Generating {} files with {} samples each ({}B per record)",
        num_files, samples_per_file, record_size
    );

    // Generate data files using s3dlio's object store
    for file_idx in 0..num_files {
        // Create full URI path by combining base data folder with filename
        let file_name = format!("train_file_{:06}.{}", file_idx, config.dataset.format);
        let data_folder = &config.dataset.data_folder;
        let full_path = if data_folder.ends_with('/') {
            format!("{}{}", data_folder, file_name)
        } else {
            format!("{}/{}", data_folder, file_name)
        };

        // Generate synthetic data (simple for now)
        let data = generate_synthetic_data(samples_per_file, record_size);

        let write_start = std::time::Instant::now();
        store
            .put(&full_path, &data)
            .await
            .with_context(|| format!("Failed to write file {}", full_path))?;
        let write_time = write_start.elapsed();

        if file_idx % 10 == 0 || file_idx == num_files - 1 {
            info!(
                "Generated {}/{} files (wrote {} bytes to {} in {:?})",
                file_idx + 1, num_files, data.len(), full_path, write_time
            );
        }
    }

    let generation_time = start_time.elapsed();
    info!("✅ Data generation completed in {:?}", generation_time);
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
    
    // Display dataset info using the flat RunPlan
    println!("  - Dataset: {} files, {} samples/file, {} bytes/record",
        run_plan.num_files_train.unwrap_or(0),
        run_plan.num_samples_per_file.unwrap_or(0),
        run_plan.record_length_bytes.unwrap_or(0));
        
    // Calculate totals
    let total_samples = run_plan.num_files_train.unwrap_or(0) * 
                       run_plan.num_samples_per_file.unwrap_or(0);
    let total_bytes = total_samples * run_plan.record_length_bytes.unwrap_or(0);
    
    println!("  - Total: {} samples, {:.2} MB",
        total_samples,
        total_bytes as f64 / 1024.0 / 1024.0);

    println!("🎉 DLIO configuration is valid and ready to run!");

    Ok(())
}


