// v0.8.7: Live stats tracking for distributed execution
//
// This module provides thread-safe tracking of operations during workload execution,
// enabling real-time progress monitoring in distributed environments via gRPC streaming.
// Extends base pattern from sai3-bench with AI/ML samples tracking.

use hdrhistogram::Histogram;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Thread-safe tracker for live workload statistics (storage + AI/ML metrics)
///
/// Uses atomic counters for operation counts/bytes/samples and parking_lot::Mutex<Histogram>
/// for latency tracking. Designed for high-throughput workload execution with
/// minimal performance overhead.
#[derive(Clone)]
pub struct LiveStatsTracker {
    // Operation counters (atomic for thread-safe updates)
    get_ops: Arc<AtomicU64>,
    get_bytes: Arc<AtomicU64>,
    put_ops: Arc<AtomicU64>,
    put_bytes: Arc<AtomicU64>,

    // AI/ML metrics - samples processed during training
    total_samples: Arc<AtomicU64>,

    // Latency histograms (microseconds) - Mutex for snapshot operations
    get_hist: Arc<Mutex<Histogram<u64>>>,
    put_hist: Arc<Mutex<Histogram<u64>>>,

    // Timing
    start_time: Instant,
}

impl LiveStatsTracker {
    /// Create new tracker with histogram configuration matching workload patterns
    ///
    /// Histograms track latencies from 1 microsecond to 1 hour with 3 significant digits.
    pub fn new() -> Self {
        Self {
            get_ops: Arc::new(AtomicU64::new(0)),
            get_bytes: Arc::new(AtomicU64::new(0)),
            put_ops: Arc::new(AtomicU64::new(0)),
            put_bytes: Arc::new(AtomicU64::new(0)),
            total_samples: Arc::new(AtomicU64::new(0)),
            get_hist: Arc::new(Mutex::new(
                Histogram::new_with_bounds(1, 3_600_000_000, 3).unwrap(),
            )),
            put_hist: Arc::new(Mutex::new(
                Histogram::new_with_bounds(1, 3_600_000_000, 3).unwrap(),
            )),
            start_time: Instant::now(),
        }
    }

    /// Record a GET operation (non-blocking atomic update)
    #[inline]
    pub fn record_get(&self, bytes: usize, latency: Duration) {
        self.get_ops.fetch_add(1, Ordering::Relaxed);
        self.get_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        let us = latency.as_micros().min(u64::MAX as u128) as u64;
        // Histogram update requires lock but is fast (O(1) amortized)
        let _ = self.get_hist.lock().record(us);
    }

    /// Record multiple GET operations from a batch (non-blocking atomic update)
    ///
    /// Used when processing multiple files in parallel with a single I/O operation.
    /// Records accurate file operation count while using batch-level latency.
    #[inline]
    pub fn record_get_batch(&self, count: u64, bytes: usize, latency: Duration) {
        self.get_ops.fetch_add(count, Ordering::Relaxed);
        self.get_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        let us = latency.as_micros().min(u64::MAX as u128) as u64;
        tracing::debug!("record_get_batch: count={}, bytes={}, latency={:?}, us={}", 
                       count, bytes, latency, us);
        // Histogram records batch-level latency (single I/O operation)
        let _ = self.get_hist.lock().record(us);
    }

    /// Record a PUT operation (non-blocking atomic update)
    #[inline]
    pub fn record_put(&self, bytes: usize, latency: Duration) {
        self.put_ops.fetch_add(1, Ordering::Relaxed);
        self.put_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        let us = latency.as_micros().min(u64::MAX as u128) as u64;
        let _ = self.put_hist.lock().record(us);
    }

    /// Record multiple PUT operations from a batch (non-blocking atomic update)
    ///
    /// Used when generating multiple files in parallel with a single I/O operation.
    /// Records accurate file operation count while using batch-level latency.
    #[inline]
    pub fn record_put_batch(&self, count: u64, bytes: usize, latency: Duration) {
        self.put_ops.fetch_add(count, Ordering::Relaxed);
        self.put_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        let us = latency.as_micros().min(u64::MAX as u128) as u64;
        // Histogram records batch-level latency (single I/O operation)
        let _ = self.put_hist.lock().record(us);
    }

    /// Record AI/ML samples processed (training phase)
    ///
    /// Called once per batch with the batch size. Non-blocking atomic update.
    #[inline]
    pub fn record_samples(&self, count: u64) {
        self.total_samples.fetch_add(count, Ordering::Relaxed);
    }

