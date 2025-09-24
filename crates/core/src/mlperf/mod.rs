// crates/core/src/mlperf/mod.rs
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use futures_util::StreamExt;
use tracing::info;

use crate::config::DlioConfig;
use crate::plan::RunPlan;
use crate::plugins::PluginManager;

// Import s3dlio components
use s3dlio::data_loader::{AsyncPoolDataLoader, MultiBackendDataset};

#[derive(Debug)]
pub struct MlperfRunner {
    config: DlioConfig,
    plan: RunPlan,
    plugins: PluginManager,
    metrics: MlperfMetrics,
    max_epochs: u32,
    max_steps: u32,
}

impl MlperfRunner {
    pub fn new(config: DlioConfig, plugins: PluginManager) -> Self {
        let plan = RunPlan::from_config(&config);
        Self { 
            config, 
            plan, 
            plugins, 
            metrics: MlperfMetrics::new(),
            max_epochs: 3,    // Default values, can be overridden
            max_steps: 1000,
        }
    }

    /// Set maximum epochs for training
    pub fn with_max_epochs(mut self, max_epochs: u32) -> Self {
        self.max_epochs = max_epochs;
        self
    }

    /// Set maximum steps for training  
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    pub async fn run(&mut self) -> Result<MlperfReport> {
        let start_time = Instant::now();
        
        info!("Starting MLPerf DLIO benchmark: {}", 
              self.config.model.as_ref()
                .and_then(|m| m.name.as_ref())
                .unwrap_or(&"unknown".to_string()));
        
        // Initialize plugins
        self.plugins.initialize(&self.config).await
            .context("Failed to initialize plugins")?;

        // Build dataset from URI (supports file://, directio://, s3://, az://)
        let dataset = MultiBackendDataset::from_prefix(&self.plan.uri).await
            .context("Failed to create dataset from URI")?;

        // Create loader with plan configuration
        let loader_opts = self.plan.to_loader_options();
        let pool_config = self.plan.to_pool_config();
        
        let loader = AsyncPoolDataLoader::new(dataset, loader_opts);

        let mut stream = loader.stream_with_pool(pool_config);

        self.metrics.begin_run();

        let mut step: u32 = 0;
        let mut epoch: u32 = 0;
        let samples_per_epoch = self.plan.num_files_train.unwrap_or(100) 
            * self.plan.num_samples_per_file.unwrap_or(1);
        let mut samples_this_epoch = 0;

        info!("Starting data streaming with {} samples per epoch", samples_per_epoch);

        // Main data streaming loop
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.context("Failed to load batch")?;
            
            // Record metrics for this batch
            self.metrics.on_batch(&batch);
            
            // Record access order for deterministic validation
            // TODO: Enhance s3dlio to expose actual item keys/paths instead of step indices
            // For now, record step indices as a fallback for deterministic ordering
            self.metrics.record_item_access(format!("step_{:08}", step));
            
            // Update sample count
            samples_this_epoch += batch.len();
            
            // Plugin hook after each step
            self.plugins.after_step(step).await
                .context("Plugin after_step failed")?;
            
            step += 1;

            // Check if we've completed an epoch
            if samples_this_epoch >= samples_per_epoch {
                epoch += 1;
                samples_this_epoch = 0;
                
                info!("Completed epoch {} after {} steps", epoch, step);
                
                // Plugin hook after each epoch
                self.plugins.after_epoch(epoch).await
                    .context("Plugin after_epoch failed")?;

                // Check configurable epoch limit
                if epoch >= self.max_epochs {
                    info!("Completed {} epochs (limit: {}), ending benchmark", epoch, self.max_epochs);
                    break;
                }
            }

            // Check configurable step limit
            if step >= self.max_steps {
                info!("Reached step limit ({} steps), ending benchmark", self.max_steps);
                break;
            }
        }

        // Finalize plugins
        self.plugins.finalize().await
            .context("Failed to finalize plugins")?;

        let total_time = start_time.elapsed();
        self.metrics.complete_run(total_time);

        // Generate MLPerf report
        let report = MlperfReport::from_metrics(&self.metrics, &self.config);
        
        info!("MLPerf benchmark completed in {:.2}s", total_time.as_secs_f64());
        
        Ok(report)
    }
}

#[derive(Debug, Default)]
pub struct MlperfMetrics {
    pub start_time: Option<Instant>,
    pub end_time: Option<Instant>,
    pub total_bytes: u64,
    pub total_samples: u64,
    pub batch_latencies_ms: Vec<f64>,
    // Per-stage timing for detailed MLPerf analysis
    pub io_latencies_ms: Vec<f64>,        // read/fetch timing
    pub decode_latencies_ms: Vec<f64>,    // format decode timing  
    pub h2d_latencies_ms: Vec<f64>,       // host→device transfer (stub for now)
    // Access order tracking for deterministic validation
    pub visited_items: Vec<String>,       // file paths or dataset indices for determinism
}

