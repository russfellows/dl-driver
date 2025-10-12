# AI/ML Workload Metrics Requirements

**Date:** October 12, 2025  
**Context:** Phase 2 Agent Implementation - Metrics Enhancement

## Problem Statement

Current WorkloadSummary metrics are storage-centric:
- ops_per_s (files/s)
- mib_per_s (bandwidth)
- Latency percentiles
- Total ops, errors, duration

**Missing:** AI/ML practitioners need training-centric metrics to understand data loading performance in the context of model training.

## AI/ML Perspective vs Storage Perspective

### AI/ML Practitioners Care About:

1. **Training Velocity**
   - Samples per second - "How fast am I training?"
   - Batches per second - "How many iterations per second?"
   - Epochs per hour - "When will training finish?"

2. **Data Pipeline Efficiency**
   - Is data loading keeping up with the GPU?
   - What's my data loading bottleneck?
   - Am I I/O bound or compute bound?

3. **Batch-Level Metrics**
   - Average batch loading time
   - Batch processing rate
   - Samples per batch (batch size)

4. **Scale Metrics**
   - Total samples processed
   - Total batches completed
   - Dataset coverage

### Storage Engineers Care About:

1. **Raw Performance**
   - Files/objects per second (IOPS)
   - Throughput (MiB/s, GiB/s)
   - Latency (p50, p90, p95, p99)

2. **Efficiency**
   - Queue depth / concurrency
   - Cache hit rates
   - Read amplification

## Proposed Enhanced Metrics

### WorkloadSummary Message Enhancement

```protobuf
message WorkloadSummary {
    // Agent identification
    string agent_id = 1;
    
    // === STORAGE METRICS (existing) ===
    double ops_per_s = 2;           // Files/objects per second
    double mib_per_s = 3;           // Raw bandwidth
    double p50_ms = 4;              // Latency percentiles
    double p90_ms = 5;
    double p95_ms = 6;
    double p99_ms = 7;
    uint32 errors = 8;
    uint64 total_ops = 9;           // Total files/objects
    double duration_s = 10;
    
    // === AI/ML TRAINING METRICS (new) ===
    // Sample-level metrics
    double samples_per_second = 11;  // Core training throughput
    uint64 total_samples = 12;       // Total samples processed
    uint64 samples_per_batch = 13;   // Batch size
    
    // Batch-level metrics
    double batches_per_second = 14;  // Training iteration rate
    uint64 total_batches = 15;       // Batches completed
    double avg_batch_time_ms = 16;   // Average batch loading time
    
    // Epoch-level metrics (optional)
    uint32 epochs_completed = 17;    // Number of epochs
    double avg_epoch_time_s = 18;    // Average epoch duration
    
    // Pipeline breakdown (optional)
    double data_loading_time_s = 19; // Time spent in I/O
    double compute_time_s = 20;      // Time spent in compute (if simulated)
    double pipeline_efficiency = 21; // Ratio of useful work time
}
```

### Metric Calculations

#### From Existing Metrics Object:

```rust
// Storage metrics (already have)
let files_processed = metrics.files_processed();
let bytes_read = metrics.bytes_read();
let bytes_written = metrics.bytes_written();

// AI/ML metrics (need to extract)
let batches_processed = metrics.batches_processed(); // Need getter
let samples_per_file = config.dataset.num_samples_per_file.unwrap_or(1);

// Calculate
let total_samples = files_processed * samples_per_file;
let samples_per_batch = config.reader.batch_size.unwrap_or(1);
let total_batches = (total_samples / samples_per_batch as u64);

let samples_per_second = total_samples as f64 / duration_s;
let batches_per_second = total_batches as f64 / duration_s;

// Get timing breakdowns
let data_loading_time_s = metrics.total_read_time().as_secs_f64();
let compute_time_s = metrics.total_compute_time().as_secs_f64();
```

## Implementation Plan

### 1. Update Protobuf (bench.proto)
Add new fields to WorkloadSummary message

### 2. Add Metrics Getters
Add to `crates/core/src/metrics.rs`:
```rust
pub fn batches_processed(&self) -> u64
pub fn total_read_time(&self) -> Duration
pub fn total_compute_time(&self) -> Duration
pub fn batch_times(&self) -> Vec<Duration>
pub fn epoch_times(&self) -> Vec<Duration>
```

### 3. Update Agent Calculation
In `crates/core/src/dist/agent.rs`, calculate all metrics from:
- Metrics object
- DlioConfig (for samples_per_file, batch_size)
- Duration measurements

### 4. Update Types
Update `crates/core/src/dist/types.rs` WorkloadResult to include new fields

### 5. Update Aggregation
Update `AggregateResults::from_results()` to aggregate:
- Sum: total_samples, total_batches, total_ops
- Average: samples_per_second, batches_per_second
- Weighted avg: timing metrics

## Example Output

### Before (Storage-Only):
```
Agent host1:50051 - ops/s: 1234.5, MiB/s: 567.8, p50: 12.3ms
```

### After (AI/ML + Storage):
```
Agent host1:50051:
  Training: 15,234 samples/s, 238 batches/s (batch_size=64)
  Storage:  1,234 files/s, 567.8 MiB/s, p50: 12.3ms
  Totals:   976,576 samples, 15,259 batches, 10,000 files in 64.1s
```

## Benefits

1. **AI/ML Users** immediately understand training performance
2. **Storage Users** still get detailed I/O metrics
3. **Both Groups** can correlate training speed with storage performance
4. **Reproducibility** - Full context for comparing runs
5. **Bottleneck Analysis** - See if I/O or compute is limiting

## Compatibility

- Backward compatible: Old controllers ignore new fields
- Forward compatible: New controllers work with old agents (zeros for new fields)
- All existing tests continue to work

## Priority

**HIGH** - This is critical for AI/ML workload validation and comparison with DLIO/MLPerf benchmarks.
