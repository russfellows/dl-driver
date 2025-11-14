# dl-driver v0.8.8 Roadmap
**Planned Release:** Q1 2026  
**Theme:** Multi-Node Training Realism

## Overview

Version 0.8.8 will close critical gaps identified in the **Multi-Node Training Emulation Analysis** (see `docs/MULTI_NODE_TRAINING_EMULATION_ANALYSIS.md`), bringing dl-driver to 95% realism for emulating PyTorch/TensorFlow distributed training patterns.

**Current Status:** dl-driver is 80% of the way there  
**After v0.8.8:** dl-driver will be 95%+ realistic for multi-node training emulation

---

## Planned Features

### � Priority 0: Distributed Mode Multi-Rank Support (Infrastructure)

**Problem:** Current distributed mode runs each agent as a single rank. Agents don't participate in global data sharding - each agent runs `WorkloadRunner` with `world_size=1`.

**Impact:** Cannot realistically emulate multi-node training where:
- 8 agents × 8 ranks/agent = 64 total ranks
- Each rank gets a unique shard of the global dataset
- All ranks coordinate with `global_rank` and `global_world_size`

**This is the infrastructure prerequisite for testing all other gaps (per-epoch shuffle, sample-level sharding) in distributed environments.**

---

#### Phase 1: Single Rank Per Agent (Global Sharding)

**Goal:** Each agent represents one global rank with proper cluster-wide sharding.

**CLI Changes:**
```bash
# Before (v0.8.7): No rank awareness
./dl-driver distributed run --config config.yaml --agents host1:50051,host2:50051

# After (v0.8.8): Implicit global ranks
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --shard-strategy interleaved  # NEW - passes to agents
# Automatically assigns: agent-0 = rank 0, agent-1 = rank 1, ..., world_size = 8
```

**Proto Changes (bench.proto):**
```protobuf
message RunWorkloadRequest {
    string workload_config = 1;        // Existing: YAML config
    bool dry_run = 2;                  // Existing
    
    // NEW: Distributed rank information
    uint32 global_rank = 3;            // This agent's global rank
    uint32 global_world_size = 4;      // Total ranks across all agents
    string shard_strategy = 5;         // "interleaved" | "contiguous" | "hash"
}
```

**Controller Changes (controller.rs):**
```rust
// In run_distributed():
let num_agents = agents.len();
let global_world_size = num_agents;  // Phase 1: 1 rank per agent

for (agent_index, agent) in agents.iter().enumerate() {
    let req = RunWorkloadRequest {
        workload_config: config_yaml.clone(),
        dry_run: false,
        global_rank: agent_index as u32,           // NEW
        global_world_size: global_world_size as u32, // NEW
        shard_strategy: dist_config.shard_strategy.clone(), // NEW
    };
    
    // Send to agent via gRPC...
}
```

**Agent Changes (agent.rs):**
```rust
// In execute_workload():
let rank = req.global_rank as usize;
let world_size = req.global_world_size as usize;
let strategy = req.shard_strategy.as_str();

// Load and parse file list
let file_list = discover_files(&config.dataset.data_folder)?;

// Apply global sharding (uses existing CLI logic!)
let sharded_files = apply_sharding_strategy(
    &file_list,
    world_size,
    rank,
    strategy,
)?;

// Create runner with rank config (ALREADY EXISTS in CLI path!)
let mut runner = WorkloadRunner::new(config)?;
runner = runner.with_rank_config(rank, world_size, sharded_files);

// Run workload
runner.run().await?;
```

**Key Insight:** The `with_rank_config()` method **already exists** for CLI multi-rank mode. We just need to use it in the agent path!

**Testing:**
- 8 agents, each gets 1/8th of dataset
- Verify no file overlap between agents
- Verify union of all agent files = full dataset

---

#### Phase 2: Multiple Ranks Per Agent (8×8 Emulation)

**Goal:** Each agent simulates multiple GPUs (e.g., 8 ranks per agent).

**CLI Changes:**
```bash
# 8 agents × 8 ranks/agent = 64 total ranks
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --ranks-per-agent 8 \
    --shard-strategy interleaved
# Auto-computes:
#   host1 gets ranks [0..7], host2 gets ranks [8..15], ..., world_size=64
```

