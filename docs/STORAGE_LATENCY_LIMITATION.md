# Storage Latency Metrics Limitation (v0.8.8)

## Overview

**Current Status**: Storage latency metrics (GET/PUT latency) report `0.0µs` in v0.8.8 because they are **not yet accurately instrumented**.

**Throughput metrics remain accurate** and provide valid performance measurements.

## Why Latency is Not Available

dl-driver uses `s3dlio::AsyncPoolDataLoader` for high-performance data loading with prefetch. This architecture provides excellent throughput but currently hides individual file I/O latencies:

### Architecture Diagram

```
Background Workers (s3dlio)     Main Thread (dl-driver)
─────────────────────────       ──────────────────────
                                
store.get(file1) [150ms] ──┐
store.get(file2) [180ms] ──┼──> Channel ──> batch = recv() [3µs]
store.get(file3) [120ms] ──┘                      │
                                                   v
                           ❌ ONLY MEASURE THIS (channel recv time)
                           ✅ NEED TO MEASURE THIS (actual store.get time)
```

### What We Currently Measure

- **Channel receive time**: ~0-3µs (reading from in-memory channel)
- This is **memory access time**, not storage I/O time

### What We Should Measure

- **Actual storage operation time**: 50-200ms for 20MB files with `direct://`
- This happens in s3dlio background workers and is not exposed

## Impact

### Still Accurate ✅
- **Throughput**: ops/s, MiB/s, total bytes
- **Operation counts**: Total GETs, PUTs, operations
- **Training metrics**: Samples/s, batch time, AU%, pipeline efficiency
- **Overall performance**: Wall-clock time, epochs completed

### Not Available ⚠️
- **Storage latency percentiles**: p50, p90, p95, p99
- **Mean latency**: Average I/O operation time
- **Latency distributions**: Cannot analyze tail latencies

## Workaround for v0.8.8

If you need storage latency metrics, use **sai3-bench** (sister project) which directly instruments storage operations:

```bash
# sai3-bench provides accurate storage latency metrics
./sai3-bench run --config myworkload.yaml

# Output includes real storage latencies:
#   GET Latency: mean=150ms, p50=140ms, p95=250ms, p99=400ms
```

## Planned Fix for v0.8.9

### Upstream s3dlio Enhancement (Option 2)

Add timing instrumentation and metrics API to `AsyncPoolDataLoader`:

1. **Instrument background workers** in `s3dlio/src/data_loader/async_pool_dataloader.rs`:
   ```rust
   let fut: RequestFuture = Box::pin(async move {
       let start = Instant::now();  // ← Add timing
       let result = match tokio::time::timeout(timeout, store.get(&uri)).await {
           Ok(Ok(data)) => Ok(data.to_vec()),
           Ok(Err(e)) => Err(anyhow::anyhow!("Store error: {}", e)),
           Err(_) => Err(anyhow::anyhow!("Request timeout")),
       };
       let latency = start.elapsed();  // ← Capture latency
       // TODO: Send latency to metrics collector
       (index, result, latency)  // ← Return latency
   });
   ```

2. **Add metrics API** to expose per-file latencies:
   ```rust
   pub struct AsyncPoolDataLoader {
       // ... existing fields ...
       metrics: Arc<DataLoaderMetrics>,  // New
   }
   
   impl AsyncPoolDataLoader {
       pub fn get_metrics(&self) -> DataLoaderMetrics {
           self.metrics.snapshot()
       }
   }
   ```

3. **Wire into dl-driver** `crates/core/src/workload.rs`:
   ```rust
   // Get metrics from dataloader
   let loader_metrics = batch_stream.get_metrics();
   tracker.record_get_batch(
       batch_size_actual as u64,
       batch_bytes,
       loader_metrics.mean_latency  // ← Real I/O latency!
   );
   ```

### Implementation Timeline

- **v0.8.9 target**: Q1 2026
- **Requires**: s3dlio v0.10.0 with metrics API
- **Tracking**: 
  - dl-driver issue #[TBD] 
  - s3dlio issue #[TBD]

## Related Issues

- **dl-driver**: [Issue #TBD] - Add storage latency instrumentation
- **s3dlio**: [Issue #TBD] - Expose per-file latency metrics from AsyncPoolDataLoader
- **Reference**: sai3-bench implements correct pattern in `src/workload.rs:1261-1277`

## For Developers

If you're implementing the fix:

1. Start with s3dlio enhancement (expose metrics)
2. Update dl-driver to consume new metrics API
3. Test with `direct://` backend on real disk (not tmpfs)
4. Verify latencies match expected I/O performance (50-200ms for 20MB files)
5. Compare with sai3-bench baseline for same workload

## Questions?

See:
- dl-driver Bug #8 investigation in git history
- sai3-bench reference implementation: `src/workload.rs:1261-1277`
- Cross-project research in `.github/CURRENT_WORK_STATUS.md`
