//! Integration test for results directory creation and TSV export
//!
//! Tests the complete workflow:
//! 1. Controller receives histogram data from agents
//! 2. Creates results directory structure
//! 3. Writes per-agent TSV files
//! 4. Creates consolidated TSV with merged histograms
//! 5. Verifies accurate percentile aggregation

use dl_driver_core::dist::histogram::{histogram_from_samples, serialize_histogram};
use dl_driver_core::dist::proto::WorkloadSummary;
use dl_driver_core::dist::types::{AggregateResults, WorkloadResult};
use tempfile::TempDir;

#[test]
fn test_results_directory_structure() {
    // Create temporary directory for test output
    let temp_dir = TempDir::new().unwrap();
    let test_config = temp_dir.path().join("test_config.yaml");
    std::fs::write(&test_config, "# test config").unwrap();

    // Create results directory
    let results_dir = dl_driver_core::results_dir::ResultsDir::create(
        &test_config,
        Some("integration_test"),
        Some(temp_dir.path()),
        3, // 3 agents
    )
    .unwrap();

    // Verify basic structure
    assert!(results_dir.path().exists());
    assert!(results_dir.path().join("config.yaml").exists());
    assert!(results_dir.path().join("console.log").exists());

    // Create agents directory
    let mut results_dir_mut = results_dir;
    let agents_dir = results_dir_mut.create_agents_dir().unwrap();
    assert!(agents_dir.exists());

    // Write some console output
    results_dir_mut
        .write_console("Test console output")
        .unwrap();

    // Finalize
    results_dir_mut.finalize(10.5, 3).unwrap();

    // Verify metadata.json was created
    assert!(results_dir_mut.path().join("metadata.json").exists());

    // Read and verify metadata
    let metadata_content = std::fs::read_to_string(results_dir_mut.path().join("metadata.json")).unwrap();
    println!("Metadata content: {}", metadata_content);
    assert!(metadata_content.contains("total_agents"));
    assert!(metadata_content.contains("successful_agents"));
    assert!(metadata_content.contains("duration_secs"));
}

#[test]
fn test_per_agent_results_writing() {
    let temp_dir = TempDir::new().unwrap();
    let test_config = temp_dir.path().join("test_config.yaml");
    std::fs::write(&test_config, "# test config").unwrap();

    let results_dir = dl_driver_core::results_dir::ResultsDir::create(
        &test_config,
        Some("agent_test"),
        Some(temp_dir.path()),
        2,
    )
    .unwrap();

    let agents_dir = temp_dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    // Write agent results
    results_dir
        .write_agent_results(
            &agents_dir,
            "agent-0",
            "agent_id\tops_s\nmyagent\t1000.0\n",
            "agent_id\tsamples_s\nmyagent\t5000.0\n",
            "{\"agent_id\": \"agent-0\"}",
        )
        .unwrap();

    // Verify agent directory structure
    let agent_dir = agents_dir.join("agent-0");
    assert!(agent_dir.exists());
    assert!(agent_dir.join("storage_results.tsv").exists());
    assert!(agent_dir.join("aiml_results.tsv").exists());
    assert!(agent_dir.join("metadata.json").exists());

    // Verify content
    let storage_content = std::fs::read_to_string(agent_dir.join("storage_results.tsv")).unwrap();
    assert!(storage_content.contains("1000.0"));

    let aiml_content = std::fs::read_to_string(agent_dir.join("aiml_results.tsv")).unwrap();
    assert!(aiml_content.contains("5000.0"));
}

