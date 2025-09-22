use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use dl_driver_core::{Config, DlioConfig, WorkloadRunner, DatasetGenerator};
use tracing::{info, error};
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset};

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
        Commands::Legacy { config, pretty } => {
            run_legacy_config(&config, pretty).await
        }
        Commands::Dlio { config, pretty, pool_size, readahead, max_inflight, timeout } => {
            run_dlio_config(&config, pretty, pool_size, readahead, max_inflight, timeout).await
        }
        Commands::Validate { config, to_json } => {
            validate_dlio_config(&config, to_json).await
        }
        Commands::Generate { config, verbose, skip_existing } => {
            generate_dataset_from_config(&config, verbose, skip_existing).await
        }
    }
}

async fn run_legacy_config(config_path: &std::path::Path, pretty: bool) -> Result<()> {
    info!("Loading legacy dl-driver config from: {:?}", config_path);

    // Load and parse configuration
    let config = Config::from_yaml_file(&config_path)?;

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
    timeout: u64
) -> Result<()> {
    info!("Loading DLIO config from: {:?}", config_path);
    
    // Load DLIO configuration
    let yaml_content = std::fs::read_to_string(config_path)?;
    let dlio_config = DlioConfig::from_yaml(&yaml_content)?;
    
    if pretty {
        println!("=== Parsed DLIO Configuration ===");
        println!("{:#?}", dlio_config);
        println!("Data folder URI: {}", dlio_config.data_folder_uri());
        println!("Should generate data: {}", dlio_config.should_generate_data());
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
    
    loader_opts.loading_mode = s3dlio::data_loader::options::LoadingMode::AsyncPool(pool_config.clone());
    
    info!("Creating object store for URI: {}", dlio_config.data_folder_uri());
    
    // Create dataset from DLIO config
    let dataset = MultiBackendDataset::from_prefix(dlio_config.data_folder_uri()).await?;
    info!("✅ Dataset created successfully");
    
    // Create AsyncPoolDataLoader
    let dataloader = AsyncPoolDataLoader::new(dataset, loader_opts);
    info!("✅ AsyncPoolDataLoader created with pool_size={}, readahead={}", pool_size, readahead);
    
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
                    info!("Processed {} batches, {} files total", batch_count, total_files);
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
    info!("📊 Final Stats: {} batches, {} files in {:?}", batch_count, total_files, elapsed);
    
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
    println!("✅ Model name: {:?}", dlio_config.model.as_ref().and_then(|m| m.name.as_ref()));
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
    
    // Test RunPlan conversion  
    let run_plan = dlio_config.to_run_plan()?;
    println!("✅ RunPlan conversion: SUCCESS");
    println!("  - Model: {} ({})", run_plan.model.name, run_plan.model.framework);
    println!("  - Workflow: generate_data={}, train={}, checkpoint={}, evaluation={}", 
             run_plan.workflow.generate_data, 
             run_plan.workflow.train, 
             run_plan.workflow.checkpoint, 
             run_plan.workflow.evaluation);
    println!("  - Dataset: {} files, {} samples/file, {} bytes/record",
             run_plan.dataset.train.num_files,
             run_plan.dataset.train.num_samples_per_file,
             run_plan.dataset.train.record_length_bytes);
    println!("  - Total: {} samples, {:.2} MB",
             run_plan.dataset.train.total_samples,
             run_plan.dataset.train.total_bytes as f64 / 1024.0 / 1024.0);
    
    println!("🎉 DLIO configuration is valid and ready to run!");
    
    Ok(())
}

async fn generate_dataset_from_config(
    config_path: &std::path::Path, 
    verbose: bool,
    skip_existing: bool
) -> Result<()> {
    info!("Loading DLIO config from: {:?}", config_path);
    
    // Read and parse the DLIO configuration
    let yaml_content = tokio::fs::read_to_string(config_path).await
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
    
    let dlio_config: DlioConfig = serde_yaml::from_str(&yaml_content)
        .with_context(|| "Failed to parse DLIO YAML configuration")?;
    
    // Convert to RunPlan
    let run_plan = dlio_config.to_run_plan()
        .with_context(|| "Failed to convert DLIO config to RunPlan")?;
    
    println!("📋 Dataset Generation Plan");
    println!("  Format: {}", run_plan.dataset.format);
    println!("  Training files: {}", run_plan.dataset.train.num_files);
    println!("  Samples per file: {}", run_plan.dataset.train.num_samples_per_file);
    println!("  Record size: {} bytes", run_plan.dataset.train.record_length_bytes);
    println!("  Total size: {:.2} MB", run_plan.dataset.train.total_bytes as f64 / 1024.0 / 1024.0);
    println!("  Output directory: {}", run_plan.dataset.data_folder_uri);
    
    if let Some(eval) = &run_plan.dataset.eval {
        if eval.num_files > 0 {
            println!("  Evaluation files: {}", eval.num_files);
        }
    }
    
    // Check if data already exists
    let data_path = if run_plan.dataset.data_folder_uri.starts_with("file://") {
        std::path::Path::new(&run_plan.dataset.data_folder_uri[7..])
    } else {
        std::path::Path::new(&run_plan.dataset.data_folder_uri)
    };
    
    if data_path.exists() && skip_existing {
        println!("⏭️  Data directory already exists, skipping generation");
        return Ok(());
    }
    
    // Create dataset generator
    let generator = DatasetGenerator::new(run_plan);
    let mut metrics = dl_driver_core::Metrics::new();
    
    let start_time = std::time::Instant::now();
    
    if verbose {
        println!("🚀 Starting dataset generation...");
    }
    
    // Generate the dataset
    generator.generate_dataset(&mut metrics).await
        .with_context(|| "Dataset generation failed")?;
    
    let total_time = start_time.elapsed();
    
    println!("✅ Dataset generation completed!");
    println!("  Files processed: {}", metrics.files_processed());
    println!("  Bytes written: {} MB", metrics.bytes_written() / 1024 / 1024);
    println!("  Total time: {:.2} seconds", total_time.as_secs_f64());
    
    if metrics.bytes_written() > 0 && total_time.as_secs_f64() > 0.0 {
        let throughput = (metrics.bytes_written() as f64) / (1024.0 * 1024.0) / total_time.as_secs_f64();
        println!("  Throughput: {:.2} MB/s", throughput);
    }
    
    Ok(())
}
