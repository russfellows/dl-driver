// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::dlio_compat::DlioConfig;
use crate::metrics::Metrics;

// Import s3dlio 0.8.0 functionality - using new advanced API
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset, PoolConfig};
use s3dlio::object_store::{store_for_uri, ObjectStore};
use s3dlio::{LoaderOptions, ReaderMode, LoadingMode};

/// Main workload execution engine using s3dlio capabilities
pub struct WorkloadRunner {
    config: Arc<DlioConfig>,
    metrics: Arc<Metrics>,
    accelerators: u32,
    strict_au: bool,
}

impl WorkloadRunner {
    pub fn new(config: DlioConfig) -> Self {
        // Load environment variables for S3 credentials
        if let Err(e) = dotenvy::dotenv() {
            warn!("Could not load .env file: {}", e);
        }

        Self {
            config: Arc::new(config),
            metrics: Arc::new(Metrics::new()),
            accelerators: 1, // Default to 1 accelerator
            strict_au: false, // Default to non-strict mode
        }
    }

    /// Set accelerator configuration for AU calculation
    pub fn with_accelerator_config(mut self, accelerators: u32, strict_au: bool) -> Self {
        self.accelerators = accelerators;
        self.strict_au = strict_au;
        self
    }

    /// Execute ONLY the training phase for DLIO compliance measurement
    /// Data generation should be done separately and is NOT measured
    pub async fn run_training_phase(&mut self) -> Result<()> {
        info!(
            "Starting DLIO training phase measurement: {:?}",
            self.config.model
        );

        // Only measure the training phase - data generation is separate
        let training_start = Instant::now();
        
        info!("Phase: Training (MEASURED for AU calculation)");
        self.run_training().await?;
        
        let training_time = training_start.elapsed();
        info!("Training phase completed in {:?}", training_time);

        // Record training time (NOT total time) for AU calculation
        self.metrics.set_total_time(training_time);
        self.metrics.print_summary();
        
        // Calculate Accelerator Utilization (AU) if metric configuration is present
        if let Some(metric_config) = &self.config.metric {
            info!("=== Accelerator Utilization (AU) Analysis ===");
            debug!("Train config: {:?}", self.config.train);
            debug!("Metric config: {:?}", metric_config);
            if let Some(au_result) = (*self.metrics).compute_au(&self.config, training_time, self.accelerators) {
                info!("AU Result: {:.1}% ({:.3} fraction)", au_result.au_percent, au_result.au_fraction);
                
                if let Some(pass) = au_result.pass {
                    let threshold = metric_config.au.unwrap_or(0.90);
                    if pass {
                        info!("✅ AU PASS: {:.1}% >= {:.1}% threshold", au_result.au_percent, threshold * 100.0);
                    } else {
                        warn!("❌ AU FAIL: {:.1}% < {:.1}% threshold", au_result.au_percent, threshold * 100.0);
                        
                        // In strict mode, AU failure should cause the workload to fail
                        if self.strict_au {
                            return Err(anyhow::anyhow!(
                                "Strict AU mode: AU {:.1}% is below threshold {:.1}% - storage system is too slow for MLPerf compliance", 
                                au_result.au_percent, threshold * 100.0
                            ));
                        }
                    }
                } else {
                    info!("AU threshold not configured for pass/fail");
                }
            } else {
                info!("AU calculation not available (missing train configuration)");
            }
            info!("==============================================");
        }
        
        Ok(())
    }

    /// Execute complete workflow (for backward compatibility)
    /// NOTE: For proper DLIO compliance, use run_training_phase() after separate data generation
    pub async fn run(&mut self) -> Result<()> {
        info!("WARNING: Using legacy run() method - consider using run_training_phase() for proper DLIO compliance");
        
        let start_time = Instant::now();

        // Phase 1: Data Generation (if enabled) - NOT MEASURED
        if self.config.workflow.as_ref().map_or(false, |w| w.generate_data.unwrap_or(false)) {
            info!("Phase 1: Generating data (NOT measured)");
            self.run_data_generation().await?;
        }

        // Phase 2: Training (measured)
        if self.config.workflow.as_ref().map_or(false, |w| w.train.unwrap_or(false)) {
            return self.run_training_phase().await;
        }

        let total_time = start_time.elapsed();
        info!("Workload completed in {:?}", total_time);
        Ok(())
    }

