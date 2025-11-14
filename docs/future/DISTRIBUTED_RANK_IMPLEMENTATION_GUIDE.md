# Distributed Mode Multi-Rank Implementation Guide
**Version:** v0.8.8 Planning  
**Date:** November 13, 2025  
**Status:** Implementation Blueprint

## Overview

This document provides concrete implementation patterns for adding rank awareness to dl-driver's distributed mode, enabling true multi-node, multi-GPU training emulation.

**Goal:** Transform distributed mode from "multiple independent agents" to "coordinated multi-rank execution" where agents participate in global data sharding.

---

## Current State vs Target State

### Current (v0.8.7)

```rust
// Controller sends config to each agent
for agent in agents {
    send_config(agent, config);  // No rank info
}

// Agent runs as single rank
let runner = WorkloadRunner::new(config)?;
runner.run().await?;  // world_size = 1, rank = 0
```

**Problem:** Each agent operates independently. No global data sharding.

### Target (v0.8.8)

```rust
// Controller assigns global ranks
for (agent_index, agent) in agents.iter().enumerate() {
    let global_rank = agent_index;
    let global_world_size = agents.len();
    send_config_with_rank(agent, config, global_rank, global_world_size);
}

// Agent runs with rank awareness
let rank = req.global_rank;
let world_size = req.global_world_size;
let sharded_files = apply_sharding(&files, world_size, rank);
let runner = WorkloadRunner::new(config)?.with_rank_config(rank, world_size, sharded_files);
runner.run().await?;
```

**Result:** Agents coordinate on global dataset sharding, exactly like PyTorch DDP.

---

## Implementation Phases

### Phase 1: Single Rank Per Agent (Weeks 1-2)

**Scope:** Each agent = 1 global rank. 8 agents = world_size 8.

**Target CLI:**
```bash
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,host2:50051,...,host8:50051 \
    --shard-strategy interleaved
```

**Automatic behavior:**
- agent-0 → global_rank 0
- agent-1 → global_rank 1
- ...
- agent-7 → global_rank 7
- global_world_size = 8

---

#### Step 1: Extend Proto (bench.proto)

**File:** `crates/core/src/dist/proto/bench.proto`

```protobuf
message RunWorkloadRequest {
    string workload_config = 1;        // Existing: YAML config
    bool dry_run = 2;                  // Existing
    
    // NEW in v0.8.8: Distributed rank information
    uint32 global_rank = 3;            // This agent's rank in global cluster
    uint32 global_world_size = 4;      // Total ranks across all agents
    string shard_strategy = 5;         // "interleaved" | "contiguous" | "hash"
}
```

**After editing, regenerate Rust bindings:**
```bash
cd crates/core
cargo build  # Tonic build.rs regenerates from proto
```

---

#### Step 2: Update Controller (controller.rs)

**File:** `crates/core/src/dist/controller.rs`

**Add to DistributedConfig struct:**
```rust
pub struct DistributedConfig {
    pub agents: Vec<AgentEndpoint>,
    pub shard_strategy: String,  // NEW: "interleaved" | "contiguous" | "hash"
    pub path_template: Option<String>,
    // ... existing fields
}
```

**Modify `run_distributed()` function:**

```rust
pub async fn run_distributed(
    config: &DlioConfig,
    dist_config: &DistributedConfig,
) -> Result<()> {
    let agents = &dist_config.agents;
    let num_agents = agents.len();
    let global_world_size = num_agents;  // Phase 1: 1 rank per agent
    
    info!("Starting distributed run with {} agents (world_size={})", 
          num_agents, global_world_size);
    
    // Load and serialize config once
    let config_yaml = serde_yaml::to_string(config)?;
    
    // Create channels for each agent
    let mut handles = vec![];
    
    for (agent_index, agent) in agents.iter().enumerate() {
        let agent_id = format!("agent-{}", agent_index);
        let global_rank = agent_index;  // Phase 1: agent_index = rank
        
        info!("Assigning {} as global rank {} of {}", 
              agent_id, global_rank, global_world_size);
        
        // Build request with rank info
        let request = RunWorkloadRequest {
            workload_config: config_yaml.clone(),
            dry_run: false,
            global_rank: global_rank as u32,
            global_world_size: global_world_size as u32,
            shard_strategy: dist_config.shard_strategy.clone(),
        };
        
        // Spawn task to send to agent
        let agent_endpoint = agent.clone();
        let handle = tokio::spawn(async move {
            let mut client = BenchServiceClient::connect(agent_endpoint.url).await?;
            let response = client.run_workload(request).await?;
            Ok::<_, anyhow::Error>(response.into_inner())
        });
        
        handles.push((agent_id, handle));
    }
    
    // Wait for all agents (existing aggregation logic...)
    // ...
}
```

