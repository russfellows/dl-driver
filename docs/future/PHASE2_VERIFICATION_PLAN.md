# Phase 2 Multi-Rank Storage Latency Verification Plan

## Current Status: UNVERIFIED ⚠️

**Date**: November 13, 2025  
**Version**: v0.8.8 (feature branch)

## Problem Statement

Phase 2 multi-rank implementation shows `GET Latency: mean=0µs` after fixing Bug #8 (was incorrectly showing 196ms when including compute time). While the fix is logically correct (recording `io_time` instead of `batch_total_time`), **we cannot verify correctness** with current test conditions:

### Why Current Tests Are Inconclusive

1. **Page Cache Masking**: Small files (64KB NPZ) fit entirely in page cache
2. **tmpfs Storage**: `/tmp` is memory-backed (tmpfs), not real disk I/O
3. **Prefetch Pipeline**: Background workers load data → main thread sees memory access only
4. **Zero Latency**: Reported 0µs is plausible for prefetched+cached data, BUT masks whether we're measuring correctly

### The Core Question

Is `io_time` measuring:
- ✅ **Correct**: Time to access already-prefetched data (microseconds, essentially memory speed)
- ❌ **Wrong**: Something else entirely, and we're just getting lucky with 0µs

## Verification Strategy

To prove the latency measurement is correct, we need test conditions where:
1. **I/O is SLOW and MEASURABLE** (not cached/prefetched away)
2. **Real disk latency** is observable (not tmpfs)
3. **Dataset exceeds RAM** to prevent full page caching

### Test Configuration Requirements

#### Storage Backend
```yaml
data_folder: "direct:///mnt/test/dl-driver-latency-verify/"
# Use direct:// to bypass page cache entirely
# Use /mnt/test (real disk), NOT /tmp (tmpfs)
```

#### Dataset Size
```yaml
dataset:
  num_files_train: 500  # Scale up for real I/O
  record_length: 10485760  # 10MB per file (not 64KB)
  # Total: 500 files × 10MB = 5GB dataset
  # Ensure this exceeds available page cache (test on system with <8GB free RAM)
```

#### Prefetch Configuration
```yaml
train:
  prefetch_size: 0  # DISABLE prefetch to see raw I/O latency first
  read_threads: 1   # Single-threaded to simplify measurement
  computation_time: 0.0  # Remove compute to isolate I/O
```

#### Expected Results

| Condition | Expected GET Latency | Why |
|-----------|---------------------|-----|
| **No prefetch, direct://, 10MB files** | 5-50ms | Real disk random read latency |
| **With prefetch=4, background loading** | <1ms | Data ready before main thread needs it |
| **file://, page cached** | <100µs | Cached data from previous read |
| **tmpfs (/tmp)** | <10µs | Memory-speed access |

### Test Phases

#### Phase A: Baseline (No Prefetch, Direct I/O)
**Goal**: See raw disk latency to validate measurement is working

```bash
# Config: prefetch_size=0, direct:///mnt/test, computation_time=0
# Expected: GET latency = 10-50ms (disk read time)
```

If this shows **0µs**, measurement is WRONG.  
If this shows **10-50ms**, measurement is CORRECT.

#### Phase B: Prefetch Enabled
**Goal**: Verify prefetch pipeline hides I/O latency

```bash
# Config: prefetch_size=4, direct:///mnt/test, computation_time=0
# Expected: GET latency = <1ms (prefetched data ready)
```

Should be much lower than Phase A.

#### Phase C: With Computation
**Goal**: Ensure compute time NOT included in I/O latency

```bash
# Config: prefetch_size=4, direct:///mnt/test, computation_time=0.195
# Expected: 
#   - GET latency = <1ms (same as Phase B)
#   - Batch time = ~195ms (includes compute)
```

GET latency should NOT change when compute is added.

## Test Execution Checklist

- [ ] **Prepare /mnt/test directory** (verify it's on real disk, not tmpfs)
- [ ] **Create phase2_verify_latency_no_prefetch.yaml** (Phase A config)
- [ ] **Create phase2_verify_latency_with_prefetch.yaml** (Phase B config)
- [ ] **Create phase2_verify_latency_with_compute.yaml** (Phase C config)
- [ ] **Generate large dataset** (500 files × 10MB = 5GB)
- [ ] **Run Phase A** and verify non-zero disk latency
- [ ] **Run Phase B** and verify prefetch reduces latency
- [ ] **Run Phase C** and verify compute doesn't affect I/O latency
- [ ] **Document actual results** vs expected ranges
- [ ] **Update code comments** based on findings

## Current Test Limitations

The `scripts/test_multi_rank_v0.8.8_phase2.sh` test:
- Uses `/tmp/dl-driver-phase2-test` (tmpfs, memory-backed)
- Uses 40 files × 64KB = 2.5MB total (fits in L3 cache!)
- Uses `file://` backend (page cache enabled)
- Shows 0µs latency (expected for cached+prefetched, but unverifiable)

**Conclusion**: Current test proves nothing about correctness of I/O latency measurement.

## Success Criteria

✅ **Verification PASSED** if:
1. Phase A (no prefetch, direct I/O) shows measurable latency (5-50ms range)
2. Phase B (with prefetch) shows lower latency than Phase A
3. Phase C (with compute) shows same I/O latency as Phase B, but higher batch time
4. Latency values align with known disk performance characteristics

❌ **Verification FAILED** if:
- Phase A shows 0µs (measurement not capturing disk I/O)
- Phases have unexpected relationships
- Values don't make physical sense

## Related Issues

- **Bug #8**: Fixed recording of `batch_total_time` → `io_time` (logically correct, but unverified)
- **Bug #4**: Previous latency measurement used `file_op_duration` (since removed)
- **Prefetch Pipeline**: Background workers + tokio channels hide I/O from main thread

## References

- Code: `crates/core/src/workload.rs:689` (record_get_batch call)
- Test: `scripts/test_multi_rank_v0.8.8_phase2.sh`
- Configs: `tests/phase2_distributed_read.yaml`
