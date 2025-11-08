// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/formats/src/npz.rs
//
// REFACTORING NOTE (November 2025):
// The in-memory .npy serialization code in this file (NpzFormat::array_to_npy_bytes)
// should eventually be moved to s3dlio library as part of its data_formats module.
// s3dlio already supports HDF5, TFRecord, and NPZ reading - this complements that.
// See: s3dlio/.github/ISSUE_TEMPLATE_npy_serialization.md for implementation plan.
// When available in s3dlio, replace local implementation with s3dlio::data_formats call.

use anyhow::{Context, Result};
use ndarray::{ArrayD, IxDyn};
use std::io::{Cursor, Write};
use std::path::Path;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

use crate::Format;

/// NPZ format generator + reader
/// Creates proper ZIP archives containing multiple .npy files
/// Leverages s3dlio's generate_controlled_data for synthetic data
pub struct NpzFormat {
    shape: Vec<usize>,
    num_arrays: usize,
}

impl NpzFormat {
    pub fn new(shape: Vec<usize>, num_arrays: usize) -> Self {
        Self {
            shape,
            num_arrays: num_arrays.max(1), // Ensure at least 1 array
        }
    }

    /// Serialize ndarray to .npy format in memory with zero-copy semantics
    /// Implements NPY 1.0 format: magic (6 bytes) + header_len (2 bytes) + header (dict) + data
    /// 
    /// TODO(REFACTOR): This functionality should be moved to s3dlio library
    /// - s3dlio already has format support (HDF5, TFRecord, NPZ reading)
    /// - This is a temporary implementation until s3dlio adds array_to_npy_bytes()
    /// - See: s3dlio/.github/ISSUE_TEMPLATE_npy_serialization.md
    /// - When available in s3dlio, replace this with: s3dlio::data_formats::array_to_npy_bytes()
    fn array_to_npy_bytes(array: &ArrayD<f32>) -> Result<Vec<u8>> {
        // NPY 1.0 magic number
        const MAGIC: &[u8] = b"\x93NUMPY";
        const VERSION: &[u8] = &[1, 0]; // NPY version 1.0
        
        // Build header dict describing the array
        let shape_str = format!("({}{})", 
            array.shape().iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "),
            if array.ndim() == 1 { "," } else { "" }
        );
        let header = format!(
            "{{'descr': '<f4', 'fortran_order': False, 'shape': {}, }}",
            shape_str
        );
        
        // Header must be padded to 64-byte alignment (including magic + version + len)
        let header_len = header.len();
        let total_prefix = 6 + 2 + 2; // magic + version + header_len field
        let padding_needed = (64 - ((total_prefix + header_len) % 64)) % 64;
        let padded_header = format!("{}{}\n", header, " ".repeat(padding_needed));
        
        // Allocate buffer with exact size (no reallocs)
        let data_bytes = array.len() * std::mem::size_of::<f32>();
        let total_size = 10 + padded_header.len() + data_bytes;
        let mut buffer = Vec::with_capacity(total_size);
        
        // Write header
        buffer.extend_from_slice(MAGIC);
        buffer.extend_from_slice(VERSION);
        buffer.extend_from_slice(&(padded_header.len() as u16).to_le_bytes());
        buffer.extend_from_slice(padded_header.as_bytes());
        
        // Write data with minimal copying - use as_slice_memory_order for contiguous access
        if let Some(slice) = array.as_slice_memory_order() {
            // Zero-copy path: data is contiguous
            unsafe {
                let byte_slice = std::slice::from_raw_parts(
                    slice.as_ptr() as *const u8,
                    slice.len() * std::mem::size_of::<f32>()
                );
                buffer.extend_from_slice(byte_slice);
            }
        } else {
            // Fallback: iterate elements (one copy)
            for &val in array.iter() {
                buffer.extend_from_slice(&val.to_le_bytes());
            }
        }
        
        Ok(buffer)
    }

