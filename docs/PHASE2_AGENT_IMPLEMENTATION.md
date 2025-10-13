# Phase 2: Agent Implementation

**Branch:** `v0.7.5-phase2-agent-implementation`  
**Version:** v0.7.5  
**Date:** October 12, 2025

## Prerequisites ✅

Phase 1 has been completed and merged to main:
- ✅ gRPC dependencies added
- ✅ Protocol buffer definitions created
- ✅ Distributed module with path utilities
- ✅ DistributedConfig schema
- ✅ DlioConfig::apply_agent_prefix() method
- ✅ All tests passing

## Phase 2 Goals

Implement the gRPC agent server that can:
1. Receive DLIO workload configurations via gRPC
2. Apply agent-specific path prefixes for local storage isolation
3. Coordinate start times across multiple agents
4. Execute DLIO workloads using existing WorkloadRunner
5. Collect and return performance metrics
6. Handle errors gracefully with proper logging

## Tasks

### 2.1 Implement Agent Server ✅

**Status:** Complete

Created `crates/core/src/dist/agent.rs`:

**Key Components:**
- `AgentService` struct implementing the `DistAgent` gRPC trait
- Parse incoming YAML → `DlioConfig`
- Apply agent prefix using `apply_agent_prefix()`
- Coordinated start: `sleep_until(start_unix_ms)`
- Run existing `WorkloadRunner`
- **Dual metrics collection:** Both storage I/O and AI/ML training metrics
- Error handling with context

**Metrics Enhancement:**
- **Storage Metrics:** ops/s, MiB/s, latency percentiles (p50/p90/p95/p99)
- **AI/ML Metrics:** samples/s, batches/s, epochs, pipeline efficiency
- See `docs/DUAL_METRICS_REPORTING.md` for full details

**API:**
```rust
pub struct AgentService {
    // Service state
}

impl DistAgent for AgentService {
    async fn run_workload(
        &self,
        request: Request<RunWorkloadRequest>,
    ) -> Result<Response<WorkloadSummary>, Status>;
    
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status>;
}
```

### 2.2 Create Agent Binary ✅

**Status:** Complete

Created `crates/cli/src/bin/dl_driver_agent.rs`:

**Features:**
- CLI argument parsing (port, bind address, log level, agent ID)
- Start Tonic gRPC server
- Wire up `AgentService`
- Graceful shutdown handling (SIGTERM, SIGINT)
- Logging configuration

**Usage:**
```bash
dl-driver-agent --port 50051 --bind-addr 0.0.0.0 --log-level info --agent-id agent-0
```

### 2.3 Integration Tests ⏳ (In Progress)

**Completed:**
- ✅ Agent service creation test
- ✅ Coordinated start timing tests (past, immediate, future)
- ✅ Dual TSV output tests (storage + AI/ML)
- ✅ Aggregate results tests

**To Do:**
- Health check endpoint test with running server
- Full end-to-end workload execution test
- Path prefix application test (file:// vs s3://)
- Error handling tests for invalid configs

**Test Files:**
- `crates/core/src/dist/agent.rs` - Unit tests (4 tests passing)
- `crates/core/src/dist/types.rs` - TSV output tests (2 tests passing)
- Future: `crates/cli/tests/agent_integration_test.rs` - Full integration tests

## Implementation Plan

### Step 1: Agent Service (2-3 hours)
1. Create agent.rs skeleton
2. Implement health_check RPC (simple)
3. Implement run_workload RPC:
   - Parse YAML to DlioConfig
   - Apply agent prefix
   - Check coordinated start time
   - Run WorkloadRunner
   - Collect metrics
4. Error handling and logging

### Step 2: Agent Binary (1-2 hours)
1. Create bin/dl_driver_agent.rs
2. Add CLI argument parsing
3. Set up Tonic server
4. Wire up AgentService
5. Add graceful shutdown
6. Test manually with local server

### Step 3: Integration Tests (2-3 hours)
1. Create test file
2. Write health check test
3. Write single workload test
4. Write path prefix tests
5. Write coordinated start test
6. Verify all tests pass

### Step 4: Documentation (1 hour)
1. Update README with agent usage
2. Add example agent configs
3. Update Changelog for v0.7.5
4. Document agent deployment patterns

## Success Criteria

- ✅ Agent binary compiles and runs
- ✅ Health check RPC works
- ✅ Can receive DLIO config and execute workload
- ✅ Returns valid WorkloadSummary with metrics
- ✅ Path prefix isolation works correctly
- ✅ Coordinated start timing accurate
- ✅ All integration tests pass
- ✅ No breaking changes to existing functionality
- ✅ Clean build with no warnings

## Technical Notes

### Coordinated Start Implementation
```rust
async fn wait_for_start(start_unix_ms: i64) -> Result<()> {
    let start_time = UNIX_EPOCH + Duration::from_millis(start_unix_ms as u64);
    let now = SystemTime::now();
    
    if start_time > now {
        let wait_duration = start_time.duration_since(now)?;
        tokio::time::sleep(wait_duration).await;
    }
    
    Ok(())
}
```

### Metrics Collection
Use existing `Metrics` struct from core library:
```rust
let metrics = workload_runner.get_metrics();
let summary = WorkloadSummary {
    agent_id: request.agent_id,
    ops_per_s: metrics.ops_per_second(),
    mib_per_s: metrics.mib_per_second(),
    // ... percentiles from metrics
};
```

### Error Handling Pattern
```rust
.map_err(|e| Status::internal(format!("Failed to parse config: {}", e)))?
```

## Next Phase Preview

**Phase 3: Controller & CLI Integration**
- Implement controller client for broadcasting to agents
- Add `ctl` subcommand to dl-driver CLI
- Results aggregation and reporting
- TSV export for analysis
- Dry-run validation mode

## Timeline Estimate

- Agent Service: 2-3 hours
- Agent Binary: 1-2 hours  
- Integration Tests: 2-3 hours
- Documentation: 1 hour

**Total: 6-9 hours for Phase 2**

---

Ready to begin implementation! 🚀