---

#### Step 3: Update Agent (agent.rs)

**File:** `crates/core/src/dist/agent.rs`

**Modify `execute_workload()` function:**

```rust
async fn execute_workload(
    &self,
    request: RunWorkloadRequest,
) -> Result<WorkloadSummary> {
    // Parse config
    let config: DlioConfig = serde_yaml::from_str(&request.workload_config)
        .context("Failed to parse workload config")?;
    
    // Extract rank info (NEW)
    let global_rank = request.global_rank as usize;
    let global_world_size = request.global_world_size as usize;
    let shard_strategy = request.shard_strategy.as_str();
    
    info!("Agent running as global rank {} of {} (strategy: {})",
          global_rank, global_world_size, shard_strategy);
    
    // Discover files from data_folder
    let data_folder = &config.dataset.data_folder;
    let file_list = discover_files(data_folder)
        .context("Failed to discover files")?;
    
    info!("Discovered {} files before sharding", file_list.len());
    
    // Apply sharding (REUSE existing CLI logic!)
    let sharded_files = apply_sharding_strategy(
        &file_list,
        global_world_size,
        global_rank,
        shard_strategy,
    )?;
    
    info!("After sharding: {} files assigned to rank {}", 
          sharded_files.len(), global_rank);
    
    // Create runner with rank config (ALREADY EXISTS from CLI path!)
    let mut runner = WorkloadRunner::new(config)?;
    runner = runner.with_rank_config(global_rank, global_world_size, sharded_files);
    
    // Run workload
    let summary = runner.run().await?;
    
    Ok(summary)
}
```

**Key insight:** `WorkloadRunner::with_rank_config()` **already exists** for CLI multi-rank mode. We just reuse it!

---

#### Step 4: Add Sharding Logic (agent.rs helper)

**File:** `crates/core/src/dist/agent.rs`

```rust
fn apply_sharding_strategy(
    files: &[String],
    world_size: usize,
    rank: usize,
    strategy: &str,
) -> Result<Vec<String>> {
    if world_size == 1 {
        return Ok(files.to_vec());  // No sharding needed
    }
    
    let sharded = match strategy {
        "interleaved" => {
            // Rank r gets files [r, r+w, r+2w, ...]
            files.iter()
                .enumerate()
                .filter(|(i, _)| i % world_size == rank)
                .map(|(_, f)| f.clone())
                .collect()
        }
        "contiguous" => {
            // Divide into equal chunks with remainder distribution
            let total = files.len();
            let chunk_size = total / world_size;
            let remainder = total % world_size;
            
            let start = rank * chunk_size + rank.min(remainder);
            let end = start + chunk_size + if rank < remainder { 1 } else { 0 };
            
            files[start..end].to_vec()
        }
        "hash" => {
            // Hash-based: hash(filename) % world_size == rank
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            
            files.iter()
                .filter(|f| {
                    let mut hasher = DefaultHasher::new();
                    f.hash(&mut hasher);
                    (hasher.finish() % world_size as u64) as usize == rank
                })
                .cloned()
                .collect()
        }
        _ => bail!("Unknown shard strategy: {}", strategy),
    };
    
    Ok(sharded)
}
```

**Note:** This logic already exists in `crates/cli/src/main.rs` for CLI multi-rank. Consider extracting to shared module.

---

#### Step 5: Update CLI (cli/main.rs)

**File:** `crates/cli/src/main.rs`

**Add `--shard-strategy` to distributed subcommand:**

```rust
#[derive(Args)]
pub struct DistributedArgs {
    /// gRPC agent endpoints (comma-separated)
    #[arg(long, value_delimiter = ',', required = true)]
    agents: Vec<String>,
    
    /// Sharding strategy for file distribution
    #[arg(long, default_value = "interleaved")]
    shard_strategy: String,  // NEW
    
    /// Path template for per-agent isolation
    #[arg(long)]
    path_template: Option<String>,
    
    // ... existing fields
}
```

**Pass to controller:**

