# s3-bench Replay Integration in dl-driver

## Overview

dl-driver integrates with s3-bench to provide operation log replay functionality without duplicating replay logic. This document explains how the integration works and the data flow.

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   dl-driver     │    │  S3BenchReplay   │    │    s3-bench     │
│   CLI           │───▶│   Engine         │───▶│   workload      │
│                 │    │                  │    │   engine        │
└─────────────────┘    └──────────────────┘    └─────────────────┘
        │                       │                       │
        │                       │                       │
        ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Operation Log   │    │ s3-bench Config  │    │ Execution &     │
│ (.jsonl)        │    │ (WeightedOp)     │    │ Metrics         │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## Data Flow

### 1. Input: Operation Log Format
dl-driver reads operation logs in JSONL format:
```json
{"operation": "GET", "file": "s3://bucket/file1.npz", "bytes": 1024, "t_start_ns": 1000000000}
{"operation": "PUT", "file": "/local/file2.npz", "bytes": 2048, "t_start_ns": 1500000000}
```

### 2. Transformation: OpLog → s3-bench Config
The `S3BenchReplayEngine` converts these to s3-bench workload configurations:

```rust
// dl-driver operation log entry
OpLogRec {
    operation: "GET",
    file: "s3://bucket/file1.npz", 
    bytes: 1024,
    t_start_ns: 1000000000
}

// Converts to s3-bench WeightedOp
s3_bench::config::WeightedOp {
    weight: 1,
    spec: s3_bench::config::OpSpec::Get { 
        uri: "s3://bucket/file1.npz" 
    }
}
```

### 3. Execution: s3-bench Workload Engine
The converted config is passed to s3-bench:
```rust
let summary = s3_bench::workload::run(&workload_config).await?;
```

### 4. Results: Metrics Conversion
s3-bench results are converted back to dl-driver format:
```rust
ReplayStats {
    total_operations: summary.total_ops,
    total_bytes: summary.total_bytes,
    wall_seconds: summary.wall_seconds,
    throughput_mbps: calculated_from_summary,
    p50_ms: summary.p50_ms,
    p95_ms: summary.p95_ms,
    p99_ms: summary.p99_ms,
}
```

## Key Components

### S3BenchReplayEngine (`crates/core/src/replay.rs`)
- **Input**: `ReplayConfig` with operation log path, concurrency, duration, path remaps
- **Process**: Parses operation log, groups operations, converts to s3-bench format
- **Output**: `ReplayStats` with execution metrics

### Operation Mapping
- **GET operations**: Mapped to `OpSpec::Get { uri }`
- **PUT operations**: Grouped by bucket/prefix, mapped to `OpSpec::Put { bucket, prefix, object_size }`
- **Path remapping**: Applied before conversion to handle cross-environment replay

### Configuration
```rust
ReplayConfig {
    op_log_path: String,           // Path to .jsonl operation log
    fast_mode: bool,               // Ignore timing delays
    duration: Duration,            // How long to run the workload
    concurrency: usize,            // Concurrent workers
    path_remaps: HashMap<String, String>,  // Path transformations
}
```

## Supported Storage Backends

The integration works with any storage backend that s3-bench supports:
- **S3**: `s3://bucket/path`
- **File**: `file:///local/path`  
- **Direct I/O**: `direct:///local/path`
- **Azure**: `az://container/path` (via s3-bench's backend support)

## CLI Usage

```bash
# Basic replay
dl-driver replay --oplog operations.jsonl --workers 4 --timeout 60

# With path remapping for cross-environment replay
dl-driver replay --oplog operations.jsonl --workers 4 --timeout 60 --remap mappings.json

# Fast mode (ignore timing delays)
dl-driver replay --oplog operations.jsonl --workers 4 --timeout 60 --fast

# Export metrics
dl-driver replay --oplog operations.jsonl --workers 4 --timeout 60 --metrics results.json
```

## Benefits of Integration

1. **No Code Duplication**: Leverage s3-bench's mature workload engine
2. **Consistent Metrics**: Same performance measurement across tools  
3. **Backend Support**: Automatic support for all s3-bench storage backends
4. **Proven Reliability**: s3-bench has battle-tested replay logic
5. **Focused Development**: dl-driver can focus on DLIO compatibility

## Limitations

- Currently only supports GET and PUT operations
- Requires s3-bench's operation format constraints
- Some dl-driver specific features may need adaptation

## Dependency Management

s3-bench is included as a GitHub dependency:
```toml
[dependencies]
s3-bench = { git = "https://github.com/russfellows/s3-bench.git", branch = "main" }
```

This ensures we get the latest replay capabilities without version lag.