impl MlperfMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_run(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn on_batch(&mut self, batch: &[Vec<u8>]) {
        let batch_start = Instant::now();
        
        // Record batch size and sample count
        for item in batch {
            self.total_bytes += item.len() as u64;
            self.total_samples += 1;
        }
        
        let batch_latency = batch_start.elapsed().as_secs_f64() * 1000.0;
        self.batch_latencies_ms.push(batch_latency);
    }

    /// Record I/O latency (time to read/fetch data from storage)
    pub fn record_io_latency(&mut self, latency_ms: f64) {
        self.io_latencies_ms.push(latency_ms);
    }

    /// Record decode latency (time to decode format like NPZ, HDF5, etc.)
    pub fn record_decode_latency(&mut self, latency_ms: f64) {
        self.decode_latencies_ms.push(latency_ms);
    }

    /// Record host-to-device transfer latency (stub for GPU workloads)
    pub fn record_h2d_latency(&mut self, latency_ms: f64) {
        self.h2d_latencies_ms.push(latency_ms);
    }

    /// Record an accessed item for deterministic validation
    /// This tracks the order in which dataset items are accessed
    pub fn record_item_access(&mut self, item_id: String) {
        self.visited_items.push(item_id);
    }

    pub fn complete_run(&mut self, duration: std::time::Duration) {
        self.end_time = Some(self.start_time.unwrap() + duration);
    }

    pub fn throughput_samples_per_sec(&self) -> f64 {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            let duration_secs = (end - start).as_secs_f64();
            if duration_secs > 0.0 {
                return self.total_samples as f64 / duration_secs;
            }
        }
        0.0
    }

    pub fn latency_percentile(&self, percentile: f64) -> f64 {
        Self::calculate_percentile(&self.batch_latencies_ms, percentile)
    }

    /// Calculate percentile for I/O latencies
    pub fn io_percentile(&self, percentile: f64) -> f64 {
        Self::calculate_percentile(&self.io_latencies_ms, percentile)
    }

    /// Calculate percentile for decode latencies  
    pub fn decode_percentile(&self, percentile: f64) -> f64 {
        Self::calculate_percentile(&self.decode_latencies_ms, percentile)
    }

    /// Calculate percentile for host-to-device latencies
    pub fn h2d_percentile(&self, percentile: f64) -> f64 {
        Self::calculate_percentile(&self.h2d_latencies_ms, percentile)
    }

    /// Helper function to calculate percentile from a vector of latencies
    fn calculate_percentile(latencies: &[f64], percentile: f64) -> f64 {
        if latencies.is_empty() {
            return 0.0;
        }

        let mut sorted = latencies.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let index = ((percentile / 100.0) * (sorted.len() - 1) as f64) as usize;
        sorted[index.min(sorted.len() - 1)]
    }
}

/// MLPerf-compatible report structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlperfReport {
    pub benchmark_name: String,
    pub backend_type: String,
    pub framework: Option<String>,
    pub total_samples: u64,
    pub total_bytes: u64,
    pub throughput_samples_per_sec: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    // Per-stage latency percentiles for detailed analysis
    pub io_p50_latency_ms: f64,
    pub io_p95_latency_ms: f64,
    pub io_p99_latency_ms: f64,
    pub decode_p50_latency_ms: f64,
    pub decode_p95_latency_ms: f64,
    pub decode_p99_latency_ms: f64,
    pub h2d_p50_latency_ms: f64,
    pub h2d_p95_latency_ms: f64,
    pub h2d_p99_latency_ms: f64,
    pub seed: Option<u64>,
    pub data_folder: String,
    pub format: String,
    pub batch_size: usize,
    pub read_threads: usize,
    pub shuffle: bool,
    pub dl_driver_version: String,
    pub s3dlio_version: String,
    // Access order for deterministic validation (not included in CSV to avoid bloat)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub access_order_sample: Vec<String>, // First 10 items for validation
}

