# Shared Replay Architecture Proposal

**Date**: October 3, 2025  
**Objective**: Eliminate code duplication for sophisticated replay functionality across dl-driver, io-bench, and future tools  
**Current Situation**: Three levels of replay implementations with overlapping features

---

## Current State Analysis

### Three Projects, Three Replay Implementations

#### 1. s3dlio-oplog (Basic Foundation)
**Location**: `s3dlio/crates/s3dlio-oplog/`  
**Features**:
- ✅ Format-tolerant parsing (JSONL/TSV + zstd)
- ✅ Timeline-based replay with microsecond precision
- ✅ Pluggable OpExecutor trait
- ✅ Simple URI translation (1:1 remapping)
- ✅ Operation filtering
- ❌ No statistics return
- ❌ No concurrency control
- ❌ No timeout support
- ❌ No progress reporting
- ❌ No complex path remapping

#### 2. dl-driver Replay
**Location**: `dl-driver/crates/core/src/replay.rs`  
**Features** (377 lines):
- ✅ Statistics tracking (ops/sec, MB/s, timing)
- ✅ Concurrency control (worker count)
- ✅ Timeout configuration
- ✅ Complex path remapping (JSON-based, multiple mappings)
- ✅ Endpoint remapping
- ✅ Progress reporting
- ✅ Metrics export to JSON
- ✅ Fast mode (skip delays)

#### 3. io-bench Replay
**Location**: `s3-bench/src/replay.rs`  
**Features** (similar to dl-driver):
- ✅ Timing-faithful replay
- ✅ Backend retargeting
- ✅ Speed control (multipliers)
- ✅ Continue-on-error mode
- ✅ zstd compression support
- ✅ Operation type parsing
- ❌ Statistics tracking (not visible in initial review)
- ❌ Complex path remapping

### Code Duplication Problem

**Parsing Logic**: Duplicated 3 times (s3dlio-oplog, dl-driver, io-bench)  
**Replay Engine**: Duplicated 3 times with slight variations  
**Type Definitions**: OpType, OpLogEntry defined multiple times  
**Maintenance Burden**: Bug fixes and features need to be applied 3 times

---

## Recommended Architecture: Three-Layer Approach

### Layer 1: s3dlio-oplog (Core Parsing & Basic Replay)
**Keep as-is**: Minimal, focused, stable  
**Responsibility**: 
- Format parsing (JSONL/TSV/zstd)
- Basic type definitions (OpType, OpLogEntry)
- OpExecutor trait
- Simple replay with timing

### Layer 2: s3dlio-replay-pro (NEW - Advanced Features)
**Location**: `s3dlio/crates/s3dlio-replay-pro/` (new workspace member)  
**Dependencies**: Builds on s3dlio-oplog  
**Responsibility**:
- **Statistics Collection**: Comprehensive metrics tracking
- **Concurrency Management**: Worker pools, parallel execution control
- **Timeout Handling**: Per-operation and global timeouts
- **Path Remapping**: Complex multi-mapping with patterns
- **Progress Reporting**: Callback-based progress hooks
- **Error Recovery**: Retry logic, fallback strategies
- **Performance Optimization**: Batching, caching, prefetching

### Layer 3: Application-Specific (dl-driver, io-bench)
**Location**: Each project's codebase  
**Dependencies**: Uses s3dlio-replay-pro  
**Responsibility**:
- CLI argument parsing
- Application-specific configuration
- Custom metrics formatting
- Domain-specific features (ML workload vs benchmarking)

---

## Proposed s3dlio-replay-pro Design

### Public API

