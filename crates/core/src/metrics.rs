// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use std::path::Path;
use std::fs::File;
use std::io::Write;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use crate::dlio_compat::DlioConfig;

/// Performance metrics collection with interior mutability for Arc compatibility
#[derive(Debug, Default)]
pub struct Metrics {
    data: Mutex<MetricsData>,
}

#[derive(Debug, Default)]
struct MetricsData {
    pub total_time: Option<Duration>,
    pub read_times: Vec<Duration>,        // Pure I/O times
    pub write_times: Vec<Duration>,
    pub compute_times: Vec<Duration>,     // Pure computation times
    pub batch_times: Vec<Duration>,       // Total batch times (I/O + compute)
    pub epoch_times: Vec<Duration>,       // Per-epoch times
    pub files_processed: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub batches_processed: u64,
}

/// Result of Accelerator Utilization calculation
#[derive(Debug, Clone)]
pub struct AuResult {
    pub au_fraction: f64,   // 0..1
    pub au_percent: f64,    // 0..100
    pub pass: Option<bool>, // None if no threshold in config
}

/// Serializable metrics summary for JSON/CSV export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub mode: String, // "generate", "train", "replay"
    pub totals: TotalMetrics,
    pub performance: PerformanceMetrics,
    pub timing: TimingMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accelerator_utilization: Option<AuMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalMetrics {
    pub files_processed: u64,
    pub batches_processed: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub bytes_read_mb: f64,
    pub bytes_written_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub read_throughput_mbps: f64,
    pub write_throughput_mbps: f64,
    pub read_throughput_gibps: f64,
    pub write_throughput_gibps: f64,
    pub average_read_time_ms: f64,
    pub average_write_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingMetrics {
    pub total_time_secs: f64,
    pub total_epoch_time_secs: f64,
    pub total_compute_time_secs: f64,
    pub average_batch_time_ms: f64,
    pub average_epoch_time_secs: f64,
    pub num_epochs: usize,
    pub latency_ms_p95: Option<f64>, // Will be calculated if we have enough samples
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuMetrics {
    pub au_fraction: f64,
    pub au_percent: f64,
    pub pass: Option<bool>,
    pub threshold: Option<f64>,
}

/// CSV row for simple tabular export
#[derive(Debug, Clone, Serialize)]
pub struct CsvMetric {
    pub metric: String,
    pub value: String,
    pub unit: String,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a write operation
    pub fn record_write_operation(&self, bytes: u64, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.bytes_written += bytes;
        data.write_times.push(duration);
        data.files_processed += 1;
    }

    /// Record batch processing
    pub fn record_batch_processing(&self, batch_size: usize, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.batches_processed += 1;
        data.read_times.push(duration);
        // Assuming each batch item represents some bytes read
        data.bytes_read += batch_size as u64 * 1024; // Estimate 1KB per item
    }

    /// Set total time
    pub fn set_total_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.total_time = Some(duration);
    }

    // Getter methods for tests
    pub fn files_processed(&self) -> u64 {
        self.data.lock().unwrap().files_processed
    }

    pub fn bytes_read(&self) -> u64 {
        self.data.lock().unwrap().bytes_read
    }

    pub fn bytes_written(&self) -> u64 {
        self.data.lock().unwrap().bytes_written
    }

    pub fn total_time(&self) -> Option<Duration> {
        self.data.lock().unwrap().total_time
    }

    pub fn batches_processed(&self) -> u64 {
        self.data.lock().unwrap().batches_processed
    }

    pub fn total_read_time(&self) -> Duration {
        self.data.lock().unwrap().read_times.iter().sum()
    }

    pub fn total_compute_time(&self) -> Duration {
        self.data.lock().unwrap().compute_times.iter().sum()
    }

    pub fn batch_times(&self) -> Vec<Duration> {
        self.data.lock().unwrap().batch_times.clone()
    }

    pub fn epoch_times(&self) -> Vec<Duration> {
        self.data.lock().unwrap().epoch_times.clone()
    }

    /// Record a read operation
    pub fn record_read_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.read_times.push(duration);
        data.files_processed += 1;
    }

    /// Record write time
    pub fn record_write_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.write_times.push(duration);
    }

    /// Record bytes read
    pub fn record_bytes_read(&self, bytes: u64) {
        let mut data = self.data.lock().unwrap();
        data.bytes_read += bytes;
    }

    /// Record computation time (GPU simulation)
    pub fn record_compute_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.compute_times.push(duration);
    }

    /// Record total batch time (I/O + compute)
    pub fn record_batch_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.batch_times.push(duration);
    }

    /// Record epoch time
    pub fn record_epoch_time(&self, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.epoch_times.push(duration);
    }

    /// Record bytes written
    pub fn record_bytes_written(&self, bytes: u64) {
        let mut data = self.data.lock().unwrap();
        data.bytes_written += bytes;
    }

    /// Record a file generation operation
    pub fn record_file_generated(&self, _filename: String, size_bytes: u64, duration: Duration) {
        let mut data = self.data.lock().unwrap();
        data.write_times.push(duration);
        data.bytes_written += size_bytes;
        data.files_processed += 1;
    }

    /// Print performance summary
    pub fn print_summary(&self) {
        let data = self.data.lock().unwrap();
        println!("\n=== Performance Summary ===");
        println!("Files processed: {}", data.files_processed);
        println!("Batches processed: {}", data.batches_processed);
        println!("Bytes written: {} MB", data.bytes_written / 1024 / 1024);
        println!("Bytes read: {} MB", data.bytes_read / 1024 / 1024);

        if !data.write_times.is_empty() {
            let avg_write =
                data.write_times.iter().sum::<Duration>() / data.write_times.len() as u32;
            let total_write_time = data.write_times.iter().sum::<Duration>();
            let write_throughput = if total_write_time.as_secs_f64() > 0.0 {
                (data.bytes_written as f64) / (1024.0 * 1024.0) / total_write_time.as_secs_f64()
            } else {
                0.0
            };
            println!("Average write time: {:?}", avg_write);
            println!("Write throughput: {:.2} MB/s", write_throughput);
        }

        if !data.read_times.is_empty() {
            let avg_read = data.read_times.iter().sum::<Duration>() / data.read_times.len() as u32;
            
            // CORRECT STORAGE THROUGHPUT CALCULATION:
            // Use wall-clock time from epochs, not sum of individual I/O times
            // (Individual I/O times are microseconds with parallel I/O, wall-clock is real storage time)
            let wall_clock_time = if !data.epoch_times.is_empty() {
                data.epoch_times.iter().sum::<Duration>()
            } else {
                data.total_time.unwrap_or(Duration::from_secs(1)) // Fallback to 1 second
            };
            
            let storage_throughput_mbps = if wall_clock_time.as_secs_f64() > 0.0 {
                (data.bytes_read as f64) / (1024.0 * 1024.0) / wall_clock_time.as_secs_f64()
            } else {
                0.0
            };
            
            let storage_throughput_gibps = storage_throughput_mbps / 1024.0; // Convert MB/s to GiB/s
            
            println!("Average read time: {:?}", avg_read);
            println!("Read throughput: {:.2} MB/s ({:.2} GiB/s) [STORAGE WALL-CLOCK]", 
                     storage_throughput_mbps, storage_throughput_gibps);
        }

        // Enhanced timing breakdown
        if !data.compute_times.is_empty() {
            let total_compute = data.compute_times.iter().sum::<Duration>();
            let avg_compute = total_compute / data.compute_times.len() as u32;
            println!("Total compute time: {:?}", total_compute);
            println!("Average compute time: {:?}", avg_compute);
        }

        if !data.batch_times.is_empty() {
            let total_batch = data.batch_times.iter().sum::<Duration>();
            let avg_batch = total_batch / data.batch_times.len() as u32;
            println!("Total batch time: {:?}", total_batch);
            println!("Average batch time: {:?}", avg_batch);
        }

        if !data.epoch_times.is_empty() {
            let total_epoch = data.epoch_times.iter().sum::<Duration>();
            let avg_epoch = total_epoch / data.epoch_times.len() as u32;
            println!("Total epoch time: {:?}", total_epoch);
            println!("Average epoch time: {:?}", avg_epoch);
            println!("Number of epochs: {}", data.epoch_times.len());
        }

        println!("=============================\n");
    }

    pub fn average_read_time(&self) -> Option<Duration> {
        let data = self.data.lock().unwrap();
        if data.read_times.is_empty() {
            return None;
        }
        let total: Duration = data.read_times.iter().sum();
        Some(total / data.read_times.len() as u32)
    }

    pub fn average_write_time(&self) -> Option<Duration> {
        let data = self.data.lock().unwrap();
        if data.write_times.is_empty() {
            return None;
        }
        let total: Duration = data.write_times.iter().sum();
        Some(total / data.write_times.len() as u32)
    }

    pub fn read_throughput_mbps(&self) -> Option<f64> {
        if let Some(avg_time) = self.average_read_time() {
            let bytes_read = self.bytes_read();
            if avg_time.as_secs_f64() > 0.0 && bytes_read > 0 {
                let mb_per_sec = (bytes_read as f64) / (1024.0 * 1024.0) / avg_time.as_secs_f64();
                Some(mb_per_sec)
            } else {
                Some(0.0)
            }
        } else {
            None
        }
    }

    pub fn write_throughput_mbps(&self) -> Option<f64> {
        if let Some(avg_time) = self.average_write_time() {
            let bytes_written = self.bytes_written();
            if avg_time.as_secs_f64() > 0.0 && bytes_written > 0 {
                let mb_per_sec =
                    (bytes_written as f64) / (1024.0 * 1024.0) / avg_time.as_secs_f64();
                Some(mb_per_sec)
            } else {
                Some(0.0)
            }
        } else {
            None
        }
    }

    /// Compute Accelerator Utilization (AU) for MLPerf Storage compliance
    pub fn compute_au(&self, cfg: &DlioConfig, _total_runtime: Duration, _accelerators: u32) -> Option<AuResult> {
        use tracing::debug;
        
        // Use the same calculation as calculate_au_internal for consistency
        let data = self.data.lock().unwrap();
        
        debug!("AU calculation: {} compute times, {} epoch times recorded", 
               data.compute_times.len(), data.epoch_times.len());
        
        // Ensure we have timing data
        if data.compute_times.is_empty() || data.epoch_times.is_empty() {
            debug!("AU calculation failed: no timing data available");
            return None;
        }
        
        // Use measured timing data (same as JSON export) for consistency
        let total_compute = data.compute_times.iter().sum::<Duration>();
        let wall_clock_time = data.epoch_times.iter().sum::<Duration>();
        
        debug!("AU calculation: total_compute={:.3}s, wall_clock={:.3}s", 
               total_compute.as_secs_f64(), wall_clock_time.as_secs_f64());
        
        if wall_clock_time.is_zero() {
            debug!("AU calculation failed: wall clock time is zero");
            return None;
        }
        
        let au_fraction = total_compute.as_secs_f64() / wall_clock_time.as_secs_f64();
        let au_percent = (au_fraction * 100.0).min(100.0);
        
        let pass = cfg.metric.as_ref()
            .and_then(|m| m.au)
            .map(|threshold| au_fraction >= threshold);
        
        debug!("AU calculation result: {:.3} fraction ({:.1}%), pass={:?}", 
               au_fraction, au_percent, pass);
            
        Some(AuResult { au_fraction, au_percent, pass })
    }

    /// Export metrics as JSON for multi-rank aggregation
    pub fn to_json(&self, rank: u32, config: &DlioConfig) -> serde_json::Value {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let data = self.data.lock().unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        
        // Calculate comprehensive metrics
        let total_read_time: Duration = data.read_times.iter().sum();
        let total_compute_time: Duration = data.compute_times.iter().sum();
        let total_batch_time: Duration = data.batch_times.iter().sum();
        let wall_clock_time = data.epoch_times.iter().sum::<Duration>();
        
        let throughput_gib_s = if wall_clock_time.as_secs_f64() > 0.0 {
            (data.bytes_read as f64) / (1024.0_f64.powi(3)) / wall_clock_time.as_secs_f64()
        } else {
            0.0
        };
        
        // Calculate AU if we have the data
        let au_result = if !data.compute_times.is_empty() && !data.batch_times.is_empty() {
            self.calculate_au_internal(&data, config)
        } else {
            AuResult { au_fraction: 0.0, au_percent: 0.0, pass: None }
        };
        
        serde_json::json!({
            "rank": rank,
            "timestamp": now,
            "start_time": now - wall_clock_time.as_secs_f64(),
            "end_time": now,
            "config": {
                "data_folder": config.data_folder_uri(),
                "batch_size": config.reader.batch_size.unwrap_or(1),
                "epochs": config.train.as_ref().and_then(|t| t.epochs).unwrap_or(1),
                "computation_time": config.train.as_ref().and_then(|t| t.computation_time).unwrap_or(0.1)
            },
            "metrics": {
                "files_processed": data.files_processed,
                "bytes_read": data.bytes_read,
                "bytes_written": data.bytes_written,
                "batches_processed": data.batches_processed,
                "storage_throughput_gib_s": throughput_gib_s,
                "total_read_time_ms": total_read_time.as_millis(),
                "total_compute_time_ms": total_compute_time.as_millis(),
                "total_batch_time_ms": total_batch_time.as_millis(),
                "wall_clock_time_ms": wall_clock_time.as_millis(),
                "average_batch_time_ms": if !data.batch_times.is_empty() {
                    total_batch_time.as_millis() / data.batch_times.len() as u128
                } else { 0 },
                "au_fraction": au_result.au_fraction,
                "au_percent": au_result.au_percent,
                "au_pass": au_result.pass
            },
            "timing_details": {
                "read_times_ms": data.read_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>(),
                "compute_times_ms": data.compute_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>(),
                "batch_times_ms": data.batch_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>(),
                "epoch_times_ms": data.epoch_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>()
            }
        })
    }

    /// Internal AU calculation helper
    fn calculate_au_internal(&self, data: &MetricsData, config: &DlioConfig) -> AuResult {
        // Replicate the logic from calculate_au but with already-locked data
        let total_compute = data.compute_times.iter().sum::<Duration>();
        let wall_clock_time = data.epoch_times.iter().sum::<Duration>();
        
        if wall_clock_time.is_zero() {
            return AuResult { au_fraction: 0.0, au_percent: 0.0, pass: None };
        }
        
        let au_fraction = total_compute.as_secs_f64() / wall_clock_time.as_secs_f64();
        let au_percent = (au_fraction * 100.0).min(100.0);
        
        let pass = config.metric.as_ref()
            .and_then(|m| m.au)
            .map(|threshold| au_fraction >= threshold);
            
        AuResult { au_fraction, au_percent, pass }
    }

    /// Export metrics summary to JSON file
    pub fn export_json(&self, path: &Path, mode: &str) -> anyhow::Result<()> {
        let summary = self.create_summary(mode);
        let json = serde_json::to_string_pretty(&summary)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Export metrics to CSV file
    pub fn export_csv(&self, path: &Path, mode: &str) -> anyhow::Result<()> {
        let summary = self.create_summary(mode);
        let metrics = self.summary_to_csv_rows(&summary);
        
        let mut file = File::create(path)?;
        writeln!(file, "metric,value,unit")?;
        
        for metric in metrics {
            writeln!(file, "{},{},{}", metric.metric, metric.value, metric.unit)?;
        }
        
        Ok(())
    }

    /// Create a comprehensive summary of all metrics
    pub fn create_summary(&self, mode: &str) -> MetricsSummary {
        let data = self.data.lock().unwrap();
        
        // Calculate derived metrics
        let bytes_read_mb = data.bytes_read as f64 / (1024.0 * 1024.0);
        let bytes_written_mb = data.bytes_written as f64 / (1024.0 * 1024.0);
        
        // Calculate wall-clock time for throughput
        let wall_clock_time = if !data.epoch_times.is_empty() {
            data.epoch_times.iter().sum::<Duration>()
        } else {
            data.total_time.unwrap_or(Duration::from_secs(1))
        };
        
        let wall_clock_secs = wall_clock_time.as_secs_f64();
        let read_throughput_mbps = if wall_clock_secs > 0.0 { bytes_read_mb / wall_clock_secs } else { 0.0 };
        let write_throughput_mbps = if wall_clock_secs > 0.0 { bytes_written_mb / wall_clock_secs } else { 0.0 };
        
        // Average timings
        let avg_read_time_ms = if !data.read_times.is_empty() {
            data.read_times.iter().sum::<Duration>().as_secs_f64() * 1000.0 / data.read_times.len() as f64
        } else { 0.0 };
        
        let avg_write_time_ms = if !data.write_times.is_empty() {
            data.write_times.iter().sum::<Duration>().as_secs_f64() * 1000.0 / data.write_times.len() as f64
        } else { 0.0 };
        
        let avg_batch_time_ms = if !data.batch_times.is_empty() {
            data.batch_times.iter().sum::<Duration>().as_secs_f64() * 1000.0 / data.batch_times.len() as f64
        } else { 0.0 };
        
        let avg_epoch_time_secs = if !data.epoch_times.is_empty() {
            data.epoch_times.iter().sum::<Duration>().as_secs_f64() / data.epoch_times.len() as f64
        } else { 0.0 };
        
        // Calculate p95 latency if we have enough read samples
        let latency_ms_p95 = if data.read_times.len() >= 20 {
            let mut sorted_times: Vec<f64> = data.read_times.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
            sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p95_idx = (sorted_times.len() as f64 * 0.95) as usize;
            Some(sorted_times[p95_idx.min(sorted_times.len() - 1)])
        } else { None };
        
        MetricsSummary {
            mode: mode.to_string(),
            totals: TotalMetrics {
                files_processed: data.files_processed,
                batches_processed: data.batches_processed,
                bytes_read: data.bytes_read,
                bytes_written: data.bytes_written,
                bytes_read_mb,
                bytes_written_mb,
            },
            performance: PerformanceMetrics {
                read_throughput_mbps,
                write_throughput_mbps,
                read_throughput_gibps: read_throughput_mbps / 1024.0,
                write_throughput_gibps: write_throughput_mbps / 1024.0,
                average_read_time_ms: avg_read_time_ms,
                average_write_time_ms: avg_write_time_ms,
            },
            timing: TimingMetrics {
                total_time_secs: data.total_time.map(|d| d.as_secs_f64()).unwrap_or(0.0),
                total_epoch_time_secs: data.epoch_times.iter().sum::<Duration>().as_secs_f64(),
                total_compute_time_secs: data.compute_times.iter().sum::<Duration>().as_secs_f64(),
                average_batch_time_ms: avg_batch_time_ms,
                average_epoch_time_secs: avg_epoch_time_secs,
                num_epochs: data.epoch_times.len(),
                latency_ms_p95,
            },
            accelerator_utilization: None, // Will be filled separately if AU is calculated
        }
    }

    /// Convert summary to CSV rows
    fn summary_to_csv_rows(&self, summary: &MetricsSummary) -> Vec<CsvMetric> {
        let mut rows = Vec::new();
        
        // Totals
        rows.push(CsvMetric { metric: "files_processed".to_string(), value: summary.totals.files_processed.to_string(), unit: "count".to_string() });
        rows.push(CsvMetric { metric: "batches_processed".to_string(), value: summary.totals.batches_processed.to_string(), unit: "count".to_string() });
        rows.push(CsvMetric { metric: "bytes_read".to_string(), value: summary.totals.bytes_read.to_string(), unit: "bytes".to_string() });
        rows.push(CsvMetric { metric: "bytes_written".to_string(), value: summary.totals.bytes_written.to_string(), unit: "bytes".to_string() });
        rows.push(CsvMetric { metric: "bytes_read_mb".to_string(), value: format!("{:.2}", summary.totals.bytes_read_mb), unit: "MB".to_string() });
        rows.push(CsvMetric { metric: "bytes_written_mb".to_string(), value: format!("{:.2}", summary.totals.bytes_written_mb), unit: "MB".to_string() });
        
        // Performance
        rows.push(CsvMetric { metric: "read_throughput_mbps".to_string(), value: format!("{:.2}", summary.performance.read_throughput_mbps), unit: "MB/s".to_string() });
        rows.push(CsvMetric { metric: "write_throughput_mbps".to_string(), value: format!("{:.2}", summary.performance.write_throughput_mbps), unit: "MB/s".to_string() });
        rows.push(CsvMetric { metric: "read_throughput_gibps".to_string(), value: format!("{:.3}", summary.performance.read_throughput_gibps), unit: "GiB/s".to_string() });
        rows.push(CsvMetric { metric: "write_throughput_gibps".to_string(), value: format!("{:.3}", summary.performance.write_throughput_gibps), unit: "GiB/s".to_string() });
        rows.push(CsvMetric { metric: "average_read_time_ms".to_string(), value: format!("{:.2}", summary.performance.average_read_time_ms), unit: "ms".to_string() });
        rows.push(CsvMetric { metric: "average_write_time_ms".to_string(), value: format!("{:.2}", summary.performance.average_write_time_ms), unit: "ms".to_string() });
        
        // Timing
        rows.push(CsvMetric { metric: "total_time_secs".to_string(), value: format!("{:.2}", summary.timing.total_time_secs), unit: "seconds".to_string() });
        rows.push(CsvMetric { metric: "average_batch_time_ms".to_string(), value: format!("{:.2}", summary.timing.average_batch_time_ms), unit: "ms".to_string() });
        rows.push(CsvMetric { metric: "num_epochs".to_string(), value: summary.timing.num_epochs.to_string(), unit: "count".to_string() });
        
        if let Some(p95) = summary.timing.latency_ms_p95 {
            rows.push(CsvMetric { metric: "latency_ms_p95".to_string(), value: format!("{:.2}", p95), unit: "ms".to_string() });
        }
        
        // AU metrics if present
        if let Some(au) = &summary.accelerator_utilization {
            rows.push(CsvMetric { metric: "au_fraction".to_string(), value: format!("{:.3}", au.au_fraction), unit: "fraction".to_string() });
            rows.push(CsvMetric { metric: "au_percent".to_string(), value: format!("{:.1}", au.au_percent), unit: "percent".to_string() });
            if let Some(pass) = au.pass {
                rows.push(CsvMetric { metric: "au_pass".to_string(), value: pass.to_string(), unit: "bool".to_string() });
            }
        }
        
        rows
    }

    /// Set AU result in the summary (used when AU is calculated)
    pub fn set_au_result(&self, summary: &mut MetricsSummary, au_result: &AuResult, threshold: Option<f64>) {
        summary.accelerator_utilization = Some(AuMetrics {
            au_fraction: au_result.au_fraction,
            au_percent: au_result.au_percent,
            pass: au_result.pass,
            threshold,
        });
    }
}