impl MlperfReport {
    pub fn from_metrics(metrics: &MlperfMetrics, config: &DlioConfig) -> Self {
        Self {
            benchmark_name: config.model.as_ref()
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| "dl-driver-benchmark".to_string()),
            backend_type: backend_from_uri(&config.dataset.data_folder),
            framework: config.framework.clone(),
            total_samples: metrics.total_samples,
            total_bytes: metrics.total_bytes,
            throughput_samples_per_sec: metrics.throughput_samples_per_sec(),
            p50_latency_ms: metrics.latency_percentile(50.0),
            p95_latency_ms: metrics.latency_percentile(95.0),
            p99_latency_ms: metrics.latency_percentile(99.0),
            // Per-stage latency percentiles
            io_p50_latency_ms: metrics.io_percentile(50.0),
            io_p95_latency_ms: metrics.io_percentile(95.0),
            io_p99_latency_ms: metrics.io_percentile(99.0),
            decode_p50_latency_ms: metrics.decode_percentile(50.0),
            decode_p95_latency_ms: metrics.decode_percentile(95.0),
            decode_p99_latency_ms: metrics.decode_percentile(99.0),
            h2d_p50_latency_ms: metrics.h2d_percentile(50.0),
            h2d_p95_latency_ms: metrics.h2d_percentile(95.0),
            h2d_p99_latency_ms: metrics.h2d_percentile(99.0),
            seed: config.reader.seed,
            data_folder: config.dataset.data_folder.clone(),
            format: config.dataset.format.clone(),
            batch_size: config.reader.batch_size.unwrap_or(1),
            read_threads: config.reader.read_threads.unwrap_or(1),
            shuffle: config.reader.shuffle.unwrap_or(false),
            dl_driver_version: env!("CARGO_PKG_VERSION").to_string(),
            // Note: s3dlio version matches s3dlio/Cargo.toml version 0.8.1
            // When s3dlio is updated, update this version string accordingly
            s3dlio_version: "0.8.1".to_string(),
            // Include first 10 access order items for deterministic validation
            access_order_sample: metrics.visited_items.iter()
                .take(10)
                .cloned()
                .collect(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize MLPerf report to JSON")
    }

    pub fn to_csv_header() -> String {
        "benchmark_name,backend_type,framework,total_samples,total_bytes,throughput_samples_per_sec,p50_latency_ms,p95_latency_ms,p99_latency_ms,io_p50_latency_ms,io_p95_latency_ms,io_p99_latency_ms,decode_p50_latency_ms,decode_p95_latency_ms,decode_p99_latency_ms,h2d_p50_latency_ms,h2d_p95_latency_ms,h2d_p99_latency_ms,batch_size,read_threads,shuffle,data_folder,dl_driver_version,s3dlio_version".to_string()
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{:.2},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{},{}",
            self.benchmark_name,
            self.backend_type,
            self.framework.as_deref().unwrap_or("none"),
            self.total_samples,
            self.total_bytes,
            self.throughput_samples_per_sec,
            self.p50_latency_ms,
            self.p95_latency_ms,
            self.p99_latency_ms,
            self.io_p50_latency_ms,
            self.io_p95_latency_ms,
            self.io_p99_latency_ms,
            self.decode_p50_latency_ms,
            self.decode_p95_latency_ms,
            self.decode_p99_latency_ms,
            self.h2d_p50_latency_ms,
            self.h2d_p95_latency_ms,
            self.h2d_p99_latency_ms,
            self.batch_size,
            self.read_threads,
            self.shuffle,
            self.data_folder,
            self.dl_driver_version,
            self.s3dlio_version
        )
    }
}

fn backend_from_uri(uri: &str) -> String {
    if uri.starts_with("s3://") {
        "s3"
    } else if uri.starts_with("az://") {
        "azure"
    } else if uri.starts_with("directio://") || uri.starts_with("direct://") {
        "directio"
    } else {
        "file"
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    #[test]
    fn test_backend_detection() {
        assert_eq!(backend_from_uri("file:///tmp/test"), "file");
        assert_eq!(backend_from_uri("s3://bucket/path"), "s3");
        assert_eq!(backend_from_uri("az://account/container"), "azure");
        assert_eq!(backend_from_uri("directio:///tmp/direct"), "directio");
        assert_eq!(backend_from_uri("direct:///tmp/direct"), "directio");
    }

    #[test]
    fn test_mlperf_report_json_serialization() {
        let config = DlioConfig {
            model: Some(Model { 
                name: Some("test_model".to_string()), 
                model_size: Some(1024) 
            }),
            framework: Some("pytorch".to_string()),
            workflow: None,
            dataset: Dataset {
                data_folder: "s3://test-bucket/data".to_string(),
                format: "npz".to_string(),
                num_files_train: Some(100),
                num_files_eval: None,
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(10),
                compression: None,
            },
            reader: Reader {
                batch_size: Some(32),
                prefetch: None,
                shuffle: Some(true),
                read_threads: Some(4),
                compute_threads: None,
                drop_last: None,
                seed: Some(42),
                data_loader: None,
            },
            checkpoint: None,
        };

        let metrics = MlperfMetrics {
            total_samples: 1000,
            total_bytes: 1024000,
            batch_latencies_ms: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            ..Default::default()
        };

        let report = MlperfReport::from_metrics(&metrics, &config);
        
        assert_eq!(report.benchmark_name, "test_model");
        assert_eq!(report.backend_type, "s3");
        assert_eq!(report.framework, Some("pytorch".to_string()));
        assert_eq!(report.total_samples, 1000);
        assert_eq!(report.batch_size, 32);
        assert_eq!(report.seed, Some(42));

        // Test JSON serialization
        let json = report.to_json().expect("Should serialize to JSON");
        assert!(json.contains("test_model"));
        assert!(json.contains("s3"));
    }
}