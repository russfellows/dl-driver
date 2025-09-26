// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::RwLock;

/// Performance metrics collection with interior mutability for Arc compatibility
#[derive(Debug, Default)]
pub struct Metrics {
    data: Mutex<MetricsData>,
}

#[derive(Debug, Default)]
struct MetricsData {
    pub total_time: Option<Duration>,
    pub read_times: Vec<Duration>,
    pub write_times: Vec<Duration>,
    pub files_processed: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub batches_processed: u64,
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
            let write_throughput = if avg_write.as_secs_f64() > 0.0 {
                (data.bytes_written as f64) / (1024.0 * 1024.0) / avg_write.as_secs_f64()
            } else {
                0.0
            };
            println!("Average write time: {:?}", avg_write);
            println!("Write throughput: {:.2} MB/s", write_throughput);
        }

        if !data.read_times.is_empty() {
            let avg_read = data.read_times.iter().sum::<Duration>() / data.read_times.len() as u32;
            let read_throughput = if avg_read.as_secs_f64() > 0.0 {
                (data.bytes_read as f64) / (1024.0 * 1024.0) / avg_read.as_secs_f64()
            } else {
                0.0
            };
            println!("Average read time: {:?}", avg_read);
            println!("Read throughput: {:.2} MB/s", read_throughput);
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
