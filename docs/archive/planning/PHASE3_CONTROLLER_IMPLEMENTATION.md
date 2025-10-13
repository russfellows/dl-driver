# Phase 3: Controller Implementation

**Branch:** `v0.8.0-phase3-controller`  
**Version:** v0.8.0  
**Date:** October 12, 2025

## Prerequisites ✅

Phase 1 and Phase 2 complete and merged:
- ✅ gRPC infrastructure (proto, build system)
- ✅ Distributed module with path utilities
- ✅ Agent service (RunWorkload, HealthCheck RPCs)
- ✅ Agent binary (`dl_driver_agent`)
- ✅ Dual metrics system (storage + AI/ML)
- ✅ All tests passing (57 tests)

## Phase 3 Goals

Implement the controller that orchestrates multiple agents:
1. Read distributed config with agent endpoints
2. Connect to all agents via gRPC
3. Distribute DLIO configs with path prefixes
4. Coordinate synchronized start times
5. Collect WorkloadSummary from all agents
6. Aggregate results across agents
7. Output both storage and AI/ML TSV files
8. Implement --dry-run for validation

## Tasks

### 3.1 Implement Controller Logic ⏳

Create `crates/core/src/dist/controller.rs`:

**Key Components:**
```rust
pub struct Controller {
    config: DlioConfig,
    agents: Vec<String>,              // ["host1:50051", "host2:50051"]
    path_template: String,            // "agent-{id}/" or "{id}/"
    start_delay_ms: u64,
    request_timeout_ms: u64,
    max_retries: u32,
}

impl Controller {
    pub fn new(config: DlioConfig, distributed_config: DistributedConfig) -> Self;
    
    pub async fn health_check_all(&self) -> Result<Vec<bool>>;
    
    pub async fn run_distributed(&self) -> Result<AggregateResults>;
    
    async fn connect_agent(&self, endpoint: &str) -> Result<DistAgentClient<Channel>>;
    
    async fn send_workload(&self, agent: DistAgentClient, agent_id: &str) 
        -> Result<WorkloadSummary>;
}
```

**Workflow:**
1. Parse distributed config
2. Health check all agents (fail fast if any unavailable)
3. Calculate coordinated start time (now + start_delay_ms)
4. For each agent:
   - Clone DLIO config
   - Apply agent-specific path prefix (if local storage)
   - Serialize to YAML
   - Send via `RunWorkload` RPC with start_unix_ms
5. Collect all `WorkloadSummary` responses (parallel with timeout)
6. Convert to `WorkloadResult` and aggregate
7. Return `AggregateResults` with both storage and AI/ML metrics

### 3.2 Add CLI Controller Command ⏳

Extend `crates/cli/src/main.rs`:

**New Subcommand:**
```rust
Commands::Distributed {
    #[command(subcommand)]
    command: DistributedCommands,
}

enum DistributedCommands {
    Run {
        /// DLIO config file
        #[arg(long)]
        config: PathBuf,
        
        /// Distributed config file or inline agents
        #[arg(long)]
        distributed_config: Option<PathBuf>,
        
        /// Agent endpoints (alternative to distributed_config)
        #[arg(long, value_delimiter = ',')]
        agents: Option<Vec<String>>,
        
        /// Path template for agent-specific directories
        #[arg(long, default_value = "{id}/")]
        path_template: String,
        
        /// Coordinated start delay in milliseconds
        #[arg(long, default_value = "1000")]
        start_delay_ms: u64,
        
        /// Request timeout in milliseconds
        #[arg(long, default_value = "300000")]
        request_timeout_ms: u64,
        
        /// Maximum retries per agent
        #[arg(long, default_value = "3")]
        max_retries: u32,
        
        /// Dry-run: validate without running
        #[arg(long)]
        dry_run: bool,
        
        /// Output storage TSV file
        #[arg(long)]
        storage_tsv: Option<PathBuf>,
        
        /// Output AI/ML TSV file
        #[arg(long)]
        aiml_tsv: Option<PathBuf>,
    }
}
```

**Usage Examples:**
```bash
# Run with explicit agent list
dl-driver distributed run \
  --config workload.yaml \
  --agents host1:50051,host2:50051,host3:50051 \
  --path-template "{id}/" \
  --start-delay-ms 2000 \
  --storage-tsv results_storage.tsv \
  --aiml-tsv results_aiml.tsv

# Run with distributed config file
dl-driver distributed run \
  --config workload.yaml \
  --distributed-config distributed.yaml \
  --storage-tsv results_storage.tsv \
  --aiml-tsv results_aiml.tsv

# Dry-run validation
dl-driver distributed run \
  --config workload.yaml \
  --agents host1:50051,host2:50051 \
  --dry-run
```

### 3.3 Implement Distributed Config File ⏳

Create `crates/core/src/config/distributed.rs`:

**Schema:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    /// List of agent endpoints
    pub agents: Vec<String>,
    
    /// Path template for agent-specific directories
    /// Variables: {id} (0-based index), {hostname}
    #[serde(default = "default_path_template")]
    pub path_template: String,
    
    /// Coordinated start delay in milliseconds
    #[serde(default = "default_start_delay_ms")]
    pub start_delay_ms: u64,
    
    /// Request timeout in milliseconds
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    
    /// Maximum retries per agent
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