    /// Create synthetic array data using s3dlio utilities with diverse patterns
    fn create_synthetic_array(&self, array_index: usize) -> Result<ArrayD<f32>> {
        let total_elements = self.shape.iter().product::<usize>();
        let total_bytes = total_elements * std::mem::size_of::<f32>();

        // Use s3dlio's controlled data generation for base synthetic data
        let base_data = s3dlio::generate_controlled_data(total_bytes, array_index, 0);

        // Convert bytes to f32 array with proper patterns
        let data: Vec<f32> = match array_index {
            0 => {
                // Main data array: use s3dlio data + sine wave pattern
                base_data
                    .chunks_exact(4)
                    .enumerate()
                    .map(|(i, chunk)| {
                        let base_val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        (base_val * 0.1) + (i as f32 * 0.1).sin()
                    })
                    .take(total_elements)
                    .collect()
            }
            1 => {
                // Labels array: categorical pattern derived from s3dlio data
                base_data
                    .iter()
                    .enumerate()
                    .map(|(i, &byte)| ((byte as usize + i) % 10) as f32 / 10.0)
                    .take(total_elements)
                    .collect()
            }
            2 => {
                // Metadata array: gradient based on s3dlio data
                base_data
                    .iter()
                    .enumerate()
                    .map(|(i, &byte)| (byte as f32 + i as f32) / (total_elements as f32 + 255.0))
                    .take(total_elements)
                    .collect()
            }
            _ => {
                // Additional arrays: controlled randomness from s3dlio
                base_data
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .take(total_elements)
                    .collect()
            }
        };

        // Reshape to target shape
        ArrayD::from_shape_vec(IxDyn(&self.shape), data)
            .with_context(|| "Failed to reshape synthetic data array")
    }
}

impl Format for NpzFormat {
    fn generate(&self, path: &Path) -> Result<()> {
        // Create a proper NPZ file (ZIP archive containing multiple .npy files)
        let file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create NPZ file at {:?}", path))?;

        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o755);

        // Generate diverse synthetic data arrays using s3dlio utilities
        for i in 0..self.num_arrays {
            let array_name = match i {
                0 => "data.npy",
                1 => "labels.npy",
                2 => "metadata.npy",
                _ => &format!("array_{}.npy", i),
            };

            // Create diverse synthetic data using s3dlio + patterns
            let synthetic_array = self.create_synthetic_array(i)?;

            // Serialize to .npy format in memory
            let buffer = Self::array_to_npy_bytes(&synthetic_array)?;

            // Add to ZIP archive
            zip.start_file(array_name, options)
                .with_context(|| format!("Failed to start ZIP file entry for {}", array_name))?;
            zip.write_all(&buffer)
                .with_context(|| format!("Failed to write array {} to ZIP", array_name))?;
        }

        zip.finish()
            .with_context(|| "Failed to finalize NPZ ZIP archive")?;

        Ok(())
    }

    fn read(&self, path: &Path) -> Result<()> {
        // Validate that it's a proper ZIP file with .npy entries
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open NPZ file at {:?}", path))?;

        let mut archive =
            zip::ZipArchive::new(file).with_context(|| "Failed to read NPZ as ZIP archive")?;

        if archive.is_empty() {
            anyhow::bail!("NPZ file is empty");
        }

        // Verify all entries are .npy files
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .with_context(|| format!("Failed to read ZIP entry {}", i))?;
            let name = entry.name();

            if !name.ends_with(".npy") {
                anyhow::bail!("NPZ contains non-.npy file: {}", name);
            }
        }

        Ok(())
    }
}

/// Streaming format implementation for NPZ
/// Uses s3dlio utilities for data generation
pub struct NpzStreamingFormat {
    shape: Vec<usize>,
    num_arrays: usize,
}

impl NpzStreamingFormat {
    pub fn new(shape: Vec<usize>, num_arrays: usize) -> Self {
        Self {
            shape,
            num_arrays: num_arrays.max(1),
        }
    }

    /// Serialize ndarray to .npy format in memory with zero-copy semantics
    /// Implements NPY 1.0 format: magic (6 bytes) + header_len (2 bytes) + header (dict) + data
    fn array_to_npy_bytes(array: &ArrayD<f32>) -> Result<Vec<u8>> {
        // Delegate to NpzFormat implementation to avoid code duplication
        NpzFormat::array_to_npy_bytes(array)
    }
}

use crate::{FormatMetadata, StreamingFormat};

impl Format for NpzStreamingFormat {
    fn generate(&self, path: &Path) -> Result<()> {
        let format = NpzFormat::new(self.shape.clone(), self.num_arrays);
        format.generate(path)
    }

    fn read(&self, path: &Path) -> Result<()> {
        let format = NpzFormat::new(self.shape.clone(), self.num_arrays);
        format.read(path)
    }
}

