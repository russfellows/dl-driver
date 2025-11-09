//! TSV export for machine-readable DLIO benchmark results with histogram data
//!
//! Exports both storage metrics (ops/s, MiB/s, latency percentiles) and
//! AI/ML training metrics (samples/s, batches/s, epochs) to TSV format.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::metrics::{SIZE_BUCKET_LABELS, StorageOpHists, BatchTimeHist};

/// TSV exporter for storage performance metrics with histogram data
pub struct StorageTsvExporter {
    output_path: std::path::PathBuf,
}

impl StorageTsvExporter {
    /// Create exporter with explicit output path
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            output_path: path.as_ref().to_path_buf(),
        }
    }

    /// Export storage results with histogram data to TSV file
    /// 
    /// # Arguments
    /// * `read_hists` - Read operation histograms (size-bucketed)
    /// * `write_hists` - Write operation histograms (size-bucketed)
    /// * `total_read_bytes` - Total bytes read
    /// * `total_write_bytes` - Total bytes written
    /// * `wall_seconds` - Total execution time
    pub fn export_results(
        &self,
        read_hists: &StorageOpHists,
        write_hists: &StorageOpHists,
        total_read_bytes: u64,
        total_write_bytes: u64,
        wall_seconds: f64,
    ) -> Result<()> {
        let mut f = File::create(&self.output_path)
            .with_context(|| format!("Failed to create {}", self.output_path.display()))?;

        // Write header
        writeln!(
            f,
            "operation\tsize_bucket\tbucket_idx\tmean_us\tp50_us\tp90_us\tp95_us\tp99_us\tmax_us\tavg_bytes\tops_per_sec\tthroughput_mibps\tcount"
        )?;

        // Collect all rows (including per-bucket and aggregate rows)
        let mut rows = Vec::new();
        
        // Read operations
        self.collect_op_buckets(&mut rows, "READ", read_hists, total_read_bytes, wall_seconds)?;
        
        // Write operations
        self.collect_op_buckets(&mut rows, "WRITE", write_hists, total_write_bytes, wall_seconds)?;

        // Add aggregate summary rows (bucket_idx 98 and 99 for proper sorting)
        self.collect_aggregate_row(&mut rows, "READ", 98, read_hists, total_read_bytes, wall_seconds)?;
        self.collect_aggregate_row(&mut rows, "WRITE", 99, write_hists, total_write_bytes, wall_seconds)?;

        // Sort by bucket_idx
        rows.sort_by_key(|(bucket_idx, _)| *bucket_idx);

        // Write sorted rows
        for (_, row) in rows {
            writeln!(f, "{}", row)?;
        }

        Ok(())
    }

    /// Export storage results to in-memory String (for distributed agents)
    /// 
    /// Same as export_results but returns TSV content as String instead of writing to file.
    /// Used by distributed agents to send TSV data via gRPC without temp files.
    pub fn export_to_string(
        read_hists: &StorageOpHists,
        write_hists: &StorageOpHists,
        total_read_bytes: u64,
        total_write_bytes: u64,
        wall_seconds: f64,
    ) -> Result<String> {
        let mut output = String::new();
        
        // Write header
        output.push_str("operation\tsize_bucket\tbucket_idx\tmean_us\tp50_us\tp90_us\tp95_us\tp99_us\tmax_us\tavg_bytes\tops_per_sec\tthroughput_mibps\tcount\n");

        // Collect all rows
        let mut rows = Vec::new();
        
        Self::collect_op_buckets_static(&mut rows, "READ", read_hists, total_read_bytes, wall_seconds)?;
        Self::collect_op_buckets_static(&mut rows, "WRITE", write_hists, total_write_bytes, wall_seconds)?;
        Self::collect_aggregate_row_static(&mut rows, "READ", 98, read_hists, total_read_bytes, wall_seconds)?;
        Self::collect_aggregate_row_static(&mut rows, "WRITE", 99, write_hists, total_write_bytes, wall_seconds)?;

        // Sort by bucket_idx
        rows.sort_by_key(|(bucket_idx, _)| *bucket_idx);

        // Write sorted rows
        for (_, row) in rows {
            output.push_str(&row);
            output.push('\n');
        }

        Ok(output)
    }

    fn collect_op_buckets(
        &self,
        rows: &mut Vec<(usize, String)>,
        op: &str,
        hists: &StorageOpHists,
        _total_bytes: u64,
        wall_seconds: f64,
    ) -> Result<()> {
        for (i, bucket_label) in SIZE_BUCKET_LABELS.iter().enumerate() {
            let hist = hists.buckets[i].lock().unwrap();
            let count = hist.len();

            if count == 0 {
                continue;
            }

            // Get actual bytes from SizeBins (not estimated!)
            let (bucket_ops, bucket_bytes) = hists.size_bins.get_bucket_stats(i);

            let avg_bytes = if bucket_ops > 0 {
                bucket_bytes as f64 / bucket_ops as f64
            } else {
                0.0
            };

            let ops_per_sec = count as f64 / wall_seconds;
            let throughput_mibps = (bucket_bytes as f64 / 1_048_576.0) / wall_seconds;

            let row = format!(
                "{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.2}\t{:.2}\t{}",
                op,
                bucket_label,
                i,
                hist.mean(),
                hist.value_at_quantile(0.50) as f64,
                hist.value_at_quantile(0.90) as f64,
                hist.value_at_quantile(0.95) as f64,
                hist.value_at_quantile(0.99) as f64,
                hist.max() as f64,
                avg_bytes,
                ops_per_sec,
                throughput_mibps,
                count
            );

            rows.push((i, row));
        }

        Ok(())
    }

    fn collect_aggregate_row(
        &self,
        rows: &mut Vec<(usize, String)>,
        op: &str,
        bucket_idx: usize,
        hists: &StorageOpHists,
        total_bytes: u64,
        wall_seconds: f64,
    ) -> Result<()> {
        let combined_hist = hists.combined_histogram();
        let count = combined_hist.len();

        if count == 0 {
            return Ok(());
        }

        let avg_bytes = if count > 0 {
            total_bytes as f64 / count as f64
        } else {
            0.0
        };

        let ops_per_sec = count as f64 / wall_seconds;
        let throughput_mibps = (total_bytes as f64 / 1_048_576.0) / wall_seconds;

        let row = format!(
            "{}\tALL\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.2}\t{:.2}\t{}",
            op,
            bucket_idx,
            combined_hist.mean(),
            combined_hist.value_at_quantile(0.50) as f64,
            combined_hist.value_at_quantile(0.90) as f64,
            combined_hist.value_at_quantile(0.95) as f64,
            combined_hist.value_at_quantile(0.99) as f64,
            combined_hist.max() as f64,
            avg_bytes,
            ops_per_sec,
            throughput_mibps,
            count
        );

        rows.push((bucket_idx, row));

        Ok(())
    }
    
    // Static helpers for export_to_string (no self reference needed)
    
    fn collect_op_buckets_static(
        rows: &mut Vec<(usize, String)>,
        op: &str,
        hists: &StorageOpHists,
        _total_bytes: u64,
        wall_seconds: f64,
    ) -> Result<()> {
        for (i, bucket_label) in SIZE_BUCKET_LABELS.iter().enumerate() {
            let hist = hists.buckets[i].lock().unwrap();
            let count = hist.len();

            if count == 0 {
                continue;
            }

            let (bucket_ops, bucket_bytes) = hists.size_bins.get_bucket_stats(i);
            let avg_bytes = if bucket_ops > 0 {
                bucket_bytes as f64 / bucket_ops as f64
            } else {
                0.0
            };

            let ops_per_sec = count as f64 / wall_seconds;
            let throughput_mibps = (bucket_bytes as f64 / 1_048_576.0) / wall_seconds;

            let row = format!(
                "{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.2}\t{:.2}\t{}",
                op,
                bucket_label,
                i,
                hist.mean(),
                hist.value_at_quantile(0.50) as f64,
                hist.value_at_quantile(0.90) as f64,
                hist.value_at_quantile(0.95) as f64,
                hist.value_at_quantile(0.99) as f64,
                hist.max() as f64,
                avg_bytes,
                ops_per_sec,
                throughput_mibps,
                count
            );

            rows.push((i, row));
        }

        Ok(())
    }
    
    fn collect_aggregate_row_static(
        rows: &mut Vec<(usize, String)>,
        op: &str,
        bucket_idx: usize,
        hists: &StorageOpHists,
        total_bytes: u64,
        wall_seconds: f64,
    ) -> Result<()> {
        let combined_hist = hists.combined_histogram();
        let count = combined_hist.len();

        if count == 0 {
            return Ok(());
        }

        let avg_bytes = if count > 0 {
            total_bytes as f64 / count as f64
        } else {
            0.0
        };

        let ops_per_sec = count as f64 / wall_seconds;
        let throughput_mibps = (total_bytes as f64 / 1_048_576.0) / wall_seconds;

        let row = format!(
            "{}\tALL\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.2}\t{:.2}\t{}",
            op,
            bucket_idx,
            combined_hist.mean(),
            combined_hist.value_at_quantile(0.50) as f64,
            combined_hist.value_at_quantile(0.90) as f64,
            combined_hist.value_at_quantile(0.95) as f64,
            combined_hist.value_at_quantile(0.99) as f64,
            combined_hist.max() as f64,
            avg_bytes,
            ops_per_sec,
            throughput_mibps,
            count
        );

        rows.push((bucket_idx, row));

        Ok(())
    }
}

