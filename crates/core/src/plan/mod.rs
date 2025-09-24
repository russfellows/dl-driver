// crates/core/src/plan/mod.rs
use crate::config::DlioConfig;
use s3dlio::data_loader::{LoaderOptions, PoolConfig};
use s3dlio::{ReaderMode, LoadingMode};

#[derive(Debug, Clone)]
pub struct RunPlan {
    pub uri: String,
    pub format: String,
    pub batch_size: usize,
    pub prefetch: usize,
    pub shuffle: bool,
    pub read_threads: usize,
    pub drop_last: bool,
    pub seed: Option<u64>,
    pub num_files_train: Option<usize>,
    pub record_length_bytes: Option<usize>,
    pub num_samples_per_file: Option<usize>,
}

impl RunPlan {
    pub fn from_config(cfg: &DlioConfig) -> Self {
        let r = &cfg.reader;
        Self {
            uri: cfg.dataset.data_folder.clone(),
            format: cfg.dataset.format.clone(),
            batch_size: r.batch_size.unwrap_or(1),
            prefetch: r.prefetch.unwrap_or(4),
            shuffle: r.shuffle.unwrap_or(false),
            read_threads: r.read_threads.unwrap_or(1),
            drop_last: r.drop_last.unwrap_or(false),
            seed: r.seed,
            num_files_train: cfg.dataset.num_files_train,
            record_length_bytes: cfg.dataset.record_length_bytes,
            num_samples_per_file: cfg.dataset.num_samples_per_file,
        }
    }

    /// Convert this RunPlan to s3dlio LoaderOptions
    pub fn to_loader_options(&self) -> LoaderOptions {
        LoaderOptions {
            batch_size: self.batch_size,
            prefetch: self.prefetch,
            shuffle: self.shuffle,
            num_workers: self.read_threads,
            reader_mode: ReaderMode::Sequential, // Start with sequential for DLIO compatibility
            loading_mode: LoadingMode::AsyncPool(self.to_pool_config()),
            seed: self.seed.unwrap_or(0),
            ..Default::default()
        }
    }

    /// Create PoolConfig for AsyncPoolDataLoader
    pub fn to_pool_config(&self) -> PoolConfig {
        // These settings aren't in DLIO YAML - use reasonable defaults
        // Can be overridden via CLI flags
        PoolConfig {
            pool_size: 16,
            readahead_batches: self.prefetch.max(2),
            batch_timeout: std::time::Duration::from_secs(10),
            max_inflight: 64,
        }
    }
}

impl Default for RunPlan {
    fn default() -> Self {
        Self {
            uri: "file:///tmp/default".to_string(),
            format: "npz".to_string(),
            batch_size: 1,
            prefetch: 4,
            shuffle: false,
            read_threads: 1,
            drop_last: false,
            seed: None,
            num_files_train: None,
            record_length_bytes: None,
            num_samples_per_file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    #[test]
    fn test_run_plan_from_config() {
        let cfg = DlioConfig {
            model: Some(Model { name: Some("test".to_string()), model_size: None }),
            framework: Some("pytorch".to_string()),
            workflow: Some(Workflow { train: Some(true), ..Default::default() }),
            dataset: Dataset {
                data_folder: "s3://test-bucket/data".to_string(),
                format: "npz".to_string(),
                num_files_train: Some(100),
                num_files_eval: None,
                record_length_bytes: Some(1024),
                num_samples_per_file: Some(10),
                compression: None,
            },
            reader: Reader {
                batch_size: Some(32),
                prefetch: Some(8),
                shuffle: Some(true),
                read_threads: Some(4),
                compute_threads: None,
                drop_last: Some(true),
                seed: Some(42),
                data_loader: None,
            },
            checkpoint: None,
        };

        let plan = RunPlan::from_config(&cfg);

        assert_eq!(plan.uri, "s3://test-bucket/data");
        assert_eq!(plan.format, "npz");
        assert_eq!(plan.batch_size, 32);
        assert_eq!(plan.prefetch, 8);
        assert_eq!(plan.shuffle, true);
        assert_eq!(plan.read_threads, 4);
        assert_eq!(plan.drop_last, true);
        assert_eq!(plan.seed, Some(42));
        assert_eq!(plan.num_files_train, Some(100));
    }

    #[test]
    fn test_loader_options_conversion() {
        let plan = RunPlan {
            batch_size: 16,
            prefetch: 6,
            shuffle: true,
            read_threads: 2,
            seed: Some(123),
            ..Default::default()
        };

        let opts = plan.to_loader_options();
        assert_eq!(opts.batch_size, 16);
        assert_eq!(opts.prefetch, 6);
        assert_eq!(opts.shuffle, true);
        assert_eq!(opts.num_workers, 2);
        assert_eq!(opts.seed, 123);
    }
}