impl StreamingFormat for NpzStreamingFormat {
    fn generate_bytes(&self, _filename: &str) -> Result<Vec<u8>> {
        // Generate NPZ data in memory
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                FileOptions::<()>::default().compression_method(CompressionMethod::Deflated);

            // Generate diverse synthetic data arrays using s3dlio utilities
            for i in 0..self.num_arrays {
                let array_name = match i {
                    0 => "data.npy",
                    1 => "labels.npy",
                    2 => "metadata.npy",
                    _ => &format!("array_{}.npy", i),
                };

                // Create diverse synthetic data using s3dlio + patterns
                let synthetic_array = self.create_synthetic_array(i)?;

                // Serialize to .npy format in memory
                let npy_buffer = Self::array_to_npy_bytes(&synthetic_array)?;

                // Add to ZIP archive
                zip.start_file(array_name, options).with_context(|| {
                    format!("Failed to start ZIP file entry for {}", array_name)
                })?;
                zip.write_all(&npy_buffer)
                    .with_context(|| format!("Failed to write array {} to ZIP", array_name))?;
            }

            zip.finish()
                .with_context(|| "Failed to finalize NPZ ZIP archive")?;
        }
        Ok(buffer)
    }

    fn read_from_bytes(&self, data: &[u8]) -> Result<()> {
        // Validate NPZ data from bytes
        let cursor = Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)
            .with_context(|| "Failed to read NPZ data as ZIP archive")?;

        if archive.is_empty() {
            anyhow::bail!("NPZ data is empty");
        }

        // Verify all entries are .npy files
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .with_context(|| format!("Failed to read ZIP entry {}", i))?;
            let name = entry.name();

            if !name.ends_with(".npy") {
                anyhow::bail!("NPZ contains non-.npy file: {}", name);
            }
        }

        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        "npz"
    }

    fn format_metadata(&self) -> FormatMetadata {
        let total_elements = self.shape.iter().product::<usize>();
        let size_per_array = total_elements * std::mem::size_of::<f32>();
        let estimated_size = size_per_array * self.num_arrays;

        FormatMetadata {
            expected_size_bytes: Some(estimated_size),
            compression_ratio: Some(0.7), // ZIP compression typically achieves ~30% reduction
            is_binary: true,
            supports_streaming: true,
        }
    }
}

impl NpzStreamingFormat {
    /// Create synthetic array data using s3dlio utilities with diverse patterns
    fn create_synthetic_array(&self, array_index: usize) -> Result<ArrayD<f32>> {
        // Reuse the same logic as NpzFormat
        let format = NpzFormat::new(self.shape.clone(), self.num_arrays);
        format.create_synthetic_array(array_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array2;
    use std::io::Read;
    use tempfile::NamedTempFile;

    #[test]
    fn test_npy_format_correctness() {
        // Test that our custom .npy serialization produces valid NPY 1.0 format
        let shape = vec![10, 5];
        let format = NpzFormat::new(shape.clone(), 1);
        
        // Create test array
        let test_array = format.create_synthetic_array(0).expect("Failed to create array");
        
        // Serialize with our custom implementation
        let our_bytes = NpzFormat::array_to_npy_bytes(&test_array).expect("Failed to serialize");
        
        // Verify magic number
        assert_eq!(&our_bytes[0..6], b"\x93NUMPY", "Invalid magic number");
        
        // Verify version (NPY 1.0)
        assert_eq!(our_bytes[6], 1, "Invalid major version");
        assert_eq!(our_bytes[7], 0, "Invalid minor version");
        
        // Verify header
        let header_len = u16::from_le_bytes([our_bytes[8], our_bytes[9]]) as usize;
        assert!(header_len > 0, "Zero header length");
        
        let header_start = 10;
        let header_end = header_start + header_len;
        assert!(header_end <= our_bytes.len(), "Header extends beyond data");
        
        let header = std::str::from_utf8(&our_bytes[header_start..header_end])
            .expect("Header is not valid UTF-8");
        
        // Verify header contains required keys
        assert!(header.contains("'descr'"), "Header missing 'descr' key");
        assert!(header.contains("'fortran_order'"), "Header missing 'fortran_order' key");
        assert!(header.contains("'shape'"), "Header missing 'shape' key");
        assert!(header.contains("<f4"), "Header missing f32 dtype");
        assert!(header.ends_with('\n'), "Header must end with newline");
        
        // Verify data size
        let expected_data_size = 10 * 5 * 4; // shape * sizeof(f32)
        let actual_data_size = our_bytes.len() - header_end;
        assert_eq!(actual_data_size, expected_data_size, "Data size mismatch");
    }

    #[test]
    fn test_npy_header_format() {
        // Verify our .npy header follows NPY 1.0 spec
        let shape = vec![3, 4];
        let format = NpzFormat::new(shape, 1);
        let test_array = format.create_synthetic_array(0).expect("Failed to create array");
        let bytes = NpzFormat::array_to_npy_bytes(&test_array).expect("Failed to serialize");
        
        // Check magic number
        assert_eq!(&bytes[0..6], b"\x93NUMPY", "Invalid NPY magic number");
        
        // Check version
        assert_eq!(bytes[6], 1, "Invalid NPY major version");
        assert_eq!(bytes[7], 0, "Invalid NPY minor version");
        
        // Check header length field
        let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        assert!(header_len > 0, "Header length is zero");
        assert!(header_len < 10000, "Header length suspiciously large: {}", header_len);
        
        // Verify header is ASCII and contains required keys
        let header_bytes = &bytes[10..10+header_len];
        let header_str = std::str::from_utf8(header_bytes).expect("Header is not valid UTF-8");
        
        assert!(header_str.contains("'descr'"), "Header missing 'descr' key");
        assert!(header_str.contains("'fortran_order'"), "Header missing 'fortran_order' key");
        assert!(header_str.contains("'shape'"), "Header missing 'shape' key");
        assert!(header_str.contains("<f4"), "Header missing dtype");
        assert!(header_str.ends_with('\n'), "Header must end with newline");
    }

    #[test]
    fn test_npz_contains_valid_npy_files() {
        // Test that NPZ files contain properly formatted .npy entries
        let shape = vec![5, 5];
        let format = NpzFormat::new(shape, 3);
        
        // Generate NPZ file
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        format.generate(temp_file.path()).expect("Failed to generate NPZ");
        
        // Read as ZIP archive
        let file = std::fs::File::open(temp_file.path()).expect("Failed to open NPZ");
        let mut archive = zip::ZipArchive::new(file).expect("Failed to read ZIP");
        
        assert_eq!(archive.len(), 3, "Expected 3 arrays in NPZ");
        
        // Verify each entry is a valid .npy file
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).expect("Failed to get ZIP entry");
            let name = entry.name().to_string();
            
            assert!(name.ends_with(".npy"), "Entry {} is not .npy: {}", i, name);
            
            // Read entry data
            let mut npy_data = Vec::new();
            entry.read_to_end(&mut npy_data).expect("Failed to read ZIP entry");
            
            // Verify NPY magic number
            assert!(npy_data.len() >= 10, "NPY data too short: {} bytes", npy_data.len());
            assert_eq!(&npy_data[0..6], b"\x93NUMPY", "Invalid NPY magic in entry {}", name);
            
            // Verify NPY version
            assert_eq!(npy_data[6], 1, "Invalid NPY major version in entry {}", name);
            assert_eq!(npy_data[7], 0, "Invalid NPY minor version in entry {}", name);
            
            // Verify header is parseable
            let header_len = u16::from_le_bytes([npy_data[8], npy_data[9]]) as usize;
            assert!(header_len > 0, "Zero header length in entry {}", name);
            assert!(10 + header_len < npy_data.len(), "Header extends beyond data in entry {}", name);
        }
    }

