use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dl_driver_core::{DlioConfig, WorkloadRunner, MlperfRunner};
use dl_driver_core::plugins::PluginManager;
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset};
use tracing::{error, info};

use futures_util::StreamExt;

/// dl-driver CLI – A high-performance DLIO benchmark alternative built in Rust.
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
    /// Run with legacy dl-driver config format
    Legacy {
        /// Path to a dl-driver YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// If set, dump the parsed YAML back to stdout
        #[arg(long)]
        pretty: bool,
    },
    /// Run with DLIO-compatible config format
    Dlio {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// If set, dump the parsed YAML back to stdout
        #[arg(long)]
        pretty: bool,

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
    /// Run MLPerf benchmark (standalone, no WorkloadRunner)
    Mlperf {
        /// Path to a DLIO YAML config file
        #[arg(short, long)]
        config: std::path::PathBuf,

        /// Output format for report
        #[arg(long, default_value = "json")]
        format: String,

        /// Save report to file instead of stdout
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
}

#[tokio::main]
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
        Commands::Legacy { config, pretty } => run_legacy_config(&config, pretty).await,
        Commands::Dlio {
            config,
            pretty,
            pool_size,
            readahead,
            max_inflight,
            timeout,
        } => run_dlio_config(&config, pretty, pool_size, readahead, max_inflight, timeout).await,
        Commands::Validate { config, to_json } => validate_dlio_config(&config, to_json).await,
        Commands::Generate {
            config: _,
            verbose: _,
            skip_existing: _,
        } => {
            eprintln!("Generate command temporarily disabled during refactor");
            Ok(())
        },
        Commands::Mlperf { config, format, output } => run_mlperf_benchmark(&config, &format, output.as_deref()).await,
    }
}

async fn run_legacy_config(config_path: &std::path::Path, pretty: bool) -> Result<()> {
    info!("Loading legacy dl-driver config from: {:?}", config_path);

    // Load and parse configuration
    let config = DlioConfig::from_yaml_file(config_path)?;

    if pretty {
        // Pretty-print the parsed config
        println!("=== Parsed Legacy Configuration ===");
        println!("{:#?}", config);
        println!("Storage Backend: {:?}", config.storage_backend());
        println!("Storage URI: {}", config.storage_uri());
        return Ok(());
    }

    // Run the workload
    let mut runner = WorkloadRunner::new(config);

    match runner.run().await {
        Ok(()) => {
            info!("Legacy workload completed successfully");
            runner.get_metrics().print_summary();
        }
        Err(e) => {
            error!("Legacy workload failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

async fn run_dlio_config(
    config_path: &std::path::Path,
    pretty: bool,
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
        return Ok(());
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
        "Creating object store for URI: {}",
        dlio_config.data_folder_uri()
    );

    // Create dataset from DLIO config
    let dataset = MultiBackendDataset::from_prefix(dlio_config.data_folder_uri()).await?;
    info!("✅ Dataset created successfully");

    // Create AsyncPoolDataLoader
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_opts);
    info!(
        "✅ AsyncPoolDataLoader created with pool_size={}, readahead={}",
        pool_size, readahead
    );

    // Run the data loading simulation
    let mut stream = dataloader.stream_with_pool(pool_config);
    let mut batch_count = 0;
    let mut total_files = 0;
    let start_time = std::time::Instant::now();

    info!("🚀 Starting DLIO workload execution...");

    while let Some(batch_result) = stream.next().await {
        match batch_result {
            Ok(batch) => {
                batch_count += 1;
                total_files += batch.len();

                if batch_count % 10 == 0 {
                    info!(
                        "Processed {} batches, {} files total",
                        batch_count, total_files
                    );
                }

                // Simulate some compute work based on DLIO config
                if let Some(_compute_threads) = dlio_config.reader.compute_threads {
                    tokio::task::yield_now().await; // Simulate compute delay
                }
            }
            Err(e) => {
                error!("Batch processing error: {}", e);
                break;
            }
        }
    }

    let elapsed = start_time.elapsed();
    info!("✅ DLIO workload completed successfully");
    info!(
        "📊 Final Stats: {} batches, {} files in {:?}",
        batch_count, total_files, elapsed
    );

    if total_files > 0 {
        let throughput = total_files as f64 / elapsed.as_secs_f64();
        info!("📈 Throughput: {:.2} files/sec", throughput);
    }

    Ok(())
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

async fn run_mlperf_benchmark(
    config_path: &std::path::Path,
    format: &str,
    output_path: Option<&std::path::Path>,
) -> Result<()> {
    info!("Loading DLIO config for MLPerf benchmark from: {:?}", config_path);

    // Load and parse DLIO configuration
    let config = DlioConfig::from_yaml_file(config_path)
        .with_context(|| format!("Failed to load DLIO config from {:?}", config_path))?;

    info!("Running MLPerf benchmark: {}", 
          config.model.as_ref()
              .and_then(|m| m.name.as_ref())
              .unwrap_or(&"unknown".to_string()));

    // Create plugin manager (empty for now, will add checkpointing in M5)
    let plugins = PluginManager::new();

    // Create and run MLPerf runner
    let mut runner = MlperfRunner::new(config, plugins);
    let report = runner.run().await
        .context("MLPerf benchmark execution failed")?;

    // Generate output based on format
    let output_content = match format.to_lowercase().as_str() {
        "json" => report.to_json()?,
        "csv" => {
            let mut csv_content = String::new();
            csv_content.push_str(&format!("{}\n", dl_driver_core::MlperfReport::to_csv_header()));
            csv_content.push_str(&format!("{}\n", report.to_csv_row()));
            csv_content
        }
        _ => return Err(anyhow::anyhow!("Unsupported format '{}'. Use 'json' or 'csv'", format)),
    };

    // Output to file or stdout
    if let Some(output_file) = output_path {
        std::fs::write(output_file, output_content)
            .with_context(|| format!("Failed to write report to {:?}", output_file))?;
        println!("✅ MLPerf report written to {:?}", output_file);
    } else {
        println!("{}", output_content);
    }

    // Print summary to stderr so it doesn't interfere with JSON/CSV output
    eprintln!("🏁 MLPerf benchmark completed:");
    eprintln!("  Backend: {}", report.backend_type);
    eprintln!("  Samples: {}", report.total_samples);
    eprintln!("  Throughput: {:.2} samples/sec", report.throughput_samples_per_sec);
    eprintln!("  P99 latency: {:.3} ms", report.p99_latency_ms);

    Ok(())
}
