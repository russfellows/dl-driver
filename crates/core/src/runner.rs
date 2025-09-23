use anyhow::Result;
use std::time::Instant;

use crate::config::Config;
use crate::metrics::Metrics;

/// Placeholder runner for milestone M1
pub struct Runner {
    config: Config,
    metrics: Metrics,
}

impl Runner {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            metrics: Metrics::new(),
        }
    }

    pub fn run_once(&mut self, _input: &str, _output: &str) -> Result<()> {
        let start = Instant::now();

        // Placeholder for actual execution
        println!("Running workload for config: {:?}", self.config.model);

        self.metrics.record_read_time(start.elapsed());
        Ok(())
    }

    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }
}
