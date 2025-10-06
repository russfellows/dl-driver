# Phase 2: Streaming Replay Infrastructure (COMPLETED - SIMULATION ONLY)

> ⚠️ **IMPORTANT NOTE**: This infrastructure is **NOT OPERATIONAL** for real I/O execution.
> All replay operations are currently **simulated** via `simulate_operation()` which only adds
> minimal delays without executing actual storage operations.
>
> **For real I/O replay**, use **sai3-bench** (https://github.com/russfellows/sai3-bench)
> which provides production-grade replay with actual ObjectStore operations, advanced remapping,
> microsecond timing precision, and comprehensive metrics.
>
> This module serves as a **stub infrastructure** for potential future integration with
> sai3-bench's replay engine. See `docs/REPLAY_ANALYSIS.md` for full rationale.

**Date:** October 3, 2025  
**Status:** Completed (Infrastructure Only)  
**Goal:** Streaming architecture for operation log processing (foundation for future real I/O)

## 🎯 Overview

This document describes the streaming replay **infrastructure** implemented in v0.7.1.
The implementation successfully provides:

1. **Streaming reads** - Iterator-based, constant memory usage via s3dlio-oplog
2. **Background decompression** - Separate thread for zstd decompression
3. **1MB chunk buffering** - Efficient I/O with configurable chunks
4. **Multi-format support** - JSONL and TSV with automatic detection
5. **Timing control** - Maintain inter-arrival delays or fast mode
6. **URI remapping** - Cross-environment path translation

However, **all operations are simulated** - no actual I/O is executed.

## 🔍 Current Problems

### Memory Issues in `crates/core/src/replay.rs`
```rust
// Line 186-188: Loads ENTIRE file into memory!
let reader = OpLogReader::from_file(&self.config.op_log_path)?;
let parsed_log = reader.records();
self.stats.total_operations = parsed_log.len();  // Requires full file in memory

// Lines 192-197: Builds Vec<ReplayOperation> in memory before execution
for rec in parsed_log {
    let replay_op = ReplayOperation::from_op_log_rec(rec, &self.config, prev_timestamp_ns);
    replay_ops.push(replay_op);  // Accumulates entire workload!
    prev_timestamp_ns = rec.t_start_ns;
}

// Only then starts execution
if self.config.concurrency > 1 {
    self.execute_concurrent(replay_ops).await?;
```

**Problems:**
- 10GB op-log → 10GB+ memory usage
- Must wait for entire file to parse before starting
- No streaming, no pipeline parallelism
- Doesn't leverage background decompression

## ✅ s3dlio-oplog Capabilities

### OpLogStreamReader (from s3dlio/crates/s3dlio-oplog/src/reader.rs)

```rust
/// Streaming op-log reader with background decompression and constant memory usage
/// 
/// This reader processes op-log files in 1MB chunks using a background thread for decompression,
/// providing constant memory usage regardless of file size. Entries are streamed via an iterator
/// rather than loaded all at once.
pub struct OpLogStreamReader {
    receiver: Receiver<Result<OpLogEntry>>,
    _background_handle: Option<JoinHandle<()>>,
}

impl OpLogStreamReader {
    /// Create a streaming reader from a file path
    /// 
    /// This spawns a background thread for decompression and parsing.
    /// Environment variables:
    /// - S3DLIO_OPLOG_READ_BUF: Channel buffer size (default: 1024 entries)
    /// - S3DLIO_OPLOG_CHUNK_SIZE: Read chunk size (default: 1MB)
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self>
}

// Iterator implementation for streaming
impl Iterator for OpLogStreamReader {
    type Item = Result<OpLogEntry>;
    fn next(&mut self) -> Option<Self::Item>
}
```

**Key features:**
- ✅ **Background thread** spawned automatically for decompression
- ✅ **Constant memory** - only buffers 1024 entries by default (configurable)
- ✅ **1MB chunks** - efficient disk I/O
- ✅ **Automatic format detection** - JSONL, TSV, CSV
- ✅ **Zstd support** - automatic decompression for `.zst` files
- ✅ **Iterator-based** - `for entry in stream { ... }`

### OpLogEntry Structure

```rust
pub struct OpLogEntry {
    pub timestamp_ns: u64,
    pub op: OpType,
    pub uri: String,
    pub bytes: Option<usize>,
    pub status: Option<String>,
    pub latency_us: Option<u64>,
}

pub enum OpType {
    GET,
    PUT,
    DELETE,
    LIST,
    STAT,
}
```

## 🔄 Migration Plan

### Step 1: Add Conversion Functions

Create `OpLogEntry` ↔ `ReplayOperation` converters:

```rust
// In crates/core/src/replay.rs

impl ReplayOperation {
    /// Convert s3dlio-oplog OpLogEntry to ReplayOperation with remapping
    pub fn from_oplog_entry(
        entry: &s3dlio_oplog::OpLogEntry,
        config: &ReplayConfig,
        prev_timestamp_ns: Option<u64>,
    ) -> Self {
        // Calculate delay from previous operation
        let delay_ms = if config.fast_mode {
            None
        } else if let Some(prev_ns) = prev_timestamp_ns {
            let delay_ns = entry.timestamp_ns.saturating_sub(prev_ns);
            Some(delay_ns / 1_000_000) // Convert to milliseconds
        } else {
            None
        };

        // Apply path/endpoint remapping to URI
        let mut uri = entry.uri.clone();
        for (from, to) in &config.path_remaps {
            if uri.contains(from) {
                uri = uri.replace(from, to);
            }
        }
        for (from, to) in &config.endpoint_remaps {
            if uri.contains(from) {
                uri = uri.replace(from, to);
            }
        }

        // Construct complete URI if base_uri provided
        let complete_uri = if !config.base_uri.is_empty() {
            Self::construct_complete_uri(&config.base_uri, &uri)
        } else {
            uri
        };

        ReplayOperation {
            operation_type: format!("{:?}", entry.op), // GET, PUT, DELETE, etc.
            path: complete_uri,
            bytes: entry.bytes.map(|b| b as u64),
            delay_ms,
        }
    }
}
```

### Step 2: Replace OpLogReader with OpLogStreamReader

```rust
// In SimpleReplayEngine::run_replay()

pub async fn run_replay(&mut self) -> Result<ReplayStats> {
    info!("Starting streaming replay of {}", self.config.op_log_path);
    self.stats.start_time = Some(Instant::now());

    // Create streaming reader - spawns background decompression thread
    let stream = s3dlio_oplog::OpLogStreamReader::from_file(&self.config.op_log_path)
        .context("Failed to open operation log stream")?;

    info!("Opened streaming reader with background decompression");

    // Execute operations directly from stream (no buffering!)
    if self.config.concurrency > 1 {
        self.execute_concurrent_streaming(stream).await?;
    } else {
        self.execute_sequential_streaming(stream).await?;
    }

    self.stats.end_time = Some(Instant::now());

    info!(
        "Streaming replay completed: {}/{} operations ({} failed), {:.2} ops/sec, {:.2} MB/s",
        self.stats.completed_operations,
        self.stats.total_operations,
        self.stats.failed_operations,
        self.stats.operations_per_second(),
        self.stats.throughput_mbps()
    );

    Ok(self.stats.clone())
}
```

### Step 3: Streaming Sequential Execution

```rust
async fn execute_sequential_streaming(
    &mut self,
    stream: s3dlio_oplog::OpLogStreamReader,
) -> Result<()> {
    let mut prev_timestamp_ns = None;

    for entry_result in stream {
        let entry = entry_result.context("Failed to read entry from stream")?;
        
        self.stats.total_operations += 1;

        // Convert to replay operation
        let op = ReplayOperation::from_oplog_entry(&entry, &self.config, prev_timestamp_ns);
        prev_timestamp_ns = Some(entry.timestamp_ns);

        // Apply timing delay
        if let Some(delay_ms) = op.delay_ms {
            if delay_ms > 0 {
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        // Execute the operation
        if let Err(e) = self.execute_operation(&op).await {
            warn!("Operation failed: {}", e);
            self.stats.failed_operations += 1;
            if !self.config.continue_on_error {
                return Err(e);
            }
        }
    }

    Ok(())
}
```

### Step 4: Streaming Concurrent Execution

```rust
async fn execute_concurrent_streaming(
    &mut self,
    stream: s3dlio_oplog::OpLogStreamReader,
) -> Result<()> {
    use tokio::sync::Semaphore;
    use std::sync::Arc;

    let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
    let stats = Arc::new(tokio::sync::Mutex::new(ReplayStats::default()));
    let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
    
    let mut tasks = Vec::new();
    let mut prev_timestamp_ns = None;

    // Stream entries and spawn tasks without buffering full workload
    for entry_result in stream {
        let entry = entry_result.context("Failed to read entry from stream")?;
        
        {
            let mut stats_guard = stats.lock().await;
            stats_guard.total_operations += 1;
        }

        // Convert to replay operation
        let op = ReplayOperation::from_oplog_entry(&entry, &self.config, prev_timestamp_ns);
        prev_timestamp_ns = Some(entry.timestamp_ns);

        let sem = semaphore.clone();
        let stats_ref = stats.clone();
        let continue_on_error = self.config.continue_on_error;
        
        let task = tokio::spawn(async move {
            // Apply timing delay before acquiring semaphore
            if let Some(delay_ms) = op.delay_ms {
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }

            // Acquire concurrency permit
            let _permit = sem.acquire().await.unwrap();

            // Execute operation with timeout
            let result = tokio::time::timeout(
                timeout_duration,
                Self::simulate_operation(&op)
            ).await;
            
            // Update stats
            {
                let mut stats_guard = stats_ref.lock().await;
                match result {
                    Ok(Ok(())) => {
                        stats_guard.completed_operations += 1;
                        stats_guard.total_bytes += op.bytes.unwrap_or(0);
                    }
                    Ok(Err(_)) | Err(_) => {
                        stats_guard.failed_operations += 1;
                    }
                }
            }

            // Return error if not continuing on error
            if !continue_on_error {
                match result {
                    Ok(r) => r,
                    Err(_) => anyhow::bail!("Operation timed out"),
                }
            } else {
                Ok(())
            }
        });

        tasks.push(task);
        
        // Optional: limit in-flight tasks to prevent unbounded memory
        if tasks.len() >= 10_000 {
            // Wait for some tasks to complete
            let (completed, remaining): (Vec<_>, Vec<_>) = 
                tasks.into_iter().partition(|t| t.is_finished());
            
            for task in completed {
                let _ = task.await?;
            }
            tasks = remaining;
        }
    }

    // Wait for remaining tasks
    for task in tasks {
        let _ = task.await?;
    }

    // Copy final stats
    let final_stats = stats.lock().await;
    self.stats.completed_operations = final_stats.completed_operations;
    self.stats.failed_operations = final_stats.failed_operations;
    self.stats.total_bytes = final_stats.total_bytes;
    self.stats.total_operations = final_stats.total_operations;

    Ok(())
}
```

### Step 5: Add continue_on_error to Config

```rust
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub op_log_path: String,
    pub base_uri: String,
    pub concurrency: usize,
    pub fast_mode: bool,
    pub timeout_seconds: u64,
    pub path_remaps: HashMap<String, String>,
    pub endpoint_remaps: HashMap<String, String>,
    pub continue_on_error: bool,  // NEW: Continue even if operations fail
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            // ... existing fields ...
            continue_on_error: true,  // Default: keep going on errors
        }
    }
}
```

## 🎯 Benefits

### Before (Current)
```
10GB op-log.tsv.zst
    ↓
Load entire file (10GB+ memory)
    ↓
Build Vec<ReplayOperation> (10GB+ memory)
    ↓
Start execution
```
**Memory:** 20GB+  
**Latency:** Must parse entire file first  
**Decompression:** Single-threaded, blocks parsing

### After (Streaming)
```
10GB op-log.tsv.zst
    ↓
Background thread: decompress 1MB chunks
    ↓
Stream entries via channel (1024 entry buffer)
    ↓
Execute as they arrive (concurrent)
```
**Memory:** ~10MB constant  
**Latency:** Start executing immediately  
**Decompression:** Parallel with execution, separate thread

## 📊 Performance Tuning

Users can tune via environment variables:

```bash
# Increase channel buffer for high-throughput scenarios
export S3DLIO_OPLOG_READ_BUF=8192

# Increase chunk size for large sequential reads
export S3DLIO_OPLOG_CHUNK_SIZE=4194304  # 4MB

# Then run replay
dl-driver replay --oplog large.tsv.zst --workers 64
```

## 🧪 Testing Plan

1. **Small files** - Verify correctness with existing tests
2. **Large files** - Create 1GB+ test op-log, verify constant memory
3. **Compressed files** - Test `.zst` decompression in background
4. **Multiple formats** - Test JSONL, TSV, CSV
5. **Concurrent execution** - Verify no race conditions with streaming
6. **Error handling** - Test stream errors, partial failures

## 📝 Implementation Checklist

- [ ] Add `s3dlio_oplog` import to `crates/core/src/replay.rs`
- [ ] Create `ReplayOperation::from_oplog_entry()` converter
- [ ] Replace `OpLogReader` with `OpLogStreamReader` in `run_replay()`
- [ ] Implement `execute_sequential_streaming()`
- [ ] Implement `execute_concurrent_streaming()` with task limiting
- [ ] Add `continue_on_error` field to `ReplayConfig`
- [ ] Update tests for streaming behavior
- [ ] Add memory profiling test for large files
- [ ] Update CLI to expose `continue_on_error` flag
- [ ] Update documentation

## 🔗 Related Files

- `crates/core/src/replay.rs` - Main changes
- `crates/core/Cargo.toml` - Already has s3dlio-oplog dependency
- `crates/cli/src/main.rs` - CLI replay command updates (Phase 3)
- `s3dlio/crates/s3dlio-oplog/src/reader.rs` - Streaming reader implementation

## 🚀 Next Steps After Phase 2

**Phase 3:** Update CLI replay command to use streaming and expose tuning options  
**Phase 4:** Comprehensive testing with large op-logs  
**Phase 5:** Performance benchmarking and documentation
