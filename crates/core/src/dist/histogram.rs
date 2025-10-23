//! HDR Histogram utilities for distributed testing
//!
//! Provides serialization, deserialization, and merging of HDR histograms
//! for accurate percentile calculations across multiple agents.
//!
//! ## Why HDR Histograms?
//!
//! Traditional approach of averaging percentiles across agents is mathematically
//! incorrect and can lead to significant errors:
//! - Agent A: p99 = 100ms (out of 1000 ops)
//! - Agent B: p99 = 200ms (out of 100 ops)
//! - Naive average: 150ms ❌
//! - True aggregate p99: ~105ms ✅ (1100 total ops)
//!
//! HDR histograms allow us to:
//! 1. Serialize histogram data from each agent (compact format)
//! 2. Deserialize on controller
//! 3. Merge histograms with correct weighting
//! 4. Calculate accurate aggregate percentiles

use anyhow::{Context, Result};
use hdrhistogram::Histogram;
use hdrhistogram::serialization::{Serializer, Deserializer, V2DeflateSerializer};
use std::io::Cursor;

/// Serialize an HDR histogram to bytes for transport
///
/// Uses the V2 compressed format for efficient network transfer.
/// Typical compression ratio: 10-50x smaller than raw data.
///
/// # Arguments
/// * `hist` - The histogram to serialize
///
/// # Returns
/// * `Vec<u8>` - Serialized histogram data
pub fn serialize_histogram(hist: &Histogram<u64>) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut serializer = V2DeflateSerializer::new();
    serializer
        .serialize(hist, &mut buf)
        .context("Failed to serialize HDR histogram")?;
    Ok(buf)
}

/// Deserialize an HDR histogram from bytes
///
/// # Arguments
/// * `data` - Serialized histogram data
///
/// # Returns
/// * `Histogram<u64>` - Reconstructed histogram
pub fn deserialize_histogram(data: &[u8]) -> Result<Histogram<u64>> {
    let mut cursor = Cursor::new(data);
    let mut deserializer = Deserializer::new();
    let hist = deserializer
        .deserialize(&mut cursor)
        .context("Failed to deserialize HDR histogram")?;
    Ok(hist)
}

/// Merge multiple HDR histograms into one
///
/// This provides mathematically correct percentile aggregation across agents.
///
/// # Arguments
/// * `histograms` - Iterator of histograms to merge
///
/// # Returns
/// * `Histogram<u64>` - Merged histogram containing all samples
pub fn merge_histograms<'a, I>(histograms: I) -> Result<Histogram<u64>>
where
    I: IntoIterator<Item = &'a Histogram<u64>>,
{
    let mut iter = histograms.into_iter();
    
    // Start with first histogram
    let first = iter
        .next()
        .context("Cannot merge empty histogram list")?;
    let mut merged = first.clone();
    
    // Add remaining histograms
    for hist in iter {
        merged
            .add(hist)
            .context("Failed to merge histogram")?;
    }
    
    Ok(merged)
}

/// Create a histogram from a vector of latency samples (in microseconds)
///
/// # Arguments
/// * `samples` - Latency samples in microseconds
/// * `max_value` - Maximum expected value (determines precision)
///
/// # Returns
/// * `Histogram<u64>` - Histogram containing the samples
pub fn histogram_from_samples(samples: &[u64], max_value: u64) -> Result<Histogram<u64>> {
    // 3 significant digits of precision
    let mut hist = Histogram::new_with_max(max_value, 3)
        .context("Failed to create histogram")?;
    
    for &sample in samples {
        hist.record(sample)
            .context("Failed to record sample in histogram")?;
    }
    
    Ok(hist)
}

