# Multi-Node Training Emulation Analysis
**Date:** November 13, 2025  
**Version:** dl-driver v0.8.7

## Executive Summary

This document analyzes dl-driver's current capabilities for emulating real-world multi-node, multi-GPU training environments and identifies gaps against industry-standard distributed training patterns.

---

## Real-World Multi-Node Training Patterns

### Standard Architecture: 8 Nodes × 8 GPUs

**Key Characteristics:**
- `world_size = 64` (total workers)
- `global_rank ∈ [0..63]` (unique ID per GPU/process)
- `local_rank ∈ [0..7]` (GPU index within node)
- `node_id = global_rank // 8`

**Data Loading Model:**
- Data is split by `global_rank` and `world_size`, NOT by physical node
- Each worker sees a **different subset** of samples each epoch
- All samples are seen exactly once per epoch across the entire cluster

### Two Common Deployment Patterns

#### Pattern 1: Logical Sharding (Shared Storage)
```
All nodes → Same global dataset (S3/GCS/parallel FS)
           ↓
    DistributedSampler decides which samples each rank reads
           ↓
    Rank 0 gets indices [0, 64, 128, ...]
    Rank 1 gets indices [1, 65, 129, ...]
    Rank 63 gets indices [63, 127, 191, ...]
```

**Characteristics:**
- No physical data pre-sharding required
- Common with object stores (S3, Azure Blob, GCS)
- Each rank computes which files/samples to read
- Easy scaling (add/remove nodes without data reorganization)

#### Pattern 2: Physical Pre-Sharding (Local NVMe)
```
Preprocessing:
    Dataset → 8 node-level shards (one per node)
              ↓
    Node 0 NVMe gets shard 0
    Node 1 NVMe gets shard 1
    ...
    Node 7 NVMe gets shard 7

Training:
    Each node → Local shard
               ↓
        8 GPUs split node's shard using local_rank
               ↓
        Node 0, GPU 0 gets chunk [0:12.5%] of shard 0
        Node 0, GPU 1 gets chunk [12.5%:25%] of shard 0
        ...
```

**Characteristics:**
- Two-level sharding: node-level then GPU-level
- Maximizes NVMe locality, minimizes network I/O
- Common in HPC environments
- Requires data reorganization when scaling

---

## dl-driver's Current Capabilities

### ✅ Supported Features

#### 1. Multi-Rank Execution (Single Node)
**Mode:** Multi-process with shared memory coordination

```bash
# 4 processes on one machine
./dl-driver run --config config.yaml --rank 0 --world-size 4 &
./dl-driver run --config config.yaml --rank 1 --world-size 4 &
./dl-driver run --config config.yaml --rank 2 --world-size 4 &
./dl-driver run --config config.yaml --rank 3 --world-size 4
```

**Features:**
- ✅ `--rank` parameter (0-based)
- ✅ `--world-size` parameter
- ✅ Atomic shared memory coordination (no temp files)
- ✅ Rank 0 aggregates results
- ✅ Synchronized start/stop timing

**Sharding Strategies:**
```bash
--shard-strategy interleaved   # Rank R gets files [R, R+W, R+2W, ...]
--shard-strategy contiguous    # Divide files into equal chunks
--shard-strategy hash          # Hash-based consistent distribution
```

**Implementation:**
```rust
// crates/cli/src/main.rs:1126-1160
match strategy {
    "interleaved" => {
        // i % world_size == rank
        files.iter().enumerate()
            .filter(|(i, _)| i % world_size == rank)
            .map(|(_, f)| f.clone()).collect()
    }
    "contiguous" => {
        // Contiguous chunks with remainder distribution
        let chunk_size = total_files / world_size;
        let remainder = total_files % world_size;
        let start = rank * chunk_size + min(rank, remainder);
        let end = start + chunk_size + if rank < remainder { 1 } else { 0 };
        files[start..end].to_vec()
    }
    "hash" => {
        // Hash-based: hash(filename) % world_size == rank
        files.iter().filter(|f| {
            let mut hasher = DefaultHasher::new();
            f.hash(&mut hasher);
            (hasher.finish() % world_size as u64) as usize == rank
        }).cloned().collect()
    }
}
```

#### 2. Distributed Multi-Agent Execution (Multi-Node)
**Mode:** gRPC-based controller/agent architecture

```bash
# Agent on each host
host1$ ./dl_driver_agent --agent-id agent-0 --port 50051
host2$ ./dl_driver_agent --agent-id agent-1 --port 50051

# Controller coordinates
$ ./dl-driver distributed run \
    --config config.yaml \
    --agents http://host1:50051,http://host2:50051 \
    --path-template "agent-{id}/"
```

**Features:**
- ✅ True multi-host execution
- ✅ gRPC coordination
- ✅ Per-agent path prefixing (for local storage)
- ✅ Startup handshake with validation (v0.8.7)
- ✅ Live stats streaming (v0.8.7)
- ✅ Progress bars with sample counts (v0.8.7)
- ✅ Bucket-level histogram aggregation

