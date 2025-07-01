use anyhow::Result;
use clap::Parser;
use real_dlio_core::{Config, Runner};
use real_dlio_storage::PosixBackend;
use real_dlio_formats::NpzFormat;

/// real_dlio CLI – Milestone M1.
///
/// Parse a workload YAML, then confirm success or pretty-print it.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to a DLIO-style workload YAML file.
    #[arg(short, long)]
    config: std::path::PathBuf,

    /// If set, dump the parsed YAML back to stdout.
    #[arg(long)]
    pretty: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Use the core library to load & validate the YAML.
    let cfg = real_dlio_core::Config::from_yaml_file(&args.config)?;

    if args.pretty {
        println!("{}", serde_yaml::to_string(&cfg.raw)?);
    } else {
        println!("✅ Parsed YAML successfully: {:?}", args.config);
    }

    // for now we’ll ignore cfg contents and just exercise POSIX+NPZ:
    let mut runner = Runner::new(
        PosixBackend::new("./data"),
        NpzFormat::new(vec![1024, 1024]),  // 1 MiB array as demo
    );
    runner.run_once("bench.npy", "./bench.npy")?;
    Ok(())
}