impl DistributedConfig {
    pub fn from_yaml_file(path: &Path) -> Result<Self>;
    pub fn merge_with_cli_args(&mut self, args: &DistributedRunArgs);
}
```

**Example YAML:**
```yaml
# distributed.yaml
agents:
  - "gpu-node-01:50051"
  - "gpu-node-02:50051"
  - "gpu-node-03:50051"
  - "gpu-node-04:50051"

path_template: "agent-{id}/"
start_delay_ms: 2000
request_timeout_ms: 300000
max_retries: 3
```

### 3.4 Implement --dry-run Validation ⏳

**Dry-run Output:**
```
🔍 Distributed Execution Plan (DRY-RUN)
═══════════════════════════════════════

📋 Configuration:
   • DLIO Config: workload.yaml
   • Storage Backend: S3 (s3://my-bucket/data)
   • Backend Type: Shared Storage (no path prefixes needed)
   • Agents: 4
   • Path Template: {id}/
   • Start Delay: 2000ms
   • Request Timeout: 300000ms

🌐 Agents:
   ✅ gpu-node-01:50051 - Healthy (ping: 2.3ms)
   ✅ gpu-node-02:50051 - Healthy (ping: 2.1ms)
   ✅ gpu-node-03:50051 - Healthy (ping: 2.5ms)
   ✅ gpu-node-04:50051 - Healthy (ping: 2.4ms)

📊 Workload Estimate:
   • Dataset: 10,000 files @ 100 samples/file = 1,000,000 samples
   • Batch Size: 64
   • Total Batches: 15,625
   • Files per Agent: 2,500
   • Samples per Agent: 250,000
   • Batches per Agent: 3,906

🎯 Path Assignment (Shared Storage - No Prefixes):
   • agent-0: s3://my-bucket/data (all agents use same path)
   • agent-1: s3://my-bucket/data
   • agent-2: s3://my-bucket/data
   • agent-3: s3://my-bucket/data

✅ Validation Passed! Ready to run distributed workload.
```

**For Local Storage:**
```
🎯 Path Assignment (Local Storage - Agent Prefixes Applied):
   • agent-0: /mnt/data/agent-0/
   • agent-1: /mnt/data/agent-1/
   • agent-2: /mnt/data/agent-2/
   • agent-3: /mnt/data/agent-3/
```

### 3.5 Results Aggregation & Output ⏳

**Console Output:**
```
🚀 Distributed Workload Execution Complete!
═══════════════════════════════════════════

📊 Storage Performance (I/O Perspective):
┌─────────────────┬──────────┬──────────┬────────┬────────┬────────┬────────┬────────┬───────────┬──────────┐
│ Agent           │ ops/s    │ MiB/s    │ p50_ms │ p90_ms │ p95_ms │ p99_ms │ Errors │ Total Ops │ Duration │
├─────────────────┼──────────┼──────────┼────────┼────────┼────────┼────────┼────────┼───────────┼──────────┤
│ gpu-node-01     │ 1234.5   │ 567.8    │ 12.3   │ 23.4   │ 34.5   │ 45.6   │ 0      │ 123450    │ 100.0s   │
│ gpu-node-02     │ 1245.6   │ 572.1    │ 11.8   │ 22.9   │ 33.8   │ 44.2   │ 0      │ 124560    │ 100.1s   │
│ gpu-node-03     │ 1256.7   │ 578.9    │ 12.1   │ 23.1   │ 34.2   │ 45.0   │ 0      │ 125670    │ 100.2s   │
│ gpu-node-04     │ 1267.8   │ 584.2    │ 11.9   │ 22.8   │ 33.5   │ 43.8   │ 0      │ 126780    │ 100.0s   │
├─────────────────┼──────────┼──────────┼────────┼────────┼────────┼────────┼────────┼───────────┼──────────┤
│ AGGREGATE       │ 5004.6   │ 2303.0   │ 12.0   │ 23.0   │ 34.0   │ 44.6   │ 0      │ 500460    │ -        │
└─────────────────┴──────────┴──────────┴────────┴────────┴────────┴────────┴────────┴───────────┴──────────┘

🤖 AI/ML Training Performance (Training Perspective):
┌─────────────────┬────────────┬──────────────┬────────────┬──────────────┬────────┬────────┬──────────────┐
│ Agent           │ samples/s  │ Total Samples│ batches/s  │ Total Batches│ Epochs │ Batch  │ Pipeline Eff │
│                 │            │              │            │              │        │ Time   │              │
├─────────────────┼────────────┼──────────────┼────────────┼──────────────┼────────┼────────┼──────────────┤
│ gpu-node-01     │ 5000.0     │ 500000       │ 78.1       │ 7813         │ 1      │ 12.8ms │ 0.95         │
│ gpu-node-02     │ 5100.0     │ 510000       │ 79.7       │ 7969         │ 1      │ 12.6ms │ 0.96         │
│ gpu-node-03     │ 5050.0     │ 505000       │ 78.9       │ 7891         │ 1      │ 12.7ms │ 0.95         │
│ gpu-node-04     │ 5150.0     │ 515000       │ 80.5       │ 8047         │ 1      │ 12.4ms │ 0.96         │
├─────────────────┼────────────┼──────────────┼────────────┼──────────────┼────────┼────────┼──────────────┤
│ AGGREGATE       │ 20300.0    │ 2030000      │ 317.2      │ 31720        │ 4      │ 12.6ms │ 0.96         │
└─────────────────┴────────────┴──────────────┴────────────┴──────────────┴────────┴────────┴──────────────┘

💾 Results Written:
   ✅ Storage metrics: results_storage.tsv
   ✅ AI/ML metrics:   results_aiml.tsv
```

**TSV Files:**
- `results_storage.tsv` - Via `AggregateResults::to_storage_tsv()`
- `results_aiml.tsv` - Via `AggregateResults::to_aiml_tsv()`

### 3.6 Integration Tests ⏳

Create `crates/cli/tests/controller_integration_test.rs`:

**Test Scenarios:**
1. Controller can connect to multiple agents
2. Health check validates all agents before running
3. Controller distributes configs with correct prefixes
4. Start times are coordinated within 100ms
5. Results are correctly aggregated
6. Both TSV files are generated
7. --dry-run validates without running
8. Graceful error handling for agent failures

**Mock Agent Testing:**
- Spawn local test agents on different ports
- Send configs and verify path prefixes
- Collect results and verify aggregation

### 3.7 Documentation ⏳

**Update Files:**
- `README.md` - Add distributed execution section with examples
- `docs/Changelog.md` - v0.8.0 release notes
- `docs/DISTRIBUTED_EXECUTION_PLAN.md` - Mark Phase 3 complete
- Create `docs/DISTRIBUTED_USAGE_GUIDE.md` - Complete usage guide

**Example Configs:**
- `tests/dlio_configs/distributed_s3_config.yaml`
- `tests/dlio_configs/distributed_file_config.yaml`
- `tests/distributed_configs/4_node_cluster.yaml`

## Implementation Plan

### Step 1: Controller Core (3-4 hours)
1. Create controller.rs skeleton
2. Implement agent connection pool
3. Implement health_check_all()
4. Implement config distribution with path prefixes
5. Implement coordinated start timing
6. Implement result collection
7. Test with local agents

### Step 2: CLI Integration (2-3 hours)
1. Add `distributed run` subcommand
2. Add CLI argument parsing
3. Wire up controller
4. Add progress indicators
5. Test with example configs

### Step 3: Dry-run & Validation (2 hours)
1. Implement dry-run mode
2. Add validation output
3. Add health check summary
4. Test validation scenarios

### Step 4: Results Output (1-2 hours)
1. Implement console output with tables
2. Wire up TSV file writing
3. Test both storage and AI/ML outputs
4. Verify aggregation correctness

### Step 5: Integration Tests (2-3 hours)
1. Create test infrastructure
2. Write controller tests
3. Write end-to-end tests
4. Verify all scenarios pass

### Step 6: Documentation (1-2 hours)
1. Update README
2. Update Changelog
3. Create usage guide
4. Add example configs

## Success Criteria

- ✅ Controller binary compiles and runs
- ✅ Can connect to multiple agents
- ✅ Health check validates all agents
- ✅ Configs distributed with correct path prefixes
- ✅ Coordinated start timing accurate (< 100ms variance)
- ✅ Results correctly aggregated
- ✅ Both storage and AI/ML TSV files generated
- ✅ --dry-run validates without running
- ✅ All integration tests pass
- ✅ No breaking changes to existing functionality
- ✅ Clean build with no warnings
- ✅ Comprehensive documentation

## Technical Notes

### Agent Connection Management
- Use Tonic's connection pooling
- Set reasonable timeouts (default 5min for workloads)
- Implement retry logic with exponential backoff
- Gracefully handle partial failures

### Path Prefix Strategy
- Detect storage backend from `data_folder` URI
- Skip prefixes for shared storage (s3://, az://, gs://)
- Apply prefixes for local storage (file://, direct://)
- Use `{id}` variable in template (0-based index)

### Coordinated Start
- Calculate: `start_time = now() + start_delay_ms`
- Send `start_unix_ms` to all agents
- Agents sleep until start time
- Ensures synchronized workload start

### Result Aggregation
- Sum throughput metrics (ops/s, MiB/s, samples/s, batches/s)
- Average latency percentiles (simple weighted avg)
- Sum counts (total_ops, total_samples, total_batches, errors)
- Sum epoch counts across agents

### Error Handling
- Fail fast if health checks fail
- Collect partial results if some agents fail
- Report which agents succeeded/failed
- Exit with error code if any agent fails

---

## Phase 4 Preview: Advanced Features (Future)

Potential enhancements:
- Retry logic for transient failures
- Progressive rollout (subset of agents first)
- Real-time progress monitoring
- Agent resource monitoring (CPU, memory, network)
- Dynamic agent discovery (service registry)
- Load balancing across heterogeneous agents
- Checkpoint/resume for long workloads