```rust
let dist_config = DistributedConfig {
    agents: parse_agents(&args.agents)?,
    shard_strategy: args.shard_strategy,  // NEW
    path_template: args.path_template,
};

run_distributed(&config, &dist_config).await?;
```

---

#### Step 6: Testing Phase 1

**Test 1: Two-agent sharding validation**

```bash
# Terminal 1
./dl_driver_agent --agent-id agent-0 --port 50051

# Terminal 2
./dl_driver_agent --agent-id agent-1 --port 50052

# Terminal 3
./dl-driver distributed run \
    --config tests/dlio_configs/test_distributed_2agent.yaml \
    --agents http://localhost:50051,http://localhost:50052 \
    --shard-strategy interleaved
```

**Expected:**
- agent-0 logs: "Running as global rank 0 of 2... After sharding: 50 files"
- agent-1 logs: "Running as global rank 1 of 2... After sharding: 50 files"
- Controller: "8 agents ready - starting workload"

**Test 2: Eight-agent sharding**

Create config with 8000 files:
```yaml
dataset:
  data_folder: file:///data/train/
  num_files_train: 8000
```

```bash
# Start 8 agents on different hosts
for i in {0..7}; do
    ssh host$i "./dl_driver_agent --agent-id agent-$i --port 50051" &
done

# Run distributed
./dl-driver distributed run \
    --config config.yaml \
    --agents $(for i in {0..7}; do echo -n "http://host$i:50051,"; done) \
    --shard-strategy interleaved
```

**Verify:**
- Each agent gets exactly 1000 files (8000 / 8)
- No file overlap between agents
- Union of all agent files = full 8000

---

### Phase 2: Multiple Ranks Per Agent (Weeks 3-4)

**Scope:** Each agent can run multiple ranks. 8 agents × 8 ranks/agent = world_size 64.

**Target CLI:**
```bash
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --ranks-per-agent 8 \
    --shard-strategy interleaved
```

**Automatic behavior:**
- agent-0 runs ranks [0..7]
- agent-1 runs ranks [8..15]
- ...
- agent-7 runs ranks [56..63]
- global_world_size = 64

---

#### Step 7: Extend Proto Further (Phase 2)

```protobuf
message RunWorkloadRequest {
    // ... existing fields from Phase 1 ...
    
    // NEW in Phase 2: Multi-rank per agent
    uint32 rank_start = 6;       // First rank for this agent
    uint32 ranks_per_agent = 7;  // Number of ranks to spawn locally
}
```

---

#### Step 8: Update Controller for Multi-Rank (Phase 2)

```rust
pub struct DistributedConfig {
    pub agents: Vec<AgentEndpoint>,
    pub ranks_per_agent: usize,  // NEW: default 1 (Phase 1 behavior)
    pub shard_strategy: String,
    pub path_template: Option<String>,
}

pub async fn run_distributed(
    config: &DlioConfig,
    dist_config: &DistributedConfig,
) -> Result<()> {
    let agents = &dist_config.agents;
    let ranks_per_agent = dist_config.ranks_per_agent;
    let global_world_size = agents.len() * ranks_per_agent;
    
    info!("Starting distributed run: {} agents × {} ranks/agent = {} total ranks",
          agents.len(), ranks_per_agent, global_world_size);
    
    for (agent_index, agent) in agents.iter().enumerate() {
        let rank_start = agent_index * ranks_per_agent;
        let rank_end = rank_start + ranks_per_agent;
        
        info!("Agent {} assigned ranks [{}, {})", agent_index, rank_start, rank_end);
        
        let request = RunWorkloadRequest {
            workload_config: config_yaml.clone(),
            global_rank: rank_start as u32,  // First rank only
            global_world_size: global_world_size as u32,
            rank_start: rank_start as u32,
            ranks_per_agent: ranks_per_agent as u32,
            shard_strategy: dist_config.shard_strategy.clone(),
            // ...
        };
        
        // Send to agent...
    }
}
```

---

#### Step 9: Update Agent for Multi-Rank Execution (Phase 2)

