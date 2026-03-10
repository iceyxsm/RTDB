//! Metrics collection and export
//! 
//! Prometheus-compatible metrics for monitoring

use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;

/// Metrics collector
pub struct MetricsCollector;

impl MetricsCollector {
    /// Initialize Prometheus metrics exporter
    pub fn init_petheus(addr: &str) -> crate::Result<()> {
        let parsed_addr: SocketAddr = addr
            .parse()
            .map_err(|e| crate::RTDBError::Io(format!("Invalid metrics address: {}", e)))?;

        PrometheusBuilder::new()
            .with_http_listener(parsed_addr)
            .install_recorder()
            .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

        Ok(())
    }

    /// Record query metrics
    pub fn record_query(duration: std::time::Duration) {
        metrics::counter!("rtdb_queries_total").increment(1);
        metrics::histogram!("rtdb_query_duration_seconds").record(duration.as_secs_f64());
    }

    /// Record insert metrics
    pub fn record_insert(duration: std::time::Duration, count: u64) {
        metrics::counter!("rtdb_inserts_total").increment(count);
        metrics::histogram!("rtdb_insert_duration_seconds").record(duration.as_secs_f64());
    }

    /// Record delete metrics
    pub fn record_delete(count: u64) {
        metrics::counter!("rtdb_deletes_total").increment(count);
    }

    /// Update collection count
    pub fn set_collection_count(count: usize) {
        metrics::gauge!("rtdb_collections").set(count as f64);
    }

    /// Update vector count
    pub fn set_vector_count(count: u64) {
        metrics::gauge!("rtdb_vectors").set(count as f64);
    }
}

/// Query timer for automatic duration recording
pub struct QueryTimer {
    start: std::time::Instant,
}

impl QueryTimer {
    /// Start new timer
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    /// Stop and record the duration
    pub fn stop(self) -> std::time::Duration {
        self.start.elapsed()
    }
}

impl Default for QueryTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QueryTimer {
    fn drop(&mut self) {
        MetricsCollector::record_query(self.start.elapsed());
    }
}
