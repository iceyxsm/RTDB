//! Observability and telemetry

use crate::Result;

/// Metrics collector
pub struct Metrics;

impl Metrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self
    }

    /// Initialize Prometheus exporter
    pub fn init_prometheus(&self) -> Result<()> {
        Ok(())
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracing setup
pub struct Tracer;

impl Tracer {
    /// Create new tracer
    pub fn new() -> Self {
        Self
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}