```rust
async fn execute_workload(
    &self,
    request: RunWorkloadRequest,
) -> Result<WorkloadSummary> {
    let config: DlioConfig = serde_yaml::from_str(&request.workload_config)?;
    
    let rank_start = request.rank_start as usize;
    let ranks_per_agent = request.ranks_per_agent as usize;
    let global_world_size = request.global_world_size as usize;
    let shard_strategy = request.shard_strategy.as_str();
    
    info!("Agent running ranks [{}, {}) of {} total",
          rank_start, rank_start + ranks_per_agent, global_world_size);
    
    // Discover files once
    let file_list = discover_files(&config.dataset.data_folder)?;
    
    // Spawn multiple runners, one per rank
    let mut handles = vec![];
    
    for local_rank in 0..ranks_per_agent {
        let global_rank = rank_start + local_rank;
        
        // Shard files for this specific rank
        let sharded_files = apply_sharding_strategy(
            &file_list,
            global_world_size,
            global_rank,
            shard_strategy,
        )?;
        
        info!("Rank {}: {} files assigned", global_rank, sharded_files.len());
        
        // Clone config for this rank
        let rank_config = config.clone();
        
        // Spawn runner (tokio task or OS process)
        let handle = tokio::spawn(async move {
            let mut runner = WorkloadRunner::new(rank_config)?;
            runner = runner.with_rank_config(global_rank, global_world_size, sharded_files);
            runner.run().await
        });
        
        handles.push((global_rank, handle));
    }
    
    // Wait for all local ranks to complete
    let mut summaries = vec![];
    for (rank, handle) in handles {
        match handle.await {
            Ok(Ok(summary)) => {
                info!("Rank {} completed successfully", rank);
                summaries.push(summary);
            }
            Ok(Err(e)) => {
                error!("Rank {} failed: {}", rank, e);
                bail!("Rank {} workload failed", rank);
            }
            Err(e) => {
                error!("Rank {} task panicked: {}", rank, e);
                bail!("Rank {} task panicked", rank);
            }
        }
    }
    
    // Aggregate summaries from all local ranks
    let aggregated = aggregate_local_summaries(&summaries)?;
    
    Ok(aggregated)
}
```

---

#### Step 10: Testing Phase 2

**Test: 8×8 = 64 ranks**

```bash
# Start 8 agents
for i in {0..7}; do
    ssh host$i "./dl_driver_agent --agent-id agent-$i --port 50051" &
done

# Run with 8 ranks per agent
./dl-driver distributed run \
    --config tests/dlio_configs/multi_rank_64.yaml \
    --agents http://host1:50051,...,http://host8:50051 \
    --ranks-per-agent 8 \
    --shard-strategy interleaved
```

**Expected logs:**
```
INFO: Starting distributed run: 8 agents × 8 ranks/agent = 64 total ranks
INFO: Agent 0 assigned ranks [0, 8)
INFO: Agent 1 assigned ranks [8, 16)
...
INFO: Agent 7 assigned ranks [56, 64)

[Agent 0 logs]
INFO: Agent running ranks [0, 8) of 64 total
INFO: Rank 0: 125 files assigned
INFO: Rank 1: 125 files assigned
...
INFO: Rank 7: 125 files assigned
```

**Verification:**
- 64 ranks total (8 agents × 8 ranks/agent)
- Each rank gets 1/64th of dataset
- No overlap between any two ranks
- Union of all 64 ranks = full dataset

---

## Integration with Existing Features

### s3dlio LoaderOptions Wiring

**File:** `crates/core/src/workload.rs` or `crates/storage/src/dlio_compat.rs`

```rust
// When building LoaderOptions from rank config
if let Some(rank_cfg) = self.rank_config {
    loader_options.shard_rank = rank_cfg.rank;
    loader_options.shard_world_size = rank_cfg.world_size;
} else {
    loader_options.shard_rank = 0;
    loader_options.shard_world_size = 1;
}

// If PyTorch distributed mode enabled in config
if config.pytorch_config.as_ref().map_or(false, |p| p.distributed) {
    loader_options.num_workers_pytorch = config.reader.num_workers.unwrap_or(4);
}
```

This ensures s3dlio's internal sampler uses the same rank/world_size as dl-driver's file sharding.

---

### Live Stats Aggregation (from v0.8.7)

**No changes needed!** Existing live stats aggregation in controller already collects per-agent stats. With Phase 2, each agent aggregates its local ranks before reporting to controller.

```rust
// In agent (Phase 2)
let mut local_stats = LiveStatsAggregator::new(ranks_per_agent);
for rank in 0..ranks_per_agent {
    local_stats.update(rank, rank_stats);
}
let aggregated = local_stats.aggregate();

// Send to controller
yield LiveStats { ... };
```

Controller continues to aggregate across agents as before.

---

## Validation & Testing

