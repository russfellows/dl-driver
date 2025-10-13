// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! dl-driver-agent - gRPC agent binary for distributed DLIO workload execution
//!
//! This binary runs a gRPC server that receives DLIO workload configurations,
//! executes them, and returns performance metrics. It's designed to be deployed
//! on multiple hosts for coordinated multi-host benchmarking.

use anyhow::{Context, Result};
use clap::Parser;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::info;
use tracing_subscriber::EnvFilter;

use dl_driver_core::dist::agent::AgentService;
use dl_driver_core::dist::proto::dist_agent_server::DistAgentServer;

/// Command-line arguments for the agent
#[derive(Parser, Debug)]
#[command(name = "dl-driver-agent")]
#[command(about = "DLIO workload agent for distributed execution", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "50051")]
    port: u16,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind_addr: String,

    /// Agent identifier (defaults to hostname:port)
    #[arg(short, long)]
    agent_id: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing/logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    // Determine agent ID
    let agent_id = args.agent_id.unwrap_or_else(|| {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());
        format!("{}:{}", hostname, args.port)
    });

    info!("Starting dl-driver-agent");
    info!("  Agent ID: {}", agent_id);
    info!("  Bind address: {}", args.bind_addr);
    info!("  Port: {}", args.port);
    info!("  Log level: {}", args.log_level);

    // Parse socket address
    let addr: SocketAddr = format!("{}:{}", args.bind_addr, args.port)
        .parse()
        .context("Invalid bind address or port")?;

    // Create the agent service
    let agent_service = AgentService::new(agent_id.clone());

    info!("Agent {} starting gRPC server on {}", agent_id, addr);

    // Build and start the gRPC server
    Server::builder()
        .add_service(DistAgentServer::new(agent_service))
        .serve_with_shutdown(addr, shutdown_signal())
        .await
        .context("Failed to start gRPC server")?;

    info!("Agent {} shut down gracefully", agent_id);

    Ok(())
}

/// Handle shutdown signals (SIGTERM, SIGINT, Ctrl+C)
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal, shutting down");
        },
        _ = terminate => {
            info!("Received SIGTERM signal, shutting down");
        },
    }
}
