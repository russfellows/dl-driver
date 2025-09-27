//! Multi-rank coordination using shared memory and atomic operations
//! 
//! This module provides proper distributed coordination for multi-GPU/multi-rank
//! workload execution without external dependencies like MPI or network services.

use anyhow::{Context, Result};
use shared_memory::{Shmem, ShmemConf};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, Ordering};
// Removed unused Arc and Barrier imports
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Shared coordination state between all ranks
#[repr(C)]
struct CoordinationState {
    /// Total number of ranks in this execution
    world_size: AtomicU32,
    
    /// Number of ranks that have registered
    registered_ranks: AtomicU32,
    
    /// Number of ranks ready to start execution
    ready_ranks: AtomicU32,
    
    /// Number of ranks that have finished execution
    finished_ranks: AtomicU32,
    
    /// Global start timestamp (nanoseconds since UNIX_EPOCH)
    global_start_time: AtomicU64,
    
    /// Global end timestamp (nanoseconds since UNIX_EPOCH)
    global_end_time: AtomicU64,
    
    /// Flag indicating if coordination is active
    active: AtomicBool,
    
    /// Emergency abort flag
    abort: AtomicBool,
    
    /// Per-rank heartbeat timestamps (up to 64 ranks supported)
    rank_heartbeats: [AtomicU64; 64],
    
    /// Per-rank status flags (0=not_started, 1=ready, 2=running, 3=finished, 4=failed)
    rank_status: [AtomicU32; 64],
    
    /// Per-rank metrics results in shared memory (avoid temp files)
    rank_results: [RankResultsShared; 64],
}

/// Shared memory results structure for each rank (avoid temp files)
#[repr(C)]
struct RankResultsShared {
    /// Files processed by this rank
    files_processed: AtomicU64,
    
    /// Bytes read by this rank  
    bytes_read: AtomicU64,
    
    /// Storage throughput in bytes per second
    throughput_bps: AtomicU64,
    
    /// Wall clock execution time in nanoseconds
    wall_clock_time_ns: AtomicU64,
    
    /// AU fraction for this rank (stored as u64, divide by 1e15 for actual value)
    au_fraction_scaled: AtomicU64,
    
    /// Rank execution start time
    start_time_ns: AtomicU64,
    
    /// Rank execution end time  
    end_time_ns: AtomicU64,
    
    /// Results valid flag
    results_valid: AtomicBool,
}

impl RankResultsShared {
    const fn new() -> Self {
        Self {
            files_processed: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            throughput_bps: AtomicU64::new(0),
            wall_clock_time_ns: AtomicU64::new(0),
            au_fraction_scaled: AtomicU64::new(0),
            start_time_ns: AtomicU64::new(0),
            end_time_ns: AtomicU64::new(0),
            results_valid: AtomicBool::new(false),
        }
    }
}

impl CoordinationState {
    fn new(world_size: u32) -> Self {
        const INIT_ATOMIC_U64: AtomicU64 = AtomicU64::new(0);
        const INIT_ATOMIC_U32: AtomicU32 = AtomicU32::new(0);
        const INIT_RANK_RESULTS: RankResultsShared = RankResultsShared::new();
        
        Self {
            world_size: AtomicU32::new(world_size),
            registered_ranks: AtomicU32::new(0),
            ready_ranks: AtomicU32::new(0),
            finished_ranks: AtomicU32::new(0),
            global_start_time: AtomicU64::new(0),
            global_end_time: AtomicU64::new(0),
            active: AtomicBool::new(true),
            abort: AtomicBool::new(false),
            rank_heartbeats: [INIT_ATOMIC_U64; 64],
            rank_status: [INIT_ATOMIC_U32; 64],
            rank_results: [INIT_RANK_RESULTS; 64],
        }
    }
}

/// Multi-rank coordinator for proper distributed execution
pub struct RankCoordinator {
    rank: u32,
    world_size: u32,
    _shared_mem: Shmem,  // Must keep alive to maintain shared memory mapping
    state: &'static CoordinationState,
    coordination_id: String,
}

