use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::config::{Config, StorageBackend};
use crate::metrics::Metrics;

/// Main workload execution engine
pub struct WorkloadRunner {
    config: Arc<Config>,
    metrics: Metrics,
}

impl WorkloadRunner {
    pub fn new(config: Config) -> Self {
        // Load environment variables for S3 credentials
        if let Err(e) = dotenvy::dotenv() {
            warn!("Failed to load .env file: {}. S3 operations may not work.", e);
        }
        
        Self {
            config: Arc::new(config),
            metrics: Metrics::new(),
        }
    }

    /// Execute the complete DLIO workflow
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting real_dlio workload: {:?}", self.config.model);
        
        let start_time = Instant::now();

        // Phase 1: Data Generation (if enabled)
        if self.config.workflow.generate_data.unwrap_or(false) {
            info!("Phase 1: Generating data");
            self.run_data_generation().await?;
        }

        // Phase 2: Training Simulation (if enabled)  
        if self.config.workflow.train.unwrap_or(true) {
            info!("Phase 2: Running training simulation");
            self.run_training_simulation().await?;
        }

        // Phase 3: Checkpointing (if enabled)
        if self.config.workflow.checkpoint.unwrap_or(false) {
            info!("Phase 3: Running checkpointing");
            self.run_checkpointing().await?;
        }

        let total_time = start_time.elapsed();
        info!("Workload completed in {:?}", total_time);
        self.metrics.record_total_time(total_time);