    /// Capture current stats snapshot (for gRPC streaming)
    ///
    /// Returns cumulative statistics and latency percentiles. This method
    /// acquires histogram locks briefly but is designed for 1-second intervals.
    pub fn snapshot(&self) -> LiveStatsSnapshot {
        let elapsed = self.start_time.elapsed();

        // Read atomic counters (relaxed ordering sufficient for monitoring)
        let get_ops = self.get_ops.load(Ordering::Relaxed);
        let get_bytes = self.get_bytes.load(Ordering::Relaxed);
        let put_ops = self.put_ops.load(Ordering::Relaxed);
        let put_bytes = self.put_bytes.load(Ordering::Relaxed);
        let total_samples = self.total_samples.load(Ordering::Relaxed);

        // Calculate latency percentiles (requires histogram lock)
        let (get_mean_us, get_p50_us, get_p90_us, get_p95_us, get_p99_us) = {
            let hist = self.get_hist.lock();
            let hist_len = hist.len();
            if hist_len > 0 {
                let mean = hist.mean() as u64;
                let p50 = hist.value_at_quantile(0.50);
                let p90 = hist.value_at_quantile(0.90);
                let p95 = hist.value_at_quantile(0.95);
                let p99 = hist.value_at_quantile(0.99);
                tracing::debug!("LiveStatsTracker GET histogram: len={}, mean={}µs, p50={}µs, p90={}µs, p95={}µs, p99={}µs", 
                           hist_len, mean, p50, p90, p95, p99);
                (mean, p50, p90, p95, p99)
            } else {
                tracing::debug!("LiveStatsTracker GET histogram: EMPTY (len=0)");
                (0, 0, 0, 0, 0)
            }
        };

        let (put_mean_us, put_p50_us, put_p90_us, put_p95_us, put_p99_us) = {
            let hist = self.put_hist.lock();
            let hist_len = hist.len();
            if hist_len > 0 {
                let mean = hist.mean() as u64;
                let p50 = hist.value_at_quantile(0.50);
                let p90 = hist.value_at_quantile(0.90);
                let p95 = hist.value_at_quantile(0.95);
                let p99 = hist.value_at_quantile(0.99);
                tracing::debug!("LiveStatsTracker PUT histogram: len={}, mean={}µs, p50={}µs, p90={}µs, p95={}µs, p99={}µs", 
                           hist_len, mean, p50, p90, p95, p99);
                (mean, p50, p90, p95, p99)
            } else {
                tracing::debug!("LiveStatsTracker PUT histogram: EMPTY (len=0)");
                (0, 0, 0, 0, 0)
            }
        };

        LiveStatsSnapshot {
            elapsed,
            get_ops,
            get_bytes,
            get_mean_us,
            get_p50_us,
            get_p90_us,
            get_p95_us,
            get_p99_us,
            put_ops,
            put_bytes,
            put_mean_us,
            put_p50_us,
            put_p90_us,
            put_p95_us,
            put_p99_us,
            total_samples,
        }
    }
}

impl Default for LiveStatsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of live stats at a point in time (for proto conversion)
#[derive(Debug, Clone)]
pub struct LiveStatsSnapshot {
    pub elapsed: Duration,
    pub get_ops: u64,
    pub get_bytes: u64,
    pub get_mean_us: u64,
    pub get_p50_us: u64,
    pub get_p90_us: u64,
    pub get_p95_us: u64,
    pub get_p99_us: u64,
    pub put_ops: u64,
    pub put_bytes: u64,
    pub put_mean_us: u64,
    pub put_p50_us: u64,
    pub put_p90_us: u64,
    pub put_p95_us: u64,
    pub put_p99_us: u64,
    pub total_samples: u64,
}

impl LiveStatsSnapshot {
    /// Get elapsed seconds as f64 for proto conversion
    pub fn elapsed_secs(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }

    /// Get wall-clock timestamp
    pub fn timestamp_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Calculate samples per second (for AI/ML training phase)
    pub fn samples_per_second(&self) -> f64 {
        if self.elapsed.as_secs_f64() > 0.0 {
            self.total_samples as f64 / self.elapsed.as_secs_f64()
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_tracker_basic() {
        let tracker = LiveStatsTracker::new();
        
        // Record some operations
        tracker.record_get(1024, Duration::from_micros(100));
        tracker.record_get(2048, Duration::from_micros(150));
        tracker.record_put(512, Duration::from_micros(200));
        tracker.record_samples(32); // batch size

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.get_ops, 2);
        assert_eq!(snapshot.get_bytes, 3072);
        assert_eq!(snapshot.put_ops, 1);
        assert_eq!(snapshot.put_bytes, 512);
        assert_eq!(snapshot.total_samples, 32);
        
        // Verify latencies are reasonable
        assert!(snapshot.get_mean_us >= 100 && snapshot.get_mean_us <= 150);
        assert!(snapshot.put_mean_us >= 180 && snapshot.put_mean_us <= 220);
    }

    #[test]
    fn test_tracker_threadsafe() {
        let tracker = LiveStatsTracker::new();
        let mut handles = vec![];

        // Spawn 10 threads recording operations concurrently
        for _ in 0..10 {
            let t = tracker.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    t.record_get(1000, Duration::from_micros(100));
                    t.record_put(500, Duration::from_micros(200));
                    t.record_samples(8); // batch size per iteration
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.get_ops, 1000);
        assert_eq!(snapshot.get_bytes, 1_000_000);
        assert_eq!(snapshot.put_ops, 1000);
        assert_eq!(snapshot.put_bytes, 500_000);
        assert_eq!(snapshot.total_samples, 8000); // 10 threads * 100 iters * 8 samples
    }

    #[test]
    fn test_samples_per_second() {
        let tracker = LiveStatsTracker::new();
        
        // Simulate processing samples over time
        std::thread::sleep(Duration::from_millis(100));
        tracker.record_samples(1000);
        
        let snapshot = tracker.snapshot();
        let samples_per_sec = snapshot.samples_per_second();
        
        // Should be roughly 10,000 samples/sec (1000 samples in 0.1 seconds)
        assert!(samples_per_sec > 5000.0 && samples_per_sec < 15000.0);
    }

    #[test]
    fn test_snapshot_helpers() {
        let tracker = LiveStatsTracker::new();
        tracker.record_get(1024, Duration::from_micros(100));
        
        let snapshot = tracker.snapshot();
        
        assert_eq!(snapshot.get_ops, 1);
        assert_eq!(snapshot.get_bytes, 1024);
        assert!(snapshot.elapsed_secs() > 0.0);
        assert!(snapshot.timestamp_secs() > 0);
    }
}