```rust
//! s3dlio-replay-pro: Advanced operation log replay with statistics and control
//!
//! Builds on s3dlio-oplog to provide enterprise-grade replay capabilities
//! including statistics tracking, concurrency control, and progress reporting.

use s3dlio_oplog::{OpLogEntry, OpType, OpExecutor};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

/// Advanced replay configuration with statistics and control
#[derive(Debug, Clone)]
pub struct ProReplayConfig {
    /// Path to operation log file
    pub op_log_path: PathBuf,
    
    /// Optional target URI for 1:1 backend retargeting
    pub target_uri: Option<String>,
    
    /// Speed multiplier (1.0 = original speed)
    pub speed: f64,
    
    /// Continue on errors vs stop on first error
    pub continue_on_error: bool,
    
    /// Filter to specific operation types
    pub filter_ops: Option<Vec<OpType>>,
    
    // === Advanced Features ===
    
    /// Number of concurrent workers (None = automatic)
    pub concurrency: Option<usize>,
    
    /// Per-operation timeout in seconds (None = no timeout)
    pub timeout_seconds: Option<u64>,
    
    /// Path remapping rules (prefix replacements)
    pub path_remaps: Vec<PathRemap>,
    
    /// Enable statistics collection
    pub collect_stats: bool,
    
    /// Progress callback (called periodically with current stats)
    pub progress_callback: Option<ProgressCallback>,
    
    /// Custom executor (None = use default s3dlio executor)
    pub custom_executor: Option<Arc<dyn OpExecutor>>,
}

/// Path remapping rule for cross-environment replay
#[derive(Debug, Clone)]
pub struct PathRemap {
    pub from_prefix: String,
    pub to_prefix: String,
    pub match_mode: MatchMode,
}

#[derive(Debug, Clone)]
pub enum MatchMode {
    /// Exact prefix match
    Prefix,
    /// Regex pattern match
    Regex,
    /// Glob pattern match
    Glob,
}

/// Comprehensive replay statistics
#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    // Basic counts
    pub total_operations: usize,
    pub completed_operations: usize,
    pub failed_operations: usize,
    pub skipped_operations: usize,
    
    // Per-operation type breakdown
    pub get_count: usize,
    pub put_count: usize,
    pub delete_count: usize,
    pub list_count: usize,
    pub stat_count: usize,
    
    // Byte counts
    pub total_bytes: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    
    // Timing
    pub wall_time_seconds: f64,
    pub total_operation_time_ns: u64,
    
    // Derived metrics
    pub operations_per_second: f64,
    pub throughput_mbps: f64,
    pub avg_latency_ms: f64,
    
    // Concurrency
    pub max_concurrent_operations: usize,
    pub avg_concurrent_operations: f64,
    
    // Timing accuracy (for replay validation)
    pub timing_drift_ms: f64,
    pub max_scheduling_error_us: i64,
}

/// Progress callback type
pub type ProgressCallback = Arc<dyn Fn(&ReplayStats) + Send + Sync>;

/// Main replay engine with statistics
pub struct ProReplayEngine {
    config: ProReplayConfig,
    stats: ReplayStats,
}

impl ProReplayEngine {
    /// Create new replay engine
    pub fn new(config: ProReplayConfig) -> Self {
        Self {
            config,
            stats: ReplayStats::default(),
        }
    }
    
    /// Run replay and return comprehensive statistics
    pub async fn run(&mut self) -> Result<ReplayStats> {
        // Implementation details...
        unimplemented!()
    }
    
    /// Get current statistics (for progress monitoring)
    pub fn stats(&self) -> &ReplayStats {
        &self.stats
    }
}

/// Convenience function for simple replay with stats
pub async fn replay_with_stats(config: ProReplayConfig) -> Result<ReplayStats> {
    let mut engine = ProReplayEngine::new(config);
    engine.run().await
}

/// Helper: Create default config from path and target
pub fn default_config(op_log_path: PathBuf, target_uri: Option<String>) -> ProReplayConfig {
    ProReplayConfig {
        op_log_path,
        target_uri,
        speed: 1.0,
        continue_on_error: true,
        filter_ops: None,
        concurrency: Some(16),
        timeout_seconds: Some(30),
        path_remaps: vec![],
        collect_stats: true,
        progress_callback: None,
        custom_executor: None,
    }
}
```

### Key Features

#### 1. Statistics Collection
```rust
// Automatic statistics tracking during replay
let mut config = ProReplayConfig::default();
config.collect_stats = true;

let stats = replay_with_stats(config).await?;
println!("Throughput: {:.2} MB/s", stats.throughput_mbps);
println!("Operations: {}/{}", stats.completed_operations, stats.total_operations);
```