**Proto Changes:**
```protobuf
message RunWorkloadRequest {
    // ... existing fields ...
    
    uint32 rank_start = 6;      // First rank for this agent
    uint32 ranks_per_agent = 7; // Number of ranks to run on this agent
}
```

**Controller Changes:**
```rust
let ranks_per_agent = dist_config.ranks_per_agent; // default: 1
let global_world_size = agents.len() * ranks_per_agent;

for (agent_index, agent) in agents.iter().enumerate() {
    let rank_start = agent_index * ranks_per_agent;
    
    let req = RunWorkloadRequest {
        workload_config: config_yaml.clone(),
        global_rank: rank_start as u32,  // First rank for this agent
        global_world_size: global_world_size as u32,
        rank_start: rank_start as u32,
        ranks_per_agent: ranks_per_agent as u32,
        shard_strategy: dist_config.shard_strategy.clone(),
        // ...
    };
}
```

**Agent Changes (Multi-Rank Execution):**
```rust
// In execute_workload():
let rank_start = req.rank_start as usize;
let ranks_per_agent = req.ranks_per_agent as usize;
let world_size = req.global_world_size as usize;

// Spawn multiple runners locally
let mut handles = vec![];
for local_rank in 0..ranks_per_agent {
    let global_rank = rank_start + local_rank;
    
    // Shard files for this specific rank
    let sharded_files = apply_sharding_strategy(
        &file_list,
        world_size,
        global_rank,
        &req.shard_strategy,
    )?;
    
    // Spawn runner (tokio task or OS process)
    let handle = tokio::spawn(async move {
        let mut runner = WorkloadRunner::new(config.clone())?;
        runner = runner.with_rank_config(global_rank, world_size, sharded_files);
        runner.run().await
    });
    
    handles.push(handle);
}

// Wait for all ranks to complete
for handle in handles {
    handle.await??;
}
```

**Testing:**
- 8 agents × 8 ranks = 64 total ranks
- Verify each of 64 ranks gets unique file subset
- Verify no overlap between any two ranks
- Verify union of all 64 ranks = full dataset

---

#### Distributed Data Layout Config

**Problem:** Need to distinguish between two physical data patterns:
1. **Shared global dataset** (S3/Azure/GCS) - all agents see same paths
2. **Per-agent local dataset** (NVMe) - each agent sees different paths

**Config Addition:**
```yaml
dataset:
  data_folder: s3://bucket/train/
  num_files_train: 10000
  
  distributed_data:
    layout: global  # or "per_agent"
    # global: num_files_train is total across cluster, shard via (rank, world_size)
    # per_agent: num_files_train is per-agent, use path_template for isolation
```

**Interpretation:**
- **`layout: global`**:
  - `num_files_train` is total (e.g., 10000 files for entire cluster)
  - All agents use same `data_folder`
  - Sharding is purely logical via `(global_rank, world_size)`
  
- **`layout: per_agent`**:
  - `num_files_train` is per-agent (e.g., 10000 files × 8 agents = 80000 total)
  - Controller uses `--path-template "{id}/"` to isolate agents
  - Each agent operates on its own subtree

**Default:** `global` (matches single shared storage backend)

---

#### Sharding Validation Mode

**Feature:** Sanity check that distributed sharding is correct.

**CLI:**
```bash
./dl-driver distributed run \
    --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --validate-sharding  # NEW flag
```

**Behavior:**
1. Each agent records `(global_rank, file_id)` for all files it reads
2. Controller collects all records
3. Validates:
   - **Coverage:** ⋃ Files(r) = AllFiles (no files skipped)
   - **Disjoint:** Files(r_i) ∩ Files(r_j) = ∅ for i ≠ j (no overlap)
   - **Balance:** |Files(r_i)| ≈ |Files(r_j)| (±1 for remainder distribution)

**Output:**
```
✅ Sharding validation passed
   - 64 ranks participated
   - 10000 total files
   - Per-rank average: 156.25 files
   - Per-rank range: [156, 157] (perfect balance)
   - No overlaps detected
   - All files covered
```

**Use Case:** One-time verification when setting up new distributed configs.

---