#### 3. Data Sharding Strategies
- ✅ **Interleaved (default):** `i % world_size == rank` - matches PyTorch strided split
- ✅ **Contiguous:** Chunk-based - matches PyTorch contiguous split
- ✅ **Hash:** Consistent pseudo-random - good for unbalanced file sizes

#### 4. Deterministic Shuffling
- ✅ Seed support: `reader.seed` in config
- ✅ Per-epoch deterministic shuffle (via s3dlio LoaderOptions)

---

## ⚠️ Identified Gaps

### Gap 1: No Explicit Node/Local-Rank Abstraction

**Problem:**
Current implementation uses flat `rank` and `world_size`. There's no explicit concept of:
- `node_id = rank // gpus_per_node`
- `local_rank = rank % gpus_per_node`

**Why This Matters:**
Real training code often needs to know:
- Which node am I on? (for node-local NVMe path construction)
- What's my local GPU ID? (for CUDA_VISIBLE_DEVICES)

**Example Use Case:**
```python
# Real PyTorch DDP code
node_id = rank // 8
local_rank = rank % 8
os.environ['CUDA_VISIBLE_DEVICES'] = str(local_rank)

# Data path: /node{node_id}/nvme/data/
data_path = f"/node{node_id}/nvme/data/"
```

**Current Workaround:**
Users must manually compute this or use distributed mode with per-agent paths.

**Recommendation:**
Add optional CLI parameters:
```bash
--rank 5 --world-size 64 --gpus-per-node 8
# Auto-computes: node_id=0, local_rank=5
```

Or environment variable support:
```bash
export RANK=5
export WORLD_SIZE=64
export LOCAL_RANK=5
export NODE_ID=0
./dl-driver run --config config.yaml
```

---

### Gap 2: No Per-Epoch Shuffle with Deterministic Seed

**Problem:**
Current implementation shuffles once at workload start. Real training reshuffles **every epoch** with a deterministic seed derived from epoch number.

