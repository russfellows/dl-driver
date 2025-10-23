//! Integration tests for HDR histogram E2E flow
//!
//! Tests the complete histogram lifecycle:
//! 1. Agent collects latency samples during workload
//! 2. Agent creates HDR histogram from samples
//! 3. Agent serializes histogram into proto message
//! 4. Controller receives proto messages from multiple agents
//! 5. Controller deserializes histograms
//! 6. Controller merges histograms correctly
//! 7. Controller calculates accurate aggregate percentiles
//!
//! This validates that histogram-based aggregation is mathematically correct
//! compared to naive percentile averaging, especially for unbalanced workloads.

use anyhow::Result;
use dl_driver_core::dist::histogram::{
    histogram_from_samples, serialize_histogram, deserialize_histogram, 
    merge_histograms, extract_percentiles,
};
use dl_driver_core::dist::proto::WorkloadSummary;
use dl_driver_core::dist::types::{WorkloadResult, AggregateResults};

#[test]
fn test_histogram_serialization_in_proto() -> Result<()> {
    // Simulate agent collecting latency samples (in microseconds)
    let samples_agent1 = vec![100, 150, 200, 250, 300]; // 5 samples
    let samples_agent2 = vec![1000, 1500, 2000, 2500, 3000]; // 5 samples, much slower
    
    // Agent 1: Create histogram and serialize
    let hist1 = histogram_from_samples(&samples_agent1, 10_000)?;
    let hist1_bytes = serialize_histogram(&hist1)?;
    
    // Agent 2: Create histogram and serialize
    let hist2 = histogram_from_samples(&samples_agent2, 10_000)?;
    let hist2_bytes = serialize_histogram(&hist2)?;
    
    // Verify serialization produces non-empty bytes
    assert!(!hist1_bytes.is_empty(), "Agent 1 histogram should serialize to bytes");
    assert!(!hist2_bytes.is_empty(), "Agent 2 histogram should serialize to bytes");
    
    // Create proto messages as agents would
    let summary1 = WorkloadSummary {
        agent_id: "agent-0".to_string(),
        ops_per_s: 100.0,
        mib_per_s: 10.0,
        p50_ms: 0.2, // 200μs = 0.2ms
        p90_ms: 0.3,
        p95_ms: 0.3,
        p99_ms: 0.3,
        errors: 0,
        total_ops: 5,
        duration_s: 0.05,
        samples_per_second: 100.0,
        total_samples: 5,
        samples_per_batch: 1,
        batches_per_second: 100.0,
        total_batches: 5,
        avg_batch_time_ms: 10.0,
        epochs_completed: 1,
        avg_epoch_time_s: 0.05,
        data_loading_time_s: 0.02,
        compute_time_s: 0.03,
        pipeline_efficiency: 0.6,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read_latency: hist1_bytes.clone(),
        histogram_write_latency: vec![],
        histogram_batch_time: vec![],
    };
    
    let summary2 = WorkloadSummary {
        agent_id: "agent-1".to_string(),
        ops_per_s: 50.0,
        mib_per_s: 5.0,
        p50_ms: 2.0, // 2000μs = 2ms
        p90_ms: 3.0,
        p95_ms: 3.0,
        p99_ms: 3.0,
        errors: 0,
        total_ops: 5,
        duration_s: 0.1,
        samples_per_second: 50.0,
        total_samples: 5,
        samples_per_batch: 1,
        batches_per_second: 50.0,
        total_batches: 5,
        avg_batch_time_ms: 20.0,
        epochs_completed: 1,
        avg_epoch_time_s: 0.1,
        data_loading_time_s: 0.04,
        compute_time_s: 0.06,
        pipeline_efficiency: 0.6,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read_latency: hist2_bytes.clone(),
        histogram_write_latency: vec![],
        histogram_batch_time: vec![],
    };
    
    // Controller: Deserialize histograms
    let hist1_restored = deserialize_histogram(&summary1.histogram_read_latency)?;
    let hist2_restored = deserialize_histogram(&summary2.histogram_read_latency)?;
    
    // Verify deserialization worked
    assert_eq!(hist1.len(), hist1_restored.len(), "Agent 1 histogram sample count should match");
    assert_eq!(hist2.len(), hist2_restored.len(), "Agent 2 histogram sample count should match");
    
    // Controller: Merge histograms
    let merged = merge_histograms(vec![&hist1_restored, &hist2_restored])?;
    
    // Verify merged histogram has all samples
    assert_eq!(merged.len(), 10, "Merged histogram should have 10 total samples");
    
    // Extract percentiles from merged histogram
    let (p50, p90, p95, p99) = extract_percentiles(&merged);
    
    // Convert to milliseconds for comparison
    let p50_ms = p50 / 1000.0;
    let p90_ms = p90 / 1000.0;
    let p95_ms = p95 / 1000.0;
    let p99_ms = p99 / 1000.0;
    
    println!("Merged histogram percentiles:");
    println!("  p50: {:.2}ms", p50_ms);
    println!("  p90: {:.2}ms", p90_ms);
    println!("  p95: {:.2}ms", p95_ms);
    println!("  p99: {:.2}ms", p99_ms);
    
    // With 10 samples total, p50 should be around the median (somewhere between 300μs and 1000μs)
    // p90 should be in the slower agent's range (around 2500-3000μs)
    assert!(p50_ms >= 0.2 && p50_ms <= 1.5, "p50 should be between 0.2ms and 1.5ms, got {:.2}ms", p50_ms);
    assert!(p90_ms >= 2.0 && p90_ms <= 3.5, "p90 should be between 2ms and 3.5ms, got {:.2}ms", p90_ms);
    
    Ok(())
}

