//
//
use std::time::{Duration, Instant};

pub struct Metrics {
    entries: Vec<(String, Duration)>,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics { entries: Vec::new() }
    }

    /// Record an operation `name` with its start Instant.
    pub fn record(&mut self, name: &str, start: Instant) {
        self.entries.push((name.to_string(), start.elapsed()));
    }

    /// Print a simple report.
    pub fn report(&self) {
        println!("── Metrics ──");
        for (name, dur) in &self.entries {
            println!("{:12} {:>6} ms", name, dur.as_millis());
        }
    }
}