/// Enhanced async metrics for workload benchmarking
#[derive(Debug)]
pub struct WorkloadMetrics {
    data: RwLock<WorkloadData>,
}

#[derive(Debug, Default)]
struct WorkloadData {
    pub start_time: Option<std::time::Instant>,
    pub end_time: Option<std::time::Instant>,
    pub total_batches: u64,
    pub total_samples: u64,
    pub total_bytes: u64,
    pub batch_times: Vec<Duration>,
    pub throughput_measurements: Vec<f64>,
    pub error_counts: HashMap<String, u64>,
    pub backend_type: Option<String>,
}

impl Default for WorkloadMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkloadMetrics {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(WorkloadData::default()),
        }
    }

    pub async fn set_backend_type(&self, backend_type: String) {
        let mut data = self.data.write().await;
        data.backend_type = Some(backend_type);
    }

    pub async fn start_benchmark(&self) {
        let mut data = self.data.write().await;
        data.start_time = Some(std::time::Instant::now());
    }

    pub async fn end_benchmark(&self) {
        let mut data = self.data.write().await;
        data.end_time = Some(std::time::Instant::now());
    }

    pub async fn record_batch(&self, batch_size: usize, processing_time: Duration) {
        let mut data = self.data.write().await;
        data.total_batches += 1;
        data.total_samples += batch_size as u64;
        data.batch_times.push(processing_time);

        // Calculate instantaneous throughput
        if processing_time.as_secs_f64() > 0.0 {
            let throughput = (batch_size as f64) / processing_time.as_secs_f64();
            data.throughput_measurements.push(throughput);
        }
    }

    pub async fn record_bytes(&self, bytes: u64) {
        let mut data = self.data.write().await;
        data.total_bytes += bytes;
    }

    pub async fn record_error(&self, error_type: &str) {
        let mut data = self.data.write().await;
        *data.error_counts.entry(error_type.to_string()).or_insert(0) += 1;
    }

    pub async fn print_summary(&self) {
        let data = self.data.read().await;

        println!("\n==========================================");
        println!("           Workload Summary");
        println!("==========================================");

        if let Some(backend) = &data.backend_type {
            println!("Backend Type: {}", backend);
        }

        if let (Some(start), Some(end)) = (data.start_time, data.end_time) {
            let total_time = end.duration_since(start);
            println!("Total Runtime: {:?}", total_time);
        }

        println!("Total Batches: {}", data.total_batches);
        println!("Total Samples: {}", data.total_samples);
        println!("Total Bytes: {} MB", data.total_bytes / 1024 / 1024);

        if !data.batch_times.is_empty() {
            let avg_batch_time =
                data.batch_times.iter().sum::<Duration>() / data.batch_times.len() as u32;
            println!("Average Batch Time: {:?}", avg_batch_time);
        }

        if !data.throughput_measurements.is_empty() {
            let avg_throughput = data.throughput_measurements.iter().sum::<f64>()
                / data.throughput_measurements.len() as f64;
            let max_throughput = data
                .throughput_measurements
                .iter()
                .fold(0.0f64, |a, &b| a.max(b));
            println!("Average Throughput: {:.2} samples/s", avg_throughput);
            println!("Peak Throughput: {:.2} samples/s", max_throughput);
        }

        if !data.error_counts.is_empty() {
            println!("Errors:");
            for (error_type, count) in &data.error_counts {
                println!("  {}: {}", error_type, count);
            }
        }

        println!("==========================================\n");
    }
}