#[test]
fn test_aggregate_results_with_histograms() -> Result<()> {
    // Simulate unbalanced workload:
    // Agent A: 100 ops, fast (100-200μs range)
    // Agent B: 10 ops, slow (1000-2000μs range)
    
    let mut samples_a = Vec::new();
    for i in 0..100 {
        samples_a.push(100 + i); // 100-199μs
    }
    
    let mut samples_b = Vec::new();
    for i in 0..10 {
        samples_b.push(1000 + i * 100); // 1000-1900μs
    }
    
    // Create histograms
    let hist_a = histogram_from_samples(&samples_a, 10_000)?;
    let hist_b = histogram_from_samples(&samples_b, 10_000)?;
    
    // Serialize
    let hist_a_bytes = serialize_histogram(&hist_a)?;
    let hist_b_bytes = serialize_histogram(&hist_b)?;
    
    // Extract individual percentiles (for naive averaging)
    let (p50_a, p90_a, p95_a, p99_a) = extract_percentiles(&hist_a);
    let (p50_b, p90_b, p95_b, p99_b) = extract_percentiles(&hist_b);
    
    // Create WorkloadResult objects (as controller would)
    let result_a = WorkloadResult {
        agent_id: "agent-0".to_string(),
        ops_per_s: 2000.0,
        mib_per_s: 200.0,
        p50_ms: p50_a / 1000.0,
        p90_ms: p90_a / 1000.0,
        p95_ms: p95_a / 1000.0,
        p99_ms: p99_a / 1000.0,
        errors: 0,
        total_ops: 100,
        duration_s: 0.05,
        samples_per_second: 2000.0,
        total_samples: 100,
        samples_per_batch: 10,
        batches_per_second: 200.0,
        total_batches: 10,
        avg_batch_time_ms: 5.0,
        epochs_completed: 1,
        avg_epoch_time_s: 0.05,
        data_loading_time_s: 0.02,
        compute_time_s: 0.03,
        pipeline_efficiency: 0.6,
    };
    
    let result_b = WorkloadResult {
        agent_id: "agent-1".to_string(),
        ops_per_s: 100.0,
        mib_per_s: 10.0,
        p50_ms: p50_b / 1000.0,
        p90_ms: p90_b / 1000.0,
        p95_ms: p95_b / 1000.0,
        p99_ms: p99_b / 1000.0,
        errors: 0,
        total_ops: 10,
        duration_s: 0.1,
        samples_per_second: 100.0,
        total_samples: 10,
        samples_per_batch: 10,
        batches_per_second: 10.0,
        total_batches: 1,
        avg_batch_time_ms: 100.0,
        epochs_completed: 1,
        avg_epoch_time_s: 0.1,
        data_loading_time_s: 0.04,
        compute_time_s: 0.06,
        pipeline_efficiency: 0.6,
    };
    
    // Create proto summaries with histogram data
    let summary_a = WorkloadSummary {
        agent_id: result_a.agent_id.clone(),
        ops_per_s: result_a.ops_per_s,
        mib_per_s: result_a.mib_per_s,
        p50_ms: result_a.p50_ms,
        p90_ms: result_a.p90_ms,
        p95_ms: result_a.p95_ms,
        p99_ms: result_a.p99_ms,
        errors: result_a.errors,
        total_ops: result_a.total_ops,
        duration_s: result_a.duration_s,
        samples_per_second: result_a.samples_per_second,
        total_samples: result_a.total_samples,
        samples_per_batch: result_a.samples_per_batch,
        batches_per_second: result_a.batches_per_second,
        total_batches: result_a.total_batches,
        avg_batch_time_ms: result_a.avg_batch_time_ms,
        epochs_completed: result_a.epochs_completed,
        avg_epoch_time_s: result_a.avg_epoch_time_s,
        data_loading_time_s: result_a.data_loading_time_s,
        compute_time_s: result_a.compute_time_s,
        pipeline_efficiency: result_a.pipeline_efficiency,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read_latency: hist_a_bytes,
        histogram_write_latency: vec![],
        histogram_batch_time: vec![],
    };
    
    let summary_b = WorkloadSummary {
        agent_id: result_b.agent_id.clone(),
        ops_per_s: result_b.ops_per_s,
        mib_per_s: result_b.mib_per_s,
        p50_ms: result_b.p50_ms,
        p90_ms: result_b.p90_ms,
        p95_ms: result_b.p95_ms,
        p99_ms: result_b.p99_ms,
        errors: result_b.errors,
        total_ops: result_b.total_ops,
        duration_s: result_b.duration_s,
        samples_per_second: result_b.samples_per_second,
        total_samples: result_b.total_samples,
        samples_per_batch: result_b.samples_per_batch,
        batches_per_second: result_b.batches_per_second,
        total_batches: result_b.total_batches,
        avg_batch_time_ms: result_b.avg_batch_time_ms,
        epochs_completed: result_b.epochs_completed,
        avg_epoch_time_s: result_b.avg_epoch_time_s,
        data_loading_time_s: result_b.data_loading_time_s,
        compute_time_s: result_b.compute_time_s,
        pipeline_efficiency: result_b.pipeline_efficiency,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read_latency: hist_b_bytes,
        histogram_write_latency: vec![],
        histogram_batch_time: vec![],
    };
    
    // Test naive aggregation (should be inaccurate)
    let naive_agg = AggregateResults::from_results(vec![result_a.clone(), result_b.clone()])?;
    
    // Test histogram-based aggregation (should be accurate)
    let histogram_agg = AggregateResults::from_results_with_histograms(
        vec![result_a, result_b],
        &[summary_a, summary_b],
    )?;
    
    println!("\nNaive aggregation (averaging percentiles):");
    println!("  p50: {:.3}ms", naive_agg.avg_p50_ms);
    println!("  p90: {:.3}ms", naive_agg.avg_p90_ms);
    println!("  p99: {:.3}ms", naive_agg.avg_p99_ms);
    
    println!("\nHistogram-based aggregation (merged histograms):");
    println!("  p50: {:.3}ms", histogram_agg.avg_p50_ms);
    println!("  p90: {:.3}ms", histogram_agg.avg_p90_ms);
    println!("  p99: {:.3}ms", histogram_agg.avg_p99_ms);
    
    // With 110 total samples (100 fast + 10 slow):
    // - p50 (55th sample) should be in the fast range (~150μs = 0.15ms)
    // - p90 (99th sample) should be in the fast range (~199μs = 0.199ms)
    // - p99 (109th sample) should be in the slow range (~1800μs = 1.8ms)
    
    // Histogram-based should be more accurate (weighted by sample count)
    assert!(histogram_agg.avg_p50_ms < 0.2, 
        "Histogram p50 should be < 0.2ms (mostly fast samples), got {:.3}ms", 
        histogram_agg.avg_p50_ms);
    
    assert!(histogram_agg.avg_p90_ms < 0.25, 
        "Histogram p90 should be < 0.25ms (still in fast range), got {:.3}ms", 
        histogram_agg.avg_p90_ms);
    
    // The naive average will be wrong - it averages ~0.15ms and ~1.5ms to get ~0.83ms
    // But the true p50 should be ~0.15ms (since 90% of samples are fast)
    let error_percentage = ((naive_agg.avg_p50_ms - histogram_agg.avg_p50_ms).abs() 
        / histogram_agg.avg_p50_ms) * 100.0;
    
    println!("\nError from naive averaging: {:.1}%", error_percentage);
    
    // Expect significant error (>100%) for unbalanced workloads
    assert!(error_percentage > 50.0, 
        "Expected >50% error from naive averaging, got {:.1}%", 
        error_percentage);
    
    Ok(())
}