impl RankCoordinator {
    /// Create or join a coordination group
    pub fn new(rank: u32, world_size: u32, coordination_id: &str) -> Result<Self> {
        if rank >= world_size {
            return Err(anyhow::anyhow!("Rank {} >= world_size {}", rank, world_size));
        }
        
        if world_size > 64 {
            return Err(anyhow::anyhow!("World size {} > 64 (maximum supported)", world_size));
        }
        
        let shmem_name = format!("dl_driver_coord_{}", coordination_id);
        let shmem_size = std::mem::size_of::<CoordinationState>();
        
        info!("🔗 Rank {}: Joining coordination group '{}' (world_size={})", 
              rank, coordination_id, world_size);
        
        // Try to open existing shared memory first
        let (shared_mem, is_creator) = match ShmemConf::new()
            .size(shmem_size)
            .os_id(&shmem_name)
            .open() 
        {
            Ok(shmem) => {
                debug!("Rank {}: Joined existing coordination group", rank);
                (shmem, false)
            }
            Err(_) => {
                // Create new shared memory if it doesn't exist
                let shmem = ShmemConf::new()
                    .size(shmem_size)
                    .os_id(&shmem_name)
                    .create()
                    .with_context(|| format!("Failed to create shared memory: {}", shmem_name))?;
                info!("Rank {}: Created new coordination group", rank);
                (shmem, true)
            }
        };
        
        // Get pointer to shared state
        let state_ptr = shared_mem.as_ptr() as *mut CoordinationState;
        let state = unsafe { &*state_ptr };
        
        // Initialize state if we're the creator
        if is_creator {
            unsafe {
                std::ptr::write(state_ptr, CoordinationState::new(world_size));
            }
            debug!("Rank {}: Initialized coordination state", rank);
        }
        
        // Validate world size matches
        let existing_world_size = state.world_size.load(Ordering::Acquire);
        if existing_world_size != world_size {
            return Err(anyhow::anyhow!(
                "World size mismatch: expected {}, found {}", 
                world_size, existing_world_size
            ));
        }
        
        Ok(Self {
            rank,
            world_size,
            _shared_mem: shared_mem,  // Keep shared memory region alive
            state,
            coordination_id: coordination_id.to_string(),
        })
    }
    