#### 2. Concurrency Control
```rust
// Control worker pool size
config.concurrency = Some(32);  // 32 concurrent operations
// OR
config.concurrency = None;      // Automatic based on system
```

#### 3. Complex Path Remapping
```rust
// Multiple remapping rules
config.path_remaps = vec![
    PathRemap {
        from_prefix: "/mnt/old-storage/".to_string(),
        to_prefix: "s3://new-bucket/".to_string(),
        match_mode: MatchMode::Prefix,
    },
    PathRemap {
        from_prefix: "http://10.0.0.1:8000/".to_string(),
        to_prefix: "az://account/container/".to_string(),
        match_mode: MatchMode::Prefix,
    },
];
```

#### 4. Progress Callbacks
```rust
// Custom progress reporting
config.progress_callback = Some(Arc::new(|stats| {
    println!("Progress: {}/{} ops ({:.1}%)", 
        stats.completed_operations,
        stats.total_operations,
        (stats.completed_operations as f64 / stats.total_operations as f64) * 100.0
    );
}));
```

#### 5. Timeout Support
```rust
// Per-operation timeout
config.timeout_seconds = Some(30);  // 30 second timeout
```

---

## Migration Path

### Phase 1: Create s3dlio-replay-pro Crate
**Timeline**: 1-2 days  
**Location**: `s3dlio/crates/s3dlio-replay-pro/`

1. Create new workspace member in s3dlio
2. Add dependency on s3dlio-oplog
3. Implement ProReplayConfig and ReplayStats types
4. Implement ProReplayEngine with statistics tracking
5. Add path remapping logic
6. Add concurrency control
7. Add timeout support
8. Add progress callbacks
9. Write comprehensive tests
10. Document with examples

### Phase 2: Migrate dl-driver
**Timeline**: 2-3 hours  
**Dependencies**: Phase 1 complete

1. Add s3dlio-replay-pro dependency
2. Replace `SimpleReplayEngine` with `ProReplayEngine`
3. Map dl-driver config to ProReplayConfig
4. Update CLI to use new statistics
5. Remove redundant code from replay.rs
6. Update tests
7. Verify all features work

### Phase 3: Migrate io-bench
**Timeline**: 2-3 hours  
**Dependencies**: Phase 1 complete

1. Add s3dlio-replay-pro dependency
2. Replace current replay implementation
3. Map io-bench config to ProReplayConfig
4. Update CLI commands
5. Remove redundant replay.rs
6. Update tests
7. Verify all features work

### Phase 4: Deprecate Old Implementations
**Timeline**: 1 hour  

1. Mark dl-driver's replay.rs as deprecated (keep for one version)
2. Mark io-bench's replay.rs as deprecated
3. Update documentation to point to s3dlio-replay-pro
4. Plan removal in next major version

---

## Benefits Analysis

### Code Reduction
- **dl-driver**: Remove ~377 lines from replay.rs, ~200 from oplog_ingest.rs
- **io-bench**: Remove ~250 lines from replay.rs
- **Total**: ~800+ lines of duplicated code eliminated
- **Maintenance**: Single codebase for bug fixes and features

### Feature Parity
- Both tools get same advanced features automatically
- New features added once, benefit all tools
- Consistent behavior across ecosystem

### Testing
- Comprehensive test suite in one place
- Better coverage with focused testing
- Edge cases handled centrally

### Future Tools
- Any new tool in ecosystem gets sophisticated replay "for free"
- Consistent replay semantics across all tools
- Lower barrier to entry for new projects

---

## Alternative: Enhance s3dlio-oplog Directly

### Pros
- One less crate to maintain
- All functionality in core replay crate
- Simpler dependency tree

### Cons
- Bloats s3dlio-oplog with advanced features
- May not fit minimal design philosophy
- Harder to keep focused and stable
- Forces all users to take dependencies even if not needed

### Verdict
**Not recommended**: Better to have focused layers (basic vs advanced)

