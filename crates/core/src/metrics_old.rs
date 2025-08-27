use std::time::Duration;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Performance metrics collection
#[derive(Debug, Clone, Default)]
pub struct Metrics {
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
    pub fn record_write_operation(&mut self, bytes: u64, duration: Duration) {
        self.bytes_written += bytes;
        self.write_times.push(duration);
        self.files_processed += 1;
    }

    /// Record batch processing
    pub fn record_batch_processing(&mut self, batch_size: usize, duration: Duration) {
        self.batches_processed += 1;
        self.read_times.push(duration);
        // Assuming each batch item represents some bytes read
        self.bytes_read += batch_size as u64 * 1024; // Estimate 1KB per item
    }

    /// Print performance summary
    pub fn print_summary(&self) {
        println!("\n=== Performance Summary ===");
        println!("Files processed: {}", self.files_processed);
        println!("Batches processed: {}", self.batches_processed);
        println!("Bytes written: {} MB", self.bytes_written / 1024 / 1024);
        println!("Bytes read: {} MB", self.bytes_read / 1024 / 1024);
        
        if !self.write_times.is_empty() {
            let avg_write = self.write_times.iter().sum::<Duration>() / self.write_times.len() as u32;
            let write_throughput = if avg_write.as_secs_f64() > 0.0 {
                (self.bytes_written as f64) / (1024.0 * 1024.0) / avg_write.as_secs_f64()
            } else { 0.0 };
            println!("Average write time: {:?}", avg_write);
            println!("Write throughput: {:.2} MB/s", write_throughput);
        }
        
        if !self.read_times.is_empty() {
            let avg_read = self.read_times.iter().sum::<Duration>() / self.read_times.len() as u32;
            let read_throughput = if avg_read.as_secs_f64() > 0.0 {
                (self.bytes_read as f64) / (1024.0 * 1024.0) / avg_read.as_secs_f64()
            } else { 0.0 };
            println!("Average read time: {:?}", avg_read);
            println!("Read throughput: {:.2} MB/s", read_throughput);
        }
        println!("=============================\n");
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
    
    pub async fn record_batch_processed(&self, samples: usize, bytes: u64) {
        let mut data = self.data.write().await;
        data.total_batches += 1;
        data.total_samples += samples as u64;
        data.total_bytes += bytes;
    }
    
    pub async fn record_throughput(&self, mbps: f64) {
        let mut data = self.data.write().await;
        data.throughput_measurements.push(mbps);
    }
    
    pub async fn record_completion(&self, elapsed: Duration, total_bytes: u64) {
        let mut data = self.data.write().await;
        data.end_time = Some(data.start_time.unwrap_or_else(std::time::Instant::now) + elapsed);
        
        if elapsed.as_secs_f64() > 0.0 {
            let mbps = (total_bytes as f64) / (1024.0 * 1024.0) / elapsed.as_secs_f64();
            data.throughput_measurements.push(mbps);
        }
    }
    
    pub async fn record_error(&self, error_type: &str) {
        let mut data = self.data.write().await;
        *data.error_counts.entry(error_type.to_string()).or_insert(0) += 1;
    }
    
    pub async fn print_summary(&self) {
        let data = self.data.read().await;
        
        println!("\n=== Enhanced DLIO Benchmark Results ===");
        
        if let (Some(start), Some(end)) = (data.start_time, data.end_time) {
            let total_duration = end.duration_since(start);
            println!("Total Duration: {:?}", total_duration);
        }
        
        if let Some(ref backend) = data.backend_type {
            println!("Storage Backend: {}", backend);
        }
        
        println!("Total Batches: {}", data.total_batches);
        println!("Total Samples: {}", data.total_samples);
        
        if data.total_bytes < 1024 * 1024 {
            println!("Total Bytes: {:.2} KB", data.total_bytes as f64 / 1024.0);
        } else {
            println!("Total Bytes: {:.2} MB", data.total_bytes as f64 / (1024.0 * 1024.0));
        }
        
        if !data.throughput_measurements.is_empty() {
            let avg_throughput = data.throughput_measurements.iter().sum::<f64>() / data.throughput_measurements.len() as f64;
            let max_throughput = data.throughput_measurements.iter().fold(0.0f64, |a, &b| a.max(b));
            println!("Average Throughput: {:.2} MB/s", avg_throughput);
            println!("Peak Throughput: {:.2} MB/s", max_throughput);
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

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_total_time(&mut self, duration: Duration) {
        self.total_time = Some(duration);
    }

    pub fn record_read_time(&mut self, duration: Duration) {
        self.read_times.push(duration);
        self.files_processed += 1;
    }

    pub fn record_write_time(&mut self, duration: Duration) {
        self.write_times.push(duration);
    }

    pub fn record_bytes_read(&mut self, bytes: u64) {
        self.bytes_read += bytes;
    }

    pub fn record_bytes_written(&mut self, bytes: u64) {
        self.bytes_written += bytes;
    }

    pub fn average_read_time(&self) -> Option<Duration> {
        if self.read_times.is_empty() {
            return None;
        }
        let total: Duration = self.read_times.iter().sum();
        Some(total / self.read_times.len() as u32)
    }

    pub fn total_throughput_mbps(&self) -> Option<f64> {
        if let Some(total_time) = self.total_time {
            let total_bytes = self.bytes_read + self.bytes_written;
            let seconds = total_time.as_secs_f64();
            if seconds > 0.0 {
                return Some((total_bytes as f64) / (1024.0 * 1024.0) / seconds);
            }
        }
        None
    }

    pub fn print_summary(&self) {
        println!("\n=== DLIO Benchmark Results ===");
        
        if let Some(total_time) = self.total_time {
            println!("Total Time: {:?}", total_time);
        }
        
        println!("Files Processed: {}", self.files_processed);
        
        // Use KB for smaller values, MB for larger ones
        if self.bytes_read < 1024 * 1024 {
            println!("Bytes Read: {:.2} KB", self.bytes_read as f64 / 1024.0);
        } else {
            println!("Bytes Read: {:.2} MB", self.bytes_read as f64 / (1024.0 * 1024.0));
        }
        
        if self.bytes_written < 1024 * 1024 {
            println!("Bytes Written: {:.2} KB", self.bytes_written as f64 / 1024.0);
        } else {
            println!("Bytes Written: {:.2} MB", self.bytes_written as f64 / (1024.0 * 1024.0));
        }
        
        if let Some(avg_read) = self.average_read_time() {
            println!("Average Read Time: {:?}", avg_read);
        }
        
        if let Some(throughput) = self.total_throughput_mbps() {
            println!("Total Throughput: {:.2} MB/s", throughput);
        }
        
        println!("=============================\n");
    }
}