    /// Register this rank and wait for all ranks to register
    pub async fn register_and_wait(&self) -> Result<()> {
        info!("📝 Rank {}: Registering with coordination group '{}'", self.rank, self.coordination_id);
        
        // Set our status to ready
        self.state.rank_status[self.rank as usize].store(1, Ordering::Release);
        self.update_heartbeat();
        
        // Increment registered count
        let registered = self.state.registered_ranks.fetch_add(1, Ordering::AcqRel) + 1;
        debug!("📝 Rank {}: Registered ({}/{})", self.rank, registered, self.world_size);
        
        // Wait for all ranks to register
        let start_wait = Instant::now();
        loop {
            let current_registered = self.state.registered_ranks.load(Ordering::Acquire);
            if current_registered >= self.world_size {
                break;
            }
            
            if self.check_abort()? {
                return Err(anyhow::anyhow!("Coordination aborted during registration"));
            }
            
            self.update_heartbeat();
            
            // Frequent status updates for debugging (only with -vv)
            if start_wait.elapsed().as_secs() % 2 == 0 && start_wait.elapsed().as_millis() % 2000 < 100 {
                debug!("📝 Rank {}: Waiting for registration - {}/{} registered", 
                      self.rank, current_registered, self.world_size);
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Timeout after 20 seconds (reduced for testing)
            if start_wait.elapsed() > Duration::from_secs(20) {
                warn!("⚠️  Rank {}: Registration timeout - {}/{} registered", 
                      self.rank, current_registered, self.world_size);
                return Err(anyhow::anyhow!("Registration timeout: {}/{} registered", 
                                         current_registered, self.world_size));
            }
        }
        
        info!("✅ Rank {}: All ranks registered successfully", self.rank);
        Ok(())
    }
    
    /// Synchronization barrier - wait for all ranks to reach this point  
    pub async fn barrier(&self, barrier_name: &str) -> Result<()> {
        debug!("🚧 Rank {}: Entering barrier '{}'", self.rank, barrier_name);
        self.update_heartbeat();
        
        // Set our individual rank bit (avoid using ready_ranks counter which has reset issues)
        self.state.rank_status[self.rank as usize].store(2, Ordering::Release);
        debug!("🚧 Rank {}: Set ready status for barrier '{}'", self.rank, barrier_name);
        
        // Wait for all ranks to set their ready status
        let start_wait = Instant::now();
        loop {
            let mut all_ready = true;
            for i in 0..self.world_size {
                if self.state.rank_status[i as usize].load(Ordering::Acquire) < 2 {
                    all_ready = false;
                    break;
                }
            }
            
            if all_ready {
                break;
            }
            
            if self.check_abort()? {
                return Err(anyhow::anyhow!("Coordination aborted at barrier '{}'", barrier_name));
            }
            
            self.update_heartbeat();
            
            // Debug output every 5 seconds
            if start_wait.elapsed().as_secs() % 5 == 0 && start_wait.elapsed().as_millis() % 5000 < 100 {
                let ready_count = (0..self.world_size)
                    .map(|i| if self.state.rank_status[i as usize].load(Ordering::Acquire) >= 2 { 1 } else { 0 })
                    .sum::<u32>();
                debug!("🚧 Rank {}: Still waiting at barrier '{}' - ready: {}/{}", 
                      self.rank, barrier_name, ready_count, self.world_size);
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Timeout after 30 seconds
            if start_wait.elapsed() > Duration::from_secs(30) {
                let ready_count = (0..self.world_size)
                    .map(|i| if self.state.rank_status[i as usize].load(Ordering::Acquire) >= 2 { 1 } else { 0 })
                    .sum::<u32>();
                warn!("⚠️  Rank {}: Timeout at barrier '{}' - ready: {}/{}", 
                      self.rank, barrier_name, ready_count, self.world_size);
                return Err(anyhow::anyhow!("Timeout at barrier '{}': {}/{} ready", 
                                         barrier_name, ready_count, self.world_size));
            }
        }
        
        debug!("✅ Rank {}: All ranks ready at barrier '{}'", self.rank, barrier_name);
        
        // Reset rank status for next barrier (each rank resets its own)
        self.state.rank_status[self.rank as usize].store(1, Ordering::Release);
        
        debug!("✅ Rank {}: Exited barrier '{}'", self.rank, barrier_name);
        Ok(())
    }
    
    /// Mark global execution start (only rank 0 should call this)
    pub fn mark_global_start(&self) -> Result<u64> {
        if self.rank != 0 {
            return Err(anyhow::anyhow!("Only rank 0 can mark global start"));
        }
        
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get current time")?
            .as_nanos() as u64;
            
        self.state.global_start_time.store(start_time, Ordering::Release);
        info!("🚀 Rank 0: Marked global execution start");
        Ok(start_time)
    }
    
    /// Get global execution start time
    pub fn get_global_start_time(&self) -> Option<u64> {
        let start_time = self.state.global_start_time.load(Ordering::Acquire);
        if start_time > 0 { Some(start_time) } else { None }
    }
    
    /// Mark execution finished and wait for all ranks to finish
    pub async fn mark_finished_and_wait(&self) -> Result<u64> {
        info!("🏁 Rank {}: Marking execution finished", self.rank);
        
        // Set our status to finished
        self.state.rank_status[self.rank as usize].store(3, Ordering::Release);
        self.update_heartbeat();
        
        // Increment finished count
        let finished = self.state.finished_ranks.fetch_add(1, Ordering::AcqRel) + 1;
        debug!("🏁 Rank {}: Finished ({}/{})", self.rank, finished, self.world_size);
        
        // Wait for all ranks to finish
        let start_wait = Instant::now();
        while self.state.finished_ranks.load(Ordering::Acquire) < self.world_size {
            if self.check_abort()? {
                return Err(anyhow::anyhow!("Coordination aborted during finish wait"));
            }
            
            self.update_heartbeat();
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Timeout after 5 minutes
            if start_wait.elapsed() > Duration::from_secs(300) {
                return Err(anyhow::anyhow!("Timeout waiting for all ranks to finish"));
            }
        }
        
        // Mark global end time (any rank can do this, but only first one wins)
        let end_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get current time")?
            .as_nanos() as u64;
            
        self.state.global_end_time.compare_exchange(0, end_time, Ordering::AcqRel, Ordering::Relaxed).ok();
        
        let final_end_time = self.state.global_end_time.load(Ordering::Acquire);
        info!("✅ Rank {}: All ranks finished, global end time set", self.rank);
        
        Ok(final_end_time)
    }
    
    /// Get global execution end time
    pub fn get_global_end_time(&self) -> Option<u64> {
        let end_time = self.state.global_end_time.load(Ordering::Acquire);
        if end_time > 0 { Some(end_time) } else { None }
    }
    
    /// Mark execution failed
    pub fn mark_failed(&self, error: &str) {
        warn!("💥 Rank {}: Execution failed: {}", self.rank, error);
        self.state.rank_status[self.rank as usize].store(4, Ordering::Release);
        self.update_heartbeat();
    }
    
    /// Trigger abort for all ranks
    pub fn abort(&self, reason: &str) {
        warn!("🚨 Rank {}: Triggering abort: {}", self.rank, reason);
        self.state.abort.store(true, Ordering::Release);
    }
    
    /// Check if execution was aborted
    pub fn check_abort(&self) -> Result<bool> {
        let aborted = self.state.abort.load(Ordering::Acquire);
        if aborted {
            warn!("🚨 Rank {}: Execution was aborted", self.rank);
        }
        Ok(aborted)
    }
    
    /// Update heartbeat timestamp
    fn update_heartbeat(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.state.rank_heartbeats[self.rank as usize].store(now, Ordering::Release);
    }
    
    /// Get coordination statistics for debugging
    pub fn get_stats(&self) -> CoordinationStats {
        CoordinationStats {
            coordination_id: self.coordination_id.clone(),
            world_size: self.world_size,
            registered_ranks: self.state.registered_ranks.load(Ordering::Acquire),
            ready_ranks: self.state.ready_ranks.load(Ordering::Acquire),
            finished_ranks: self.state.finished_ranks.load(Ordering::Acquire),
            global_start_time: self.get_global_start_time(),
            global_end_time: self.get_global_end_time(),
            active: self.state.active.load(Ordering::Acquire),
            aborted: self.state.abort.load(Ordering::Acquire),
        }
    }
    
    /// Get coordination ID for debugging and cleanup
    pub fn coordination_id(&self) -> &str {
        &self.coordination_id
    }
    
    /// Store rank results in shared memory (eliminates temp files)
    pub fn store_results(&self, 
        files_processed: u64,
        bytes_read: u64, 
        throughput_gib_s: f64,
        wall_clock_time_ms: f64,
        au_fraction: f64,
        start_time_ns: u64,
        end_time_ns: u64
    ) -> Result<()> {
        debug!("📊 Rank {}: Storing results in shared memory", self.rank);
        
        let rank_results = &self.state.rank_results[self.rank as usize];
        
        // Convert throughput from GiB/s to bytes/s
        let throughput_bps = (throughput_gib_s * 1_073_741_824.0) as u64;
        
        // Store results atomically
        rank_results.files_processed.store(files_processed, Ordering::Release);
        rank_results.bytes_read.store(bytes_read, Ordering::Release);
        rank_results.throughput_bps.store(throughput_bps, Ordering::Release);
        rank_results.wall_clock_time_ns.store((wall_clock_time_ms * 1_000_000.0) as u64, Ordering::Release);
        rank_results.au_fraction_scaled.store((au_fraction * 1e15) as u64, Ordering::Release);
        rank_results.start_time_ns.store(start_time_ns, Ordering::Release);
        rank_results.end_time_ns.store(end_time_ns, Ordering::Release);
        
        // Mark results as valid (must be last)
        rank_results.results_valid.store(true, Ordering::Release);
        
        debug!("✅ Rank {}: Results stored in shared memory", self.rank);
        Ok(())
    }
    
    /// Get aggregated results from all ranks (eliminates temp file aggregation)
    pub fn get_aggregated_results(&self) -> Result<AggregatedResults> {
        info!("📊 Collecting aggregated results from shared memory");
        
        let mut total_files = 0u64;
        let mut total_bytes = 0u64;
        let mut total_throughput_bps = 0u64;
        let mut min_start_time = u64::MAX;
        let mut max_end_time = 0u64;
        let mut rank_details = Vec::new();
        
        // Collect results from all ranks
        for rank in 0..self.world_size {
            let rank_results = &self.state.rank_results[rank as usize];
            
            // Check if results are valid
            if !rank_results.results_valid.load(Ordering::Acquire) {
                warn!("⚠️  Rank {} results not available in shared memory", rank);
                continue;
            }
            
            let files_processed = rank_results.files_processed.load(Ordering::Acquire);
            let bytes_read = rank_results.bytes_read.load(Ordering::Acquire);
            let throughput_bps = rank_results.throughput_bps.load(Ordering::Acquire);
            let wall_clock_ns = rank_results.wall_clock_time_ns.load(Ordering::Acquire);
            let au_fraction_scaled = rank_results.au_fraction_scaled.load(Ordering::Acquire);
            let start_time_ns = rank_results.start_time_ns.load(Ordering::Acquire);
            let end_time_ns = rank_results.end_time_ns.load(Ordering::Acquire);
            
            total_files += files_processed;
            total_bytes += bytes_read;
            total_throughput_bps += throughput_bps;
            min_start_time = min_start_time.min(start_time_ns);
            max_end_time = max_end_time.max(end_time_ns);
            
            rank_details.push(RankResultDetail {
                rank,
                files_processed,
                bytes_read,
                throughput_gib_s: throughput_bps as f64 / 1_073_741_824.0,
                wall_clock_time_ms: wall_clock_ns as f64 / 1_000_000.0,
                au_fraction: au_fraction_scaled as f64 / 1e15,
            });
        }
        
        let global_runtime_ns = max_end_time.saturating_sub(min_start_time);
        let global_runtime_s = global_runtime_ns as f64 / 1e9;
        let total_throughput_gib_s = total_throughput_bps as f64 / 1_073_741_824.0;
        
        info!("📈 Aggregated: {} files, {:.2} GiB, {:.2} GiB/s from {} ranks", 
              total_files, 
              total_bytes as f64 / 1_073_741_824.0,
              total_throughput_gib_s,
              rank_details.len());
              
        Ok(AggregatedResults {
            total_ranks: self.world_size,
            total_files_processed: total_files,
            total_bytes_read: total_bytes,
            total_throughput_gib_s,
            global_runtime_seconds: global_runtime_s,
            rank_details,
        })
    }
    
    /// Cleanup coordination resources (should be called by rank 0 after all processing)
    pub fn cleanup(&self) -> Result<()> {
        if self.rank == 0 {
            info!("🧹 Rank 0: Cleaning up coordination group '{}'", self.coordination_id);
            // Mark coordination as inactive
            self.state.active.store(false, Ordering::Release);
        }
        Ok(())
    }
}

/// Coordination statistics for monitoring
#[derive(Debug, Clone)]
pub struct CoordinationStats {
    pub coordination_id: String,
    pub world_size: u32,
    pub registered_ranks: u32,
    pub ready_ranks: u32,
    pub finished_ranks: u32,
    pub global_start_time: Option<u64>,
    pub global_end_time: Option<u64>,
    pub active: bool,
    pub aborted: bool,
}

/// Aggregated results from all ranks (eliminates temp file aggregation)
#[derive(Debug, Clone)]
pub struct AggregatedResults {
    pub total_ranks: u32,
    pub total_files_processed: u64,
    pub total_bytes_read: u64,
    pub total_throughput_gib_s: f64,
    pub global_runtime_seconds: f64,
    pub rank_details: Vec<RankResultDetail>,
}

/// Individual rank result details
#[derive(Debug, Clone)]
pub struct RankResultDetail {
    pub rank: u32,
    pub files_processed: u64,
    pub bytes_read: u64,
    pub throughput_gib_s: f64,
    pub wall_clock_time_ms: f64,
    pub au_fraction: f64,
}

/// Cleanup coordination resources (call from rank 0 after all processing)
pub fn cleanup_coordination(coordination_id: &str) -> Result<()> {
    let _shmem_name = format!("dl_driver_coord_{}", coordination_id);
    
    // Note: shared_memory crate doesn't provide explicit cleanup,
    // but OS will clean up when all processes detach
    info!("🧹 Cleaning up coordination group '{}'", coordination_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_coordination_single_rank() {
        let coord = RankCoordinator::new(0, 1, "test_single").unwrap();
        coord.register_and_wait().await.unwrap();
        coord.mark_global_start().unwrap();
        coord.barrier("test_barrier").await.unwrap();
        coord.mark_finished_and_wait().await.unwrap();
        
        let stats = coord.get_stats();
        assert_eq!(stats.world_size, 1);
        assert_eq!(stats.finished_ranks, 1);
    }
}