### Sharding Sanity Check

**Add `--validate-sharding` flag:**

```bash
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --ranks-per-agent 8 \
    --validate-sharding  # NEW
```

**Implementation:**

```rust
if dist_config.validate_sharding {
    // Each agent records (rank, file_id) pairs
    let agent_records: Vec<(usize, Vec<String>)> = collect_from_agents().await?;
    
    // Validate coverage
    let all_files: HashSet<String> = agent_records.iter()
        .flat_map(|(_, files)| files.iter().cloned())
        .collect();
    
    let expected_files: HashSet<String> = discover_files(data_folder)?
        .into_iter().collect();
    
    if all_files != expected_files {
        let missing: Vec<_> = expected_files.difference(&all_files).collect();
        let extra: Vec<_> = all_files.difference(&expected_files).collect();
        error!("Sharding validation failed!");
        error!("  Missing files: {:?}", missing);
        error!("  Extra files: {:?}", extra);
        bail!("Sharding coverage check failed");
    }
    
    // Validate disjoint (no overlaps)
    for i in 0..agent_records.len() {
        for j in (i+1)..agent_records.len() {
            let set_i: HashSet<_> = agent_records[i].1.iter().collect();
            let set_j: HashSet<_> = agent_records[j].1.iter().collect();
            let overlap: Vec<_> = set_i.intersection(&set_j).collect();
            if !overlap.is_empty() {
                error!("Ranks {} and {} have overlapping files: {:?}", i, j, overlap);
                bail!("Sharding disjoint check failed");
            }
        }
    }
    
    info!("✅ Sharding validation passed");
    info!("   - {} ranks participated", agent_records.len());
    info!("   - {} total files", expected_files.len());
    info!("   - No overlaps detected");
    info!("   - All files covered");
}
```

---

## Migration from v0.8.7

**Backward compatibility:**

Phase 1 changes are **fully backward compatible**:
- Agents without rank info default to `world_size=1, rank=0` (current behavior)
- New proto fields are optional (use protobuf default values)
- Existing distributed configs work unchanged

**Recommended migration:**

1. **Update controller binary first** (sends new rank fields)
2. **Update agent binaries** (handles new rank fields)
3. **Add `--shard-strategy` to existing scripts**
4. **Test with `--validate-sharding`**
5. **Gradually roll out Phase 2 (`--ranks-per-agent`)**

---

## Performance Considerations

### Phase 1 (1 rank/agent)
- **Overhead:** Minimal (<1% - just parsing rank fields)
- **Network:** Same as v0.8.7 (no additional gRPC calls)
- **Memory:** Same as v0.8.7 (each agent runs 1 workload)

### Phase 2 (multi-rank/agent)
- **Overhead:** Linear with `ranks_per_agent` (8 ranks = ~8× local CPU/memory)
- **Network:** Reduced per-rank (agents share file discovery)
- **Memory:** Each rank holds its own dataset iterator state

**Recommendation:** For large-scale testing (64+ ranks), prefer Phase 1 with dedicated agents over Phase 2 with dense agent packing.

---

## Summary Checklist

**Phase 1 Complete When:**
- ✅ Proto has `global_rank`, `global_world_size`, `shard_strategy`
- ✅ Controller assigns ranks (agent_index = rank)
- ✅ Agent calls `with_rank_config()` with sharded file list
- ✅ CLI has `--shard-strategy` flag
- ✅ Tests pass: 2-agent, 8-agent sharding validation
- ✅ Zero regressions from v0.8.7

**Phase 2 Complete When:**
- ✅ Proto has `rank_start`, `ranks_per_agent`
- ✅ Controller computes rank ranges per agent
- ✅ Agent spawns multiple `WorkloadRunner` instances
- ✅ CLI has `--ranks-per-agent` flag
- ✅ Tests pass: 8×8=64 ranks, sharding validation
- ✅ Local rank aggregation works correctly

**Full Integration Complete When:**
- ✅ s3dlio `LoaderOptions` wired with rank info
- ✅ Per-epoch shuffle works with multi-rank (Priority 1)
- ✅ Sample-level sharding works with multi-rank (Priority 2)
- ✅ Live stats aggregation works with multi-rank (from v0.8.7)
- ✅ Documentation updated with examples

---

**Last Updated:** November 13, 2025  
**For:** dl-driver v0.8.8 development  
**Reference:** ROADMAP_V0.8.8.md Priority 0