### �🔥 Priority 1: Per-Epoch Shuffle (Gap 1)

**Problem:** Current implementation shuffles once at workload start. Real training reshuffles **every epoch** with a deterministic seed derived from epoch number.

**Industry Standard Behavior (PyTorch):**
```python
sampler = DistributedSampler(dataset, num_replicas=64, rank=5, shuffle=True, seed=42)

for epoch in range(10):
    sampler.set_epoch(epoch)  # Reshuffle with seed=42+epoch
    for batch in dataloader:
        train(batch)
```

**Current dl-driver Behavior:**
```rust
// Shuffle once at start
if shuffle {
    indices.shuffle(&mut rng);  // Single shuffle
}

// Same order for all epochs ❌
for epoch in 0..num_epochs {
    for idx in &indices {  // Same indices every epoch
        load_sample(idx)
    }
}
```

**Impact:**
- ❌ Epochs see samples in same order (unrealistic for convergence testing)
- ❌ Can't reproduce PyTorch/TF per-epoch shuffle behavior
- ❌ Multi-rank tests don't match real distributed training data flow

**Implementation Plan:**

1. **Add config option:**
```yaml
reader:
  shuffle: true
  shuffle_per_epoch: true  # NEW - default: true when shuffle=true
  seed: 42
```

2. **Wire into s3dlio LoaderOptions:**

s3dlio already has the hooks:
```rust
pub struct LoaderOptions {
    pub shard_rank: usize,
    pub shard_world_size: usize,
    pub worker_id: usize,
    pub num_workers_pytorch: usize,
    // ... existing fields
}
```

dl-driver needs to populate these from rank config:
```rust
// In workload.rs or dlio_compat.rs
if let Some(rank_cfg) = rank_config {
    loader_options.shard_rank = rank_cfg.rank;
    loader_options.shard_world_size = rank_cfg.world_size;
} else {
    loader_options.shard_rank = 0;
    loader_options.shard_world_size = 1;
}
```

3. **Update workload loop:**
```rust
for epoch in 0..num_epochs {
    let mut epoch_indices = indices.clone();
    
    if shuffle && shuffle_per_epoch {
        // Deterministic per-epoch shuffle
        let epoch_seed = base_seed.wrapping_add(epoch as u64);
        let mut epoch_rng = StdRng::seed_from_u64(epoch_seed);
        epoch_indices.shuffle(&mut epoch_rng);
    }
    
    // Shard based on shuffled indices
    let my_indices = shard_indices(&epoch_indices, rank, world_size);
    
    for idx in my_indices {
        load_sample(idx)
    }
}
```

4. **Testing:**
   - Verify same seed produces same per-epoch order across runs
   - Verify different epochs have different orders
   - Verify multi-rank coordination (all ranks use same epoch shuffle)
   - Compare against PyTorch DistributedSampler output

**Compatibility:**
- Backward compatible: `shuffle_per_epoch` defaults to `false` for v0.8.7 and earlier configs
- New configs can opt-in with `shuffle_per_epoch: true`
- Eventually make `true` the default (with deprecation warning)

---

### 🎯 Priority 2: Sample-Level Sharding (Gap 2)

**Problem:** Current sharding operates at **file level**. Real training often shards at **sample level**, allowing perfect balance even with variable-sized files.

**Example Scenario:**
```
Dataset: 100 files × 1000 samples/file = 100,000 total samples
World size: 64 ranks
Expected: Each rank gets exactly 1562-1563 samples
```

**Current dl-driver (File Sharding):**
```
Interleaved strategy:
  Rank 0 gets files [0, 64, ...]  → Maybe 1562 samples (if files balanced)
  Rank 1 gets files [1, 65, ...]  → Maybe 1800 samples (if files unbalanced ❌)
  ...
  Rank 63 gets files [63, 127, ...] → Maybe 1400 samples (if files unbalanced ❌)
```

**Real Training (Sample Sharding):**
```
PyTorch DistributedSampler:
  Total indices: [0..99999]
  Shuffle: [42873, 198, 76234, ...]
  Rank 0 gets indices [0:1562] from shuffled list (✅ Perfect balance)
  Rank 1 gets indices [1562:3124] from shuffled list (✅ Perfect balance)
  ...
```