#[test]
fn test_empty_histogram_fallback() -> Result<()> {
    // Test that aggregation falls back to naive averaging when histogram data is missing
    
    let result_a = WorkloadResult {
        agent_id: "agent-0".to_string(),
        ops_per_s: 100.0,
        mib_per_s: 10.0,
        p50_ms: 1.0,
        p90_ms: 2.0,
        p95_ms: 3.0,
        p99_ms: 4.0,
        errors: 0,
        total_ops: 100,
        duration_s: 1.0,
        samples_per_second: 100.0,
        total_samples: 100,
        samples_per_batch: 10,
        batches_per_second: 10.0,
        total_batches: 10,
        avg_batch_time_ms: 10.0,
        epochs_completed: 1,
        avg_epoch_time_s: 1.0,
        data_loading_time_s: 0.4,
        compute_time_s: 0.6,
        pipeline_efficiency: 0.6,
    };
    
    let result_b = WorkloadResult {
        agent_id: "agent-1".to_string(),
        ops_per_s: 50.0,
        mib_per_s: 5.0,
        p50_ms: 3.0,
        p90_ms: 6.0,
        p95_ms: 9.0,
        p99_ms: 12.0,
        errors: 0,
        total_ops: 50,
        duration_s: 1.0,
        samples_per_second: 50.0,
        total_samples: 50,
        samples_per_batch: 10,
        batches_per_second: 5.0,
        total_batches: 5,
        avg_batch_time_ms: 20.0,
        epochs_completed: 1,
        avg_epoch_time_s: 1.0,
        data_loading_time_s: 0.4,
        compute_time_s: 0.6,
        pipeline_efficiency: 0.6,
    };
    
    // Create summaries with NO histogram data (empty bytes)
    let summaries = vec![];
    
    // Should fall back to naive averaging
    let agg = AggregateResults::from_results_with_histograms(
        vec![result_a, result_b],
        &summaries,
    )?;
    
    // Should average: (1.0 + 3.0) / 2 = 2.0
    assert!((agg.avg_p50_ms - 2.0).abs() < 0.01, 
        "Should fall back to naive average (2.0ms), got {:.2}ms", 
        agg.avg_p50_ms);
    
    // Should average: (2.0 + 6.0) / 2 = 4.0
    assert!((agg.avg_p90_ms - 4.0).abs() < 0.01, 
        "Should fall back to naive average (4.0ms), got {:.2}ms", 
        agg.avg_p90_ms);
    
    Ok(())
}