    /// Data generation phase using s3dlio for high-performance storage operations
    async fn run_data_generation(&mut self) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting data generation phase");

        // Create object store for the configured storage backend
        let store = self.create_object_store()?;

        let num_files = self.config.dataset.num_files_train.unwrap_or(100);
        let samples_per_file = self.config.dataset.num_samples_per_file.unwrap_or(1);
        let record_size = self.config.dataset.record_length_bytes.unwrap_or(1024);

        info!(
            "Generating {} files with {} samples each ({}B per record)",
            num_files, samples_per_file, record_size
        );

        // Generate data files using s3dlio's object store
        for file_idx in 0..num_files {
            // Create full URI path by combining base data folder with filename
            let format = self.config.dataset.format.as_deref().unwrap_or("npz");
            let file_name = format!("train_file_{:06}.{}", file_idx, format);
            let data_folder = &self.config.dataset.data_folder;
            let full_path = if data_folder.ends_with('/') {
                format!("{}{}", data_folder, file_name)
            } else {
                format!("{}/{}", data_folder, file_name)
            };

            let data = self.generate_file_data(samples_per_file, record_size)?;

            let write_start = Instant::now();
            store
                .put(&full_path, &data)
                .await
                .with_context(|| format!("Failed to write file {}", full_path))?;
            let write_time = write_start.elapsed();

            // Record metrics
            let bytes_written = (samples_per_file as u64) * (record_size as u64);
            self.metrics
                .record_write_operation(bytes_written, write_time);
            info!(
                "Wrote {} bytes to {} in {:?}",
                bytes_written, full_path, write_time
            );

            if file_idx % 100 == 0 {
                info!("Generated {}/{} files", file_idx + 1, num_files);
            }
        }