**Implementation Plan:**

1. **Add CLI option:**
```bash
--shard-level samples  # Shard at sample level (NEW)
--shard-level files    # Shard at file level (current default)
```

2. **Add config option:**
```yaml
reader:
  shard_level: samples  # or 'files' (default: 'files' for compatibility)
```

3. **Sample-level sharding algorithm:**
```rust
if shard_level == "samples" {
    // Build global sample index
    let mut global_samples = Vec::new();
    for (file_idx, num_samples) in file_sample_counts.iter().enumerate() {
        for sample_idx in 0..*num_samples {
            global_samples.push((file_idx, sample_idx));
        }
    }
    
    // Shuffle globally (if enabled)
    if shuffle {
        global_samples.shuffle(&mut rng);
    }
    
    // Shard samples using chosen strategy
    let my_samples = match shard_strategy {
        "interleaved" => global_samples.iter()
            .enumerate()
            .filter(|(i, _)| i % world_size == rank)
            .map(|(_, s)| s.clone())
            .collect(),
        "contiguous" => {
            let chunk_size = global_samples.len() / world_size;
            let start = rank * chunk_size;
            let end = (start + chunk_size).min(global_samples.len());
            global_samples[start..end].to_vec()
        }
        _ => { /* hash strategy */ }
    };
    
    // Load samples (may span multiple files)
    for (file_idx, sample_idx) in my_samples {
        load_sample_from_file(file_idx, sample_idx);
    }
}
```

4. **Requirements:**
   - Must know `num_samples_per_file` for each file (already in config)
   - May need to read file headers if sample counts vary
   - s3dlio already supports random access within files (NPZ, HDF5)

**Testing:**
   - Create dataset with variable file sizes (e.g., 900, 1100, 1050 samples per file)
   - Verify each rank gets exactly `total_samples / world_size` samples (±1 for remainder)
   - Verify no samples are skipped or duplicated
   - Compare against PyTorch behavior with same dataset

**Compatibility:**
- Default remains `shard_level: files` (backward compatible)
- New configs opt-in with `shard_level: samples`

**Performance Considerations:**
- Sample-level requires reading across file boundaries
- May slightly increase I/O overhead for first sample in new file
- Trade-off: perfect balance vs. file-aligned I/O (usually worth it)

---

### 💡 Priority 3: Node/Local-Rank Abstraction (Gap 3)

**Problem:** Current implementation uses flat `rank` and `world_size`. No explicit concept of `node_id` or `local_rank`.

**Real Training Code Pattern:**
```python
# Typical PyTorch DDP setup
node_id = rank // gpus_per_node  # Which node am I on?
local_rank = rank % gpus_per_node  # Which GPU within this node?

# Use for CUDA device
os.environ['CUDA_VISIBLE_DEVICES'] = str(local_rank)

# Use for node-local NVMe path
data_path = f"/node{node_id}/nvme/data/"
```

**Implementation Plan:**

1. **Add CLI parameters:**
```bash
# Option 1: Auto-compute from rank and gpus_per_node
./dl-driver run --rank 13 --world-size 64 --gpus-per-node 8
# Auto-computes: node_id=1, local_rank=5

# Option 2: Explicit specification (for testing)
./dl-driver run --rank 13 --world-size 64 --node-id 1 --local-rank 5
```

2. **Add environment variable support:**
```bash
# Match PyTorch/Horovod/DeepSpeed conventions
export RANK=13
export WORLD_SIZE=64
export LOCAL_RANK=5
export NODE_RANK=1  # or NODE_ID

./dl-driver run --config config.yaml  # Reads from env
```

3. **Use in path construction:**
```yaml
dataset:
  data_folder: file:///node{node_id}/nvme/shard{node_id}/
  # Expands to: file:///node1/nvme/shard1/ for node_id=1
```

4. **Display in logs:**
```
INFO: Rank 13 of 64 (node 1, local rank 5)
INFO: Data path: file:///node1/nvme/shard1/
INFO: GPU affinity: [would be set to GPU 5]
```

**Benefits:**
- Makes dl-driver commands look more like real training scripts
- Easier to write launcher scripts for HPC/cloud environments
- Enables node-local optimizations (NVMe paths, etc.)

