# Distributed Execution Implementation Plan

## Overview
Add multi-host distributed execution to dl-driver, leveraging architectural patterns from sai3-bench but implemented directly in dl-driver with DLIO-focused design.

## Implementation Phases

### Phase 1: Foundation & Cleanup ✅ (Current Branch: v0.7.4-s3dlio-0.9.6-range-engine-default-off)

**Goals:**
1. Remove unused replay functionality (delegated to sai3-bench)
2. Add gRPC infrastructure (proto, build system)
3. Create distributed module structure
4. Implement path rewriting and storage detection utilities

**Tasks:**

#### 1.1 Cleanup - Remove Replay Code
- Remove `crates/core/src/replay.rs` (operation log replay - use sai3-bench instead)
- Remove `S3BenchReplayEngine` references from codebase
- Remove replay-related CLI commands from `crates/cli/src/main.rs`
- Remove `s3dlio-oplog` dependency (no longer needed)
- Update documentation to clarify dl-driver focuses on DLIO workloads, sai3-bench handles replay
- Remove any replay-related test files

#### 1.2 Add gRPC Dependencies
Add to `crates/core/Cargo.toml`:
```toml
tonic = "0.12"
prost = "0.13"
prost-types = "0.13"
async-trait = "0.1"
```

Add to `crates/cli/Cargo.toml`:
```toml
tonic = "0.12"
prost = "0.13"
```

#### 1.3 Create Proto & Build System
- Create `crates/core/src/dist/proto/bench.proto`:
  - `RunWorkloadRequest` (config_yaml, agent_id, path_prefix, start_unix_ms)
  - `WorkloadSummary` (agent_id, ops_per_s, mib_per_s, p50/p90/p95/p99, errors, total_ops, duration_s)
  - `DistAgent` service (RunWorkload, HealthCheck RPCs)
- Create `crates/core/build.rs` for prost/tonic compilation

#### 1.4 Create Distributed Module Structure
- `crates/core/src/dist/mod.rs` - Public API exports
- `crates/core/src/dist/types.rs` - Rust wrapper types for proto messages
- `crates/core/src/dist/path_utils.rs`:
  - `is_shared_storage(uri: &str) -> bool` - Detect s3://, az://, gs://
  - `apply_path_prefix(uri: &str, prefix: &str) -> Result<String>` - Rewrite file:// and direct://
  - `join_uri_path(base: &str, suffix: &str) -> Result<String>` - Safe URI joining

#### 1.5 Add DlioConfig Path Prefix Support
Extend `crates/core/src/dlio_compat.rs`:
- Add `DlioConfig::apply_agent_prefix(&mut self, agent_id: &str, path_template: &str) -> Result<()>`
- Rewrites `dataset.data_folder` and checkpoint paths for local backends
- Skips rewriting for shared storage URIs

#### 1.6 Add Distributed Config Schema
Create `crates/core/src/config/distributed.rs`:
```rust
pub struct DistributedConfig {
    pub agents: Vec<String>,              // ["host1:50051", "host2:50051"]
    pub path_template: String,            // "agent-{id}/"
    pub start_delay_ms: u64,              // Coordinated start delay
    pub request_timeout_ms: u64,          // Per-request timeout
    pub max_retries: u32,                 // Retry failed agents
    pub shared_backends: Vec<String>,     // ["s3", "az", "gs"]
}
```

**Deliverables:**
- Clean codebase (replay code removed)
- Working proto compilation
- Path rewriting utilities with unit tests
- DlioConfig can apply agent prefixes
- Ready for agent/controller implementation

---

### Phase 2: Agent Implementation (New Branch: v0.8.0-distributed-agent)

**Goals:**
1. Implement gRPC agent server
2. Agent can receive config, apply prefix, run workload, return metrics
3. CLI binary for agent

**Tasks:**

#### 2.1 Implement Agent Server
Create `crates/core/src/dist/agent.rs`:
- `AgentService` implementing `DistAgent` trait
- Parse YAML → DlioConfig
- Apply agent prefix using Phase 1 utilities
- Coordinated start: sleep until `start_unix_ms` if in future
- Run existing `WorkloadRunner`
- Collect metrics → `WorkloadSummary`
- Error handling and logging

#### 2.2 Create Agent Binary
Create `crates/cli/src/bin/dl_driver_agent.rs`:
- CLI args: `--port`, `--bind-addr`, `--log-level`
- Start Tonic server
- Wire up `AgentService`
- Graceful shutdown handling

