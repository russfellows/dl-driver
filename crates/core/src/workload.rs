use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::metrics::Metrics;

// Import s3dlio 0.8.0 functionality - using new advanced API
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset, PoolConfig};
use s3dlio::object_store::{store_for_uri, ObjectStore};
use s3dlio::{LoaderOptions, ReaderMode};

/// Main workload execution engine using s3dlio capabilities
pub struct WorkloadRunner {
    config: Arc<Config>,
    metrics: Arc<Metrics>,
}

impl WorkloadRunner {
    pub fn new(config: Config) -> Self {
        // Load environment variables for S3 credentials
        if let Err(e) = dotenvy::dotenv() {
            warn!("Could not load .env file: {}", e);
        }

        Self {
            config: Arc::new(config),
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Execute the complete DLIO workflow using s3dlio capabilities
    pub async fn run(&mut self) -> Result<()> {
        info!(
            "Starting real_dlio workload with s3dlio integration: {:?}",
            self.config.model
        );

        let start_time = Instant::now();

        // Phase 1: Data Generation (if enabled)
        if self.config.workflow.generate_data.unwrap_or(false) {
            info!("Phase 1: Generating data");
            self.run_data_generation().await?;
        }

        // Phase 2: Training (if enabled)
        if self.config.workflow.train.unwrap_or(false) {
            info!("Phase 2: Running training with s3dlio integration");
            self.run_training().await?;
        }

        // Phase 3: Checkpointing (if enabled)
        if self.config.workflow.checkpoint.unwrap_or(false) {
            info!("Phase 3: Running checkpointing");
            self.run_checkpointing().await?;
        }

        let total_time = start_time.elapsed();
        info!("Workload completed in {:?}", total_time);

        // Record total time for tests
        self.metrics.set_total_time(total_time);
        self.metrics.print_summary();
        Ok(())
    }

    /// Data generation phase using s3dlio for high-performance storage operations
    async fn run_data_generation(&mut self) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting data generation phase");

        // Create object store for the configured storage backend
        let store = self.create_object_store()?;

        let num_files = self.config.dataset.num_files_train;
        let samples_per_file = self.config.dataset.num_samples_per_file.unwrap_or(1) as usize;
        let record_size = self.config.dataset.record_length_bytes.unwrap_or(1024) as usize;

        info!(
            "Generating {} files with {} samples each ({}B per record)",
            num_files, samples_per_file, record_size
        );

        // Generate data files using s3dlio's object store
        for file_idx in 0..num_files {
            // Create full URI path by combining base data folder with filename
            let file_name = format!("train_file_{:06}.{}", file_idx, self.config.dataset.format);
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
            let bytes_written = (samples_per_file * record_size) as u64;
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

    /// Training phase using s3dlio's advanced DataLoader capabilities
    async fn run_training(&mut self) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting training phase with s3dlio DataLoader");

        let epochs = self.config.train.as_ref().map(|t| t.epochs).unwrap_or(1);

        let batch_size = self.config.reader.batch_size as usize;
        let num_workers = self.config.reader.read_threads.unwrap_or(4) as usize;
        let prefetch_size = 16; // Default prefetch size

        info!(
            "Training for {} epochs with batch_size={}, num_workers={}, prefetch={}",
            epochs, batch_size, num_workers, prefetch_size
        );

        // Create dataset using s3dlio's MultiBackendDataset for unified backend support
        let data_folder = &self.config.dataset.data_folder;
        let dataset = self.create_multi_backend_dataset(data_folder).await?;

        info!("Created dataset with {} files", dataset.len());

        for epoch in 0..epochs {
            let epoch_start = Instant::now();
            info!("Starting epoch {}/{}", epoch + 1, epochs);

            // Use s3dlio's AsyncPoolDataLoader for high-performance async loading
            let loader_options = LoaderOptions::default()
                .with_batch_size(batch_size)
                .num_workers(num_workers)
                .prefetch(prefetch_size)
                .shuffle(true, epoch as u64) // Use epoch as seed for reproducible shuffling
                .reader_mode(ReaderMode::Range) // Use range requests for better performance
                .part_size(8 * 1024 * 1024) // 8MB parts for range requests
                .max_inflight_parts(4)
                .async_pool_loading_with_config(PoolConfig {
                    pool_size: num_workers * 4,
                    readahead_batches: prefetch_size / batch_size,
                    batch_timeout: std::time::Duration::from_secs(30),
                    max_inflight: num_workers * 8,
                });

            let async_loader = AsyncPoolDataLoader::new(dataset.clone(), loader_options);
            let pool_config = PoolConfig {
                pool_size: num_workers * 4,
                readahead_batches: prefetch_size / batch_size,
                batch_timeout: std::time::Duration::from_secs(30),
                max_inflight: num_workers * 8,
            };

            let mut stream = async_loader.stream_with_pool(pool_config);
            let mut batch_count = 0;
            let mut total_samples = 0;
            let mut total_bytes = 0;

            info!("Starting batch processing for epoch {}", epoch + 1);

            // Process batches using s3dlio's async streaming
            while let Some(batch_result) = stream.next().await {
                match batch_result {
                    Ok(batch) => {
                        let batch_start = Instant::now();
                        let batch_size_actual = batch.len();
                        let batch_bytes: usize = batch.iter().map(|item| item.len()).sum();

                        // Simulate processing the batch
                        self.process_batch(&batch).await?;

                        let batch_time = batch_start.elapsed();
                        self.metrics.record_bytes_read(batch_bytes as u64);
                        self.metrics.record_read_time(batch_time);

                        batch_count += 1;
                        total_samples += batch_size_actual;
                        total_bytes += batch_bytes;

                        if batch_count % 10 == 0 {
                            debug!(
                                "Processed batch {}: {} samples, {} bytes in {:?}",
                                batch_count, batch_size_actual, batch_bytes, batch_time
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Error processing batch: {}", e);
                        return Err(e.into());
                    }
                }
            }

            let epoch_time = epoch_start.elapsed();
            info!(
                "Completed epoch {}/{}: {} batches, {} samples, {} total bytes in {:?}",
                epoch + 1,
                epochs,
                batch_count,
                total_samples,
                total_bytes,
                epoch_time
            );
        }

        let training_time = start_time.elapsed();
        info!("Training completed in {:?}", training_time);
        Ok(())
    }

    /// Checkpointing phase (placeholder for future implementation)
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
        match self.config.dataset.format.as_str() {
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

    /// Process a batch of data (simulate training computation)
    async fn process_batch(&self, batch: &[Vec<u8>]) -> Result<()> {
        // Simulate processing time proportional to batch size
        let processing_delay = std::time::Duration::from_millis(
            (batch.len() as u64 * 10).min(100), // 10ms per sample, max 100ms per batch
        );

        tokio::time::sleep(processing_delay).await;
        Ok(())
    }
}
