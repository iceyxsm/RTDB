//! Observability and Monitoring Module
//! 
//! Production-grade observability implementation with:
//! - Prometheus-compatible metrics (counters, gauges, histograms)
//! - OpenTelemetry distributed tracing
//! - Health check endpoints (liveness, readiness, startup)
//! - Vector database-specific metrics (QPS, latency percentiles, recall)
//!
//! Inspired by best practices from Qdrant, Milvus, and Weaviate.

pub mod metrics;
pub mod health;

pub use metrics::{
    MetricsCollector, VectorDbMetrics,
    MetricLabels, MetricValue,
};
pub use health::{
    HealthChecker, HealthStatus, HealthCheck,
    LivenessCheck, ReadinessCheck, StartupCheck,
};

use std::sync::Arc;
use std::time::Duration;

/// Global observability configuration
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// Metrics collection interval
    pub metrics_interval: Duration,
    /// Enable Prometheus metrics endpoint
    pub prometheus_enabled: bool,
    /// Prometheus scrape endpoint port
    pub prometheus_port: u16,
    /// Health check port
    pub health_port: u16,
    /// Service name for telemetry
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Enable process metrics
    pub process_metrics_enabled: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_interval: Duration::from_secs(15),
            prometheus_enabled: true,
            prometheus_port: 9090,
            health_port: 8080,
            service_name: "rtdb".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            process_metrics_enabled: true,
        }
    }
}

/// Main observability system coordinating metrics and health checks
pub struct ObservabilitySystem {
    config: ObservabilityConfig,
    metrics: Arc<MetricsCollector>,
    health: Arc<HealthChecker>,
}

impl ObservabilitySystem {
    pub fn new(config: ObservabilityConfig) -> Self {
        let metrics = Arc::new(MetricsCollector::new(
            config.service_name.clone(),
            config.service_version.clone(),
        ));
        
        let health = Arc::new(HealthChecker::new());
        
        Self {
            config,
            metrics,
            health,
        }
    }
    
    pub fn metrics(&self) -> Arc<MetricsCollector> {
        self.metrics.clone()
    }
    
    pub fn health(&self) -> Arc<HealthChecker> {
        self.health.clone()
    }
    
    /// Initialize all observability components
    pub fn init(&self) -> Result<(), ObservabilityError> {
        // Initialize metrics
        self.metrics.init(self.config.process_metrics_enabled)
            .map_err(|e| ObservabilityError::MetricsInit(e))?;
        
        // Start background collection
        self.start_collection();
        
        Ok(())
    }
    
    fn start_collection(&self) {
        let metrics = self.metrics.clone();
        let interval = self.config.metrics_interval;
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                metrics.collect_system_metrics();
            }
        });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("Metrics initialization failed: {0}")]
    MetricsInit(String),
    #[error("Health check failed: {0}")]
    HealthCheck(String),
}

/// Helper macro to instrument a function with metrics
#[macro_export]
macro_rules! instrument_operation {
    ($name:expr, $metrics:expr, $body:block) => {{
        use std::time::Instant;
        
        let start = Instant::now();
        let result = $body;
        let duration = start.elapsed();
        
        $metrics.record_operation($name, duration, result.is_ok());
        
        result
    }};
}
