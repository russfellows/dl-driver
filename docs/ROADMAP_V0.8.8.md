# dl-driver v0.8.8 Roadmap
**Planned Release:** Q1 2026  
**Theme:** Multi-Node Training Realism

## Overview

Version 0.8.8 will close critical gaps identified in the **Multi-Node Training Emulation Analysis** (see `docs/MULTI_NODE_TRAINING_EMULATION_ANALYSIS.md`), bringing dl-driver to 95% realism for emulating PyTorch/TensorFlow distributed training patterns.

**Current Status:** dl-driver is 80% of the way there  
**After v0.8.8:** dl-driver will be 95%+ realistic for multi-node training emulation

---

## Planned Features

### 🔥 Priority 1: Per-Epoch Shuffle (Gap 1)

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

2. **Update workload loop:**
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

3. **Testing:**
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
- Per-epoch shuffle determinism
- Sample-level sharding correctness (no skips/duplicates)
- Node/local-rank computation
- Environment variable parsing

### Integration Tests
- 64-rank simulation with per-epoch shuffle
- Variable file size dataset with sample-level sharding
- Multi-node paths with node_id substitution

### Validation Tests
- Compare dl-driver output vs PyTorch DistributedSampler
  - Same seed → same sample order per epoch
  - Same sharding → same samples per rank
- Verify Gap 1, 2, 3 coverage with real training scenario emulation

---

## Success Criteria

v0.8.8 will be considered successful when:

1. ✅ **Per-epoch shuffle** matches PyTorch behavior exactly
   - Same seed produces same per-epoch order
   - Different epochs have different orders
   - Multi-rank coordination works correctly

2. ✅ **Sample-level sharding** achieves perfect balance
   - Each rank gets `total_samples / world_size` samples (±1)
   - No samples skipped or duplicated
   - Works with variable file sizes

3. ✅ **Node/local-rank** parameters work intuitively
   - Auto-compute from `--gpus-per-node`
   - Environment variable support
   - Used in path construction

4. ✅ **Zero regressions** from v0.8.7
   - All existing tests pass
   - Backward compatible configs work unchanged
   - Zero warnings (production quality maintained)

5. ✅ **Documentation complete**
   - USER_GUIDE updated with new features
   - Examples for all three gaps
   - Migration guide from v0.8.7 to v0.8.8

---

## Timeline

**Target Release:** Q1 2026

| Milestone | Target Date | Status |
|-----------|-------------|--------|
| Per-epoch shuffle implementation | Week 1-2 | Not Started |
| Sample-level sharding implementation | Week 3-4 | Not Started |
| Node/local-rank abstraction | Week 5 | Not Started |
| Integration testing | Week 6 | Not Started |
| Documentation | Week 7 | Not Started |
| Release v0.8.8 | End Q1 2026 | Not Started |

---

## Questions / Discussion Points

1. **Default behavior change?**
   - Should `shuffle_per_epoch` default to `true` for new configs?
   - Breaking change for reproducibility, but more realistic

2. **Sample-level overhead?**
   - Is cross-file reading overhead acceptable?
   - Should we benchmark file vs sample sharding?

3. **Environment variable precedence?**
   - Which takes priority: CLI args, env vars, or config file?
   - Proposed: CLI > env vars > config file

4. **Node-local storage detection?**
   - Auto-detect NVMe mounts and suggest node-local paths?
   - Or require explicit configuration?

---

## References

- `docs/MULTI_NODE_TRAINING_EMULATION_ANALYSIS.md` - Detailed gap analysis
- PyTorch DistributedSampler: https://pytorch.org/docs/stable/data.html#torch.utils.data.distributed.DistributedSampler
- TensorFlow data sharding: https://www.tensorflow.org/api_docs/python/tf/data/Dataset#shard
- Horovod data loading: https://horovod.readthedocs.io/en/stable/data.html

---

**Last Updated:** November 13, 2025  
**Status:** Planning Document (v0.8.8 not yet started)
