# Dual Metrics Reporting: Storage vs AI/ML Perspectives

## Overview

dl-driver now provides **two separate TSV output formats** to serve different audiences and use cases:

1. **Storage Metrics TSV** - For storage engineers and I/O performance analysis
2. **AI/ML Training Metrics TSV** - For ML engineers and training pipeline optimization

This separation keeps concerns clear and makes it easy to analyze performance from either perspective without mixing unrelated metrics.

---

## Storage Metrics TSV

**Method:** `AggregateResults::to_storage_tsv()`

**Purpose:** Traditional storage performance metrics focused on I/O operations and throughput.

**Columns:**
```
agent_id    ops_s    mib_s    p50_ms    p90_ms    p95_ms    p99_ms    errors    total_ops    duration_s
```

**Metrics:**
- `ops_s` - Operations (files) per second
- `mib_s` - Megabytes per second throughput
- `p50_ms`, `p90_ms`, `p95_ms`, `p99_ms` - Latency percentiles in milliseconds
- `errors` - Count of failed operations
- `total_ops` - Total number of operations (files processed)
- `duration_s` - Total execution time in seconds

**Use Cases:**
- Benchmarking storage backend performance (S3, Azure, DirectIO, File)
- Comparing I/O throughput across different configurations
- Identifying latency bottlenecks
- Storage capacity planning

**Example:**
```
agent_id    ops_s     mib_s     p50_ms  p90_ms  p95_ms  p99_ms  errors  total_ops  duration_s
agent-0     1000.0    500.0     10.00   20.00   25.00   30.00   0       10000      10.00
agent-1     1200.0    600.0     12.00   22.00   27.00   32.00   1       12000      10.00
AGGREGATE   2200.0    1100.0    11.00   21.00   26.00   31.00   1       22000      -
```

---

## AI/ML Training Metrics TSV

**Method:** `AggregateResults::to_aiml_tsv()`

**Purpose:** AI/ML training pipeline metrics focused on samples, batches, and epochs.

**Columns:**
```
agent_id    samples_s    total_samples    batches_s    total_batches    samples_per_batch    avg_batch_ms    epochs    avg_epoch_s    data_load_s    compute_s    pipeline_eff
```

**Metrics:**
- `samples_s` - Training samples processed per second
- `total_samples` - Total number of samples (files × samples_per_file)
- `batches_s` - Training batches processed per second
- `total_batches` - Total number of batches (samples ÷ batch_size)
- `samples_per_batch` - Batch size configuration
- `avg_batch_ms` - Average time to process one batch in milliseconds
- `epochs` - Number of training epochs completed
- `avg_epoch_s` - Average time per epoch in seconds
- `data_load_s` - Time spent loading data
- `compute_s` - Time spent in compute/processing
- `pipeline_eff` - Pipeline efficiency ratio (0.0-1.0)

**Use Cases:**
- Optimizing AI/ML training pipeline throughput
- Comparing samples/s across different batch sizes
- Identifying data loading vs compute bottlenecks
- Measuring training velocity for different models (ResNet, UNet3D, BERT, etc.)

**Example:**
```
agent_id    samples_s    total_samples    batches_s    total_batches    samples_per_batch    avg_batch_ms    epochs    avg_epoch_s    data_load_s    compute_s    pipeline_eff
agent-0     5000.0       50000            78.1         781              64                   12.80           1         10.00          6.00           3.50         0.950
agent-1     6000.0       60000            93.8         938              64                   10.70           1         10.00          5.50           4.00         0.950
AGGREGATE   11000.0      110000           171.9        1719             -                    11.75           2         10.00          5.75           3.75         0.950
```

---

## Metrics Calculation

### Storage Metrics
Derived from `WorkloadRunner` metrics:
```rust
let files_processed = metrics.files_processed();
let bytes_read = metrics.bytes_read();
let bytes_written = metrics.bytes_written();

let ops_per_s = files_processed / duration_s;
let mib_per_s = (bytes_read + bytes_written) / (1024² × duration_s);
```

### AI/ML Metrics
Calculated from DLIO config and runtime data:
```rust
let samples_per_file = config.dataset.num_samples_per_file;
let batch_size = config.reader.batch_size;

let total_samples = files_processed × samples_per_file;
let total_batches = ⌈total_samples / batch_size⌉;

let samples_per_second = total_samples / duration_s;
let batches_per_second = total_batches / duration_s;

let data_loading_time_s = metrics.total_read_time();
let compute_time_s = metrics.total_compute_time();
let pipeline_efficiency = (data_loading_time_s + compute_time_s) / duration_s;
```

---

## Usage in Distributed Execution

When the **controller** aggregates results from multiple agents, it will generate both TSV files:

```bash
# Controller writes two separate files:
results_storage.tsv    # Storage I/O perspective
results_aiml.tsv       # AI/ML training perspective
```

Each file contains:
- One row per agent with individual metrics
- One `AGGREGATE` row with totals/averages across all agents

---

## Backward Compatibility

The legacy `to_tsv()` method is maintained as an alias to `to_storage_tsv()` for backward compatibility with existing tools and scripts.

---

## Key Design Principles

1. **Separation of Concerns**: Storage and AI/ML metrics serve different audiences
2. **Clarity**: Each TSV file focuses on one domain without mixing unrelated metrics
3. **Completeness**: All relevant metrics for each perspective are included
4. **Aggregation**: Both per-agent and aggregate statistics are provided
5. **Simplicity**: Standard TSV format for easy parsing and analysis

---

## Future Enhancements

Potential additions:
- JSON output format for programmatic consumption
- Prometheus/OpenMetrics export for monitoring dashboards
- Per-epoch breakdowns in AI/ML metrics
- GPU utilization metrics (when applicable)
- Network bandwidth metrics for distributed training
