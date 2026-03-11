//! Prometheus-compatible Metrics Collection
//! 
//! Implements metrics following Qdrant/Milvus best practices:
//! - Query latency histograms with p50/p95/p99 tracking
//! - QPS (queries per second) counters
//! - Index-specific metrics (recall, index size)
//! - Resource utilization (memory, CPU)

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, HistogramOpts, HistogramVec,
    Encoder, Registry, TextEncoder, Opts,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;

/// Labels for vector database metrics
#[derive(Debug, Clone, Default)]
pub struct MetricLabels {
    pub operation: Option<String>,
    pub collection: Option<String>,
    pub index_type: Option<String>,
    pub status: Option<String>,
    pub node_id: Option<String>,
}

impl MetricLabels {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }
    
    pub fn collection(mut self, col: impl Into<String>) -> Self {
        self.collection = Some(col.into());
        self
    }
    
    pub fn index_type(mut self, idx: impl Into<String>) -> Self {
        self.index_type = Some(idx.into());
        self
    }
    
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }
    
    pub fn node_id(mut self, node: impl Into<String>) -> Self {
        self.node_id = Some(node.into());
        self
    }
    
    fn to_label_values(&self) -> Vec<&str> {
        let mut values = Vec::new();
        if let Some(ref op) = self.operation {
            values.push(op.as_str());
        }
        if let Some(ref col) = self.collection {
            values.push(col.as_str());
        }
        if let Some(ref status) = self.status {
            values.push(status.as_str());
        }
        values
    }
}

/// Metric value types
#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Duration(Duration),
}

/// Vector database-specific metrics collection
/// 
/// Tracks key metrics recommended by industry best practices:
/// - Query latency (p50, p95, p99)
/// - QPS per operation type
/// - Index recall metrics
/// - Memory utilization
pub struct VectorDbMetrics {
    // Query metrics
    pub query_duration: HistogramVec,
    pub queries_total: CounterVec,
    pub query_errors_total: CounterVec,
    
    // Index metrics
    pub index_size_bytes: GaugeVec,
    pub index_vector_count: GaugeVec,
    pub index_recall: GaugeVec,
    pub index_build_duration: HistogramVec,
    
    // Storage metrics
    pub storage_size_bytes: Gauge,
    pub storage_documents_total: Gauge,
    pub storage_collections_total: Gauge,
    
    // Connection metrics
    pub connections_active: Gauge,
    pub connections_total: Counter,
    
    // Replication metrics
    pub replication_lag_seconds: GaugeVec,
    pub replication_ops_total: CounterVec,
}

