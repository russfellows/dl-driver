use std::time::Duration;

/// Performance metrics collection
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub total_time: Option<Duration>,
    pub read_times: Vec<Duration>,
    pub write_times: Vec<Duration>,
    pub files_processed: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
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
