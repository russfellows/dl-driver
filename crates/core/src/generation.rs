// src/generation.rs
//
// Dataset generation functionality for DLIO benchmark compatibility

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{info, debug};
use real_dlio_formats::{FormatFactory, Format};
use crate::dlio_compat::{RunPlan, DatasetSplit};
use crate::metrics::Metrics;

/// Dataset generator that creates synthetic datasets based on DLIO configurations
pub struct DatasetGenerator {
    run_plan: RunPlan,
}

impl DatasetGenerator {
    /// Create a new dataset generator from a RunPlan
    pub fn new(run_plan: RunPlan) -> Self {
        Self { run_plan }
    }

    /// Generate the complete dataset according to the RunPlan configuration
    pub async fn generate_dataset(&self, metrics: &mut Metrics) -> Result<()> {
        info!("Starting dataset generation for DLIO benchmark");
        
        // Create the data directory structure
        let data_dir = self.create_data_directory().await?;
        
        // Generate training files
        if self.run_plan.dataset.train.num_files > 0 {
            self.generate_train_files(&data_dir, metrics).await
                .context("Failed to generate training files")?;
        }
        
        // Generate validation files if configured
        if let Some(eval_plan) = &self.run_plan.dataset.eval {
            if eval_plan.num_files > 0 {
                self.generate_eval_files(&data_dir, metrics).await
                    .context("Failed to generate evaluation files")?;
            }
        }
        
        info!("Dataset generation completed successfully");
        Ok(())
    }

    /// Create the data directory structure
    async fn create_data_directory(&self) -> Result<PathBuf> {
        // Convert URI to local path if needed
        let data_path = if self.run_plan.dataset.data_folder_uri.starts_with("file://") {
            Path::new(&self.run_plan.dataset.data_folder_uri[7..]) // Strip "file://"
        } else {
            Path::new(&self.run_plan.dataset.data_folder_uri)
        };
        
        // Create directory if it doesn't exist
        if !data_path.exists() {
            fs::create_dir_all(data_path).await
                .with_context(|| format!("Failed to create data directory: {:?}", data_path))?;
            info!("Created data directory: {:?}", data_path);
        }
        
        Ok(data_path.to_path_buf())
    }

    /// Generate training files
    async fn generate_train_files(&self, data_dir: &Path, metrics: &mut Metrics) -> Result<()> {
        info!("Generating {} training files", self.run_plan.dataset.train.num_files);
        
        let format_impl = self.create_format_instance(&self.run_plan.dataset.train)?;
        let format_extension = self.get_format_extension();
        
        for i in 0..self.run_plan.dataset.train.num_files {
            let filename = format!("train_file_{:06}.{}", i, format_extension);
            let file_path = data_dir.join(&filename);
            
            // Generate the file
            let start_time = std::time::Instant::now();
            format_impl.generate(&file_path)
                .with_context(|| format!("Failed to generate training file: {}", filename))?;
            let generation_time = start_time.elapsed();
            
            // Update metrics
            let file_size = fs::metadata(&file_path).await
                .with_context(|| format!("Failed to get metadata for {}", filename))?
                .len();
            
            metrics.record_file_generated(filename, file_size, generation_time);
            
            if i % 100 == 0 {
                debug!("Generated training file {}/{}", i + 1, self.run_plan.dataset.train.num_files);
            }
        }
        
        info!("Training file generation completed");
        Ok(())
    }

    /// Generate evaluation files
    async fn generate_eval_files(&self, data_dir: &Path, metrics: &mut Metrics) -> Result<()> {
        let eval_plan = self.run_plan.dataset.eval.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Eval plan not configured"))?;
        
        info!("Generating {} evaluation files", eval_plan.num_files);
        
        let format_impl = self.create_format_instance(eval_plan)?;
        let format_extension = self.get_format_extension();
        
        for i in 0..eval_plan.num_files {
            let filename = format!("eval_file_{:06}.{}", i, format_extension);
            let file_path = data_dir.join(&filename);
            
            // Generate the file
            let start_time = std::time::Instant::now();
            format_impl.generate(&file_path)
                .with_context(|| format!("Failed to generate evaluation file: {}", filename))?;
            let generation_time = start_time.elapsed();
            
            // Update metrics
            let file_size = fs::metadata(&file_path).await
                .with_context(|| format!("Failed to get metadata for {}", filename))?
                .len();
            
            metrics.record_file_generated(filename, file_size, generation_time);
            
            if i % 100 == 0 {
                debug!("Generated evaluation file {}/{}", i + 1, eval_plan.num_files);
            }
        }
        
        info!("Evaluation file generation completed");
        Ok(())
    }