#[test]
fn test_aggregate_results_with_histogram_data() {
    // Create mock agent results with different workloads
    let agent1 = WorkloadResult {
        agent_id: "agent-0".to_string(),
        ops_per_s: 1000.0,
        mib_per_s: 500.0,
        p50_us: 10.0,
        p90_us: 20.0,
        p95_us: 25.0,
        p99_us: 30.0,
        errors: 0,
        total_ops: 10000,
        duration_s: 10.0,
        samples_per_second: 5000.0,
        total_samples: 50000,
        samples_per_batch: 64,
        batches_per_second: 78.0,
        total_batches: 780,
        avg_batch_time_ms: 12.8,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 6.0,
        compute_time_s: 3.5,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
    };

    let agent2 = WorkloadResult {
        agent_id: "agent-1".to_string(),
        ops_per_s: 1200.0,
        mib_per_s: 600.0,
        p50_us: 12.0,
        p90_us: 22.0,
        p95_us: 27.0,
        p99_us: 32.0,
        errors: 1,
        total_ops: 12000,
        duration_s: 10.0,
        samples_per_second: 6000.0,
        total_samples: 60000,
        samples_per_batch: 64,
        batches_per_second: 94.0,
        total_batches: 940,
        avg_batch_time_ms: 10.6,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 5.5,
        compute_time_s: 4.0,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
    };

    // Create histogram data for agents
    // Agent 1: 10000 ops at 100μs each
    let agent1_samples: Vec<u64> = (0..10000).map(|_| 100).collect();
    let agent1_hist = histogram_from_samples(&agent1_samples, 3_600_000_000).unwrap();
    let agent1_hist_bytes = serialize_histogram(&agent1_hist).unwrap();

    // Agent 2: 12000 ops at 120μs each
    let agent2_samples: Vec<u64> = (0..12000).map(|_| 120).collect();
    let agent2_hist = histogram_from_samples(&agent2_samples, 3_600_000_000).unwrap();
    let agent2_hist_bytes = serialize_histogram(&agent2_hist).unwrap();

    let summary1 = WorkloadSummary {
        agent_id: "agent-0".to_string(),
        ops_per_s: 1000.0,
        mib_per_s: 500.0,
        p50_us: 10.0,
        p90_us: 20.0,
        p95_us: 25.0,
        p99_us: 30.0,
        errors: 0,
        total_ops: 10000,
        duration_s: 10.0,
        samples_per_second: 5000.0,
        total_samples: 50000,
        samples_per_batch: 64,
        batches_per_second: 78.0,
        total_batches: 780,
        avg_batch_time_ms: 12.8,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 6.0,
        compute_time_s: 3.5,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read: agent1_hist_bytes.clone(),
        histogram_write: vec![],
        histogram_batch: vec![],
    };

    let summary2 = WorkloadSummary {
        agent_id: "agent-1".to_string(),
        ops_per_s: 1200.0,
        mib_per_s: 600.0,
        p50_us: 12.0,
        p90_us: 22.0,
        p95_us: 27.0,
        p99_us: 32.0,
        errors: 1,
        total_ops: 12000,
        duration_s: 10.0,
        samples_per_second: 6000.0,
        total_samples: 60000,
        samples_per_batch: 64,
        batches_per_second: 94.0,
        total_batches: 940,
        avg_batch_time_ms: 10.6,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 5.5,
        compute_time_s: 4.0,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
        console_log: String::new(),
        metadata_json: String::new(),
        storage_tsv_content: String::new(),
        aiml_tsv_content: String::new(),
        results_path: String::new(),
        histogram_read: agent2_hist_bytes.clone(),
        histogram_write: vec![],
        histogram_batch: vec![],
    };

    // Test aggregation with histograms
    let results = vec![agent1, agent2];
    let summaries = vec![summary1, summary2];

    let aggregate = AggregateResults::from_results_with_histograms(results, &summaries).unwrap();

    // Verify aggregated metrics
    assert_eq!(aggregate.total_ops_per_s, 2200.0);
    assert_eq!(aggregate.total_mib_per_s, 1100.0);
    assert_eq!(aggregate.total_ops, 22000);
    assert_eq!(aggregate.total_samples, 110000);

    // With histogram merging, percentiles should be calculated correctly
    // Agent 1: 10000 samples at 100μs (45.45% of total)
    // Agent 2: 12000 samples at 120μs (54.55% of total)
    // Correct p50 should be around 110-120μs (in the weighted middle)
    
    // Convert to milliseconds for comparison (histogram is in microseconds)
    let p50_us = aggregate.avg_p50_us * 1000.0;
    
    // With correct histogram merging, p50 should be between 100-120μs
    // (leaning toward 120 since agent2 has more samples)
    assert!(
        p50_us >= 100.0 && p50_us <= 120.0,
        "p50 = {:.2}μs (expected between 100-120μs with histogram merging)",
        p50_us
    );

    println!("Histogram-based p50: {:.2}μs", p50_us);
    println!("Histogram-based p90: {:.2}μs", aggregate.avg_p90_us * 1000.0);
    println!("Histogram-based p99: {:.2}μs", aggregate.avg_p99_us * 1000.0);
}

#[test]
fn test_consolidated_tsv_format() {
    // Create simple aggregate results
    let agent1 = WorkloadResult {
        agent_id: "agent-0".to_string(),
        ops_per_s: 1000.0,
        mib_per_s: 500.0,
        p50_us: 10.0,
        p90_us: 20.0,
        p95_us: 25.0,
        p99_us: 30.0,
        errors: 0,
        total_ops: 10000,
        duration_s: 10.0,
        samples_per_second: 5000.0,
        total_samples: 50000,
        samples_per_batch: 64,
        batches_per_second: 78.0,
        total_batches: 780,
        avg_batch_time_ms: 12.8,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 6.0,
        compute_time_s: 3.5,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
    };

    let aggregate = AggregateResults::from_results(vec![agent1]).unwrap();

    // Test storage TSV format
    let storage_tsv = aggregate.to_storage_tsv();
    assert!(storage_tsv.contains("agent_id\tops_s\tmib_s"));
    assert!(storage_tsv.contains("agent-0\t1000.0\t500.0"));
    assert!(storage_tsv.contains("AGGREGATE\t1000.0\t500.0"));

    // Test AI/ML TSV format
    let aiml_tsv = aggregate.to_aiml_tsv();
    assert!(aiml_tsv.contains("agent_id\tsamples_s\ttotal_samples"));
    assert!(aiml_tsv.contains("agent-0\t5000.0\t50000"));
    assert!(aiml_tsv.contains("AGGREGATE\t5000.0\t50000"));
}

