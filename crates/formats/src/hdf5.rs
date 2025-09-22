// crates/formats/src/hdf5.rs
//
// HDF5 format implementation for DLIO compatibility

use anyhow::{Context, Result};
use hdf5_metno::File;
use ndarray::{ArrayD, IxDyn};
use std::path::Path;
use crate::{Format, StreamingFormat, FormatMetadata};

/// HDF5 format generator and reader
pub struct Hdf5Format {
    shape: Vec<usize>,
    dataset_name: String,
}

impl Hdf5Format {
    /// Create with the desired multidimensional `shape` and dataset name
    pub fn new(shape: Vec<usize>, dataset_name: Option<String>) -> Self {
        Hdf5Format { 
            shape,
            dataset_name: dataset_name.unwrap_or_else(|| "data".to_string()),
        }
    }
}

impl Format for Hdf5Format {
    fn generate(&self, path: &Path) -> Result<()> {
        // Create HDF5 file
        let file = File::create(path)
            .with_context(|| format!("Failed to create HDF5 file at {:?}", path))?;
        
        // Create diverse synthetic data using s3dlio utilities
        let synthetic_array = self.create_synthetic_array()?;
        
        // Create dataset in the file
        let _dataset = file
            .new_dataset::<f32>()
            .shape(&self.shape)
            .create(self.dataset_name.as_str())
            .with_context(|| format!("Failed to create dataset '{}'", self.dataset_name))?;
        
        // Write the synthetic array data
        _dataset.write(&synthetic_array)
            .with_context(|| "Failed to write synthetic data to HDF5 dataset")?;
        
        Ok(())
    }

    fn read(&self, path: &Path) -> Result<()> {
        // Open HDF5 file for reading
        let file = File::open(path)
            .with_context(|| format!("Failed to open HDF5 file at {:?}", path))?;
        
        // Open the dataset
        let dataset = file
            .dataset(self.dataset_name.as_str())
            .with_context(|| format!("Failed to open dataset '{}'", self.dataset_name))?;
        
        // Read the data
        let arr: ArrayD<f32> = dataset.read()
            .with_context(|| "Failed to read data from HDF5 dataset")?;
        
        // Verify shape matches
        if arr.shape() != self.shape.as_slice() {
            anyhow::bail!(
                "HDF5 dataset shape mismatch: expected {:?}, got {:?}",
                self.shape,
                arr.shape()
            );
        }
        
        Ok(())
    }
}

impl Hdf5Format {
    /// Create synthetic array data using s3dlio utilities with diverse patterns
    fn create_synthetic_array(&self) -> Result<ArrayD<f32>> {
        let total_elements = self.shape.iter().product::<usize>();
        let total_bytes = total_elements * std::mem::size_of::<f32>();
        
        // Use s3dlio's controlled data generation for base synthetic data
        let base_data = s3dlio::generate_controlled_data(total_bytes, 0, 0);
        
        // Convert bytes to f32 array with scientific computing patterns
        let data: Vec<f32> = base_data.chunks_exact(4)
            .enumerate()
            .map(|(i, chunk)| {
                let base_val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                // Create scientific computing patterns - temperature/pressure/density fields
                let normalized_base = base_val * 0.001; // Scale down 
                let wave_pattern = (i as f32 * 0.01).sin() * 10.0; // Wave component
                let gradient = (i as f32 / total_elements as f32) * 100.0; // Linear gradient
                normalized_base + wave_pattern + gradient
            })
            .take(total_elements)
            .collect();
        
        // Reshape to target shape
        ArrayD::from_shape_vec(IxDyn(&self.shape), data)
            .with_context(|| "Failed to reshape synthetic HDF5 data array")
    }
}