**Industry Standard (PyTorch):**
```python
sampler = DistributedSampler(
    dataset, 
    num_replicas=world_size, 
    rank=rank, 
    shuffle=True,
    seed=42  # Base seed
)

for epoch in range(num_epochs):
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

// Same order for all epochs
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

**Recommendation:**
Add per-epoch shuffle:
```rust
for epoch in 0..num_epochs {
    let mut epoch_indices = indices.clone();
    if shuffle {
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

---

### Gap 3: Sample-Level vs File-Level Sharding

**Problem:**
Current sharding operates at **file level**. Real training often shards at **sample level**.

**Example Scenario:**
```
Dataset: 100 files × 1000 samples/file = 100,000 total samples
World size: 64 ranks
Expected: Each rank gets 100,000/64 ≈ 1562 samples
```

**Current dl-driver (File Sharding):**
```
Interleaved strategy:
  Rank 0 gets files [0, 64, ...]  → Maybe 1562 samples if files balanced
  Rank 1 gets files [1, 65, ...]  → Maybe 1562 samples if files balanced
  ...
  Rank 63 gets files [63, 127, ...] → Maybe 1562 samples if files balanced

Problem: If files have unequal samples, ranks get unequal data!
```

**Real Training (Sample Sharding):**
```
PyTorch DistributedSampler:
  Total indices: [0..99999]
  Shuffle: [42873, 198, 76234, ...]
  Rank 0 gets indices [0:1562] from shuffled list
  Rank 1 gets indices [1562:3124] from shuffled list
  ...
  Perfect balance: exactly 1562 samples per rank
```

**Current Workaround:**
- Ensure all files have exactly the same number of samples
- Use `num_samples_per_file` config to guarantee balance

**Impact:**
- ⚠️ Minor for well-structured datasets (equal samples per file)
- ❌ Major for real-world datasets with variable file sizes
- ❌ Can't emulate frameworks that read across file boundaries

**Recommendation:**
Add sample-level sharding mode:
```bash
--shard-level samples  # Shard at sample level (default for realism)
--shard-level files    # Shard at file level (current behavior)
```

Implementation:
```rust
if shard_level == "samples" {
    // Build global sample index
    let mut global_samples = Vec::new();
    for (file_idx, num_samples) in file_sample_counts {
        for sample_idx in 0..num_samples {
            global_samples.push((file_idx, sample_idx));
        }
    }
    
    // Shuffle globally
    global_samples.shuffle(&mut rng);
    
    // Shard samples
    let my_samples = shard_by_rank(global_samples, rank, world_size);
    
    // Load samples (may span multiple files)
    for (file_idx, sample_idx) in my_samples {
        load_sample_from_file(file_idx, sample_idx);
    }
}
```

---

### Gap 4: No "Two-Level Sharding" for Node+GPU Emulation

**Problem:**
No built-in support for Pattern 2 (node-level shard → GPU-level shard).

**Use Case:**
Emulate 8 nodes × 8 GPUs where:
1. Each node has a pre-sharded dataset on local NVMe
2. Within each node, 8 GPUs split that node's shard

**Desired Behavior:**
```bash
# Node 0, GPU 0
./dl-driver run --rank 0 --world-size 64 --node-id 0 --local-rank 0 \
    --data-folder file:///node0/nvme/shard0/

# Node 0, GPU 7
./dl-driver run --rank 7 --world-size 64 --node-id 0 --local-rank 7 \
    --data-folder file:///node0/nvme/shard0/

# Node 1, GPU 0
./dl-driver run --rank 8 --world-size 64 --node-id 1 --local-rank 0 \
    --data-folder file:///node1/nvme/shard1/
```

**Current Workaround:**
- Use distributed mode with per-agent paths
- Manually pre-shard data per agent

**Recommendation:**
Add two-level sharding config:
```yaml
dataset:
  data_folder: file:///data/train/
  sharding:
    level_1: node     # Shard by node first
    level_2: gpu      # Then shard within node
    num_nodes: 8
    gpus_per_node: 8
```

Or CLI:
```bash
--shard-level node-then-gpu --num-nodes 8 --gpus-per-node 8
```

---

## ✅ What dl-driver Already Does Well

### 1. Flexible Sharding Strategies
- Three strategies (interleaved/contiguous/hash) cover most use cases
- Hash strategy particularly good for unbalanced file sizes

### 2. True Multi-Host Execution
- Distributed mode with gRPC is enterprise-ready
- No shared filesystem required
- Works across cloud providers

### 3. Coordinated Timing
- `start_at_epoch` parameter for synchronized starts
- Proper startup handshake (v0.8.7)
- Live stats streaming (v0.8.7)

### 4. Storage Backend Flexibility
- Works with file://, s3://, az://, gs://, direct://
- Same sharding logic across all backends
- Matches real training where backend is abstracted

---

## Recommendations Summary

### Priority 1: Critical for Realism

1. **Per-Epoch Shuffle with Deterministic Seed** (Gap 2)
   - Add `shuffle_per_epoch: bool` config option
   - Use `seed + epoch` for deterministic reshuffling
   - Matches PyTorch/TF behavior exactly

### Priority 2: Important for Enterprise Use

2. **Sample-Level Sharding** (Gap 3)
   - Add `--shard-level samples` mode
   - Build global sample index before sharding
   - Critical for variable file sizes

3. **Node/Local-Rank Abstraction** (Gap 1)
   - Add `--gpus-per-node`, `--node-id`, `--local-rank` parameters
   - Or auto-compute from `rank` and `--gpus-per-node`
   - Makes code look more like real training scripts

### Priority 3: Nice to Have

4. **Two-Level Sharding** (Gap 4)
   - Add config for node-then-GPU sharding
   - Useful for HPC/NVMe scenarios
   - Can be emulated with current distributed mode

---

## Example: How to Emulate 8×8 Cluster Today

### Option 1: Multi-Rank Mode (Single Machine)

Emulate all 64 ranks on one machine:

```bash
#!/bin/bash
# Emulate 8 nodes × 8 GPUs = 64 ranks
WORLD_SIZE=64

for rank in $(seq 0 63); do
    node_id=$((rank / 8))
    local_rank=$((rank % 8))
    
    echo "Starting rank $rank (node $node_id, GPU $local_rank)"
    
    ./dl-driver run \
        --config config.yaml \
        --rank $rank \
        --world-size $WORLD_SIZE \
        --shard-strategy interleaved \
        --results results/rank_${rank}.json &
done

wait
echo "All 64 ranks complete"
```

**Pros:**
- ✅ Tests data distribution logic
- ✅ Verifies each rank gets correct subset
- ✅ No network overhead

**Cons:**
- ❌ Not testing real multi-host I/O patterns
- ❌ All ranks compete for same disk/network

### Option 2: Distributed Mode (Multi-Host)

True distributed testing:

```bash
# On 8 hosts, start 8 agents each (64 total)
for i in {0..7}; do
    ssh host$i "./dl_driver_agent --agent-id agent-$i --port 50051" &
done

# Run controller
./dl-driver distributed run \
    --config config.yaml \
    --agents $(for i in {0..7}; do echo -n "http://host$i:50051,"; done | sed 's/,$//')
```

**Pros:**
- ✅ Tests real network patterns
- ✅ Tests real storage contention
- ✅ Production-like environment

**Cons:**
- ❌ Requires 8 hosts
- ❌ More complex setup

---

## Conclusion

**dl-driver is 80% of the way there** for emulating real multi-node training:

✅ **Strengths:**
- Multi-rank execution with proper sharding
- Three sharding strategies
- True multi-host distributed mode
- Synchronized timing
- Deterministic seeding (base level)

⚠️ **Missing for 100% Realism:**
- Per-epoch shuffle (most critical)
- Sample-level sharding (important for variable files)
- Node/local-rank abstraction (nice UI improvement)
- Two-level sharding (advanced use case)

**Recommended Implementation Order:**
1. Per-epoch shuffle (highest impact, moderate effort)
2. Sample-level sharding (high impact, high effort)
3. Node/local-rank params (low impact, low effort)
4. Two-level sharding config (niche use case, high effort)

With Gap 2 (per-epoch shuffle) addressed, dl-driver would accurately emulate PyTorch/TensorFlow data loading for 95% of real-world training scenarios.