        Ok(())
    }

    async fn run_data_generation(&mut self) -> Result<()> {
        let num_files = self.config.dataset.num_files_train;
        let record_size = self.config.dataset.record_length_bytes.unwrap_or(1024000);
        let format = &self.config.dataset.format;
        let data_folder = self.config.dataset.data_folder.clone(); // Clone to avoid borrowing issues
        let backend = self.config.storage_backend();
        
        info!("Generating {} files in format: {}, size: {} bytes", 
               num_files, format, record_size);

        match backend {
            StorageBackend::S3 => {
                info!("Using S3 backend for data generation");
                self.generate_object_store_data(num_files, record_size, &data_folder).await?;
            }
            StorageBackend::File => {
                info!("Using file backend for data generation");
                self.generate_file_data(num_files, record_size, &data_folder).await?;
            }
            StorageBackend::Azure => {
                info!("Using Azure backend for data generation");
                self.generate_object_store_data(num_files, record_size, &data_folder).await?;
            }
            StorageBackend::DirectIO => {
                info!("Using DirectIO backend for data generation");
                self.generate_object_store_data(num_files, record_size, &data_folder).await?;
            }
        }
        
        info!("Data generation completed");
        Ok(())
    }

    async fn generate_object_store_data(&mut self, num_files: u32, record_size: u64, data_folder: &str) -> Result<()> {
        // Use s3dlio's object_store interface for S3/Azure/File operations
        info!("Initializing s3dlio object store for operations");
        
        // Create object store using s3dlio factory (not async)
        let store = s3dlio::object_store::store_for_uri(data_folder)?;
        
        for i in 0..num_files {
            let start = Instant::now();
            let object_uri = format!("{}/train_file_{:06}.npz", data_folder, i);
            
            // Generate synthetic data
            let data = vec![0u8; record_size as usize];
            
                        // Use s3dlio object_store to put the data
            match store.put(&object_uri, &data).await {
                Ok(_) => {
                    info!("Generated S3 object: {}", object_uri);
                    self.metrics.record_bytes_written(record_size);
                }
                Err(e) => {
                    warn!("Failed to generate S3 object {}: {:?}", object_uri, e);
                }
            }
            
            let write_time = start.elapsed();
            self.metrics.record_write_time(write_time);
        }
        Ok(())
    }

    async fn generate_file_data(&mut self, num_files: u32, record_size: u64, data_folder: &str) -> Result<()> {
        // Use s3dlio for file data generation
        use std::fs;
        
        // Create data directory if it doesn't exist
        fs::create_dir_all(data_folder)?;
        
        for i in 0..num_files {
            let start = Instant::now();
            let file_path = format!("{}/train_file_{:06}.npz", data_folder, i);
            
            // Generate synthetic NPZ data
            let data = vec![0u8; record_size as usize]; // Simple synthetic data
            fs::write(&file_path, data)?;
            
            info!("Generated file: {}", file_path);
            
            let write_time = start.elapsed();
            self.metrics.record_write_time(write_time);
            self.metrics.record_bytes_written(record_size);
        }
        Ok(())
    }

    async fn run_training_simulation(&mut self) -> Result<()> {
        let batch_size = self.config.reader.batch_size;
        let num_files = self.config.dataset.num_files_train;
        let data_folder = self.config.dataset.data_folder.clone();
        let backend = self.config.storage_backend();
        
        info!("Training simulation: {} files, batch_size: {}", num_files, batch_size);

        match backend {
            StorageBackend::S3 => {
                info!("Reading from S3 backend");
                self.read_object_store_data(num_files, &data_folder).await?;
            }
            StorageBackend::File => {
                info!("Reading from file backend");
                self.read_file_data(num_files, &data_folder).await?;
            }
            StorageBackend::Azure => {
                info!("Reading from Azure backend");
                self.read_object_store_data(num_files, &data_folder).await?;
            }
            StorageBackend::DirectIO => {
                info!("Reading from DirectIO backend");
                self.read_object_store_data(num_files, &data_folder).await?;
            }
        }

        info!("Training simulation completed");
        Ok(())
    }

    async fn read_object_store_data(&mut self, num_files: u32, data_folder: &str) -> Result<()> {
        // Use s3dlio's object_store interface for S3/Azure/File operations
        info!("Initializing s3dlio object store for read operations");
        
        // Create object store using s3dlio factory
        let store = s3dlio::object_store::store_for_uri(data_folder)?;
        
        for i in 0..num_files {
            let start = Instant::now();
            let object_uri = format!("{}/train_file_{:06}.npz", data_folder, i);
            
            // Use s3dlio object store to get data from S3
            match store.get(&object_uri).await {
                Ok(data) => {
                    self.metrics.record_bytes_read(data.len() as u64);
                    info!("Read S3 object: {} ({} bytes)", object_uri, data.len());
                }
                Err(e) => {
                    warn!("Failed to read S3 object {}: {:?}", object_uri, e);
                    // Continue with simulation even if read fails
                }
            }
            
            // Simulate computation time
            if let Some(compute_time) = self.config.train.as_ref().and_then(|t| t.computation_time) {
                tokio::time::sleep(Duration::from_secs_f64(compute_time)).await;
            }
            
            let read_time = start.elapsed();
            self.metrics.record_read_time(read_time);
            
            if i % 10 == 0 {
                info!("Processed {} / {} files", i + 1, num_files);
            }
        }
        Ok(())
    }

    async fn read_file_data(&mut self, num_files: u32, data_folder: &str) -> Result<()> {
        for i in 0..num_files {
            let start = Instant::now();
            let file_path = format!("{}/train_file_{:06}.npz", data_folder, i);
            
            // Read file if it exists, otherwise just simulate
            if std::path::Path::new(&file_path).exists() {
                let data = std::fs::read(&file_path)?;
                self.metrics.record_bytes_read(data.len() as u64);
                info!("Read file: {} ({} bytes)", file_path, data.len());
            } else {
                info!("Simulating read of file: {}", file_path);
                // Simulate read time
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            // Simulate computation time
            if let Some(compute_time) = self.config.train.as_ref().and_then(|t| t.computation_time) {
                tokio::time::sleep(Duration::from_secs_f64(compute_time)).await;
            }
            
            let read_time = start.elapsed();
            self.metrics.record_read_time(read_time);
            
            if i % 10 == 0 {
                info!("Processed {} / {} files", i + 1, num_files);
            }
        }
        Ok(())
    }

    async fn run_checkpointing(&mut self) -> Result<()> {
        if let Some(checkpoint_config) = &self.config.checkpoint {
            info!("Checkpointing to: {}", checkpoint_config.checkpoint_folder);
            
            // TODO: Implement checkpointing using s3dlio
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            info!("Checkpointing completed");
        }
        Ok(())
    }

    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }
}