impl VectorDbMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        // Latency histogram buckets optimized for vector database queries
        // Dense buckets 0-100ms (most traffic), sparse for outliers
        let latency_buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.075,
            0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5,
            0.75, 1.0, 1.5, 2.0, 2.5, 5.0, 10.0,
        ];
        
        // Query duration histogram with operation and collection labels
        let query_duration = HistogramVec::new(
            HistogramOpts::new(
                "rtdb_query_duration_seconds",
                "Query latency distribution in seconds",
            )
            .buckets(latency_buckets.clone()),
            &["operation", "collection"],
        )?;
        
        // Total queries counter
        let queries_total = CounterVec::new(
            Opts::new(
                "rtdb_queries_total",
                "Total number of queries",
            ),
            &["operation", "collection", "status"],
        )?;
        
        // Query errors counter
        let query_errors_total = CounterVec::new(
            Opts::new(
                "rtdb_query_errors_total",
                "Total number of query errors",
            ),
            &["operation", "error_type"],
        )?;
        
        // Index size in bytes
        let index_size_bytes = GaugeVec::new(
            Opts::new(
                "rtdb_index_size_bytes",
                "Size of index in bytes",
            ),
            &["collection", "index_type"],
        )?;
        
        // Vector count per index
        let index_vector_count = GaugeVec::new(
            Opts::new(
                "rtdb_index_vector_count",
                "Number of vectors in index",
            ),
            &["collection", "index_type"],
        )?;
        
        // Index recall metric (for approximate search quality)
        let index_recall = GaugeVec::new(
            Opts::new(
                "rtdb_index_recall",
                "Index recall ratio (approximate search quality)",
            ),
            &["collection", "index_type"],
        )?;
        
        // Index build duration
        let index_build_duration = HistogramVec::new(
            HistogramOpts::new(
                "rtdb_index_build_duration_seconds",
                "Time spent building index",
            )
            .buckets(latency_buckets),
            &["collection", "index_type"],
        )?;
        
        // Storage metrics
        let storage_size_bytes = Gauge::new(
            "rtdb_storage_size_bytes",
            "Total storage size in bytes",
        )?;
        
        let storage_documents_total = Gauge::new(
            "rtdb_storage_documents_total",
            "Total number of documents stored",
        )?;
        
        let storage_collections_total = Gauge::new(
            "rtdb_storage_collections_total",
            "Total number of collections",
        )?;
        
        // Connection metrics
        let connections_active = Gauge::new(
            "rtdb_connections_active",
            "Number of active connections",
        )?;
        
        let connections_total = Counter::new(
            "rtdb_connections_total",
            "Total number of connections",
        )?;
        
        // Replication metrics
        let replication_lag_seconds = GaugeVec::new(
            Opts::new(
                "rtdb_replication_lag_seconds",
                "Replication lag in seconds",
            ),
            &["source_node", "target_node"],
        )?;
        
        let replication_ops_total = CounterVec::new(
            Opts::new(
                "rtdb_replication_ops_total",
                "Total replication operations",
            ),
            &["operation", "status"],
        )?;
        
        // Register all metrics
        registry.register(Box::new(query_duration.clone()))?;
        registry.register(Box::new(queries_total.clone()))?;
        registry.register(Box::new(query_errors_total.clone()))?;
        registry.register(Box::new(index_size_bytes.clone()))?;
        registry.register(Box::new(index_vector_count.clone()))?;
        registry.register(Box::new(index_recall.clone()))?;
        registry.register(Box::new(index_build_duration.clone()))?;
        registry.register(Box::new(storage_size_bytes.clone()))?;
        registry.register(Box::new(storage_documents_total.clone()))?;
        registry.register(Box::new(storage_collections_total.clone()))?;
        registry.register(Box::new(connections_active.clone()))?;
        registry.register(Box::new(connections_total.clone()))?;
        registry.register(Box::new(replication_lag_seconds.clone()))?;
        registry.register(Box::new(replication_ops_total.clone()))?;
        
        Ok(Self {
            query_duration,
            queries_total,
            query_errors_total,
            index_size_bytes,
            index_vector_count,
            index_recall,
            index_build_duration,
            storage_size_bytes,
            storage_documents_total,
            storage_collections_total,
            connections_active,
            connections_total,
            replication_lag_seconds,
            replication_ops_total,
        })
    }
}

/// Central metrics collector
pub struct MetricsCollector {
    service_name: String,
    service_version: String,
    registry: Registry,
    vector_db: VectorDbMetrics,
    system_metrics: SystemMetrics,
    custom_counters: RwLock<HashMap<String, Counter>>,
    custom_gauges: RwLock<HashMap<String, Gauge>>,
}

