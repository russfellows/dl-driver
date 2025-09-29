// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Workload validation by comparing operation log envelopes
//! 
//! This module provides functionality to compare reference workload characteristics
//! (from operation logs) against current run metrics to validate storage system
//! performance and behavior.

use anyhow::Result;
use std::process::exit;
use crate::oplog_ingest::Envelope;
use crate::metrics::MetricsSummary;

/// Validation configuration with acceptable tolerance bands
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub mean_bytes_band: f64,      // ±percentage (e.g., 0.20 for ±20%)
    pub p95_latency_band: f64,     // ±percentage (e.g., 0.25 for ±25%)
    pub total_bytes_band: f64,     // ±percentage (e.g., 0.15 for ±15%)
    pub strict_mode: bool,         // Exit with error on failure
}

impl Default for ValidationConfig {
    fn default() -> Self {
        ValidationConfig {
            mean_bytes_band: 0.20,    // ±20%
            p95_latency_band: 0.25,   // ±25%
            total_bytes_band: 0.15,   // ±15%
            strict_mode: false,
        }
    }
}

/// Result of a single metric validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub metric: String,
    pub reference_value: f64,
    pub current_value: f64,
    pub tolerance_band: f64,
    pub pass: bool,
    pub reason: String,
}

/// Overall validation summary
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub overall_pass: bool,
    pub results: Vec<ValidationResult>,
    pub warnings: Vec<String>,
}

/// Validate current metrics against reference envelope
pub fn validate_against_reference(
    reference_envelope: &Envelope,
    current_summary: &MetricsSummary,
    config: &ValidationConfig,
) -> ValidationSummary {
    let mut results = Vec::new();
    let mut warnings = Vec::new();

    // Validate mean request bytes
    let ref_mean_bytes = reference_envelope.mean_req_bytes;
    let cur_mean_bytes = if current_summary.totals.files_processed > 0 {
        current_summary.totals.bytes_read as f64 / current_summary.totals.files_processed as f64
    } else {
        0.0
    };

    results.push(validate_metric(
        "mean_request_bytes",
        ref_mean_bytes,
        cur_mean_bytes,
        config.mean_bytes_band,
    ));

    // Validate p95 latency
    if let Some(ref_p95) = current_summary.timing.latency_ms_p95 {
        let cur_p95 = ref_p95;
        results.push(validate_metric(
            "p95_latency_ms",
            reference_envelope.p95_latency_ms,
            cur_p95,
            config.p95_latency_band,
        ));
    } else if reference_envelope.p95_latency_ms > 0.0 {
        warnings.push("Cannot validate p95 latency: insufficient samples in current run".to_string());
    }

    // Validate total bytes
    let ref_total_bytes = reference_envelope.total_bytes as f64;
    let cur_total_bytes = current_summary.totals.bytes_read as f64;

    results.push(validate_metric(
        "total_bytes",
        ref_total_bytes,
        cur_total_bytes,
        config.total_bytes_band,
    ));

    // Overall pass if all individual validations pass
    let overall_pass = results.iter().all(|r| r.pass);

    ValidationSummary {
        overall_pass,
        results,
        warnings,
    }
}

/// Validate a single metric against tolerance band
fn validate_metric(
    metric: &str,
    reference_value: f64,
    current_value: f64,
    tolerance_band: f64,
) -> ValidationResult {
    let (pass, reason) = if reference_value == 0.0 && current_value == 0.0 {
        (true, "Both values are zero".to_string())
    } else if reference_value == 0.0 {
        (false, "Reference value is zero but current is not".to_string())
    } else {
        let relative_diff = (current_value - reference_value).abs() / reference_value;
        let pass = relative_diff <= tolerance_band;
        let reason = if pass {
            format!("Within tolerance: {:.1}% <= {:.1}%", relative_diff * 100.0, tolerance_band * 100.0)
        } else {
            format!("Outside tolerance: {:.1}% > {:.1}%", relative_diff * 100.0, tolerance_band * 100.0)
        };
        (pass, reason)
    };

    ValidationResult {
        metric: metric.to_string(),
        reference_value,
        current_value,
        tolerance_band,
        pass,
        reason,
    }
}

/// Print validation results in a human-readable format
pub fn print_validation_results(summary: &ValidationSummary) {
    println!("=== Workload Validation Results ===");
    
    for result in &summary.results {
        let status = if result.pass { "✅ PASS" } else { "❌ FAIL" };
        println!(
            "{} {}: ref={:.2}, cur={:.2}, tolerance=±{:.1}%",
            status,
            result.metric,
            result.reference_value,
            result.current_value,
            result.tolerance_band * 100.0
        );
        println!("    Reason: {}", result.reason);
    }

    for warning in &summary.warnings {
        println!("⚠️  WARNING: {}", warning);
    }

    println!();
    if summary.overall_pass {
        println!("🎉 OVERALL RESULT: PASS - All metrics within tolerance");
    } else {
        println!("💥 OVERALL RESULT: FAIL - One or more metrics outside tolerance");
    }
    println!("=====================================");
}

/// Validate and exit with appropriate code for CI integration
pub fn validate_and_exit(
    reference_envelope: &Envelope,
    current_summary: &MetricsSummary,
    config: &ValidationConfig,
) -> ! {
    let summary = validate_against_reference(reference_envelope, current_summary, config);
    print_validation_results(&summary);

    let exit_code = if summary.overall_pass {
        0 // Success
    } else {
        if config.strict_mode {
            2 // Validation failure
        } else {
            1 // Warning but continue
        }
    };

    exit(exit_code);
}

