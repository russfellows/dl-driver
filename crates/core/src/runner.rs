//
//
use anyhow::Result;
use std::{
    fs,
    path::Path,
    time::Instant,
};
use real_dlio_storage::StorageBackend;
use real_dlio_formats::Format;

use crate::metrics::Metrics;

/// For M1: generate one NPZ, put/get via backend, collect timings.
pub struct Runner<B, F> {
    backend: B,
    format: F,
    metrics: Metrics,
}

impl<B, F> Runner<B, F>
where
    B: StorageBackend,
    F: Format,
{
    pub fn new(backend: B, format: F) -> Self {
        Runner { backend, format, metrics: Metrics::new() }
    }

    pub fn run_once<P: AsRef<Path>>(&mut self, key: &str, local_path: P) -> Result<()> {
        let local = local_path.as_ref();

        // 1) generate
        let t0 = Instant::now();
        self.format.generate(local)?;
        self.metrics.record("generate", t0);

        // 2) read raw bytes from FS
        let data = fs::read(local)?;
        self.metrics.record("read_fs", Instant::now());

        // 3) write via backend
        let t1 = Instant::now();
        self.backend.put(key, &data)?;
        self.metrics.record("backend_put", t1);

        // 4) read back via backend
        let t2 = Instant::now();
        let _ = self.backend.get(key)?;
        self.metrics.record("backend_get", t2);

        // 5) cleanup
        fs::remove_file(local)?;

        self.metrics.report();
        Ok(())
    }
}
