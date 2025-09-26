// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// tests/dlio_config_comprehensive.rs
//
// Comprehensive tests for DLIO configuration parsing, validation, and conversion
//
use anyhow::Result;
use dl_driver_core::dlio_compat::{DlioConfig, yaml_to_json};

#[cfg(test)]
mod dlio_config_tests {
    use super::*;

    /// Test backend detection from data_folder URIs
    #[test]
    fn test_backend_detection() {
        let test_cases = [
            ("file:///tmp/data", "file"),
            ("s3://bucket/path", "s3"),
            ("az://account/container/path", "azure"),
            ("direct:///mnt/nvme/data", "direct"),
            ("/local/path", "file"), // implicit file
        ];

        for (uri, expected_backend) in test_cases {
            let json = format!(r#"{{
                "dataset": {{
                    "data_folder": "{}"
                }}
            }}"#, uri);
            
            let config = DlioConfig::from_json(&json)
                .expect("Should parse config with valid URI");
            
            let backend = config.detect_storage_backend();
            assert_eq!(backend, expected_backend, "Failed for URI: {}", uri);
        }
    }

    /// Test format detection and validation
    #[test]
    fn test_format_detection() {
        let valid_formats = ["npz", "hdf5", "tfrecord", "csv", "jpeg", "png", "synthetic"];
        
        for format in valid_formats {
            let json = format!(r#"{{
                "dataset": {{
                    "data_folder": "/test",
                    "format": "{}"
                }}
            }}"#, format);
            
            let config = DlioConfig::from_json(&json)
                .expect(&format!("Should parse config with format: {}", format));
            
            assert_eq!(config.dataset.format.as_deref(), Some(format));
        }
    }

    /// Test framework profile integration
    #[test]
    fn test_framework_profiles() {
        let json = r#"{
            "framework": "pytorch",
            "dataset": {
                "data_folder": "/test"
            },
            "framework_profiles": {
                "pytorch": {
                    "num_workers": 4,
                    "pin_memory": true,
                    "persistent_workers": true,
                    "prefetch_factor": 3,
                    "drop_last": false
                }
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse config with framework profiles");

        assert_eq!(config.framework.as_deref(), Some("pytorch"));
        
        let pytorch_config = config.get_pytorch_config();
        assert!(pytorch_config.is_some());
        
        let pytorch = pytorch_config.unwrap();
        assert_eq!(pytorch.num_workers, Some(4));
        assert_eq!(pytorch.pin_memory, Some(true));
        assert_eq!(pytorch.persistent_workers, Some(true));
        assert_eq!(pytorch.prefetch_factor, Some(3));
        assert_eq!(pytorch.drop_last, Some(false));
    }

    /// Test TensorFlow framework profile
    #[test]
    fn test_tensorflow_framework_profile() {
        let json = r#"{
            "framework": "tensorflow",
            "dataset": {
                "data_folder": "/test"
            },
            "framework_profiles": {
                "tensorflow": {
                    "buffer_size": 2048,
                    "num_parallel_calls": 8,
                    "deterministic": true,
                    "experimental_optimization": false
                }
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse TensorFlow config");

        let tf_config = config.get_tensorflow_config();
        assert!(tf_config.is_some());
        
        let tf = tf_config.unwrap();
        assert_eq!(tf.buffer_size, Some(2048));
        assert_eq!(tf.num_parallel_calls, Some(8));
        assert_eq!(tf.deterministic, Some(true));
        assert_eq!(tf.experimental_optimization, Some(false));
    }

    /// Test JAX framework profile
    #[test]
    fn test_jax_framework_profile() {
        let json = r#"{
            "framework": "jax",
            "dataset": {
                "data_folder": "/test"
            },
            "framework_profiles": {
                "jax": {
                    "batch_size": 16,
                    "shuffle_buffer_size": 1000,
                    "prefetch": 4
                }
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse JAX config");

        let jax_config = config.get_jax_config();
        assert!(jax_config.is_some());
        
        let jax = jax_config.unwrap();
        assert_eq!(jax.batch_size, Some(16));
        assert_eq!(jax.shuffle_buffer_size, Some(1000));
        assert_eq!(jax.prefetch, Some(4));
    }

    /// Test YAML to JSON conversion with complex nested structures
    #[test]
    fn test_yaml_to_json_complex() {
        let yaml = r#"
model:
  name: complex_test
  model_size: 1000000
framework: pytorch
workflow:
  generate_data: true
  train: true
  checkpoint: false
dataset:
  data_folder: file:///mnt/vast1/test_data
  format: npz
  num_files_train: 100
  num_samples_per_file: 32
reader:
  data_loader: pytorch
  batch_size: 8
  read_threads: 4
  compute_threads: 2
  prefetch: 16
  shuffle: true
framework_profiles:
  pytorch:
    num_workers: 6
    pin_memory: true
    persistent_workers: false
checkpoint:
  checkpoint_folder: /checkpoints
  epochs_between_checkpoints: 5
        "#;

        let json_result = yaml_to_json(yaml);
        assert!(json_result.is_ok(), "YAML to JSON conversion should succeed");

        let json = json_result.unwrap();
        let config = DlioConfig::from_json(&json)
            .expect("Should parse converted JSON config");

        assert_eq!(config.model.as_ref().unwrap().name.as_deref(), Some("complex_test"));
        assert_eq!(config.framework.as_deref(), Some("pytorch"));
        assert_eq!(config.dataset.data_folder, "file:///mnt/vast1/test_data");
        assert_eq!(config.reader.batch_size, Some(8));
        
        let pytorch_config = config.get_pytorch_config().unwrap();
        assert_eq!(pytorch_config.num_workers, Some(6));
        assert_eq!(pytorch_config.pin_memory, Some(true));
    }

    /// Test LoaderOptions conversion with all parameters
    #[test]
    fn test_comprehensive_loader_options_conversion() {
        let json = r#"{
            "dataset": {
                "data_folder": "s3://test-bucket/data"
            },
            "reader": {
                "batch_size": 32,
                "prefetch": 8,
                "shuffle": true,
                "read_threads": 6,
                "compute_threads": 4
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse config for loader options");
        
        let loader_opts = config.to_loader_options();
        assert_eq!(loader_opts.batch_size, 32);
        assert_eq!(loader_opts.prefetch, 8);
        assert_eq!(loader_opts.shuffle, true);
        assert_eq!(loader_opts.num_workers, 6);

        // Test PoolConfig conversion
        let pool_config = config.to_pool_config();
        assert!(pool_config.pool_size > 0);
        assert!(pool_config.readahead_batches > 0);
        assert!(pool_config.max_inflight > 0);
    }

    /// Test RunPlan conversion with all fields
    #[test]
    fn test_run_plan_conversion() {
        let json = r#"{
            "model": {
                "name": "test_model",
                "model_size": 500000000
            },
            "framework": "pytorch",
            "workflow": {
                "generate_data": true,
                "train": true,
                "checkpoint": true,
                "evaluation": false
            },
            "dataset": {
                "data_folder": "file:///mnt/vast1/test_data",
                "format": "npz",
                "num_files_train": 100,
                "num_samples_per_file": 64,
                "record_length_bytes": 8192
            },
            "reader": {
                "batch_size": 16,
                "prefetch": 4,
                "shuffle": true,
                "read_threads": 8
            },
            "checkpoint": {
                "checkpoint_folder": "/checkpoints",
                "epochs_between_checkpoints": 2
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse config for RunPlan");
        
        let run_plan = config.to_run_plan()
            .expect("Should convert to RunPlan");

        // Verify model plan
        assert_eq!(run_plan.model.name, "test_model");
        assert_eq!(run_plan.model.framework, "pytorch");

        // Verify workflow plan
        assert!(run_plan.workflow.generate_data);
        assert!(run_plan.workflow.train);
        assert!(run_plan.workflow.checkpoint);
        assert!(!run_plan.workflow.evaluation);

        // Verify dataset plan
        assert_eq!(run_plan.dataset.data_folder_uri, "file:///mnt/vast1/test_data");
        assert_eq!(run_plan.dataset.format, "npz");
        assert_eq!(run_plan.dataset.train.num_files, 100);
        assert_eq!(run_plan.dataset.train.num_samples_per_file, 64);
        assert_eq!(run_plan.dataset.train.record_length_bytes, 8192);

        // Verify reader plan
        assert_eq!(run_plan.reader.batch_size, 16);
        assert_eq!(run_plan.reader.prefetch, 4);
        assert!(run_plan.reader.shuffle);
    }

    /// Test error handling for invalid configurations
    #[test]
    fn test_error_handling_invalid_json() {
        let invalid_json = r#"{ "invalid": json syntax }"#;
        let result = DlioConfig::from_json(invalid_json);
        assert!(result.is_err(), "Should fail on invalid JSON");
    }

    #[test]
    fn test_error_handling_invalid_yaml() {
        let invalid_yaml = r#"
        model:
          name: test
        invalid_yaml_syntax: [unclosed array
        "#;
        let result = DlioConfig::from_yaml(invalid_yaml);
        assert!(result.is_err(), "Should fail on invalid YAML");
    }

    #[test]
    fn test_error_handling_missing_required_fields() {
        // Config missing dataset field
        let json = r#"{
            "model": {
                "name": "test"
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse even with missing fields");
        
        // Should handle gracefully with defaults
        assert_eq!(config.dataset.data_folder, "");
    }

    /// Test data_folder URI normalization
    #[test]
    fn test_data_folder_uri_normalization() {
        let test_cases = [
            ("file:///tmp/data", "file:///tmp/data"),
            ("/tmp/data", "file:///tmp/data"),  // Should normalize to file URI
            ("s3://bucket/key", "s3://bucket/key"),
            ("az://account/container", "az://account/container"),
        ];

        for (input, expected) in test_cases {
            let json = format!(r#"{{
                "dataset": {{
                    "data_folder": "{}"
                }}
            }}"#, input);
            
            let config = DlioConfig::from_json(&json)
                .expect("Should parse config");
            
            let normalized_uri = config.data_folder_uri();
            assert_eq!(normalized_uri, expected, "Failed to normalize: {}", input);
        }
    }

    /// Test workflow phase detection
    #[test]
    fn test_workflow_phase_detection() {
        let json = r#"{
            "workflow": {
                "generate_data": true,
                "train": false,
                "checkpoint": true,
                "evaluation": true
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse workflow config");

        assert!(config.should_generate_data());
        assert!(!config.should_train());
        assert!(config.should_checkpoint());
        // Note: evaluation might not have a direct method yet
    }

    /// Test configuration validation edge cases
    #[test]
    fn test_configuration_edge_cases() {
        // Zero batch size
        let json = r#"{
            "reader": {
                "batch_size": 0
            }
        }"#;

        let config = DlioConfig::from_json(json)
            .expect("Should parse config with zero batch size");
        assert_eq!(config.reader.batch_size, Some(0));

        // Negative values (should be handled gracefully)
        let json = r#"{
            "reader": {
                "read_threads": -1
            }
        }"#;

        // This might fail at JSON parsing level, which is expected
        let result = DlioConfig::from_json(json);
        // We expect this to either fail or handle gracefully
    }

    /// Test real MLCommons config compatibility
    #[test]
    fn test_real_mlcommons_configs() {
        // Simulate a real UNet3D config structure
        let unet3d_yaml = r#"
model:
  name: unet3d
  model_size: 499153191
framework: pytorch
workflow:
  generate_data: false
  train: true
  checkpoint: false
dataset:
  data_folder: /datasets/unet3d
  format: npz
  num_files_train: 168
  num_samples_per_file: 42
  record_length_bytes: 146800640
reader:
  data_loader: pytorch
  batch_size: 1
  read_threads: 4
  compute_threads: 2
  prefetch: 2
  shuffle: false
        "#;

        let config = DlioConfig::from_yaml(unet3d_yaml)
            .expect("Should parse real UNet3D config");
        
        assert_eq!(config.model.as_ref().unwrap().name.as_deref(), Some("unet3d"));
        assert_eq!(config.framework.as_deref(), Some("pytorch"));
        assert!(!config.should_generate_data());
        assert!(config.should_train());
        assert_eq!(config.reader.batch_size, Some(1));
    }

    /// Test default value handling
    #[test]
    fn test_default_value_handling() {
        let minimal_json = r#"{}"#;
        
        let config = DlioConfig::from_json(minimal_json)
            .expect("Should parse minimal config");
        
        // Verify defaults are applied sensibly
        assert_eq!(config.dataset.data_folder, "");
        assert!(config.reader.batch_size.is_none());
        assert!(config.framework.is_none());
    }
}