/// Create a validation configuration from CLI arguments or defaults
pub fn create_validation_config(
    mean_bytes_band: Option<f64>,
    p95_latency_band: Option<f64>, 
    total_bytes_band: Option<f64>,
    strict_mode: bool,
) -> Result<ValidationConfig> {
    let validate_band = |name: &str, value: f64| -> Result<f64> {
        if value < 0.0 || value > 1.0 {
            Err(anyhow::anyhow!("{} tolerance must be between 0.0 and 1.0, got {}", name, value))
        } else {
            Ok(value)
        }
    };

    let mut config = ValidationConfig::default();
    config.strict_mode = strict_mode;

    if let Some(band) = mean_bytes_band {
        config.mean_bytes_band = validate_band("mean_bytes_band", band)?;
    }

    if let Some(band) = p95_latency_band {
        config.p95_latency_band = validate_band("p95_latency_band", band)?;
    }

    if let Some(band) = total_bytes_band {
        config.total_bytes_band = validate_band("total_bytes_band", band)?;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oplog_ingest::Envelope;
    use crate::metrics::{MetricsSummary, TotalMetrics, PerformanceMetrics, TimingMetrics};

    fn create_test_envelope() -> Envelope {
        Envelope {
            mean_req_bytes: 1000.0,
            p95_latency_ms: 50.0,
            total_bytes: 100_000,
            n_ops: 100,
            operations_breakdown: std::collections::HashMap::new(),
        }
    }

    fn create_test_summary(bytes_read: u64, files_processed: u64, p95_latency: Option<f64>) -> MetricsSummary {
        MetricsSummary {
            mode: "test".to_string(),
            totals: TotalMetrics {
                files_processed,
                batches_processed: 10,
                bytes_read,
                bytes_written: 0,
                bytes_read_mb: bytes_read as f64 / (1024.0 * 1024.0),
                bytes_written_mb: 0.0,
            },
            performance: PerformanceMetrics {
                read_throughput_mbps: 100.0,
                write_throughput_mbps: 0.0,
                read_throughput_gibps: 0.1,
                write_throughput_gibps: 0.0,
                average_read_time_ms: 10.0,
                average_write_time_ms: 0.0,
            },
            timing: TimingMetrics {
                total_time_secs: 10.0,
                total_epoch_time_secs: 10.0,
                total_compute_time_secs: 5.0,
                average_batch_time_ms: 100.0,
                average_epoch_time_secs: 10.0,
                num_epochs: 1,
                latency_ms_p95: p95_latency,
            },
            accelerator_utilization: None,
        }
    }

    #[test]
    fn test_validation_pass() {
        let reference = create_test_envelope();
        let current = create_test_summary(100_000, 100, Some(52.0)); // Within tolerance
        let config = ValidationConfig::default();

        let summary = validate_against_reference(&reference, &current, &config);
        assert!(summary.overall_pass);
    }

    #[test]
    fn test_validation_fail_bytes() {
        let reference = create_test_envelope();
        let current = create_test_summary(150_000, 100, Some(50.0)); // 50% more bytes
        let config = ValidationConfig::default();

        let summary = validate_against_reference(&reference, &current, &config);
        assert!(!summary.overall_pass);
        
        // Should fail on total bytes (50% > 15% tolerance)
        let bytes_result = summary.results.iter().find(|r| r.metric == "total_bytes").unwrap();
        assert!(!bytes_result.pass);
    }

    #[test]
    fn test_validation_fail_latency() {
        let reference = create_test_envelope();
        let current = create_test_summary(100_000, 100, Some(80.0)); // 60% higher latency
        let config = ValidationConfig::default();

        let summary = validate_against_reference(&reference, &current, &config);
        assert!(!summary.overall_pass);
        
        // Should fail on p95 latency (60% > 25% tolerance)
        let latency_result = summary.results.iter().find(|r| r.metric == "p95_latency_ms").unwrap();
        assert!(!latency_result.pass);
    }

    #[test]
    fn test_validation_missing_p95() {
        let reference = create_test_envelope();
        let current = create_test_summary(100_000, 100, None); // No p95 data
        let config = ValidationConfig::default();

        let summary = validate_against_reference(&reference, &current, &config);
        assert!(!summary.warnings.is_empty());
        assert!(summary.warnings[0].contains("p95 latency"));
    }

    #[test]
    fn test_validate_metric() {
        let result = validate_metric("test", 100.0, 120.0, 0.25); // 20% diff, 25% tolerance
        assert!(result.pass);
        assert_eq!(result.reference_value, 100.0);
        assert_eq!(result.current_value, 120.0);

        let result = validate_metric("test", 100.0, 140.0, 0.25); // 40% diff, 25% tolerance
        assert!(!result.pass);
    }

    #[test]
    fn test_validation_config_creation() {
        let config = create_validation_config(Some(0.1), Some(0.2), Some(0.3), true).unwrap();
        assert_eq!(config.mean_bytes_band, 0.1);
        assert_eq!(config.p95_latency_band, 0.2);
        assert_eq!(config.total_bytes_band, 0.3);
        assert!(config.strict_mode);

        // Test invalid bands
        assert!(create_validation_config(Some(1.5), None, None, false).is_err());
        assert!(create_validation_config(None, Some(-0.1), None, false).is_err());
    }
}