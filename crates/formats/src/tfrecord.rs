// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/formats/src/tfrecord.rs
//
// TFRecord format implementation for DLIO compatibility
// Based on s3dlio's proper TFRecord format implementation

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::{Format, FormatMetadata, StreamingFormat};

/// TFRecord format generator and reader
///
/// Implements proper TensorFlow TFRecord format with:
/// - 8-byte length header (little-endian)
/// - 4-byte masked CRC for length
/// - Data payload
/// - 4-byte masked CRC for data
pub struct TfRecordFormat {
    num_records: usize,
    target_record_size: usize,
}

impl TfRecordFormat {
    /// Create with the desired number of records and target record size
    pub fn new(num_records: usize, target_record_size: usize) -> Self {
        TfRecordFormat {
            num_records,
            target_record_size,
        }
    }

    /// CRC32C masking function as defined in TFRecord specification
    /// TensorFlow uses CRC-32C (Castagnoli), not CRC-32 (IEEE)
    fn masked_crc32c(bytes: &[u8]) -> u32 {
        let crc = crc32c::crc32c(bytes);
        // TensorFlow's mask formula
        crc.rotate_right(15).wrapping_add(0xa282_ead8)
    }

    /// Write a single TFRecord with proper format structure
    fn write_raw_record<W: Write>(writer: &mut W, data: &[u8]) -> Result<usize> {
        // 1. Write length (8 bytes, little-endian)
        let len = data.len() as u64;
        let len_buf = len.to_le_bytes();
        writer
            .write_all(&len_buf)
            .with_context(|| "Failed to write record length")?;

        // 2. Write masked CRC32C of length (4 bytes, little-endian)
        let len_crc = Self::masked_crc32c(&len_buf);
        writer
            .write_all(&len_crc.to_le_bytes())
            .with_context(|| "Failed to write length CRC")?;

        // 3. Write data payload
        writer
            .write_all(data)
            .with_context(|| "Failed to write record data")?;

        // 4. Write masked CRC32C of data (4 bytes, little-endian)
        let data_crc = Self::masked_crc32c(data);
        writer
            .write_all(&data_crc.to_le_bytes())
            .with_context(|| "Failed to write data CRC")?;

        // Total bytes written: 8 + 4 + data.len() + 4
        Ok(8 + 4 + data.len() + 4)
    }

    /// Create a valid tf.train.Example protocol buffer using s3dlio synthetic data
    fn create_tf_example(&self, record_index: usize) -> Result<Vec<u8>> {
        // Calculate how much data we need to approach target_record_size
        // Account for TFRecord overhead (8+4+4=16 bytes) and protobuf structure overhead (~100-200 bytes)
        let available_for_data = self.target_record_size.saturating_sub(250); // Conservative overhead estimate
        let num_floats = (available_for_data / 4).max(16); // At least 16 floats, each float is 4 bytes

        // Use s3dlio to generate enough synthetic data
        let base_data = s3dlio::generate_controlled_data(num_floats * 4, record_index, 0);

        // Convert bytes to float values like numpy/TensorFlow would use
        let float_values: Vec<f32> = base_data
            .chunks_exact(4)
            .map(|chunk| {
                let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                f32::from_le_bytes(bytes) / 1e6 // Scale to reasonable range
            })
            .collect();

        // Create tf.train.Example with float_list (much more common in TensorFlow)
        // Example { features { feature { key="image" value { float_list { value=[floats] } } } } }

        let mut protobuf = Vec::new();

        // Example message
        // Field 1: features (Features message) - tag 0x0A
        protobuf.push(0x0A);

        let mut features_bytes = Vec::new();

        // Features message
        // Field 1: feature map - tag 0x0A
        features_bytes.push(0x0A);

        let mut map_entry_bytes = Vec::new();

        // Map entry key - tag 0x0A
        map_entry_bytes.push(0x0A);
        map_entry_bytes.push(5); // length of "image"
        map_entry_bytes.extend_from_slice(b"image");

        // Map entry value (Feature message) - tag 0x12
        map_entry_bytes.push(0x12);

        let mut feature_bytes = Vec::new();

        // Feature with float_list - field 2, tag 0x12 (not bytes_list)
        feature_bytes.push(0x12);

        let mut float_list_bytes = Vec::new();

        // FloatList with packed floats - field 1, tag 0x0A (wire type 2 = length-delimited)
        // This is the correct way TensorFlow encodes float arrays
        float_list_bytes.push(0x0A); // field 1, wire type 2 (packed repeated)

        // Length of packed float data (4 bytes per float) - MUST be proper varint, not single byte!
        let packed_length = float_values.len() * 4;
        Self::encode_varint(&mut float_list_bytes, packed_length as u64);

        // Pack all floats together (no field tags between them)
        for &f in &float_values {
            float_list_bytes.extend_from_slice(&f.to_le_bytes());
        }

        // Assemble from inside out
        Self::encode_varint(&mut feature_bytes, float_list_bytes.len() as u64);
        feature_bytes.extend(float_list_bytes);

        Self::encode_varint(&mut map_entry_bytes, feature_bytes.len() as u64);
        map_entry_bytes.extend(feature_bytes);

        Self::encode_varint(&mut features_bytes, map_entry_bytes.len() as u64);
        features_bytes.extend(map_entry_bytes);

        Self::encode_varint(&mut protobuf, features_bytes.len() as u64);
        protobuf.extend(features_bytes);

        Ok(protobuf)
    }