impl MetricsCollector {
    pub fn new(service_name: String, service_version: String) -> Self {
        let registry = Registry::new();
        let vector_db = VectorDbMetrics::new(&registry)
            .expect("Failed to create vector DB metrics");
        let system_metrics = SystemMetrics::new(&registry)
            .expect("Failed to create system metrics");
        
        Self {
            service_name,
            service_version,
            registry,
            vector_db,
            system_metrics,
            custom_counters: RwLock::new(HashMap::new()),
            custom_gauges: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn init(&self, process_metrics: bool) -> Result<(), String> {
        if process_metrics {
            // Process metrics are registered automatically by prometheus crate
        }
        Ok(())
    }
    
    /// Get the vector database metrics
    pub fn vector_db(&self) -> &VectorDbMetrics {
        &self.vector_db
    }
    
    /// Get system metrics
    pub fn system_metrics(&self) -> &SystemMetrics {
        &self.system_metrics
    }
    
    /// Record a query operation with latency
    pub fn record_query(&self, operation: &str, collection: &str, duration: Duration, success: bool) {
        let status = if success { "success" } else { "error" };
        
        self.vector_db.query_duration
            .with_label_values(&[operation, collection])
            .observe(duration.as_secs_f64());
        
        self.vector_db.queries_total
            .with_label_values(&[operation, collection, status])
            .inc();
    }
    
    /// Record a query error
    pub fn record_query_error(&self, operation: &str, error_type: &str) {
        self.vector_db.query_errors_total
            .with_label_values(&[operation, error_type])
            .inc();
    }
    
    /// Record index metrics
    pub fn record_index_metrics(
        &self,
        collection: &str,
        index_type: &str,
        vector_count: usize,
        size_bytes: u64,
    ) {
        self.vector_db.index_vector_count
            .with_label_values(&[collection, index_type])
            .set(vector_count as f64);
        
        self.vector_db.index_size_bytes
            .with_label_values(&[collection, index_type])
            .set(size_bytes as f64);
    }
    
    /// Record index recall (search quality)
    pub fn record_index_recall(&self, collection: &str, index_type: &str, recall: f64) {
        self.vector_db.index_recall
            .with_label_values(&[collection, index_type])
            .set(recall);
    }
    
    /// Record index build time
    pub fn record_index_build(&self, collection: &str, index_type: &str, duration: Duration) {
        self.vector_db.index_build_duration
            .with_label_values(&[collection, index_type])
            .observe(duration.as_secs_f64());
    }
    
    /// Record replication lag
    pub fn record_replication_lag(&self, source: &str, target: &str, lag: Duration) {
        self.vector_db.replication_lag_seconds
            .with_label_values(&[source, target])
            .set(lag.as_secs_f64());
    }
    
    /// Record replication operation
    pub fn record_replication_op(&self, operation: &str, success: bool) {
        let status = if success { "success" } else { "error" };
        self.vector_db.replication_ops_total
            .with_label_values(&[operation, status])
            .inc();
    }
    
    /// Update storage metrics
    pub fn update_storage_metrics(
        &self,
        size_bytes: u64,
        documents: usize,
        collections: usize,
    ) {
        self.vector_db.storage_size_bytes.set(size_bytes as f64);
        self.vector_db.storage_documents_total.set(documents as f64);
        self.vector_db.storage_collections_total.set(collections as f64);
    }
    
    /// Record active connections
    pub fn record_active_connections(&self, count: usize) {
        self.vector_db.connections_active.set(count as f64);
    }
    
    /// Record new connection
    pub fn record_connection(&self) {
        self.vector_db.connections_total.inc();
    }
    
    /// Create or get a custom counter
    pub fn counter(&self, name: &str, help: &str) -> Result<Counter, prometheus::Error> {
        let counters = self.custom_counters.read();
        if let Some(counter) = counters.get(name) {
            return Ok(counter.clone());
        }
        drop(counters);
        
        let mut counters = self.custom_counters.write();
        let counter = Counter::new(name, help)?;
        self.registry.register(Box::new(counter.clone()))?;
        counters.insert(name.to_string(), counter.clone());
        Ok(counter)
    }
    
    /// Create or get a custom gauge
    pub fn gauge(&self, name: &str, help: &str) -> Result<Gauge, prometheus::Error> {
        let gauges = self.custom_gauges.read();
        if let Some(gauge) = gauges.get(name) {
            return Ok(gauge.clone());
        }
        drop(gauges);
        
        let mut gauges = self.custom_gauges.write();
        let gauge = Gauge::new(name, help)?;
        self.registry.register(Box::new(gauge.clone()))?;
        gauges.insert(name.to_string(), gauge.clone());
        Ok(gauge)
    }
    
    /// Record generic operation with metrics
    pub fn record_operation(&self, name: &str, duration: Duration, success: bool) {
        let status = if success { "success" } else { "error" };
        
        // Create operation-specific metrics on first use
        let _ = self.counter(
            &format!("rtdb_{}_total", name),
            &format!("Total {} operations", name),
        ).map(|c| {
            c.inc();
        });
        
        // Record duration if histogram exists
        let histogram_name = format!("rtdb_{}_duration_seconds", name);
        let labels = ["status"];
    }
    
    /// Collect system metrics (CPU, memory, etc.)
    pub fn collect_system_metrics(&self) {
        self.system_metrics.collect();
    }
    
    /// Export metrics in Prometheus text format
    pub fn export_prometheus(&self) -> Result<String, String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)
            .map_err(|e| format!("Failed to encode metrics: {}", e))?;
        String::from_utf8(buffer)
            .map_err(|e| format!("Invalid UTF-8 in metrics: {}", e))
    }
    
    /// Get service info
    pub fn service_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert("name".to_string(), self.service_name.clone());
        info.insert("version".to_string(), self.service_version.clone());
        info
    }
}