#[test]
fn test_complete_workflow_simulation() {
    // This test simulates the complete workflow of:
    // 1. Creating results directory
    // 2. Writing per-agent results
    // 3. Creating consolidated TSV files
    // 4. Verifying all files exist with correct content

    let temp_dir = TempDir::new().unwrap();
    let test_config = temp_dir.path().join("workflow_test.yaml");
    std::fs::write(&test_config, "# workflow test config\ndataset:\n  data_folder: /tmp/data\n").unwrap();

    // Step 1: Create results directory
    let results_dir = dl_driver_core::results_dir::ResultsDir::create(
        &test_config,
        Some("complete_workflow"),
        Some(temp_dir.path()),
        2,
    )
    .unwrap();

    let agents_dir = temp_dir.path().join(results_dir.path()).join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    // Step 2: Write per-agent results
    for i in 0..2 {
        let agent_id = format!("agent-{}", i);
        let storage_tsv = format!(
            "agent_id\tops_s\tmib_s\tp50_ms\tp90_ms\tp95_ms\tp99_ms\terrors\ttotal_ops\tduration_s\n{}\t1000.0\t500.0\t10.0\t20.0\t25.0\t30.0\t0\t10000\t10.0\n",
            agent_id
        );
        let aiml_tsv = format!(
            "agent_id\tsamples_s\ttotal_samples\tbatches_s\ttotal_batches\tsamples_per_batch\tavg_batch_ms\tepochs\tavg_epoch_s\tdata_load_s\tcompute_s\tpipeline_eff\n{}\t5000.0\t50000\t78.0\t780\t64\t12.8\t1\t10.0\t6.0\t3.5\t0.950\n",
            agent_id
        );
        let metadata = format!("{{\"agent_id\": \"{}\"}}", agent_id);

        results_dir
            .write_agent_results(&agents_dir, &agent_id, &storage_tsv, &aiml_tsv, &metadata)
            .unwrap();
    }

    // Step 3: Verify per-agent files exist
    for i in 0..2 {
        let agent_dir = agents_dir.join(format!("agent-{}", i));
        assert!(agent_dir.exists(), "Agent directory should exist");
        assert!(
            agent_dir.join("storage_results.tsv").exists(),
            "Storage TSV should exist"
        );
        assert!(
            agent_dir.join("aiml_results.tsv").exists(),
            "AI/ML TSV should exist"
        );
        assert!(
            agent_dir.join("metadata.json").exists(),
            "Metadata should exist"
        );
    }

    // Step 4: Create consolidated results
    let agent1 = WorkloadResult {
        agent_id: "agent-0".to_string(),
        ops_per_s: 1000.0,
        mib_per_s: 500.0,
        p50_us: 10.0,
        p90_us: 20.0,
        p95_us: 25.0,
        p99_us: 30.0,
        errors: 0,
        total_ops: 10000,
        duration_s: 10.0,
        samples_per_second: 5000.0,
        total_samples: 50000,
        samples_per_batch: 64,
        batches_per_second: 78.0,
        total_batches: 780,
        avg_batch_time_ms: 12.8,
        epochs_completed: 1,
        avg_epoch_time_s: 10.0,
        data_loading_time_s: 6.0,
        compute_time_s: 3.5,
        pipeline_efficiency: 0.95,
        accelerator_utilization: 0.85,
    };

    let agent2 = agent1.clone();
    let agent2 = WorkloadResult {
        agent_id: "agent-1".to_string(),
        ..agent2
    };

    let aggregate = AggregateResults::from_results(vec![agent1, agent2]).unwrap();

    // Write consolidated TSV files
    let storage_tsv_path = results_dir.storage_tsv_path();
    let aiml_tsv_path = results_dir.aiml_tsv_path();

    std::fs::write(&storage_tsv_path, aggregate.to_storage_tsv()).unwrap();
    std::fs::write(&aiml_tsv_path, aggregate.to_aiml_tsv()).unwrap();

    // Step 5: Verify consolidated files
    assert!(storage_tsv_path.exists(), "Consolidated storage TSV should exist");
    assert!(aiml_tsv_path.exists(), "Consolidated AI/ML TSV should exist");

    let storage_content = std::fs::read_to_string(&storage_tsv_path).unwrap();
    assert!(storage_content.contains("AGGREGATE"));
    assert!(storage_content.contains("2000.0")); // Combined ops_per_s

    let aiml_content = std::fs::read_to_string(&aiml_tsv_path).unwrap();
    assert!(aiml_content.contains("AGGREGATE"));
    assert!(aiml_content.contains("10000.0")); // Combined samples_per_second

    println!("\n✅ Complete workflow test passed!");
    println!("   Results directory: {}", results_dir.path().display());
    println!("   Storage TSV: {}", storage_tsv_path.display());
    println!("   AI/ML TSV: {}", aiml_tsv_path.display());
    println!("   Agents: 2 subdirectories created");
}
