// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Operation log ingestion and parsing functionality
//!
//! This module provides functionality to parse operation logs in JSONL or TSV format,
//! optionally compressed with zstd. The logs contain storage operation metadata
//! including operation types, file paths, byte counts, and timing information.
//!
//! Supported file formats:
//! - `.jsonl` - JSON Lines format (one JSON object per line)
//! - `.jsonl.zst` - zstd-compressed JSON Lines
//! - `.tsv` - Tab-separated values with header row
//! - `.tsv.zst` - zstd-compressed TSV
//! - `.csv` - Actually TSV format (tab-separated despite .csv extension)
//! - `.csv.zst` - zstd-compressed TSV files (most common for production logs like Warp output)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Operation log record - standardized format for all operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLogRec {
    pub operation: String,          // GET|PUT|LIST|DELETE|HEAD
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,   // Storage endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]  
    pub file: Option<String>,       // Object key/file path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,         // Transfer size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t_start_ns: Option<u64>,    // Start timestamp in nanoseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ns: Option<u64>,   // Operation duration in nanoseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,      // Error message if operation failed
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>, // Additional fields
}

/// Statistical envelope summary of operation log data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub mean_req_bytes: f64,        // Average request size
    pub p95_latency_ms: f64,        // 95th percentile latency in milliseconds  
    pub total_bytes: u64,           // Total bytes transferred
    pub n_ops: u64,                 // Number of operations
    pub operations_breakdown: std::collections::HashMap<String, u64>, // Count per operation type
}

/// Op-log format detection and parsing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpLogFormat {
    Jsonl,
    Tsv,
}

impl OpLogFormat {
    /// Detect format from file path
    pub fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path_str = path.as_ref().to_string_lossy();
        
        if path_str.ends_with(".jsonl") || path_str.ends_with(".jsonl.zst") {
            Ok(OpLogFormat::Jsonl)
        } else if path_str.ends_with(".tsv") || path_str.ends_with(".tsv.zst") || 
                  path_str.ends_with(".csv") || path_str.ends_with(".csv.zst") {
            // Note: .csv files are actually TSV format in practice
            Ok(OpLogFormat::Tsv)
        } else {
            Err(anyhow::anyhow!("Unsupported file format: {}", path_str))
        }
    }
}

/// Reader for op-log files with automatic compression detection
pub struct OpLogReader {
    records: Vec<OpLogRec>,
}

impl OpLogReader {
    /// Load op-log from file with automatic format and compression detection
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let format = OpLogFormat::from_path(path)?;
        let is_compressed = path.extension().map_or(false, |ext| ext == "zst");

        let reader = open_reader(path, is_compressed)?;
        let records = match format {
            OpLogFormat::Jsonl => parse_jsonl(reader)?,
            OpLogFormat::Tsv => parse_tsv(reader)?,
        };