/// Extract percentiles from a histogram
///
/// # Arguments
/// * `hist` - The histogram to query
///
/// # Returns
/// * Tuple of (p50, p90, p95, p99) in the original units
pub fn extract_percentiles(hist: &Histogram<u64>) -> (f64, f64, f64, f64) {
    let p50 = hist.value_at_quantile(0.50) as f64;
    let p90 = hist.value_at_quantile(0.90) as f64;
    let p95 = hist.value_at_quantile(0.95) as f64;
    let p99 = hist.value_at_quantile(0.99) as f64;
    (p50, p90, p95, p99)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let mut hist = Histogram::new(3).unwrap();
        hist.record(100).unwrap();
        hist.record(200).unwrap();
        hist.record(300).unwrap();

        let bytes = serialize_histogram(&hist).unwrap();
        let restored = deserialize_histogram(&bytes).unwrap();

        assert_eq!(hist.len(), restored.len());
        assert_eq!(hist.min(), restored.min());
        assert_eq!(hist.max(), restored.max());
    }

    #[test]
    fn test_merge_histograms() {
        let mut hist1 = Histogram::new(3).unwrap();
        hist1.record(100).unwrap();
        hist1.record(200).unwrap();

        let mut hist2 = Histogram::new(3).unwrap();
        hist2.record(300).unwrap();
        hist2.record(400).unwrap();

        let hists = vec![&hist1, &hist2];
        let merged = merge_histograms(hists).unwrap();

        assert_eq!(merged.len(), 4);
        assert_eq!(merged.min(), 100);
        assert_eq!(merged.max(), 400);
    }

    #[test]
    fn test_histogram_from_samples() {
        let samples = vec![100, 200, 300, 400, 500];
        let hist = histogram_from_samples(&samples, 1000).unwrap();

        assert_eq!(hist.len(), 5);
        assert_eq!(hist.min(), 100);
        assert_eq!(hist.max(), 500);
    }

    #[test]
    fn test_extract_percentiles() {
        let mut hist = Histogram::new(3).unwrap();
        for i in 1..=100 {
            hist.record(i * 10).unwrap(); // 10, 20, 30, ..., 1000
        }

        let (p50, p90, p95, p99) = extract_percentiles(&hist);

        // Should be roughly at the expected percentiles
        assert!(p50 >= 400.0 && p50 <= 600.0);
        assert!(p90 >= 850.0 && p90 <= 950.0);
        assert!(p95 >= 920.0 && p95 <= 980.0);
        assert!(p99 >= 980.0 && p99 <= 1010.0);
    }

    #[test]
    fn test_percentile_merging_correctness() {
        // Demonstrate correct merging vs naive averaging
        // Key insight: with unbalanced workloads, naive averaging is wrong
        
        // Agent A: 100 ops with latencies from 1-100ms
        let mut hist_a = Histogram::new_with_max(100_000, 3).unwrap();
        for i in 1..=100 {
            hist_a.record(i * 1000).unwrap(); // 1ms, 2ms, ..., 100ms
        }
        
        // Agent B: 10 ops with latencies from 1-10ms  
        let mut hist_b = Histogram::new_with_max(100_000, 3).unwrap();
        for i in 1..=10 {
            hist_b.record(i * 1000).unwrap(); // 1ms, 2ms, ..., 10ms
        }
        
        // Individual p90s
        let p90_a = hist_a.value_at_quantile(0.90) as f64 / 1000.0; // Convert to ms
        let p90_b = hist_b.value_at_quantile(0.90) as f64 / 1000.0;
        
        // Naive average (WRONG) - just average the two percentile values
        let naive_avg = (p90_a + p90_b) / 2.0;
        
        // Correct merge - merge histograms first, then calculate percentile
        let hists = vec![&hist_a, &hist_b];
        let merged = merge_histograms(hists).unwrap();
        let true_p90 = merged.value_at_quantile(0.90) as f64 / 1000.0;
        
        // Verify individual percentiles
        // Agent A p90 should be ~90ms (90th out of 100)
        assert!(p90_a >= 85.0 && p90_a <= 95.0, "p90_a = {}ms", p90_a);
        
        // Agent B p90 should be ~9ms (9th out of 10)  
        assert!(p90_b >= 8.0 && p90_b <= 11.0, "p90_b = {}ms", p90_b);
        
        // Naive average should be around (90 + 9) / 2 = 49.5ms
        assert!(naive_avg >= 45.0 && naive_avg <= 55.0, "naive_avg = {}ms", naive_avg);
        
        // True p90 should be higher, closer to agent A's p90 because it has 10x more samples
        // With 110 total samples, p90 is the 99th sample, which is in agent A's range
        // So true p90 should be close to agent A's p90 (~90ms)
        assert!(true_p90 >= 85.0 && true_p90 <= 95.0, "true_p90 = {}ms", true_p90);
        
        // The naive average should significantly underestimate the true p90
        assert!(naive_avg < true_p90 * 0.7, 
            "Naive avg ({}ms) should be < 70% of true p90 ({}ms)", 
            naive_avg, true_p90);
    }
}