---

## Recommended Decision Matrix

| Criterion | s3dlio-replay-pro (Recommended) | Enhance s3dlio-oplog | Keep Separate |
|-----------|-------------------------------|---------------------|---------------|
| Code Duplication | ✅ Eliminated | ✅ Eliminated | ❌ Continues |
| Maintenance Burden | ✅ Low (single crate) | ✅ Low (single crate) | ❌ High (3x) |
| Feature Parity | ✅ Automatic | ✅ Automatic | ❌ Manual |
| Design Clarity | ✅ Layered architecture | ⚠️ Bloated core | ✅ Clear separation |
| Migration Effort | ⚠️ Medium (2-3 days) | ⚠️ Medium (2-3 days) | ✅ None |
| Future Flexibility | ✅ Easy to extend | ⚠️ Constrained | ✅ Full control |
| Dependency Tree | ✅ Clear hierarchy | ✅ Minimal | ⚠️ Tangled |
| User Choice | ✅ Basic or Pro | ❌ All or nothing | ✅ Pick what you need |

**Score**: s3dlio-replay-pro wins (7/8 criteria)

---

## Implementation Checklist

### s3dlio Repository (s3dlio-replay-pro crate)
- [ ] Create `crates/s3dlio-replay-pro/` directory
- [ ] Add to workspace in s3dlio's root Cargo.toml
- [ ] Create Cargo.toml with dependencies
- [ ] Implement ProReplayConfig struct
- [ ] Implement ReplayStats struct
- [ ] Implement ProReplayEngine
- [ ] Add concurrency control (tokio::semaphore or similar)
- [ ] Add timeout support (tokio::time::timeout)
- [ ] Add path remapping logic
- [ ] Add progress callbacks
- [ ] Add statistics collection
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Add examples/
- [ ] Document in README.md
- [ ] Add to s3dlio docs/

### dl-driver Repository
- [ ] Add s3dlio-replay-pro dependency to Cargo.toml
- [ ] Update replay.rs to use ProReplayEngine
- [ ] Create config mapping layer
- [ ] Update CLI in main.rs
- [ ] Update tests
- [ ] Update documentation
- [ ] Mark old code as deprecated
- [ ] Verify all features work

### io-bench Repository
- [ ] Add s3dlio-replay-pro dependency to Cargo.toml
- [ ] Replace replay.rs with ProReplayEngine usage
- [ ] Create config mapping layer
- [ ] Update CLI commands
- [ ] Update tests
- [ ] Update documentation
- [ ] Remove old replay.rs
- [ ] Verify all features work

---

## Success Criteria

1. ✅ Single source of truth for advanced replay features
2. ✅ Both dl-driver and io-bench use s3dlio-replay-pro
3. ✅ All existing features preserved in both tools
4. ✅ Statistics, concurrency, timeouts, path remapping all work
5. ✅ Comprehensive test coverage in s3dlio-replay-pro
6. ✅ Documentation and examples available
7. ✅ ~800 lines of duplicate code eliminated
8. ✅ Future tools can use replay functionality easily

---

## Timeline Estimate

- **Phase 1** (s3dlio-replay-pro creation): 1-2 days
- **Phase 2** (dl-driver migration): 2-3 hours
- **Phase 3** (io-bench migration): 2-3 hours
- **Phase 4** (cleanup & documentation): 1 hour
- **Total**: ~2-3 days of focused work

---

## Recommendation

**Create `s3dlio-replay-pro` as a new workspace member in the s3dlio repository.**

This provides:
- ✅ Clear architectural separation (basic vs advanced)
- ✅ Shared codebase for dl-driver and io-bench
- ✅ Future-proof for additional tools
- ✅ Maintains s3dlio-oplog focus and stability
- ✅ Eliminates ~800 lines of duplicate code
- ✅ Single location for advanced features

**Next Action**: Get approval to create s3dlio-replay-pro, then proceed with Phase 1.

---

**Document Version**: 1.0  
**Date**: October 3, 2025  
**Status**: Proposal - Awaiting approval to implement
