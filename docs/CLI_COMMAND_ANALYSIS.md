# dl-driver CLI Command Analysis

**Date:** November 2, 2025  
**Purpose:** Comprehensive analysis of command overlaps, partially implemented features, and distributed execution architecture

---

## Executive Summary

Analysis revealed several areas of command overlap, partially implemented features, and opportunities for simplification:

1. **`validate` vs `--dry-run`**: Significant overlap - consolidation recommended
2. **`--mlperf` flag**: Partially implemented, not functional - needs completion or removal
3. **`generate` vs `workflow.generate_data`**: Both useful, keep both with clear documentation
4. **`aggregate` command**: Legacy from file-based coordination - may be obsolete with shared memory
5. **`distributed` command**: Essential for multi-host scale-out workloads - keep and enhance

---

## 1. Command Overlap Issues

### 1.1 `validate` vs `--dry-run` - SIGNIFICANT OVERLAP ⚠️

**Current State:**

**`validate` command** (`crates/cli/src/main.rs:939`):
- Basic YAML parsing validation
- Tests config conversions (LoaderOptions, PoolConfig, RunPlan)
- Backend detection (file://, s3://, az://, direct://)
- Shows basic dataset info (files, samples, size)
- Optional `--to-json` flag for YAML → JSON conversion
- ~100 lines of simple output
- Exit after validation (no execution)

**`run --dry-run` flag** (`crates/cli/src/main.rs:1456`):
- Everything `validate` does, PLUS:
- **Detailed directory structure analysis** (Flat, DLIO Sharding, Hierarchical modes)
- **Training workload estimation** (batches per epoch, total I/O, compute time, AU calculation)
- **Comprehensive configuration summary** with formatted boxes (┌─┐│└┘)
- Shows reader/loader configuration details
- Shows checkpoint configuration
- ~250+ lines of detailed, user-friendly display code
- Exit after summary (no execution)

**Code Evidence:**
```rust
// validate_dlio_config() - Line 939
println!("✅ YAML parsing: SUCCESS");
println!("✅ Model name: {:?}", ...);
println!("✅ Backend detection: ...");
println!("🎉 DLIO configuration is valid and ready to run!");

// display_config_summary() - Line 1456
println!("┌─ Model Configuration ────────────────────────────────────────────────┐");
println!("│ Model:         {}", ...);
println!("│ Framework:     {}", ...);
// ... 250+ more lines of detailed output
```

**Analysis:**
- **80% overlap** in functionality
- `--dry-run` is strictly superior (superset of validate features)
- Users likely confused about which to use
- Maintenance burden of keeping both in sync

**Recommendation:** 🎯 **CONSOLIDATE**

**Option A - Deprecate `validate` (Preferred):**
```bash
# Keep the intuitive flag
dl-driver run --config config.yaml --dry-run

# Make validate command print deprecation warning and redirect
dl-driver validate --config config.yaml
# Output: "⚠️  WARNING: 'validate' is deprecated. Use 'run --dry-run' for comprehensive validation."
# [then internally calls display_config_summary()]
```

**Option B - Keep Both with Clear Differentiation:**
- `validate`: Quick structural check (CI/CD pipelines, parser testing)
  - Exit code 0 = valid, non-zero = invalid
  - Minimal output unless errors
  - Add `--quiet` flag for just exit code
- `--dry-run`: Full execution preview (users planning workloads)
  - Detailed analysis for capacity planning
  - Estimates throughput, compute time, storage

**Implementation Plan (Option A):**
1. Add deprecation warning to `validate` command
2. Make `validate` internally call `display_config_summary()`
3. Update USER_GUIDE.md to recommend `--dry-run`
4. Add note to CHANGELOG for v0.8.3
5. Plan to remove `validate` in v0.9.0

---

## 2. Partially Implemented Features

### 2.1 `--mlperf` Flag - NOT FUNCTIONAL 🚧

**Current State:**

**CLI Flags** (`crates/cli/src/main.rs:40-58`):
```rust
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
```

**Critical Bug** (`crates/cli/src/main.rs:485-488`):
```rust
// Variable named with _ prefix = UNUSED!
let _metrics = if mlperf_mode {
    dl_driver_core::mlperf::MlperfMetrics::new()
} else {
    dl_driver_core::mlperf::MlperfMetrics::new() // BOTH BRANCHES IDENTICAL!
};
```

**Infrastructure EXISTS** (`crates/core/src/mlperf/mod.rs`):
- `MlperfRunner` struct (~550 lines of code)
- `MlperfMetrics` struct (tracks samples, epochs, steps, timing)
- `MlperfReport` struct (JSON/CSV output)
- Batch tracking, epoch tracking, step tracking
- NOT WIRED UP to main execution path!

**What It's SUPPOSED To Do:**
- MLPerf-style compliance reporting for AI/ML benchmarks
- Standardized metrics format (samples/sec, epochs completed, steps completed)
- Deterministic validation (access order tracking)
- Export to JSON/CSV for analysis

**Current Reality:**
- Flags accepted but ignored
- Metrics object created but never used
- Infrastructure exists but disconnected
- Users get normal output even with `--mlperf`

**Recommendation:** 🎯 **FIX OR REMOVE**

**Option A - Complete Implementation:**
1. Wire up `MlperfRunner` to main execution path
2. Use `_metrics` variable (remove underscore)
3. Conditional output formatting based on `mlperf_mode`
4. Add tests for MLPerf reporting

**Option B - Remove Incomplete Feature:**
1. Remove `--mlperf`, `--format`, `--output` flags from CLI
2. Keep `mlperf/mod.rs` as internal infrastructure
3. Add back in v0.9.0 when properly implemented
4. Add `--experimental-mlperf` flag if partial use desired

**Option C - Mark as Experimental:**
```rust
/// [EXPERIMENTAL] Enable MLPerf compliance mode (not fully implemented)
#[arg(long)]
mlperf: bool,
```
Add warning if used:
```rust
if mlperf_mode {
    warn!("⚠️  --mlperf is experimental and not fully functional in v0.8.2");
}
```

**Recommended: Option B** - Remove until properly implemented

---

## 3. Data Generation Commands

### 3.1 `generate` vs `workflow.generate_data` - BOTH USEFUL ✅

**Current State:**

**Method 1 - Standalone `generate` command** (`crates/cli/src/main.rs:1039`):
```bash
dl-driver generate --config config.yaml [--verbose] [--skip-existing]
```
- Only generates dataset
- No training phase
- Useful for pre-generating reusable test data
- `--skip-existing`: Skip if folder exists (not fully implemented)
- `--verbose`: Show progress during generation

**Method 2 - Integrated `workflow.generate_data`** (in config YAML):
```yaml
workflow:
  generate_data: true  # Phase 1: Generate
  train: true          # Phase 2: Train
```
```bash
dl-driver run --config config.yaml
```
- Part of `run` command
- Phase 1: Data generation (if enabled)
- Phase 2: Training (if enabled)
- One-shot workflow execution

**Use Cases:**

| Scenario | Command | Config |
|----------|---------|--------|
| Pre-generate dataset for reuse | `generate` | Any config |
| Generate + train in one pass | `run` | `generate_data: true, train: true` |
| Train on existing dataset | `run` | `generate_data: false, train: true` |
| Test data generation only | `generate` | - |
| CI/CD dataset preparation | `generate` | - |

**Analysis:**
- **No overlap** - serve different purposes
- `generate` = Explicit, standalone tool
- `workflow.generate_data` = Integrated phase control
- Both used in practice (see test configs)

**Recommendation:** 🎯 **KEEP BOTH - ADD DOCUMENTATION**

**Documentation Improvements:**
1. Add "When to use `generate` vs `workflow.generate_data`" section to USER_GUIDE
2. Update examples to show both patterns
3. Clarify in CLI `--help` text

---

## 4. Multi-Process & Distributed Commands

### 4.1 `aggregate` Command - LEGACY? 🤔

**Current State:**

**CLI Definition** (`crates/cli/src/main.rs:156-169`):
```rust
/// Aggregate results from multiple rank JSON files
Aggregate {
    /// Pattern or paths to rank result files (e.g., "/results/rank*.json")
    #[arg(short, long)]
    inputs: String,

    /// Output aggregated results to file
    #[arg(short, long)]
    output: std::path::PathBuf,

    /// Enable strict AU mode - fail if global AU is below threshold
    #[arg(long)]
    strict_au: bool,

    /// Expected metric AU threshold (default from first rank config)
    #[arg(long)]
    au_threshold: Option<f64>,
}
```

**Purpose:**
- Aggregates results from multiple rank JSON output files
- Used in **file-based coordination** pattern
- Each rank writes JSON results file (e.g., `rank0.json`, `rank1.json`)
- Controller aggregates files post-execution

**Example Usage:**
```bash
# After multi-rank execution with --results flag:
dl-driver run --config cfg.yaml --rank 0 --world-size 4 --results /tmp/rank0.json &
dl-driver run --config cfg.yaml --rank 1 --world-size 4 --results /tmp/rank1.json &
dl-driver run --config cfg.yaml --rank 2 --world-size 4 --results /tmp/rank2.json &
dl-driver run --config cfg.yaml --rank 3 --world-size 4 --results /tmp/rank3.json &

# Then aggregate:
dl-driver aggregate --inputs "/tmp/rank*.json" --output /tmp/aggregated.json
```

**But Wait... Shared Memory Coordination Exists!**

**Plan A1 - Shared Memory Coordination** (`crates/core/src/coordination.rs`):
```rust
pub struct RankCoordinator {
    rank: u32,
    world_size: u32,
    coordination_id: String,
}

impl RankCoordinator {
    pub async fn get_aggregated_results(&self) -> Result<AggregatedResults> {
        // Zero temp files - atomic shared memory operations
        // Rank 0 automatically aggregates results
    }
}
```

**From main.rs execution** (`crates/cli/src/main.rs:563-580`):
```rust
// Only rank 0 displays aggregated results (eliminates temp file aggregation)
if current_rank == 0 {
    match coord.get_aggregated_results() {
        Ok(results) => {
            println!("\n🎉 Plan A1 Multi-GPU Results (Shared Memory Coordination):");
            println!("Total files processed: {}", results.total_files_processed);
            println!("Combined throughput: {:.2} GiB/s", results.total_throughput_gib_s);
            println!("✅ Multi-rank coordination successful - NO TEMP FILES USED");
        }
    }
}
```

**Analysis:**
- `aggregate` = **Legacy pattern** from file-based coordination
- Shared memory coordination = **Modern pattern** (no temp files!)
- File pattern still supported via `--results` flag
- Shared memory path preferred (USER_GUIDE.md line 126: "Zero temp files")

**When is `aggregate` Still Needed?**
1. **Post-hoc analysis** of old result files
2. **Debugging** failed runs (inspect individual rank outputs)
3. **Cross-run aggregation** (multiple test runs)
4. **Distributed agents** (different hosts, no shared memory)

**Recommendation:** 🎯 **KEEP BUT CLARIFY**

**Changes Needed:**
1. Update USER_GUIDE to explain:
   - Shared memory = preferred for single-host multi-rank
   - File-based = fallback for debugging or distributed agents
   - `aggregate` = manual post-processing tool
2. Add note in CLI help text:
   ```rust
   /// Aggregate results from multiple rank JSON files
   /// NOTE: For single-host multi-rank, prefer shared memory coordination (no temp files)
   /// This command is for post-hoc analysis or distributed agent results
   ```

---

### 4.2 `distributed` Command - ESSENTIAL FOR SCALE-OUT ✅

**Current State:**

**CLI Definition** (`crates/cli/src/main.rs:171-221`):
```rust
/// Run distributed DLIO workload across multiple agents
Distributed {
    #[command(subcommand)]
    command: DistributedCommands,
}

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

        /// Path template for agent-specific directories
        #[arg(long, default_value = "{id}/")]
        path_template: String,

        /// Coordinated start delay in milliseconds
        #[arg(long, default_value = "1000")]
        start_delay_ms: u64,

        /// Timeout for agent responses in seconds
        #[arg(long, default_value = "300")]
        timeout_secs: u64,

        /// Enable dry-run: validate configuration without running workload
        #[arg(long)]
        dry_run: bool,
    },
    /// Query agent status and configuration
    Status { ... },
    /// Stop all agents
    Stop { ... },
}
```

**Architecture:**

```
┌──────────────┐
│  Controller  │  (dl-driver distributed run)
│  (any host)  │
└──────┬───────┘
       │ gRPC
       │
       ├─────────────┬─────────────┬─────────────┐
       │             │             │             │
   ┌───▼───┐    ┌───▼───┐    ┌───▼───┐    ┌───▼───┐
   │Agent 0│    │Agent 1│    │Agent 2│    │Agent 3│
   │Host 1 │    │Host 2 │    │Host 3 │    │Host 4 │
   └───┬───┘    └───┬───┘    └───┬───┘    └───┬───┘
       │             │             │             │
   ┌───▼────┐   ┌───▼────┐   ┌───▼────┐   ┌───▼────┐
   │Storage │   │Storage │   │Storage │   │Storage │
   │(local) │   │(local) │   │(local) │   │(shared)│
   └────────┘   └────────┘   └────────┘   └────────┘
```

**Use Cases:**

1. **Multi-Host Scale-Out Testing**
   - True distributed workloads
   - Each host has independent storage
   - Path template: `--path-template "{id}/"` → agent-0/, agent-1/, ...

2. **Shared Storage Testing**
   - Multiple clients hitting same storage
   - Path template: `--path-template ""` → all use same paths
   - Tests storage contention, locking, consistency

3. **Hybrid Scenarios**
   - Some agents on local storage
   - Some agents on shared storage (S3, Azure, GCS)

**Example Workflows:**

**Start Agents:**
```bash
# Host 1
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 --bind-addr 0.0.0.0

# Host 2
./target/release/dl_driver_agent --agent-id agent-1 --port 50051 --bind-addr 0.0.0.0

# Host 3
./target/release/dl_driver_agent --agent-id agent-2 --port 50051 --bind-addr 0.0.0.0

# Host 4
./target/release/dl_driver_agent --agent-id agent-3 --port 50051 --bind-addr 0.0.0.0
```

**Run Controller:**
```bash
# Method 1: Explicit agent list
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/resnet50_4hosts.yaml \
  --agents http://host1:50051,http://host2:50051,http://host3:50051,http://host4:50051 \
  --path-template "{id}/"

# Method 2: Distributed config file
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/resnet50_4hosts.yaml \
  --distributed-config distributed_agents.yaml
```

**distributed_agents.yaml:**
```yaml
agents:
  - id: agent-0
    endpoint: http://host1:50051
  - id: agent-1
    endpoint: http://host2:50051
  - id: agent-2
    endpoint: http://host3:50051
  - id: agent-3
    endpoint: http://host4:50051
```

**Why This Can't Use Shared Memory:**
- Different hosts = no shared memory space
- Requires network coordination (gRPC)
- Each agent runs independently
- Controller aggregates results via gRPC calls

**Analysis:**
- **ESSENTIAL** for scale-out testing
- **CANNOT BE REPLACED** by shared memory coordination
- Well-designed architecture (controller/agent pattern)
- Documented in USER_GUIDE.md (lines 138-170)

**Recommendation:** 🎯 **KEEP AND ENHANCE**

**Possible Enhancements:**
1. Add `distributed init` command to auto-configure agents
2. Add agent health checking / automatic retry
3. Support agent pools (dynamic agent discovery)
4. Better error handling for agent failures
5. Add distributed dry-run validation (currently exists but could be enhanced)

---

## 5. Execution Patterns Summary

### 5.1 Single-Process Execution (Simplest)

```bash
dl-driver run --config config.yaml
```

**Features:**
- Single process, single "GPU" simulation
- Simplest mode
- Good for development, testing, small workloads

---

### 5.2 Multi-Rank Shared Memory (Single Host, Multi-GPU)

```bash
# Launch 4 ranks (simulating 4 GPUs on same host)
./target/release/dl-driver run --config cfg.yaml --world-size 4 --rank 0 &
./target/release/dl-driver run --config cfg.yaml --world-size 4 --rank 1 &
./target/release/dl-driver run --config cfg.yaml --world-size 4 --rank 2 &
./target/release/dl-driver run --config cfg.yaml --world-size 4 --rank 3 &
```

**Features:**
- Zero temp files (atomic shared memory)
- Rank 0 automatically aggregates results
- Synchronized start/stop timing
- Interleaved file sharding (--shard-strategy)
- `aggregate` command NOT NEEDED (automatic)

**Best For:**
- Multi-GPU simulation on single node
- High-performance single-host testing
- Development clusters with multiple GPUs

---

### 5.3 Distributed Multi-Agent (Multi-Host Scale-Out)

```bash
# Step 1: Start agents on each host
host1$ ./target/release/dl_driver_agent --agent-id agent-0 --port 50051
host2$ ./target/release/dl_driver_agent --agent-id agent-1 --port 50051
host3$ ./target/release/dl_driver_agent --agent-id agent-2 --port 50051
host4$ ./target/release/dl_driver_agent --agent-id agent-3 --port 50051

# Step 2: Run controller (from any host)
$ ./target/release/dl-driver distributed run \
    --config config.yaml \
    --agents http://host1:50051,http://host2:50051,http://host3:50051,http://host4:50051
```

**Features:**
- True multi-host execution
- gRPC coordination (no shared memory)
- Controller aggregates results from agents
- Independent storage per host OR shared storage
- `--path-template` for storage isolation

**Best For:**
- Enterprise scale-out testing
- Multi-datacenter simulations
- Realistic distributed AI/ML workloads
- Storage contention testing

---

### 5.4 File-Based Coordination (Legacy/Fallback)

```bash
# Launch with --results flag (each rank writes JSON)
./target/release/dl-driver run --config cfg.yaml --rank 0 --world-size 4 --results /tmp/rank0.json &
./target/release/dl-driver run --config cfg.yaml --rank 1 --world-size 4 --results /tmp/rank1.json &
./target/release/dl-driver run --config cfg.yaml --rank 2 --world-size 4 --results /tmp/rank2.json &
./target/release/dl-driver run --config cfg.yaml --rank 3 --world-size 4 --results /tmp/rank3.json &

# Manual aggregation
./target/release/dl-driver aggregate --inputs "/tmp/rank*.json" --output /tmp/final.json
```

**Features:**
- Each rank writes JSON results file
- Manual aggregation via `aggregate` command
- Useful for debugging (inspect individual rank outputs)
- Legacy pattern (predates shared memory)

**Best For:**
- Debugging failed multi-rank runs
- Post-hoc analysis of old results
- Cross-run aggregation (multiple tests)
- When shared memory coordination fails

---

## 6. Recommendations Summary

### Immediate Actions (v0.8.3)

1. **`validate` command** - Add deprecation warning
   ```rust
   eprintln!("⚠️  WARNING: 'validate' command is deprecated.");
   eprintln!("    Use 'dl-driver run --config CONFIG --dry-run' for comprehensive validation.");
   ```

2. **`--mlperf` flag** - Mark as experimental or remove
   ```rust
   if mlperf {
       warn!("⚠️  --mlperf is experimental and not fully functional in v0.8.2");
       warn!("    Infrastructure exists but is not wired up. See docs/CLI_COMMAND_ANALYSIS.md");
   }
   ```

3. **Documentation updates** - Add to USER_GUIDE.md:
   - "When to use `generate` vs `workflow.generate_data`"
   - "Execution patterns comparison table"
   - "When to use `aggregate` (legacy/debugging)"

### Future Work (v0.9.0)

1. **Remove `validate` command** - Full migration to `--dry-run`

2. **Complete `--mlperf` implementation** - Wire up MlperfRunner properly

3. **Enhance `distributed` command**:
   - Add `distributed init` for auto-configuration
   - Add agent health checking
   - Better error handling for agent failures

4. **Deprecate file-based coordination** - Prefer shared memory (single-host) or distributed agents (multi-host)

---

## 7. Decision Matrix

| Command/Feature | Status | Action | Priority |
|----------------|--------|--------|----------|
| `validate` | Overlaps with `--dry-run` | Deprecate in v0.8.3, remove in v0.9.0 | HIGH |
| `--dry-run` | Working, comprehensive | Keep, enhance documentation | MEDIUM |
| `--mlperf` | Broken, unused | Mark experimental or remove flags | HIGH |
| `generate` | Working, useful | Keep, improve docs | LOW |
| `workflow.generate_data` | Working, useful | Keep, improve docs | LOW |
| `aggregate` | Legacy but useful | Keep, clarify use cases | LOW |
| `distributed` | Essential, working | Keep, enhance features | MEDIUM |
| Shared memory coordination | Modern, preferred | Document as preferred pattern | MEDIUM |

---

## 8. Code References

### Key Files:
- `crates/cli/src/main.rs` - CLI definitions and command handlers
  - Lines 25-221: Command enum definitions
  - Lines 939-1038: `validate_dlio_config()`
  - Lines 1039-1070: `run_generate_only()`
  - Lines 1456-1720: `display_config_summary()` (--dry-run)
  
- `crates/core/src/coordination.rs` - Shared memory coordination
  - `RankCoordinator` struct
  - Zero-temp-file aggregation
  
- `crates/core/src/mlperf/mod.rs` - MLPerf infrastructure (unused)
  - `MlperfRunner` struct
  - `MlperfMetrics` struct
  - `MlperfReport` struct

- `docs/USER_GUIDE.md` - User documentation
  - Lines 70-106: Basic execution patterns
  - Lines 107-137: Multi-rank shared memory
  - Lines 138-170: Distributed multi-agent

### Test Configurations:
- `tests/dlio_configs/resnet50_1host.yaml` - Single host
- `tests/dlio_configs/resnet50_4hosts.yaml` - 4-host distributed
- `tests/dlio_configs/resnet50_8hosts.yaml` - 8-host distributed

---

## 9. Open Questions

1. **MLPerf Implementation**: Complete or remove? If complete, estimated effort?

2. **File-based coordination**: Officially deprecate in favor of shared memory + distributed agents?

3. **Aggregate command**: Keep as debugging tool or integrate into other commands?

4. **Distributed enhancements**: Which features are highest priority?
   - Auto-configuration?
   - Health checking?
   - Agent pools?
   - Better error handling?

5. **CLI simplification**: Any other commands that could be consolidated?

---

## 10. Complete CLI Reference (v0.8.2)

Generated on November 2, 2025 via `--help` flags for future reference.

### Main Command

```
dl-driver – Unified DLIO execution engine with optional MLPerf compliance mode

Usage: dl-driver [OPTIONS] <COMMAND>

Commands:
  run          Run DLIO workload (use --mlperf for enhanced reporting and compliance)
  validate     Validate a DLIO config without running it
  generate     Generate synthetic dataset from DLIO config
  aggregate    Aggregate results from multiple rank JSON files
  distributed  Run distributed DLIO workload across multiple agents
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Increase verbosity (default: warnings only, -v: info, -vv: debug, -vvv: trace)
  -h, --help        Print help
  -V, --version     Print version
```

---

### `dl-driver run` - Main Workload Execution

```
Run DLIO workload (use --mlperf for enhanced reporting and compliance)

Usage: dl-driver run [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>
          Path to a DLIO YAML config file

  --pretty
          If set, dump the parsed YAML back to stdout

  --dry-run
          Validate config and show execution summary without running (dry-run mode)

  --mlperf
          Enable MLPerf compliance mode with enhanced reporting

  --format <FORMAT>
          Output format for MLPerf reports (json, csv) [default: json]

  -o, --output <OUTPUT>
          Save MLPerf report to file instead of stdout

  --max-epochs <MAX_EPOCHS>
          Maximum number of epochs to run (MLPerf mode) [default: 3]

  --max-steps <MAX_STEPS>
          Maximum number of steps to run (MLPerf mode) [default: 1000]

  --pool-size <POOL_SIZE>
          Override pool size for AsyncPoolDataLoader [default: 16]

  --readahead <READAHEAD>
          Override readahead batches [default: 8]

  --max-inflight <MAX_INFLIGHT>
          Override max inflight requests [default: 64]

  --timeout <TIMEOUT>
          Batch timeout in seconds [default: 10]

  --accelerators <ACCELERATORS>
          Number of accelerators for AU calculation (default: 1) [default: 1]

  --strict-au
          Enable strict AU mode - fail if AU is below threshold

  --gpus <GPUS>
          Number of GPUs to simulate for multi-GPU scaling (default: auto-detect or 1)

  --use-real-gpus
          [FUTURE] GPU environment mode - detects GPUs but uses same CPU simulation (for future GPU integration)

  --filelist <FILELIST>
          Read file list from specified file (one path per line)

  --rank <RANK>
          Rank ID for multi-process execution (0-based)

  --world-size <WORLD_SIZE>
          Total number of ranks in world

  --start-at-epoch <START_AT_EPOCH>
          Unix timestamp to start execution (for synchronized multi-rank)

  --shard-strategy <SHARD_STRATEGY>
          Sharding strategy: interleaved, contiguous, or hash [default: interleaved]

  --results <RESULTS>
          Output JSON results to specified file

  --profile <PROFILE>
          Use realistic framework-specific workload profile (torch-like, tf-like, jax-like)

  --metrics-json <METRICS_JSON>
          Export metrics summary to JSON file

  --metrics-csv <METRICS_CSV>
          Export metrics summary to CSV file

  -h, --help
          Print help
```

**Key Options by Category:**

**Validation & Debug:**
- `--dry-run`: Show config summary without execution (RECOMMENDED for validation)
- `--pretty`: Dump parsed config to stdout

**MLPerf Compliance (EXPERIMENTAL - see Section 2.1):**
- `--mlperf`: Enable MLPerf mode (currently not fully functional)
- `--format json|csv`: Output format
- `--output <file>`: Save report to file
- `--max-epochs`, `--max-steps`: Execution limits

**Performance Tuning:**
- `--pool-size`, `--readahead`, `--max-inflight`: AsyncPoolDataLoader config
- `--timeout`: Batch timeout

**Multi-Rank Coordination (Shared Memory - Single Host):**
- `--rank <N>`: Rank ID (0-based)
- `--world-size <N>`: Total ranks
- `--start-at-epoch <UNIX_TIMESTAMP>`: Synchronized start
- `--shard-strategy interleaved|contiguous|hash`: File sharding
- `--filelist <file>`: Explicit file list for sharding

**GPU Simulation:**
- `--accelerators <N>`: Number of accelerators for AU calculation
- `--gpus <N>`: Number of GPUs to simulate
- `--strict-au`: Fail if AU below threshold
- `--use-real-gpus`: [FUTURE] Real GPU detection

**Framework Profiles:**
- `--profile torch|tf|jax`: Realistic workload profiles

**Metrics Export:**
- `--results <file>`: Output JSON results (for file-based coordination)
- `--metrics-json <file>`: Export metrics to JSON
- `--metrics-csv <file>`: Export metrics to CSV

---

### `dl-driver validate` - Configuration Validation

```
Validate a DLIO config without running it

Usage: dl-driver validate [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>  Path to a DLIO YAML config file
      --to-json          Convert YAML to JSON and print it
  -h, --help             Print help
```

**NOTE:** See Section 1.1 - Significant overlap with `--dry-run`. Deprecation planned for v0.8.3.

**Features:**
- Basic YAML parsing validation
- Config conversion testing (LoaderOptions, PoolConfig, RunPlan)
- Backend detection
- Optional YAML → JSON conversion
- Simple pass/fail output

**Recommended Alternative:**
```bash
# Use this instead (more comprehensive):
dl-driver run --config config.yaml --dry-run
```

---

### `dl-driver generate` - Dataset Generation

```
Generate synthetic dataset from DLIO config

Usage: dl-driver generate [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>  Path to a DLIO YAML config file
      --verbose          Show progress during generation
      --skip-existing    Skip generation if data folder already exists
  -h, --help             Print help
```

**Use Cases:**
- Pre-generate reusable test datasets
- Separate data generation from training workflow
- CI/CD dataset preparation
- Storage capacity testing

**Difference from `workflow.generate_data`:**
- `generate` command: Standalone, only generates data
- `workflow.generate_data: true`: Integrated with `run` command (Phase 1 + Phase 2)

See Section 3.1 for detailed comparison.

---

### `dl-driver aggregate` - Multi-Rank Result Aggregation

```
Aggregate results from multiple rank JSON files

Usage: dl-driver aggregate [OPTIONS] --inputs <INPUTS> --output <OUTPUT>

Options:
  -i, --inputs <INPUTS>              Pattern or paths to rank result files (e.g., "/results/rank*.json")
  -o, --output <OUTPUT>              Output aggregated results to file
      --strict-au                    Enable strict AU mode - fail if global AU is below threshold
      --au-threshold <AU_THRESHOLD>  Expected metric AU threshold (default from first rank config)
  -h, --help                         Print help
```

**Use Cases:**
- **Legacy**: File-based coordination (predates shared memory)
- **Debugging**: Inspect individual rank outputs after failed runs
- **Post-hoc analysis**: Aggregate old result files
- **Cross-run aggregation**: Combine results from multiple test runs

**Modern Alternative (Preferred):**
Shared memory coordination handles aggregation automatically:
```bash
# Launch ranks with shared memory (NO --results flag)
dl-driver run --config cfg.yaml --rank 0 --world-size 4 &
dl-driver run --config cfg.yaml --rank 1 --world-size 4 &
dl-driver run --config cfg.yaml --rank 2 --world-size 4 &
dl-driver run --config cfg.yaml --rank 3 --world-size 4 &

# Rank 0 automatically aggregates - NO temp files, NO aggregate command needed!
```

**File-based Pattern (when aggregate is needed):**
```bash
# Each rank writes JSON file
dl-driver run --config cfg.yaml --rank 0 --world-size 4 --results /tmp/rank0.json &
dl-driver run --config cfg.yaml --rank 1 --world-size 4 --results /tmp/rank1.json &
dl-driver run --config cfg.yaml --rank 2 --world-size 4 --results /tmp/rank2.json &
dl-driver run --config cfg.yaml --rank 3 --world-size 4 --results /tmp/rank3.json &

# Manual aggregation
dl-driver aggregate --inputs "/tmp/rank*.json" --output /tmp/final.json
```

See Section 4.1 for detailed analysis.

---

### `dl-driver distributed` - Multi-Host Scale-Out

```
Run distributed DLIO workload across multiple agents

Usage: dl-driver distributed <COMMAND>

Commands:
  run   Run workload across multiple agents
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### `dl-driver distributed run`

```
Run workload across multiple agents

Usage: dl-driver distributed run [OPTIONS] --config <CONFIG>

Options:
      --config <CONFIG>
          Path to DLIO YAML config file

      --distributed-config <DISTRIBUTED_CONFIG>
          Distributed config file (YAML with agents list)

      --agents <AGENTS>
          Agent endpoints (alternative to distributed_config)

      --path-template <PATH_TEMPLATE>
          Path template for agent-specific directories (e.g., "{id}/", "agent-{id}/")
          [default: {id}/]

      --start-delay-ms <START_DELAY_MS>
          Coordinated start delay in milliseconds
          [default: 1000]

      --request-timeout-ms <REQUEST_TIMEOUT_MS>
          Request timeout in milliseconds
          [default: 300000]

      --max-retries <MAX_RETRIES>
          Maximum retries per agent
          [default: 3]

      --dry-run
          Dry-run: validate configuration without running workload

      --storage-tsv <STORAGE_TSV>
          Output storage metrics TSV file

      --aiml-tsv <AIML_TSV>
          Output AI/ML metrics TSV file

  -h, --help
          Print help
```

**Architecture:**
- **Controller**: Orchestrates distributed workload (runs on any host)
- **Agents**: Execute workload on each host (dl_driver_agent process)
- **Coordination**: gRPC protocol for controller ↔ agent communication

**Use Cases:**
- Enterprise-scale multi-host testing
- Storage contention testing (many clients → one backend)
- Realistic distributed AI/ML training I/O simulation
- Multi-datacenter workload scenarios

**Example Workflow:**

**Step 1 - Start agents on each host:**
```bash
# Host 1
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 --bind-addr 0.0.0.0

# Host 2
./target/release/dl_driver_agent --agent-id agent-1 --port 50051 --bind-addr 0.0.0.0

# Host 3
./target/release/dl_driver_agent --agent-id agent-2 --port 50051 --bind-addr 0.0.0.0

# Host 4
./target/release/dl_driver_agent --agent-id agent-3 --port 50051 --bind-addr 0.0.0.0
```

**Step 2 - Run controller (from any host):**

**Method A - Explicit agent list:**
```bash
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/resnet50_4hosts.yaml \
  --agents http://host1:50051,http://host2:50051,http://host3:50051,http://host4:50051 \
  --path-template "{id}/"
```

**Method B - Distributed config file:**
```bash
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/resnet50_4hosts.yaml \
  --distributed-config agents.yaml
```

**agents.yaml:**
```yaml
agents:
  - id: agent-0
    endpoint: http://host1:50051
  - id: agent-1
    endpoint: http://host2:50051
  - id: agent-2
    endpoint: http://host3:50051
  - id: agent-3
    endpoint: http://host4:50051
```

**Path Template Options:**
- `{id}/`: Agent-specific subdirectories (local storage isolation)
  - agent-0 → `data/0/`, agent-1 → `data/1/`, etc.
- `agent-{id}/`: Alternative naming
  - agent-0 → `data/agent-0/`, agent-1 → `data/agent-1/`, etc.
- `""` (empty): Shared paths (all agents use same storage)
  - Tests contention, locking, consistency

**Why Can't This Use Shared Memory?**
- Different physical hosts = no shared memory address space
- Requires network-based coordination (gRPC)
- Each agent is independent process on different machine
- Controller aggregates via gRPC calls, not shared memory

See Section 4.2 for detailed analysis.

---

## 11. Execution Pattern Decision Tree

```
┌─────────────────────────────────────────┐
│  What do you want to test?             │
└─────────────────┬───────────────────────┘
                  │
    ┌─────────────┴────────────────┐
    │                              │
    ▼                              ▼
Single Host                    Multi-Host
    │                              │
    │                              ▼
    │                         dl-driver distributed run
    │                         (Section 5.3)
    │
    ├─────────────┬──────────────┐
    ▼             ▼              ▼
  1 GPU     Multiple GPUs    Debugging
    │             │              │
    ▼             ▼              ▼
dl-driver      Multi-rank    File-based
   run         Shared Mem    + aggregate
(Section 5.1)  (Section 5.2) (Section 5.4)
```

**Quick Reference:**
- **Single process, 1 GPU**: `dl-driver run --config cfg.yaml`
- **Single host, multi-GPU**: `dl-driver run --config cfg.yaml --rank X --world-size N` (no temp files!)
- **Multi-host scale-out**: `dl-driver distributed run --config cfg.yaml --agents ...`
- **Debugging multi-rank**: Use `--results rank.json` + `aggregate` command

---

## 12. Files Modified/Created for v0.8.3 Planning

**This document:**
- `docs/CLI_COMMAND_ANALYSIS.md` - Complete CLI analysis and recommendations

**No code changes yet** - this is analysis for v0.8.3 planning.

**Recommended changes for v0.8.3:**
1. Add deprecation warning to `validate` command (see Section 1.1)
2. Mark `--mlperf` flags as experimental or remove (see Section 2.1)
3. Update USER_GUIDE.md with execution pattern comparison (see Section 5)
4. Add "When to use each command" quick reference

---

**End of Analysis**
