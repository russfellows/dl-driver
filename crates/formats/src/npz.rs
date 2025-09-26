// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/formats/src/npz.rs

use anyhow::{Context, Result};
use ndarray::{ArrayD, IxDyn};
use ndarray_npy::WriteNpyExt;
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

            // Write array to memory buffer first
            let mut buffer = Vec::new();
            {
                let mut cursor = Cursor::new(&mut buffer);
                synthetic_array
                    .write_npy(&mut cursor)
                    .with_context(|| format!("Failed to serialize array {}", array_name))?;
            }

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

                // Write array to memory buffer first
                let mut npy_buffer = Vec::new();
                {
                    let mut cursor = Cursor::new(&mut npy_buffer);
                    synthetic_array
                        .write_npy(&mut cursor)
                        .with_context(|| format!("Failed to serialize array {}", array_name))?;
                }

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