        Ok(OpLogReader { records })
    }

    /// Get all records
    pub fn records(&self) -> &[OpLogRec] {
        &self.records
    }

    /// Filter records by operation type
    pub fn filter_operations(&self, operations: &[&str]) -> Vec<&OpLogRec> {
        self.records
            .iter()
            .filter(|rec| operations.contains(&rec.operation.as_str()))
            .collect()
    }

    /// Get records count
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Open a reader with optional zstd decompression
fn open_reader<P: AsRef<Path>>(path: P, is_compressed: bool) -> Result<Box<dyn BufRead>> {
    let file = File::open(&path)
        .with_context(|| format!("Failed to open file: {}", path.as_ref().display()))?;

    if is_compressed {
        let decoder = zstd::stream::read::Decoder::new(file)
            .with_context(|| "Failed to create zstd decoder")?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Parse JSONL format (one JSON object per line)
fn parse_jsonl(reader: Box<dyn BufRead>) -> Result<Vec<OpLogRec>> {
    let mut records = Vec::new();
    
    for (line_num, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed to read line {}", line_num + 1))?;
        
        // Skip empty lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let record: OpLogRec = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON on line {}: {}", line_num + 1, line))?;
        
        records.push(record);
    }

    Ok(records)
}

/// Parse TSV format with header row
fn parse_tsv(reader: Box<dyn BufRead>) -> Result<Vec<OpLogRec>> {
    let mut lines = reader.lines();
    
    // Read header to determine column mapping
    let header_line = lines.next()
        .ok_or_else(|| anyhow::anyhow!("TSV file is empty"))?
        .with_context(|| "Failed to read TSV header")?;
    
    let headers: Vec<&str> = header_line.split('\t').collect();
    let col_mapping = create_column_mapping(&headers)?;

    let mut records = Vec::new();
    
    for (line_num, line) in lines.enumerate() {
        let line = line.with_context(|| format!("Failed to read line {}", line_num + 2))?;
        
        // Skip empty lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        let record = parse_tsv_record(&fields, &col_mapping)
            .with_context(|| format!("Failed to parse TSV record on line {}", line_num + 2))?;
        
        records.push(record);
    }

    Ok(records)
}

/// Create mapping from column names to indices
fn create_column_mapping(headers: &[&str]) -> Result<std::collections::HashMap<String, usize>> {
    let mut mapping = std::collections::HashMap::new();
    
    for (idx, &header) in headers.iter().enumerate() {
        mapping.insert(header.to_lowercase(), idx);
    }

    // Ensure required fields are present (handle both standard and Warp column names)
    if !mapping.contains_key("operation") && !mapping.contains_key("op") {
        return Err(anyhow::anyhow!("TSV header must contain 'operation' or 'op' column"));
    }

    Ok(mapping)
}

/// Parse a single TSV record using column mapping
fn parse_tsv_record(
    fields: &[&str], 
    col_mapping: &std::collections::HashMap<String, usize>
) -> Result<OpLogRec> {
    let get_field = |name: &str| -> Option<&str> {
        col_mapping.get(name).and_then(|&idx| fields.get(idx)).copied()
    };

    let get_optional_u64 = |name: &str| -> Result<Option<u64>> {
        match get_field(name) {
            Some(s) if !s.is_empty() => Ok(Some(s.parse::<u64>()
                .with_context(|| format!("Invalid {} value: {}", name, s))?)),
            _ => Ok(None),
        }
    };

    let operation = get_field("operation")
        .or_else(|| get_field("op"))
        .ok_or_else(|| anyhow::anyhow!("Missing required 'operation' or 'op' field"))?
        .to_string();

    let record = OpLogRec {
        operation,
        endpoint: get_field("endpoint").map(|s| s.to_string()),
        file: get_field("file").map(|s| s.to_string()),
        bytes: get_optional_u64("bytes")?,
        t_start_ns: get_optional_u64("t_start_ns")?,
        duration_ns: get_optional_u64("duration_ns")?,
        error: get_field("error").map(|s| s.to_string()),
        extra: std::collections::HashMap::new(), // Additional fields not supported in TSV
    };

    Ok(record)
}

/// Summarize operation records into statistical envelope
pub fn summarize_ops(records: &[OpLogRec], only: &[&str]) -> Envelope {
    let filtered_records: Vec<&OpLogRec> = if only.is_empty() {
        records.iter().collect()
    } else {
        records.iter()
            .filter(|rec| only.contains(&rec.operation.as_str()))
            .collect()
    };

    if filtered_records.is_empty() {
        return Envelope {
            mean_req_bytes: 0.0,
            p95_latency_ms: 0.0,
            total_bytes: 0,
            n_ops: 0,
            operations_breakdown: std::collections::HashMap::new(),
        };
    }

    // Calculate statistics
    let byte_sizes: Vec<u64> = filtered_records.iter()
        .filter_map(|rec| rec.bytes)
        .collect();

    let latencies_ms: Vec<f64> = filtered_records.iter()
        .filter_map(|rec| rec.duration_ns.map(|ns| ns as f64 / 1_000_000.0))
        .collect();

    let mean_req_bytes = if !byte_sizes.is_empty() {
        byte_sizes.iter().sum::<u64>() as f64 / byte_sizes.len() as f64
    } else {
        0.0
    };

    let p95_latency_ms = if latencies_ms.len() >= 2 {
        let mut sorted_latencies = latencies_ms.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = ((sorted_latencies.len() as f64) * 0.95) as usize;
        sorted_latencies[p95_idx.min(sorted_latencies.len() - 1)]
    } else {
        latencies_ms.first().copied().unwrap_or(0.0)
    };

    let total_bytes = byte_sizes.iter().sum();

    // Operations breakdown
    let mut operations_breakdown = std::collections::HashMap::new();
    for record in &filtered_records {
        *operations_breakdown.entry(record.operation.clone()).or_insert(0) += 1;
    }

    Envelope {
        mean_req_bytes,
        p95_latency_ms,
        total_bytes,
        n_ops: filtered_records.len() as u64,
        operations_breakdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_oplog_format_detection() {
        assert_eq!(OpLogFormat::from_path("test.jsonl").unwrap(), OpLogFormat::Jsonl);
        assert_eq!(OpLogFormat::from_path("test.jsonl.zst").unwrap(), OpLogFormat::Jsonl);
        assert_eq!(OpLogFormat::from_path("test.tsv").unwrap(), OpLogFormat::Tsv);
        assert_eq!(OpLogFormat::from_path("test.tsv.zst").unwrap(), OpLogFormat::Tsv);
        assert_eq!(OpLogFormat::from_path("test.csv").unwrap(), OpLogFormat::Tsv);
        assert_eq!(OpLogFormat::from_path("test.csv.zst").unwrap(), OpLogFormat::Tsv);
        assert_eq!(OpLogFormat::from_path("warp-remote-2024-12-25_112107_-yNgU.csv.zst").unwrap(), OpLogFormat::Tsv);
        
        assert!(OpLogFormat::from_path("test.txt").is_err());
    }

    #[test]
    fn test_jsonl_parsing() {
        let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();
        writeln!(file, r#"{{"operation": "GET", "file": "test.dat", "bytes": 1024, "duration_ns": 5000000}}"#).unwrap();
        writeln!(file, r#"{{"operation": "PUT", "file": "test2.dat", "bytes": 2048}}"#).unwrap();
        file.flush().unwrap();

        let reader = OpLogReader::from_file(file.path()).unwrap();
        assert_eq!(reader.len(), 2);
        
        let records = reader.records();
        assert_eq!(records[0].operation, "GET");
        assert_eq!(records[0].bytes, Some(1024));
        assert_eq!(records[1].operation, "PUT");
        assert_eq!(records[1].bytes, Some(2048));
    }

    #[test]
    fn test_tsv_parsing() {
        let mut file = NamedTempFile::with_suffix(".tsv").unwrap();
        writeln!(file, "operation\tfile\tbytes\tduration_ns").unwrap();
        writeln!(file, "GET\ttest.dat\t1024\t5000000").unwrap();
        writeln!(file, "PUT\ttest2.dat\t2048\t").unwrap();
        file.flush().unwrap();

        let reader = OpLogReader::from_file(file.path()).unwrap();
        assert_eq!(reader.len(), 2);
        
        let records = reader.records();
        assert_eq!(records[0].operation, "GET");
        assert_eq!(records[0].bytes, Some(1024));
        assert_eq!(records[0].duration_ns, Some(5000000));
        assert_eq!(records[1].operation, "PUT");
        assert_eq!(records[1].bytes, Some(2048));
        assert_eq!(records[1].duration_ns, None);
    }

    #[test]
    fn test_zstd_compressed_tsv() {
        // Create a temporary file with .tsv.zst extension
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.tsv.zst");
        
        // Create TSV content
        let tsv_content = "operation\tfile\tbytes\tduration_ns\nGET\ttest.dat\t1024\t5000000\nPUT\ttest2.dat\t2048\t\n";
        
        // Compress and write to file
        let file = std::fs::File::create(&file_path).unwrap();
        let mut encoder = zstd::stream::write::Encoder::new(file, 0).unwrap();
        encoder.write_all(tsv_content.as_bytes()).unwrap();
        encoder.finish().unwrap();

        // Test reading the compressed file
        let reader = OpLogReader::from_file(&file_path).unwrap();
        assert_eq!(reader.len(), 2);
        
        let records = reader.records();
        assert_eq!(records[0].operation, "GET");
        assert_eq!(records[0].bytes, Some(1024));
        assert_eq!(records[0].duration_ns, Some(5000000));
        assert_eq!(records[1].operation, "PUT");
        assert_eq!(records[1].bytes, Some(2048));
        assert_eq!(records[1].duration_ns, None);
    }

    #[test]
    fn test_real_warp_file() {
        // Test with a real warp output file if it exists
        let warp_file = "/mnt/scratch/Warp-test-results/warp-remote-2024-12-25_112107_-yNgU.csv.zst";
        if std::path::Path::new(warp_file).exists() {
            let reader = OpLogReader::from_file(warp_file).unwrap();
            assert!(reader.len() > 0, "Warp file should contain records");
            
            // Check first record has expected fields
            let records = reader.records();
            let first_record = &records[0];
            
            // Warp files use "PUT", "GET", etc. in the "op" field
            assert!(!first_record.operation.is_empty());
            assert!(first_record.bytes.is_some());
            
            println!("Successfully parsed {} records from real Warp file", reader.len());
        } else {
            println!("Skipping real Warp file test - file not found at {}", warp_file);
        }
    }

    #[test]
    fn test_summarize_ops() {
        let records = vec![
            OpLogRec {
                operation: "GET".to_string(),
                endpoint: None,
                file: Some("test1.dat".to_string()),
                bytes: Some(1000),
                t_start_ns: None,
                duration_ns: Some(10_000_000), // 10ms
                error: None,
                extra: std::collections::HashMap::new(),
            },
            OpLogRec {
                operation: "GET".to_string(),
                endpoint: None,
                file: Some("test2.dat".to_string()),
                bytes: Some(2000),
                t_start_ns: None,
                duration_ns: Some(20_000_000), // 20ms
                error: None,
                extra: std::collections::HashMap::new(),
            },
        ];

        let envelope = summarize_ops(&records, &["GET"]);
        assert_eq!(envelope.n_ops, 2);
        assert_eq!(envelope.total_bytes, 3000);
        assert_eq!(envelope.mean_req_bytes, 1500.0);
        assert!(envelope.p95_latency_ms > 15.0); // Should be closer to 20ms
        assert_eq!(envelope.operations_breakdown.get("GET"), Some(&2));
    }

    #[test]
    fn test_filter_operations() {
        let records = vec![
            OpLogRec {
                operation: "GET".to_string(),
                endpoint: None,
                file: None,
                bytes: None,
                t_start_ns: None,
                duration_ns: None,
                error: None,
                extra: std::collections::HashMap::new(),
            },
            OpLogRec {
                operation: "PUT".to_string(),
                endpoint: None,
                file: None,
                bytes: None,
                t_start_ns: None,
                duration_ns: None,
                error: None,
                extra: std::collections::HashMap::new(),
            },
        ];

        let reader = OpLogReader { records };
        let get_records = reader.filter_operations(&["GET"]);
        assert_eq!(get_records.len(), 1);
        assert_eq!(get_records[0].operation, "GET");
    }
}