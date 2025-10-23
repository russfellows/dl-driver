# Results Directory Format & Histogram Aggregation

## Overview

dl-driver v0.8.1 introduces comprehensive results directory support for distributed workloads, featuring **accurate histogram-based percentile aggregation**. This ensures statistically correct performance metrics even when agents have unbalanced workloads.

## Why Histogram Aggregation Matters

### The Problem with Naive Averaging

When aggregating latency percentiles (p50, p90, p95, p99) from multiple agents, **simply averaging the percentiles produces incorrect results**:

```
Agent 1: 1,000 operations, p50 = 10ms
Agent 2: 100 operations, p50 = 100ms

Naive average: (10ms + 100ms) / 2 = 55ms  ❌ WRONG!
Correct p50: ~13ms (weighted toward Agent 1)  ✅ CORRECT!
```

The error can exceed **50%** for unbalanced workloads. See `crates/cli/tests/histogram_integration_test.rs::test_aggregate_results_with_histograms()` for a concrete example.

### The Solution: HDR Histogram Merging

dl-driver uses [HDR Histogram](https://github.com/HdrHistogram/HdrHistogram_rust) to:
1. Track latency distributions per agent (not just percentiles)
2. Serialize histograms with V2 deflate compression (10-50x reduction)
3. Transport histogram data via gRPC protobuf
4. Merge histograms from all agents before calculating percentiles
5. Produce statistically correct aggregate percentiles

## Results Directory Structure

When running a distributed workload with `run_distributed_with_results()`, dl-driver creates:

```
dlio-YYYYMMDD-HHMM-{test_name}/
├── config.yaml                  # Copy of input DLIO config
├── console.log                  # Real-time execution log
├── metadata.json                # Run metadata (see below)
├── storage_results.tsv          # Consolidated storage metrics (see below)
├── aiml_results.tsv            # Consolidated AI/ML metrics (see below)
└── agents/                      # Per-agent results
    ├── agent-0/
    │   ├── storage_results.tsv  # Agent 0 storage metrics
    │   ├── aiml_results.tsv     # Agent 0 AI/ML metrics
    │   └── metadata.json        # Agent 0 metadata
    ├── agent-1/
    │   └── ...
    └── agent-N/
        └── ...
```

### Directory Naming Convention

- **Format:** `dlio-YYYYMMDD-HHMM-{test_name}`
- **Example:** `dlio-20251022-1430-unet3d_config`
- **test_name:** Extracted from config filename or specified via `custom_name` parameter

## File Formats

### metadata.json

Top-level run metadata:

```json
{
  "version": "0.8.1",
  "test_name": "unet3d_config",
  "config_path": "/path/to/config.yaml",
  "start_time": "2025-10-22T14:30:00Z",
  "end_time": "2025-10-22T14:35:30Z",
  "duration_secs": 330.5,
  "command_line": ["dl-driver", "dlio", "--config", "..."],
  "hostname": "controller-node",
  "distributed": true,
  "agents": ["agent-0 (192.168.1.10:50051)", "agent-1 (192.168.1.11:50051)"],
  "total_agents": 2,
  "successful_agents": 2
}
```

### storage_results.tsv

Consolidated storage performance metrics with **histogram-based percentiles**:

```tsv
agent_id    ops_s   mib_s   p50_ms  p90_ms  p95_ms  p99_ms  errors  total_ops   duration_s
agent-0     1000.0  500.0   10.50   20.30   25.10   30.80   0       10000       10.00
agent-1     1200.0  600.0   12.10   22.50   27.40   32.60   1       12000       10.00
AGGREGATE   2200.0  1100.0  11.23   21.35   26.18   31.65   1       22000       -
```

**Column Descriptions:**
- `agent_id`: Agent identifier (or "AGGREGATE" for merged results)
- `ops_s`: Operations per second (throughput)
- `mib_s`: MiB/s (bandwidth)
- `p50_ms`, `p90_ms`, `p95_ms`, `p99_ms`: Latency percentiles in milliseconds (histogram-based)
- `errors`: Number of failed operations
- `total_ops`: Total operations completed
- `duration_s`: Workload duration in seconds

**Key Point:** The AGGREGATE row shows **correctly merged percentiles** calculated from combined histograms, not naive averages.

### aiml_results.tsv

Consolidated AI/ML training metrics:

```tsv
agent_id    samples_s   total_samples   batches_s   total_batches   samples_per_batch   avg_batch_ms    epochs  avg_epoch_s data_load_s compute_s   pipeline_eff
agent-0     5000.0      50000           78.1        781             64                  12.8            1       10.0        6.0         3.5         0.950
agent-1     6000.0      60000           93.8        938             64                  10.7            1       10.0        5.5         4.0         0.950
AGGREGATE   11000.0     110000          171.9       1719            -                   11.8            2       10.0        5.8         3.8         0.950
```

**Column Descriptions:**
- `samples_s`: Samples processed per second (training throughput)
- `total_samples`: Total samples processed
- `batches_s`: Batches per second
- `total_batches`: Total batches processed
- `samples_per_batch`: Samples per batch (consistent across agents)
- `avg_batch_ms`: Average batch processing time (milliseconds)
- `epochs`: Number of epochs completed
- `avg_epoch_s`: Average epoch time (seconds)
- `data_load_s`: Time spent loading data (seconds)
- `compute_s`: Time spent in computation (seconds)
- `pipeline_eff`: Pipeline efficiency (ratio of useful work)

### Per-Agent Results

Each `agents/agent-X/` subdirectory contains:
- **storage_results.tsv**: Single-row TSV with agent's storage metrics
- **aiml_results.tsv**: Single-row TSV with agent's AI/ML metrics
- **metadata.json**: Agent-specific metadata (ops, samples, etc.)

Format matches consolidated TSV files (same columns).

## Size-Bucketed Histogram Tracking

dl-driver tracks latency histograms in **9 size buckets** for accurate per-size performance analysis:

| Bucket | Size Range | Example |
|--------|-----------|---------|
| 0 | Zero bytes | Empty files |
| 1 | 1B - 8KiB | Small metadata |
| 2 | 8KiB - 64KiB | Config files |
| 3 | 64KiB - 512KiB | Medium data |
| 4 | 512KiB - 4MiB | Image tiles |
| 5 | 4MiB - 32MiB | Large images |
| 6 | 32MiB - 256MiB | Model checkpoints |
| 7 | 256MiB - 2GiB | Large datasets |
| 8 | > 2GiB | Huge files |

**Benefits:**
- Understand latency characteristics by file size
- Identify performance bottlenecks for specific size ranges
- Accurate throughput calculations using **actual bytes per bucket** (not estimates)

## How to Use

### Running Distributed Workloads with Results

From Rust code:

```rust
use dl_driver_core::dist::controller::{Controller, DistributedConfig};
use dl_driver_core::dlio_compat::DlioConfig;
use std::path::Path;

// Load DLIO config
let config = DlioConfig::from_yaml_file("tests/dlio_configs/unet3d_config.yaml")?;

// Configure distributed execution
let distributed = DistributedConfig {
    agents: vec![
        "192.168.1.10:50051".to_string(),
        "192.168.1.11:50051".to_string(),
    ],
    path_template: "{id}/".to_string(),
    start_delay_ms: 1000,
    request_timeout_ms: 300_000,
    max_retries: 3,
};

let controller = Controller::new(config, distributed);

// Run workload with results directory
let aggregate = controller.run_distributed_with_results(
    Some(Path::new("tests/dlio_configs/unet3d_config.yaml")),
    Some(Path::new("/results")),  // Output directory
).await?;

println!("Results saved to: /results/dlio-YYYYMMDD-HHMM-unet3d_config/");
```

### Analyzing Results

1. **Check metadata.json** for run details (duration, agents, errors)
2. **Review storage_results.tsv** for I/O performance
   - Look at AGGREGATE row for cluster-wide metrics
   - Compare per-agent rows for load balance
3. **Review aiml_results.tsv** for training performance
   - Samples/s indicates training throughput
   - Pipeline efficiency shows data loading efficiency
4. **Inspect per-agent results** in `agents/` for debugging
5. **Correlate with console.log** for execution timeline

### Comparing Runs

Since all results are timestamped and self-contained:
```bash
diff -u \
  dlio-20251022-1430-unet3d_config/storage_results.tsv \
  dlio-20251022-1500-unet3d_config/storage_results.tsv
```

### Exporting to Analysis Tools

TSV format imports directly into:
- **Excel/Sheets:** Open as tab-separated values
- **Pandas:** `pd.read_csv('storage_results.tsv', sep='\t')`
- **R:** `read.delim('storage_results.tsv')`
- **Grafana/Prometheus:** Parse and ingest metrics

## Implementation Details

### Histogram Serialization

- **Format:** V2 deflate compressed (HdrHistogram standard)
- **Compression:** 10-50x size reduction (typical ~2KB per histogram)
- **Transport:** Binary bytes field in protobuf (efficient over gRPC)
- **Parameters:** 1μs to 1 hour range, 3 significant figures

### Percentile Calculation

1. Agent records latencies in HDR histogram (thread-safe, lock-free)
2. Agent serializes histogram using V2DeflateSerializer
3. Agent sends histogram bytes in WorkloadSummary proto
4. Controller deserializes histograms from all agents
5. Controller merges histograms (correct sample weighting)
6. Controller calculates percentiles from merged histogram

### Accuracy Validation

See `crates/cli/tests/histogram_integration_test.rs` for validation:
- **test_aggregate_results_with_histograms()**: Proves >50% error with naive averaging
- **test_histogram_serialization_in_proto()**: Validates proto transport
- **test_empty_histogram_fallback()**: Tests graceful degradation

## API Reference

### ResultsDir

```rust
use dl_driver_core::results_dir::ResultsDir;

// Create results directory
let results_dir = ResultsDir::create(
    config_path: &Path,      // Path to DLIO config
    custom_name: Option<&str>, // Optional custom name
    base_dir: Option<&Path>,   // Output directory (default: ".")
    num_agents: usize,         // Number of agents
)?;

// Write console output
results_dir.write_console("Processing batch 1...")?;

// Create agents subdirectory
let agents_dir = results_dir.create_agents_dir()?;

// Write per-agent results
results_dir.write_agent_results(
    &agents_dir,
    "agent-0",
    storage_tsv: &str,
    aiml_tsv: &str,
    metadata_json: &str,
)?;

// Finalize (writes metadata.json, closes logs)
results_dir.finalize(duration_secs: f64, successful_agents: usize)?;
```

### AggregateResults

```rust
use dl_driver_core::dist::types::AggregateResults;

// Aggregate with histogram merging (correct percentiles)
let aggregate = AggregateResults::from_results_with_histograms(
    results: Vec<WorkloadResult>,
    summaries: &[WorkloadSummary],
)?;

// Export to TSV
let storage_tsv = aggregate.to_storage_tsv();
let aiml_tsv = aggregate.to_aiml_tsv();
```

## Performance Considerations

- **Histogram overhead:** Minimal (<1% CPU, ~10KB memory per histogram)
- **Serialization cost:** ~50μs per histogram (V2 deflate)
- **Network transfer:** ~2KB per histogram (compressed)
- **Merge cost:** O(n) where n = number of agents (typically <10ms for 100 agents)

## Troubleshooting

### Missing Histogram Data

If consolidated percentiles fall back to naive averaging:
- Check agent logs for histogram collection errors
- Verify protobuf WorkloadSummary includes histogram bytes
- Ensure agents use `record_read_with_histogram()` / `record_write_with_histogram()`

### Incorrect Percentiles

If percentiles seem off:
- Verify histogram max_value is large enough (default: 1 hour = 3.6e9 μs)
- Check for histogram overflow (values > max_value are clamped)
- Review per-agent histograms for anomalies

### Large Results Directories

If results directories are too large:
- Histogram compression is already optimal (V2 deflate)
- Consider archiving old results directories
- Reduce console.log verbosity if needed

## Version History

- **v0.8.1:** Initial results directory support with histogram aggregation
- **v0.8.0:** Added distributed controller/agent architecture
- **v0.7.x:** DLIO compatibility and s3dlio integration

## See Also

- [DLIO Documentation](https://github.com/argonne-lcf/dlio_benchmark)
- [HDR Histogram](https://hdrhistogram.github.io/HdrHistogram/)
- [s3dlio Library](https://github.com/russfellows/s3dlio)
- [sai3-bench](https://github.com/russfellows/sai3-bench) (inspiration for results format)
