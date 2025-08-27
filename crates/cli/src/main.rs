use anyhow::Result;
use clap::Parser;
use real_dlio_core::{Config, WorkloadRunner};
use tracing::{info, error};

/// real_dlio CLI – A simplified, high-performance DLIO benchmark alternative.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to a DLIO-style workload YAML file.
    #[arg(short, long)]
    config: std::path::PathBuf,

    /// If set, dump the parsed YAML back to stdout.
    #[arg(long)]
    pretty: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "info" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("real_dlio={}", log_level))
        .init();

    info!("real_dlio v{} starting", env!("CARGO_PKG_VERSION"));
    info!("Loading config from: {:?}", args.config);

    // Load and parse configuration
    let config = Config::from_yaml_file(&args.config)?;

    if args.pretty {
        // Pretty-print the parsed config
        println!("=== Parsed Configuration ===");
        println!("{:#?}", config);
        println!("Storage Backend: {:?}", config.storage_backend());
        println!("Storage URI: {}", config.storage_uri());
        return Ok(());
    }

    // Run the workload
    let mut runner = WorkloadRunner::new(config);
    
    match runner.run().await {
        Ok(()) => {
            info!("Workload completed successfully");
            runner.get_metrics().print_summary();
        }
        Err(e) => {
            error!("Workload failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}