**Compatibility:**
- All new parameters optional
- Existing `--rank` / `--world-size` continue to work
- Default: `node_id=0`, `local_rank=rank` (single-node assumption)

---

## Deferred Features

### ⏸️ Gap 4: Two-Level Sharding (Future)

**Status:** Deferred to v0.8.9 or later

**Reason:** Can be emulated with current distributed mode:
- Use per-agent path prefixes for node-level sharding
- Each agent gets a node-specific data shard
- Within each agent, use file-level or sample-level sharding

**Example Workaround:**
```bash
# Node 0: agents 0-7 all point to /node0/nvme/shard0/
# Node 1: agents 8-15 all point to /node1/nvme/shard1/
./dl-driver distributed run \
    --agents node0:50051,...,node7:50051,node1:50051,... \
    --path-template "gpu-{id}/"
```

This achieves the same result without adding complex two-level sharding config.

**Future Consideration:** If demand is high, add explicit config:
```yaml
dataset:
  sharding:
    level_1: node     # Shard by node first
    level_2: gpu      # Then shard within node
    num_nodes: 8
    gpus_per_node: 8
```

---

## Testing Plan

### Unit Tests
- Distributed mode rank assignment (Phase 1 & 2)
- Per-epoch shuffle determinism
- Sample-level sharding correctness (no skips/duplicates)
- Node/local-rank computation
- Environment variable parsing
- Sharding validation logic

### Integration Tests
- Phase 1: 8 agents × 1 rank = 8 global ranks with file-level sharding
- Phase 2: 8 agents × 8 ranks = 64 global ranks with file-level sharding
- 64-rank simulation with per-epoch shuffle
- Variable file size dataset with sample-level sharding
- Multi-node paths with node_id substitution
- Sharding validation with `--validate-sharding` flag

### Validation Tests
- Compare dl-driver output vs PyTorch DistributedSampler
  - Same seed → same sample order per epoch
  - Same sharding → same samples per rank
- Verify Gap 0 (distributed rank awareness) enables multi-node testing
- Verify Gap 1, 2, 3 coverage with real training scenario emulation
- Sharding sanity checks: coverage, disjoint, balance

### Distributed Testing Scenarios

**Scenario 1: Object Store (Global Sharding)**
```yaml
dataset:
  data_folder: s3://bucket/train/
  num_files_train: 10000
  distributed_data:
    layout: global
```
```bash
# 8 agents × 1 rank = 8 global ranks
./dl-driver distributed run --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --shard-strategy interleaved
# Each agent reads 1/8th of 10000 files from shared S3
```

**Scenario 2: Local NVMe (Per-Agent Sharding)**
```yaml
dataset:
  data_folder: file:///data/train/
  num_files_train: 1250  # Per agent
  distributed_data:
    layout: per_agent
```
```bash
# 8 agents × 1 rank = 8 global ranks
./dl-driver distributed run --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --path-template "node-{id}/"
# Each agent reads 1250 files from file:///data/train/node-0/ through node-7/
```

**Scenario 3: Full 8×8 Emulation (64 Ranks)**
```yaml
dataset:
  data_folder: s3://bucket/imagenet/
  num_files_train: 128000  # ~2000 files per rank
  distributed_data:
    layout: global
```
```bash
# 8 agents × 8 ranks/agent = 64 global ranks
./dl-driver distributed run --config config.yaml \
    --agents host1:50051,...,host8:50051 \
    --ranks-per-agent 8 \
    --shard-strategy interleaved
# Each of 64 ranks reads 1/64th of 128000 files
```

---

## Success Criteria

v0.8.8 will be considered successful when:

1. ✅ **Distributed mode rank awareness** (Priority 0)
   - Phase 1: 1 rank per agent with global sharding
   - Phase 2: Multiple ranks per agent (8×8 = 64 ranks)
   - Proto, controller, agent all properly pass rank info
   - `with_rank_config()` used in agent path (reuses CLI logic)

2. ✅ **Per-epoch shuffle** matches PyTorch behavior exactly
   - Same seed produces same per-epoch order
   - Different epochs have different orders
   - Multi-rank coordination works correctly
   - s3dlio LoaderOptions properly wired