/// TSV exporter for AI/ML training metrics with histogram data
pub struct AiMlTsvExporter {
    output_path: std::path::PathBuf,
}

impl AiMlTsvExporter {
    /// Create exporter with explicit output path
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            output_path: path.as_ref().to_path_buf(),
        }
    }

    /// Export AI/ML training metrics to TSV file
    /// 
    /// # Arguments
    /// * `batch_hist` - Batch processing time histogram
    /// * `total_samples` - Total samples processed
    /// * `samples_per_batch` - Samples per batch
    /// * `epochs_completed` - Number of epochs completed
    /// * `wall_seconds` - Total execution time
    pub fn export_results(
        &self,
        batch_hist: &BatchTimeHist,
        total_samples: u64,
        samples_per_batch: u64,
        epochs_completed: u32,
        wall_seconds: f64,
    ) -> Result<()> {
        let mut f = File::create(&self.output_path)
            .with_context(|| format!("Failed to create {}", self.output_path.display()))?;

        // Write header
        writeln!(
            f,
            "metric\tvalue\tunit"
        )?;

        // Basic metrics
        writeln!(f, "total_samples\t{}\tsamples", total_samples)?;
        writeln!(f, "samples_per_batch\t{}\tsamples", samples_per_batch)?;
        writeln!(f, "epochs_completed\t{}\tepochs", epochs_completed)?;
        writeln!(f, "wall_time\t{:.2}\tseconds", wall_seconds)?;

        // Throughput metrics
        let samples_per_second = total_samples as f64 / wall_seconds;
        let total_batches = if samples_per_batch > 0 {
            (total_samples + samples_per_batch - 1) / samples_per_batch
        } else {
            0
        };
        let batches_per_second = total_batches as f64 / wall_seconds;

        writeln!(f, "samples_per_second\t{:.2}\tsamples/s", samples_per_second)?;
        writeln!(f, "batches_per_second\t{:.2}\tbatches/s", batches_per_second)?;

        // Batch time histogram statistics (if available)
        if let Some(hist) = batch_hist.get_histogram() {
            writeln!(f, "batch_time_mean_ms\t{:.2}\tms", hist.mean() / 1000.0)?;
            writeln!(f, "batch_time_p50_ms\t{:.2}\tms", hist.value_at_quantile(0.50) as f64 / 1000.0)?;
            writeln!(f, "batch_time_p90_ms\t{:.2}\tms", hist.value_at_quantile(0.90) as f64 / 1000.0)?;
            writeln!(f, "batch_time_p95_ms\t{:.2}\tms", hist.value_at_quantile(0.95) as f64 / 1000.0)?;
            writeln!(f, "batch_time_p99_ms\t{:.2}\tms", hist.value_at_quantile(0.99) as f64 / 1000.0)?;
            writeln!(f, "batch_time_max_ms\t{:.2}\tms", hist.max() as f64 / 1000.0)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_storage_tsv_export() {
        let temp_dir = TempDir::new().unwrap();
        let tsv_path = temp_dir.path().join("storage.tsv");
        let exporter = StorageTsvExporter::new(&tsv_path);

        let read_hists = StorageOpHists::new();
        let write_hists = StorageOpHists::new();

        // Record some test data
        read_hists.record_with_size(1024, Duration::from_micros(100));
        read_hists.record_with_size(10 * 1024, Duration::from_micros(200));
        write_hists.record_with_size(1024, Duration::from_micros(150));

        exporter.export_results(&read_hists, &write_hists, 11264, 1024, 1.0).unwrap();

        // Verify file was created
        assert!(tsv_path.exists());
        
        // Read and verify content
        let content = std::fs::read_to_string(&tsv_path).unwrap();
        assert!(content.contains("operation\tsize_bucket"));
        assert!(content.contains("READ"));
        assert!(content.contains("WRITE"));
        assert!(content.contains("ALL"));
    }

    #[test]
    fn test_aiml_tsv_export() {
        let temp_dir = TempDir::new().unwrap();
        let tsv_path = temp_dir.path().join("aiml.tsv");
        let exporter = AiMlTsvExporter::new(&tsv_path);

        let batch_hist = BatchTimeHist::new();
        batch_hist.record(Duration::from_millis(10));
        batch_hist.record(Duration::from_millis(12));
        batch_hist.record(Duration::from_millis(15));

        exporter.export_results(&batch_hist, 1000, 64, 1, 10.0).unwrap();

        // Verify file was created
        assert!(tsv_path.exists());
        
        // Read and verify content
        let content = std::fs::read_to_string(&tsv_path).unwrap();
        assert!(content.contains("metric\tvalue\tunit"));
        assert!(content.contains("total_samples"));
        assert!(content.contains("samples_per_second"));
        assert!(content.contains("batch_time_p50_ms"));
    }

}

/// Export multi-endpoint statistics to TSV file
/// 
/// This function writes per-endpoint metrics for multi-endpoint configurations.
/// Each endpoint gets a row with its URI, request count, bytes transferred, errors, etc.
pub fn export_endpoint_stats<P: AsRef<Path>>(
    output_path: P,
    endpoint_stats: &[(String, s3dlio::EndpointStatsSnapshot)],
    wall_seconds: f64,
) -> Result<()> {
    let mut f = File::create(output_path.as_ref())
        .with_context(|| format!("Failed to create {}", output_path.as_ref().display()))?;

    // Write header
    writeln!(
        f,
        "endpoint_uri\ttotal_requests\tbytes_read\tbytes_written\terror_count\tactive_requests\trequests_per_sec\tread_throughput_mibps\twrite_throughput_mibps"
    )?;

    // Write stats for each endpoint
    for (uri, stats) in endpoint_stats {
        let requests_per_sec = if wall_seconds > 0.0 {
            stats.total_requests as f64 / wall_seconds
        } else {
            0.0
        };

        let read_throughput_mibps = if wall_seconds > 0.0 {
            (stats.bytes_read as f64 / wall_seconds) / (1024.0 * 1024.0)
        } else {
            0.0
        };

        let write_throughput_mibps = if wall_seconds > 0.0 {
            (stats.bytes_written as f64 / wall_seconds) / (1024.0 * 1024.0)
        } else {
            0.0
        };

        writeln!(
            f,
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}",
            uri,
            stats.total_requests,
            stats.bytes_read,
            stats.bytes_written,
            stats.error_count,
            stats.active_requests,
            requests_per_sec,
            read_throughput_mibps,
            write_throughput_mibps,
        )?;
    }

    Ok(())
}