    #[test]
    fn test_zero_copy_path_contiguous_array() {
        // Test that zero-copy path works for contiguous arrays
        let array = Array2::<f32>::zeros((100, 50)).into_dyn();
        
        // This should use the zero-copy path (as_slice_memory_order)
        let bytes = NpzFormat::array_to_npy_bytes(&array).expect("Failed to serialize");
        
        // Verify format is correct
        assert_eq!(&bytes[0..6], b"\x93NUMPY", "Invalid magic number");
        assert_eq!(bytes[6], 1, "Invalid major version");
        assert_eq!(bytes[7], 0, "Invalid minor version");
        
        // Verify size: header + 100*50*4 bytes of float32 data
        let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        let expected_data_size = 100 * 50 * 4; // f32 is 4 bytes
        assert_eq!(bytes.len(), 10 + header_len + expected_data_size);
    }

    #[test]
    #[ignore] // Run with: cargo test --package dl-driver-formats test_npy_python_validation --release -- --ignored --nocapture
    fn test_npy_python_validation() {
        // Generate a test NPZ file and validate it with Python
        let shape = vec![10, 5];
        let format = NpzFormat::new(shape, 3);
        
        let test_file = std::path::PathBuf::from("/tmp/test_dl_driver_npz_validation.npz");
        format.generate(&test_file).expect("Failed to generate NPZ");
        
        println!("Generated test NPZ: {}", test_file.display());
        println!("\nTo validate with Python:");
        println!("  cd ../s3dlio");
        println!("  source .venv/bin/activate");
        println!("  python3 ../dl-driver/tests/validate_npy_format.py {}", test_file.display());
        println!("\nOr run directly:");
        println!("  (cd ../s3dlio && source .venv/bin/activate && python3 ../dl-driver/tests/validate_npy_format.py {})", test_file.display());
    }
}

