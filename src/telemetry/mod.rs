//! Observability and telemetry

pub mod metrics;
pub mod tracing;
pub mod health;

use crate::Result;

/// Initialize all telemetry systems
pub fn init_telemetry(json_format: bool) -> Result<()> {
    if json_format {
        tracing::init_json_tracing();
    } else {
        tracing::init_tracing();
    }
    
    Ok(())
}

/// Initialize Prometheus metrics
pub fn init_metrics(addr: &str) -> Result<()> {
    metrics::MetricsCollector::init_petheus(addr)
}