    /// Create a format instance based on the dataset configuration
    fn create_format_instance(&self, plan: &DatasetSplit) -> Result<Box<dyn Format>> {
        // Extract format parameters from the plan
        let shape = self.extract_shape_from_plan(plan);
        let record_length = plan.record_length_bytes;
        let num_records = Some(plan.num_samples_per_file);
        
        FormatFactory::create_format(
            &self.run_plan.dataset.format,
            shape,
            Some(record_length),
            num_records,
        )
    }

    /// Extract shape information from the plan
    fn extract_shape_from_plan(&self, plan: &DatasetSplit) -> Option<Vec<usize>> {
        // For now, use default shapes based on format
        // TODO: Extract actual shape from DLIO config if available
        match self.run_plan.dataset.format.to_lowercase().as_str() {
            "npz" | "hdf5" => {
                // Use image-like shape or from record_length
                let length = plan.record_length_bytes;
                if length > 0 {
                    // Assume square image if possible
                    let side = (length as f64).sqrt() as usize;
                    if side * side == length {
                        Some(vec![side, side])
                    } else {
                        Some(vec![length])
                    }
                } else {
                    Some(vec![224, 224, 3]) // Default image shape
                }
            },
            "tfrecord" => None, // TFRecord uses record_length directly
            _ => None,
        }
    }

    /// Get the file extension for the current format
    fn get_format_extension(&self) -> &str {
        match self.run_plan.dataset.format.to_lowercase().as_str() {
            "npz" => "npz",
            "hdf5" => "h5",
            "tfrecord" => "tfrecord",
            _ => "bin", // Default binary extension
        }
    }
}

/// Progress callback for dataset generation
pub type GenerationProgressCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlio_compat::DlioConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_dataset_generation_npz() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().join("data");
        
        // Create a minimal DLIO config for testing
        let config = DlioConfig {
            model: None,
            framework: None,
            workflow: None,
            dataset: crate::dlio_compat::DatasetConfig {
                data_folder: data_path.to_string_lossy().to_string(),
                format: Some("npz".to_string()),
                num_files_train: Some(5),
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(10),
                num_files_eval: Some(0),
                compression: None,
            },
            reader: crate::dlio_compat::ReaderConfig {
                data_loader: Some("pytorch".to_string()),
                batch_size: Some(32),
                read_threads: Some(4),
                prefetch: None,
                shuffle: Some(false),
                compute_threads: None,
                transfer_size: None,
                file_access_type: None,
                seed: None,
            },
            checkpointing: None,
            profiling: None,
        };
        
        let run_plan = config.to_run_plan().unwrap();
        let generator = DatasetGenerator::new(run_plan);
        let mut metrics = Metrics::new();
        
        generator.generate_dataset(&mut metrics).await.unwrap();
        
        // Verify files were created
        assert!(data_path.exists());
        let entries: Vec<_> = std::fs::read_dir(&data_path).unwrap().collect();
        assert_eq!(entries.len(), 5); // 5 training files
    }

    #[tokio::test]
    async fn test_format_factory_integration() {
        // Test that all supported formats can be created
        let formats = FormatFactory::supported_formats();
        assert!(formats.contains(&"npz"));
        assert!(formats.contains(&"hdf5"));
        assert!(formats.contains(&"tfrecord"));
        
        // Test format creation
        for format_name in formats {
            let format_impl = FormatFactory::create_format(
                format_name,
                Some(vec![10, 10]),
                Some(100),
                Some(5),
            );
            assert!(format_impl.is_ok(), "Failed to create format: {}", format_name);
        }
    }
}