    /// Encode a varint (variable-length integer) in protobuf format
    fn encode_varint(buffer: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            buffer.push((value & 0x7F) as u8 | 0x80);
            value >>= 7;
        }
        buffer.push(value as u8);
    }
}

impl Format for TfRecordFormat {
    fn generate(&self, path: &Path) -> Result<()> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create TFRecord file at {:?}", path))?;

        let mut writer = BufWriter::new(file);

        // Generate records with proper TFRecord format structure
        for i in 0..self.num_records {
            // Create proper tf.train.Example protocol buffer data using s3dlio utilities
            let example_protobuf = self.create_tf_example(i)?;

            // Write proper TFRecord with CRCs
            Self::write_raw_record(&mut writer, &example_protobuf)
                .with_context(|| format!("Failed to write TFRecord {}", i))?;
        }

        writer
            .flush()
            .with_context(|| "Failed to flush TFRecord file")?;

        Ok(())
    }

    fn read(&self, path: &Path) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open TFRecord file at {:?}", path))?;

        let mut reader = BufReader::new(file);
        let mut records_read = 0;

        // Read records with proper TFRecord format structure
        while records_read < self.num_records {
            // Read length (8 bytes)
            let mut length_bytes = [0u8; 8];
            match reader.read_exact(&mut length_bytes) {
                Ok(()) => {}
                Err(_) if records_read == self.num_records => break,
                Err(e) => return Err(e).with_context(|| "Failed to read record length")?,
            }

            let length = u64::from_le_bytes(length_bytes) as usize;

            // TFRecord payloads are variable-length, validate CRC instead
            // Read and validate length CRC
            let mut length_crc_bytes = [0u8; 4];
            reader.read_exact(&mut length_crc_bytes).with_context(|| {
                format!("Failed to read length CRC for record {}", records_read)
            })?;

            let expected_len_crc = u32::from_le_bytes(length_crc_bytes);
            let actual_len_crc = Self::masked_crc32c(&length_bytes);
            if actual_len_crc != expected_len_crc {
                anyhow::bail!("Length CRC32C mismatch at record {}", records_read);
            }

            // Read record data
            let mut record_data = vec![0u8; length];
            reader
                .read_exact(&mut record_data)
                .with_context(|| format!("Failed to read record {} data", records_read))?;

            // Read data CRC (4 bytes, ignored for now)
            let mut data_crc_bytes = [0u8; 4];
            reader
                .read_exact(&mut data_crc_bytes)
                .with_context(|| format!("Failed to read record {} data CRC", records_read))?;

            records_read += 1;
        }

        // Verify we read the expected number of records
        if records_read != self.num_records {
            anyhow::bail!(
                "TFRecord count mismatch: expected {} records, got {}",
                self.num_records,
                records_read
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn tfrecord_generate_and_read() {
        // 10 records of 128 bytes each
        let fmt = TfRecordFormat::new(10, 128);
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("tfrecord");

        fmt.generate(&path).unwrap();
        fmt.read(&path).unwrap();
    }

    #[test]
    fn tfrecord_single_record() {
        let fmt = TfRecordFormat::new(1, 64);
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("tfrecord");

        fmt.generate(&path).unwrap();
        fmt.read(&path).unwrap();
    }

    #[test]
    fn tfrecord_large_records() {
        let fmt = TfRecordFormat::new(5, 1024);
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("tfrecord");

        fmt.generate(&path).unwrap();
        fmt.read(&path).unwrap();
    }
}

impl StreamingFormat for TfRecordFormat {
    fn generate_bytes(&self, _filename: &str) -> Result<Vec<u8>> {
        // Generate TFRecord format in memory using proper tf.train.Example protos
        let mut buffer = Vec::new();

        // Generate records with proper TFRecord format structure
        for i in 0..self.num_records {
            // Create proper tf.train.Example protocol buffer (variable-length!)
            let example_protobuf = self.create_tf_example(i)?;

            // Write proper TFRecord with CRCs to in-memory buffer
            Self::write_raw_record(&mut buffer, &example_protobuf)
                .with_context(|| format!("Failed to write TFRecord {} to buffer", i))?;
        }

        Ok(buffer)
    }

    fn read_from_bytes(&self, data: &[u8]) -> Result<()> {
        // Parse proper TFRecord format from memory
        let mut offset = 0;
        let mut records_read = 0;

        while records_read < self.num_records && offset < data.len() {
            // Read length (8 bytes)
            if offset + 8 > data.len() {
                anyhow::bail!(
                    "TFRecord: insufficient data for length at record {}",
                    records_read
                );
            }

            let length = u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]) as usize;
            offset += 8;

            // TFRecord payloads are variable-length, validate CRC instead
            // Validate length CRC (4 bytes)
            if offset + 4 > data.len() {
                anyhow::bail!(
                    "TFRecord: insufficient data for length CRC at record {}",
                    records_read
                );
            }

            let length_bytes = [
                data[offset - 8],
                data[offset - 7],
                data[offset - 6],
                data[offset - 5],
                data[offset - 4],
                data[offset - 3],
                data[offset - 2],
                data[offset - 1],
            ];
            let expected_len_crc = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let actual_len_crc = Self::masked_crc32c(&length_bytes);
            if actual_len_crc != expected_len_crc {
                anyhow::bail!("Length CRC32C mismatch at record {}", records_read);
            }

            // Skip past length CRC (already processed)
            if offset + 4 > data.len() {
                anyhow::bail!(
                    "TFRecord: insufficient data for length CRC at record {}",
                    records_read
                );
            }
            offset += 4;

            // Read and validate record data
            if offset + length > data.len() {
                anyhow::bail!(
                    "TFRecord: insufficient data for record {} data",
                    records_read
                );
            }
            let record_data = &data[offset..offset + length];
            offset += length;

            // Validate data CRC (4 bytes)
            if offset + 4 > data.len() {
                anyhow::bail!(
                    "TFRecord: insufficient data for data CRC at record {}",
                    records_read
                );
            }
            let expected_data_crc = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let actual_data_crc = Self::masked_crc32c(record_data);
            if actual_data_crc != expected_data_crc {
                anyhow::bail!("Data CRC32C mismatch at record {}", records_read);
            }
            offset += 4;

            records_read += 1;
        }

        // Verify we read the expected number of records
        if records_read != self.num_records {
            anyhow::bail!(
                "TFRecord count mismatch: expected {} records, got {}",
                self.num_records,
                records_read
            );
        }

        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        "tfrecord"
    }

    fn format_metadata(&self) -> FormatMetadata {
        FormatMetadata {
            expected_size_bytes: None, // TFRecord payloads are variable-length, cannot predict exact size
            compression_ratio: Some(1.0), // No compression in basic TFRecord
            is_binary: true,
            supports_streaming: true,
        }
    }
}

/// Alias for s3dlio integration  
pub type TfRecordStreamingFormat = TfRecordFormat;