#### 2.3 Integration Tests
- Test agent server starts/stops cleanly
- Test single agent can run DLIO workload
- Test path prefix application (file:// vs s3://)
- Test coordinated start timing

**Deliverables:**
- Working agent binary: `dl-driver-agent --port 50051`
- Can receive config over gRPC and execute workload
- Returns valid WorkloadSummary
- Unit and integration tests passing

---

### Phase 3: Controller & CLI Integration (New Branch: v0.8.0-distributed-controller)

**Goals:**
1. Implement controller client
2. Add `ctl` subcommand to dl-driver CLI
3. Results aggregation and reporting
4. Complete distributed execution workflow

**Tasks:**

#### 3.1 Implement Controller Client
Create `crates/core/src/dist/controller.rs`:
- `DistributedController` client
- Broadcast `RunWorkloadRequest` to all agents
- Compute coordinated `start_unix_ms = now + start_delay_ms`
- Retry logic with exponential backoff
- Collect `WorkloadSummary` from all agents
- Aggregate metrics:
  - Sum ops/s and MiB/s
  - Approximate merged percentiles (simple weighted avg initially)
  - Count errors across agents

#### 3.2 Add CLI Subcommand
Extend `crates/cli/src/main.rs`:
```rust
Commands::Ctl {
    #[command(subcommand)]
    command: CtlCommands,
}

enum CtlCommands {
    Run {
        config: PathBuf,
        agents: Vec<String>,        // Or read from config
        start_delay_ms: Option<u64>,
        path_template: Option<String>,
        dry_run: bool,
        tsv_out: Option<PathBuf>,
    }
}
```

#### 3.3 Implement --dry-run Validation
- Parse DLIO config
- Validate all required fields
- Check agent connectivity (HealthCheck RPC)
- Print summary:
  - Backend detection (shared vs local)
  - Path prefix strategy
  - Estimated duration
  - Number of agents
  - Total operations
- Exit without running workload

#### 3.4 Results Output
- Per-agent output:
  ```
  Agent host1:50051 - ops/s: 1234.5, MiB/s: 567.8, p50: 12.3ms, p99: 45.6ms, errors: 0
  Agent host2:50051 - ops/s: 1245.6, MiB/s: 572.1, p50: 11.8ms, p99: 44.2ms, errors: 0
  ```
- Aggregate output:
  ```
  AGGREGATE - ops/s: 2480.1, MiB/s: 1139.9, p50: 12.0ms, p99: 44.9ms, total_errors: 0
  ```
- Optional TSV export:
  ```tsv
  agent_id	ops_s	mib_s	p50_ms	p90_ms	p95_ms	p99_ms	errors	total_ops	duration_s
  host1:50051	1234.5	567.8	12.3	23.4	34.5	45.6	0	123450	100.0
  host2:50051	1245.6	572.1	11.8	22.9	33.8	44.2	0	124560	100.0
  AGGREGATE	2480.1	1139.9	12.0	23.1	34.1	44.9	0	248010	100.0
  ```

#### 3.5 Documentation
- Update README.md with distributed execution section
- Add example configs with `distributed:` block
- Add usage examples for multi-host execution
- Update Changelog for v0.8.0

#### 3.6 End-to-End Tests
- 2-agent test on localhost (different ports/paths)
- File backend with path isolation
- S3 backend (no path rewriting)
- DirectIO backend with path isolation
- Error handling (agent failure, timeout)

**Deliverables:**
- Working controller: `dl-driver ctl run --agents host1:50051,host2:50051 --config test.yaml`
- Per-agent and aggregate reporting
- TSV export for dashboards
- --dry-run validation
- Full documentation
- All tests passing

---

## Success Criteria

- ✅ Clean codebase (replay removed)
- ✅ Agent binary runs DLIO workloads via gRPC
- ✅ Controller coordinates multi-host execution
- ✅ Path isolation for local storage
- ✅ Shared storage detection works correctly
- ✅ Coordinated start timing works
- ✅ Metrics aggregation is accurate
- ✅ TSV export for analysis
- ✅ All 80+ existing tests still pass
- ✅ New distributed tests pass
- ✅ Documentation complete

## Branch Strategy

1. **Phase 1**: Work on current branch `v0.7.4-s3dlio-0.9.6-range-engine-default-off`
   - Commit and push after Phase 1 complete
   
2. **Phase 2**: Create new branch `v0.8.0-distributed-agent`
   - Branch from Phase 1
   - Merge to main after testing
   
3. **Phase 3**: Create new branch `v0.8.0-distributed-controller`
   - Branch from Phase 2
   - Merge to main after testing
   - Tag as v0.8.0 release

## Timeline Estimate

- Phase 1: 1-2 days (cleanup + foundation)
- Phase 2: 2-3 days (agent implementation + tests)
- Phase 3: 2-3 days (controller + CLI + e2e tests)

**Total: ~5-8 days for complete implementation**