/// System-level metrics (CPU, memory, etc.)
pub struct SystemMetrics {
    pub cpu_usage: Gauge,
    pub memory_usage_bytes: Gauge,
    pub memory_total_bytes: Gauge,
    pub disk_usage_bytes: Gauge,
    pub disk_total_bytes: Gauge,
    pub open_files: Gauge,
    pub goroutines: Gauge, // Will map to async tasks in Rust
}

impl SystemMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let cpu_usage = Gauge::new(
            "rtdb_system_cpu_usage_percent",
            "CPU usage percentage",
        )?;
        
        let memory_usage_bytes = Gauge::new(
            "rtdb_system_memory_usage_bytes",
            "Memory usage in bytes",
        )?;
        
        let memory_total_bytes = Gauge::new(
            "rtdb_system_memory_total_bytes",
            "Total system memory in bytes",
        )?;
        
        let disk_usage_bytes = Gauge::new(
            "rtdb_system_disk_usage_bytes",
            "Disk usage in bytes",
        )?;
        
        let disk_total_bytes = Gauge::new(
            "rtdb_system_disk_total_bytes",
            "Total disk space in bytes",
        )?;
        
        let open_files = Gauge::new(
            "rtdb_system_open_files",
            "Number of open file descriptors",
        )?;
        
        let goroutines = Gauge::new(
            "rtdb_system_async_tasks",
            "Number of active async tasks",
        )?;
        
        registry.register(Box::new(cpu_usage.clone()))?;
        registry.register(Box::new(memory_usage_bytes.clone()))?;
        registry.register(Box::new(memory_total_bytes.clone()))?;
        registry.register(Box::new(disk_usage_bytes.clone()))?;
        registry.register(Box::new(disk_total_bytes.clone()))?;
        registry.register(Box::new(open_files.clone()))?;
        registry.register(Box::new(goroutines.clone()))?;
        
        Ok(Self {
            cpu_usage,
            memory_usage_bytes,
            memory_total_bytes,
            disk_usage_bytes,
            disk_total_bytes,
            open_files,
            goroutines,
        })
    }
    
    /// Collect system metrics
    fn collect(&self) {
        // Memory info
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb.parse::<f64>() {
                                self.memory_total_bytes.set(kb * 1024.0);
                            }
                        }
                    }
                    if line.starts_with("MemAvailable:") {
                        if let Some(kb) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb.parse::<f64>() {
                                let total = self.memory_total_bytes.get();
                                self.memory_usage_bytes.set(total - kb * 1024.0);
                            }
                        }
                    }
                }
            }
            
            // Open file descriptors
            if let Ok(read_dir) = std::fs::read_dir("/proc/self/fd") {
                let count = read_dir.count();
                self.open_files.set(count as f64);
            }
        }
        
        // Async task count (approximation via tokio runtime metrics if available)
        // For now, we'll leave this for manual instrumentation
    }
}

/// Metrics middleware for gRPC/HTTP services
pub struct MetricsMiddleware {
    collector: Arc<MetricsCollector>,
}

impl MetricsMiddleware {
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self { collector }
    }
    
    /// Record an incoming request
    pub fn record_request(&self, method: &str, duration: Duration, success: bool) {
        self.collector.record_query("grpc", method, duration, success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new("test".to_string(), "1.0".to_string());
        assert!(collector.init(true).is_ok());
    }
    
    #[test]
    fn test_query_recording() {
        let collector = MetricsCollector::new("test".to_string(), "1.0".to_string());
        collector.record_query("search", "test_collection", Duration::from_millis(50), true);
        collector.record_query("search", "test_collection", Duration::from_millis(100), false);
        
        let output = collector.export_prometheus().unwrap();
        assert!(output.contains("rtdb_query_duration_seconds"));
        assert!(output.contains("rtdb_queries_total"));
    }
    
    #[test]
    fn test_index_metrics() {
        let collector = MetricsCollector::new("test".to_string(), "1.0".to_string());
        collector.record_index_metrics("users", "hnsw", 10000, 1024 * 1024 * 100);
        collector.record_index_recall("users", "hnsw", 0.98);
        
        let output = collector.export_prometheus().unwrap();
        assert!(output.contains("rtdb_index_vector_count"));
        assert!(output.contains("rtdb_index_recall"));
    }
}