impl StreamingFormat for Hdf5Format {
    fn generate_bytes(&self, _filename: &str) -> Result<Vec<u8>> {
        // Create a simple HDF5-like binary format in memory
        // Format: [magic: 4 bytes][dataset_name_len: 4 bytes][dataset_name: var][ndim: 4 bytes][shape: ndim*4 bytes][data: shape.product()*4 bytes (f32)]
        
        let element_count = self.shape.iter().product::<usize>();
        let dataset_name_bytes = self.dataset_name.as_bytes();
        let mut buffer = Vec::with_capacity(16 + dataset_name_bytes.len() + self.shape.len() * 4 + element_count * 4);
        
        // Magic number for our simple HDF5-like format
        buffer.extend_from_slice(b"SHD5"); // Simple HDF5
        
        // Dataset name length and name
        buffer.extend_from_slice(&(dataset_name_bytes.len() as u32).to_le_bytes());
        buffer.extend_from_slice(dataset_name_bytes);
        
        // Number of dimensions
        buffer.extend_from_slice(&(self.shape.len() as u32).to_le_bytes());
        
        // Shape dimensions
        for &dim in &self.shape {
            buffer.extend_from_slice(&(dim as u32).to_le_bytes());
        }
        
        // Zero-filled f32 data (simulating the actual array content)
        for _ in 0..element_count {
            buffer.extend_from_slice(&0.0f32.to_le_bytes());
        }
        
        Ok(buffer)
    }
    
    fn read_from_bytes(&self, data: &[u8]) -> Result<()> {
        // Parse our simple HDF5-like binary format
        if data.len() < 12 {
            anyhow::bail!("Invalid HDF5 data: too short");
        }
        
        // Check magic number
        if &data[0..4] != b"SHD5" {
            anyhow::bail!("Invalid HDF5 magic number");
        }
        
        // Read dataset name length
        let name_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        
        if data.len() < 12 + name_len {
            anyhow::bail!("Invalid HDF5 data: insufficient name data");
        }
        
        // Read dataset name
        let name = String::from_utf8(data[8..8 + name_len].to_vec())
            .map_err(|_| anyhow::anyhow!("Invalid HDF5 dataset name encoding"))?;
        
        if name != self.dataset_name {
            anyhow::bail!("HDF5 dataset name mismatch: expected '{}', got '{}'", self.dataset_name, name);
        }
        
        let offset = 8 + name_len;
        
        // Read number of dimensions
        let ndim = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]) as usize;
        
        if data.len() < offset + 4 + ndim * 4 {
            anyhow::bail!("Invalid HDF5 data: insufficient shape data");
        }
        
        // Read shape
        let mut shape = Vec::with_capacity(ndim);
        for i in 0..ndim {
            let shape_offset = offset + 4 + i * 4;
            let dim = u32::from_le_bytes([
                data[shape_offset], data[shape_offset + 1], 
                data[shape_offset + 2], data[shape_offset + 3]
            ]) as usize;
            shape.push(dim);
        }
        
        // Verify shape matches
        if shape != self.shape {
            anyhow::bail!("HDF5 shape mismatch: expected {:?}, got {:?}", self.shape, shape);
        }
        
        // Verify data size (f32 = 4 bytes per element)
        let expected_data_size = shape.iter().product::<usize>() * 4;
        let actual_data_size = data.len() - (offset + 4 + ndim * 4);
        if actual_data_size != expected_data_size {
            anyhow::bail!("HDF5 data size mismatch: expected {}, got {}", expected_data_size, actual_data_size);
        }
        
        Ok(())
    }
    
    fn file_extension(&self) -> &'static str {
        "h5"
    }
    
    fn format_metadata(&self) -> FormatMetadata {
        let element_count = self.shape.iter().product::<usize>();
        FormatMetadata {
            expected_size_bytes: Some(element_count * 4 + 64), // f32 data + HDF5 overhead
            compression_ratio: Some(0.8), // HDF5 can have some compression
            is_binary: true,
            supports_streaming: true,
        }
    }
}

/// Alias for s3dlio integration
pub type Hdf5StreamingFormat = Hdf5Format;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn hdf5_generate_and_read() {
        // Skip test if HDF5 is not available
        if std::env::var("SKIP_HDF5_TESTS").is_ok() {
            return;
        }
        
        // a small 4×5 example
        let fmt = Hdf5Format::new(vec![4, 5], None);
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("h5");
        
        fmt.generate(&path).unwrap();
        fmt.read(&path).unwrap();
    }

    #[test]
    fn hdf5_custom_dataset_name() {
        if std::env::var("SKIP_HDF5_TESTS").is_ok() {
            return;
        }
        
        let fmt = Hdf5Format::new(vec![2, 3], Some("my_data".to_string()));
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("h5");
        
        fmt.generate(&path).unwrap();
        fmt.read(&path).unwrap();
    }
}