3. ✅ **Sample-level sharding** achieves perfect balance
   - Each rank gets `total_samples / world_size` samples (±1)
   - No samples skipped or duplicated
   - Works with variable file sizes

4. ✅ **Node/local-rank** parameters work intuitively
   - Auto-compute from `--gpus-per-node`
   - Environment variable support
   - Used in path construction

5. ✅ **Sharding validation** mode works
   - `--validate-sharding` flag
   - Verifies coverage, disjoint, balance
   - Clear pass/fail output

6. ✅ **Zero regressions** from v0.8.7
   - All existing tests pass
   - Backward compatible configs work unchanged
   - Zero warnings (production quality maintained)

7. ✅ **Documentation complete**
   - USER_GUIDE updated with new features
   - Examples for all features
   - Migration guide from v0.8.7 to v0.8.8

---

## Timeline

**Target Release:** Q1 2026

| Milestone | Target Date | Status |
|-----------|-------------|--------|
| Priority 0: Distributed rank awareness - Phase 1 | Week 1-2 | Not Started |
| Priority 0: Distributed rank awareness - Phase 2 | Week 3-4 | Not Started |
| Priority 1: Per-epoch shuffle implementation | Week 5 | Not Started |
| Priority 2: Sample-level sharding implementation | Week 6-7 | Not Started |
| Priority 3: Node/local-rank abstraction | Week 8 | Not Started |
| Integration testing | Week 9 | Not Started |
| Documentation | Week 10 | Not Started |
| Release v0.8.8 | End Q1 2026 | Not Started |

---

## Questions / Discussion Points

1. **Distributed mode implementation order?**
   - Phase 1 (1 rank/agent) first, then Phase 2 (multi-rank/agent)?
   - Or implement both together?
   - **Recommendation:** Phase 1 first (simpler, tests infrastructure)

2. **Default behavior for shuffle_per_epoch?**
   - Should `shuffle_per_epoch` default to `true` for new configs?
   - Breaking change for reproducibility, but more realistic
   - **Recommendation:** Default to `true` when `shuffle: true`

3. **Sample-level overhead acceptable?**
   - Is cross-file reading overhead acceptable?
   - Should we benchmark file vs sample sharding?
   - **Recommendation:** Make it optional, default to file-level for compatibility

4. **Environment variable precedence?**
   - Which takes priority: CLI args, env vars, or config file?
   - **Recommendation:** CLI > env vars > config file (matches industry standard)

5. **Node-local storage detection?**
   - Auto-detect NVMe mounts and suggest node-local paths?
   - Or require explicit configuration?
   - **Recommendation:** Explicit configuration (more predictable)

6. **Sharding validation mode overhead?**
   - Should `--validate-sharding` be default for distributed runs?
   - Or opt-in only?
   - **Recommendation:** Opt-in (avoids overhead in production testing)

7. **Proto versioning?**
   - New fields in `RunWorkloadRequest` - how to handle version compatibility?
   - **Recommendation:** Add version field, graceful degradation for old agents

---

## References

- `docs/MULTI_NODE_TRAINING_EMULATION_ANALYSIS.md` - Detailed gap analysis
- **External guidance on distributed mode multi-rank implementation** (November 13, 2025)
  - Proto extensions: `global_rank`, `global_world_size`, `rank_start`, `ranks_per_agent`
  - Controller rank computation and assignment
  - Agent `with_rank_config()` reuse from CLI path
  - Two-phase approach: 1 rank/agent → multi-rank/agent
- PyTorch DistributedSampler: https://pytorch.org/docs/stable/data.html#torch.utils.data.distributed.DistributedSampler
- TensorFlow data sharding: https://www.tensorflow.org/api_docs/python/tf/data/Dataset#shard
- Horovod data loading: https://horovod.readthedocs.io/en/stable/data.html
- s3dlio LoaderOptions: `shard_rank`, `shard_world_size`, `worker_id`, `num_workers_pytorch`

---

**Last Updated:** November 13, 2025  
**Status:** Planning Document (v0.8.8 not yet started)  
**Updated:** Incorporated distributed mode multi-rank support as Priority 0
