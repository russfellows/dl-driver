// Unit tests for core functionality
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_backend_detection() {
        // Test each URI scheme detection
        let test_cases = vec![
            ("file:///tmp/test", StorageBackend::File),
            ("s3://bucket/path", StorageBackend::S3), 
            ("az://account/container/path", StorageBackend::Azure),
            ("direct:///tmp/test", StorageBackend::DirectIO),
            ("/local/path", StorageBackend::File), // default to file
            ("https://example.com", StorageBackend::File), // unknown becomes file
        ];

        for (uri, expected_backend) in test_cases {
            let config = Config {
                model: None,
                framework: None,
                workflow: WorkflowConfig {
                    generate_data: Some(true),
                    train: Some(true),
                    checkpoint: Some(false),
                },
                dataset: DatasetConfig {
                    data_folder: uri.to_string(),
                    format: "npz".to_string(),
                    num_files_train: 1,
                    num_samples_per_file: Some(1),
                    record_length_bytes: Some(1024),
                    record_length_bytes_stdev: Some(0),
                    record_length_bytes_resize: Some(1024),
                },
                reader: ReaderConfig {
                    data_loader: "pytorch".to_string(),
                    batch_size: 1,
                    read_threads: Some(1),
                    file_shuffle: Some("seed".to_string()),
                    sample_shuffle: Some("seed".to_string()),
                },
                train: Some(TrainConfig {
                    epochs: 1,
                    computation_time: Some(0.01),
                }),
                checkpoint: None,
            };

            assert!(matches!(config.storage_backend(), expected_backend), 
                    "URI '{}' should detect as {:?}", uri, expected_backend);
        }
    }

    #[test]
    fn test_storage_uri_extraction() {
        let config = Config {
            model: None,
            framework: None,
            workflow: WorkflowConfig {
                generate_data: Some(true),
                train: Some(true),
                checkpoint: Some(false),
            },
            dataset: DatasetConfig {
                data_folder: "s3://my-bucket/my-path/".to_string(),
                format: "npz".to_string(),
                num_files_train: 10,
                num_samples_per_file: Some(1),
                record_length_bytes: Some(2048),
                record_length_bytes_stdev: Some(0),
                record_length_bytes_resize: Some(2048),
            },
            reader: ReaderConfig {
                data_loader: "pytorch".to_string(),
                batch_size: 4,
                read_threads: Some(2),
                file_shuffle: Some("seed".to_string()),
                sample_shuffle: Some("seed".to_string()),
            },
            train: Some(TrainConfig {
                epochs: 5,
                computation_time: Some(0.05),
            }),
            checkpoint: None,
        };

        assert_eq!(config.storage_uri(), "s3://my-bucket/my-path/");
        assert_eq!(config.dataset.num_files_train, 10);
        assert_eq!(config.reader.batch_size, 4);
    }
}