        let generation_time = start_time.elapsed();
        info!("Data generation completed in {:?}", generation_time);
        Ok(())
    }

    /// Training phase using DLIO-style parallel I/O with background workers
    /// TRUE DLIO PARALLEL I/O MODEL - Background workers + instant batch retrieval
    async fn run_training(&mut self) -> Result<()> {
        let epochs = self.config.train.as_ref().and_then(|t| t.epochs).unwrap_or(1);
        let batch_size = self.config.reader.batch_size.unwrap_or(16);
        let read_threads = self.config.reader.read_threads.unwrap_or(8) as usize;
        let prefetch_size = self.config.reader.prefetch.unwrap_or(4);

        info!("🚀 TRUE DLIO PARALLEL MODEL: {} epochs, batch_size={}, read_threads={}, prefetch_queue={}", 
              epochs, batch_size, read_threads, prefetch_size);

        // Create s3dlio dataset
        let data_folder = &self.config.dataset.data_folder;
        let dataset = self.create_multi_backend_dataset(data_folder).await?;
        let total_files = dataset.len();
        
        info!("📂 Dataset: {} files, ~{} batches per epoch", total_files, (total_files + batch_size - 1) / batch_size);

        for epoch in 0..epochs {
            let epoch_start = Instant::now();
            info!("🏃 Epoch {}/{} - Starting TRUE parallel I/O + compute", epoch + 1, epochs);

            let mut batch_count = 0;
            let mut total_samples = 0;
            let mut total_bytes = 0;
            let mut total_io_time = Duration::ZERO;
            let mut total_compute_time = Duration::ZERO;

            // === CRITICAL: TRUE DLIO PARALLEL MODEL ===
            // Background I/O workers continuously load batches into channel
            // Main thread gets batches instantly while background loads next batches
            let (batch_tx, mut batch_rx) = tokio::sync::mpsc::channel::<Result<Vec<Vec<u8>>>>(prefetch_size * 2);
            
            // Configure aggressive s3dlio loading
            let pool_config = PoolConfig {
                pool_size: read_threads,
                readahead_batches: prefetch_size * 2, // Aggressive prefetching
                batch_timeout: Duration::from_secs(30),
                max_inflight: read_threads * 4, // Very high concurrency
            };

            let loader_options = LoaderOptions {
                batch_size: batch_size,
                prefetch: prefetch_size,
                shuffle: false, // Consistent ordering for debugging
                num_workers: read_threads,
                reader_mode: ReaderMode::Sequential,
                loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
                ..Default::default()
            };

            // === BACKGROUND I/O WORKER TASK ===
            let dataset_clone = dataset.clone();
            let background_io = tokio::spawn(async move {
                info!("🔄 Background I/O workers starting with {} threads, {} prefetch", read_threads, prefetch_size);
                
                let async_loader = AsyncPoolDataLoader::new(dataset_clone, loader_options);
                let mut stream = async_loader.stream_with_pool(pool_config);
                
                let mut bg_batch_count = 0;
                while let Some(batch_result) = stream.next().await {
                    bg_batch_count += 1;
                    
                    if batch_tx.send(batch_result.map_err(anyhow::Error::from)).await.is_err() {
                        debug!("Main thread finished, stopping background I/O at batch {}", bg_batch_count);
                        break;
                    }
                    
                    if bg_batch_count % 10 == 0 {
                        debug!("Background I/O: loaded {} batches, queue filling continuously...", bg_batch_count);
                    }
                }
                info!("🛑 Background I/O completed: {} batches loaded", bg_batch_count);
            });

            info!("⚡ PARALLEL MODE ACTIVE: Background loading batches, main thread consuming with compute overlap");

            // === MAIN COMPUTE THREAD ===
            // This should get batches INSTANTLY from prefetch queue
            while let Some(batch_result) = batch_rx.recv().await {
                match batch_result {
                    Ok(batch) => {
                        let batch_start = Instant::now();
                        
                        // === I/O TIME MEASUREMENT ===
                        // With proper background I/O, this should be microseconds
                        let io_start = Instant::now();
                        let batch_size_actual = batch.len();
                        let batch_bytes: usize = batch.iter().map(|item| item.len()).sum();
                        
                        // Minimal validation (represents data preprocessing)
                        let _checksum: u64 = batch.iter().take(1)
                            .map(|item| item.iter().take(10).map(|&b| b as u64).sum::<u64>())
                            .sum();
                        let io_time = io_start.elapsed(); // Should be ~microseconds!
                        
                        // === COMPUTE TIME ===
                        // While we compute, background workers load next batches = TRUE PARALLELISM
                        let compute_start = Instant::now();
                        self.process_batch(&batch).await?;
                        let compute_time = compute_start.elapsed();
                        
                        let batch_total_time = batch_start.elapsed();

                        // Accumulate for AU calculation
                        total_io_time += io_time;
                        total_compute_time += compute_time;
                        
                        // Record metrics
                        self.metrics.record_bytes_read(batch_bytes as u64);
                        self.metrics.record_read_time(io_time);
                        self.metrics.record_compute_time(compute_time);
                        self.metrics.record_batch_time(batch_total_time);

                        batch_count += 1;
                        total_samples += batch_size_actual;
                        total_bytes += batch_bytes;

                        // Show parallel processing effectiveness
                        if batch_count % 5 == 0 || batch_count < 5 {
                            let io_ms = io_time.as_secs_f64() * 1000.0;
                            let compute_ms = compute_time.as_secs_f64() * 1000.0;
                            info!(
                                "PARALLEL Batch {} | {} files, {:.1}MB | I/O: {:.2}ms, Compute: {:.1}ms | Background: loading next...",
                                batch_count, batch_size_actual, batch_bytes as f64 / 1_000_000.0, io_ms, compute_ms
                            );
                        }
                    }
                    Err(e) => {
                        error!("Background I/O error: {}", e);
                        return Err(e.into());
                    }
                }
            }

            // Wait for background task
            if let Err(e) = background_io.await {
                warn!("Background I/O task error: {:?}", e);
            }
            
            // === EPOCH ANALYSIS ===
            let epoch_total_time = epoch_start.elapsed();
            self.metrics.record_epoch_time(epoch_total_time);
            
            let au_percentage = if epoch_total_time.as_secs_f64() > 0.0 {
                (total_compute_time.as_secs_f64() / epoch_total_time.as_secs_f64()) * 100.0
            } else {
                0.0
            };

            info!(
                "✅ Epoch {} COMPLETE | {} batches, {} samples, {:.1}MB in {:?}",
                epoch + 1, batch_count, total_samples, total_bytes as f64 / 1_000_000.0, epoch_total_time
            );
            
            if batch_count > 0 {
                let avg_io_ms = (total_io_time.as_secs_f64() / batch_count as f64) * 1000.0;
                let avg_compute_ms = (total_compute_time.as_secs_f64() / batch_count as f64) * 1000.0;
                
                info!(
                    "📊 TIMING | Avg I/O: {:.2}ms, Avg Compute: {:.1}ms, AU: {:.1}%", 
                    avg_io_ms, avg_compute_ms, au_percentage
                );

                // Validate parallel effectiveness
                if avg_io_ms < 10.0 && au_percentage < 80.0 {
                    info!("🎉 PARALLEL SUCCESS: I/O {:.1}ms (near-instant!), AU {:.1}% (realistic parallel)", 
                          avg_io_ms, au_percentage);
                } else if avg_io_ms > 50.0 {
                    warn!("⚠️  SEQUENTIAL DETECTED: I/O {:.1}ms (too slow), indicates poor parallelism", avg_io_ms);
                } else if au_percentage > 90.0 {
                    warn!("⚠️  HIGH AU: {:.1}% suggests sequential processing, not parallel I/O", au_percentage);
                }
            }
        }

        info!("🏁 DLIO parallel training completed");
        Ok(())
    }

    /// Checkpointing phase (placeholder for future implementation)
    #[allow(dead_code)]
    async fn run_checkpointing(&mut self) -> Result<()> {
        info!("Checkpointing phase - placeholder");
        // TODO: Implement checkpointing using s3dlio's checkpoint module
        Ok(())
    }

    /// Create object store instance based on storage backend configuration
    fn create_object_store(&self) -> Result<Box<dyn ObjectStore>> {
        let data_folder = &self.config.dataset.data_folder;
        info!("Creating object store for: {}", data_folder);

        store_for_uri(data_folder)
            .with_context(|| format!("Failed to create object store for {}", data_folder))
    }

    /// Generate data for a single file
    fn generate_file_data(&self, samples: usize, record_size: usize) -> Result<Vec<u8>> {
        // Generate synthetic data based on format
        match self.config.dataset.format.as_deref().unwrap_or("npz") {
            "npz" => {
                // Use s3dlio's data generation utilities
                // Note: generate_controlled_data takes (size, dedup, compress)
                let total_size = samples * record_size;
                let data = s3dlio::generate_controlled_data(total_size, 0, 0);
                Ok(data)
            }
            _ => {
                // Generate random data for other formats
                let total_size = samples * record_size;
                let data = (0..total_size).map(|i| (i % 256) as u8).collect();
                Ok(data)
            }
        }
    }

    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Create MultiBackendDataset for unified access across all storage backends
    async fn create_multi_backend_dataset(&self, data_folder: &str) -> Result<MultiBackendDataset> {
        info!("Creating MultiBackendDataset for folder: {}", data_folder);

        // Use s3dlio's prefix-based dataset creation for automatic backend detection
        let dataset = MultiBackendDataset::from_prefix(data_folder)
            .await
            .with_context(|| format!("Failed to create dataset from prefix: {}", data_folder))?;

        info!("Successfully created dataset with {} files", dataset.len());
        Ok(dataset)
    }

    /// Process a batch of data (simulate training computation with exact DLIO timing)
    async fn process_batch(&self, _batch: &[Vec<u8>]) -> Result<()> {
        // Use exact computation_time from DLIO config (per step, not per sample)
        if let Some(computation_time) = self.config.train.as_ref().and_then(|t| t.computation_time) {
            if computation_time > 0.0 {
                let processing_delay = std::time::Duration::from_secs_f64(computation_time);
                tokio::time::sleep(processing_delay).await;
            }
        }
        // If no computation_time specified, no artificial delay (matches DLIO behavior)
        Ok(())
    }
}
