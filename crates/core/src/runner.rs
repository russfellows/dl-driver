// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use std::time::Instant;

use crate::config::DlioConfig;
use crate::metrics::Metrics;

/// Placeholder runner for milestone M1
pub struct Runner {
    config: DlioConfig,
    metrics: Metrics,
}

impl Runner {
    pub fn new(config: DlioConfig) -> Self {
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
