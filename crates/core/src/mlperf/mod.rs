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
}

impl MlperfRunner {
    pub fn new(config: DlioConfig, plugins: PluginManager) -> Self {
        let plan = RunPlan::from_config(&config);
        Self { 
            config, 
            plan, 
            plugins, 
            metrics: MlperfMetrics::new(),
        }
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

                // For benchmark purposes, limit to a reasonable number of epochs
                if epoch >= 3 {
                    info!("Completed {} epochs, ending benchmark", epoch);
                    break;
                }
            }

            // Limit steps for reasonable benchmark duration
            if step >= 1000 {
                info!("Reached step limit, ending benchmark");
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
        if self.batch_latencies_ms.is_empty() {
            return 0.0;
        }

        let mut sorted = self.batch_latencies_ms.clone();
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
    pub seed: Option<u64>,
    pub data_folder: String,
    pub format: String,
    pub batch_size: usize,
    pub read_threads: usize,
    pub shuffle: bool,
    pub dl_driver_version: String,
    pub s3dlio_version: String,
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
            seed: config.reader.seed,
            data_folder: config.dataset.data_folder.clone(),
            format: config.dataset.format.clone(),
            batch_size: config.reader.batch_size.unwrap_or(1),
            read_threads: config.reader.read_threads.unwrap_or(1),
            shuffle: config.reader.shuffle.unwrap_or(false),
            dl_driver_version: env!("CARGO_PKG_VERSION").to_string(),
            s3dlio_version: "0.8.1".to_string(), // TODO: Get from s3dlio crate
        }
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize MLPerf report to JSON")
    }

    pub fn to_csv_header() -> String {
        "benchmark_name,backend_type,framework,total_samples,total_bytes,throughput_samples_per_sec,p50_latency_ms,p95_latency_ms,p99_latency_ms,batch_size,read_threads,shuffle,data_folder".to_string()
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{:.2},{:.3},{:.3},{:.3},{},{},{},{}",
            self.benchmark_name,
            self.backend_type,
            self.framework.as_deref().unwrap_or("none"),
            self.total_samples,
            self.total_bytes,
            self.throughput_samples_per_sec,
            self.p50_latency_ms,
            self.p95_latency_ms,
            self.p99_latency_ms,
            self.batch_size,
            self.read_threads,
            self.shuffle,
            self.data_folder
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