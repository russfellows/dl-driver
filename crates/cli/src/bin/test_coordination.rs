//! Simple coordination test to debug hanging issues

use anyhow::Result;
use dl_driver_core::coordination::RankCoordinator;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .init();
    
    // Get rank and world_size from command line
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <rank> <world_size>", args[0]);
        std::process::exit(1);
    }
    
    let rank: u32 = args[1].parse()?;
    let world_size: u32 = args[2].parse()?;
    
    println!("🚀 Starting coordination test: rank={}, world_size={}", rank, world_size);
    
    // Create coordinator
    let coord = RankCoordinator::new(rank, world_size, "test_coord")?;
    println!("✅ Rank {}: Created coordinator", rank);
    
    // Register and wait
    println!("📝 Rank {}: Registering...", rank);
    coord.register_and_wait().await?;
    println!("✅ Rank {}: All registered", rank);
    
    // Test barrier
    println!("🚧 Rank {}: Entering barrier...", rank);
    coord.barrier("test_barrier").await?;
    println!("✅ Rank {}: Passed barrier", rank);
    
    // Mark finished
    println!("🏁 Rank {}: Finishing...", rank);
    coord.mark_finished_and_wait().await?;
    println!("✅ Rank {}: All finished", rank);
    
    // Get stats
    let stats = coord.get_stats();
    println!("📊 Rank {}: Final stats: {:?}", rank, stats);
    
    // Cleanup
    coord.cleanup()?;
    println!("🧹 Rank {}: Cleanup complete", rank);
    
    Ok(())
}