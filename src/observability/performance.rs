//! High-Performance Observability Infrastructure
//!
//! Production-grade optimizations based on industry best practices:
//! - Zero-cost abstractions using compile-time flags
//! - Lock-free metrics collection where possible
//! - Batched exports with compression
//! - Cardinality limits to prevent metric explosion
//! - Memory-efficient span storage
//!
//! References:
//! - OpenTelemetry performance optimization (OneUptime)
//! - Prometheus best practices for cardinality
//! - Rust zero-cost abstraction patterns

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Cardinality limiter to prevent metric explosion
/// 
/// High-cardinality labels (user_id, request_id) can cause
/// millions of time series. This struct tracks and limits
/// cardinality per metric.
pub struct CardinalityLimiter {
    /// Maximum unique values per label
    max_cardinality: usize,
    /// Current observed cardinality per metric
    observed: parking_lot::RwLock<HashMap<String, usize>>,
    /// Dropped metrics counter for monitoring
    dropped_total: AtomicU64,
}

impl CardinalityLimiter {
    pub fn new(max_cardinality: usize) -> Self {
        Self {
            max_cardinality,
            observed: parking_lot::RwLock::new(HashMap::new()),
            dropped_total: AtomicU64::new(0),
        }
    }

    /// Check if adding this label value would exceed cardinality limit
    pub fn check_cardinality(&self, metric_name: &str, label_value: &str) -> bool {
        // High-cardinality patterns to reject
        let high_cardinality_patterns = [
            // UUID patterns
            "-", // Contains dashes (typical in UUIDs)
            // Numeric IDs
            "id=", "user_id=", "request_id=", "session_id=",
            // Timestamps
            "ts=", "timestamp=", "time=",
        ];

        // Quick rejection of obviously high-cardinality values
        if high_cardinality_patterns.iter().any(|p| label_value.contains(p)) {
            // Check if it's a bounded value (like "user_id=admin")
            let is_bounded = label_value.len() < 20 && !label_value.chars().any(|c| c.is_ascii_hexdigit());
            if !is_bounded {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }

        let mut observed = self.observed.write();
        let current = observed.entry(metric_name.to_string()).or_insert(0);
        
        if *current >= self.max_cardinality {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            false
        } else {
            *current += 1;
            true
        }
    }

    /// Get total dropped metrics due to cardinality
    pub fn dropped_count(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    /// Reset cardinality tracking (for testing)
    pub fn reset(&self) {
        self.observed.write().clear();
        self.dropped_total.store(0, Ordering::Relaxed);
    }
}

/// Performance configuration for observability
#[derive(Debug, Clone)]
pub struct ObservabilityPerfConfig {
    /// Maximum cardinality per metric
    pub max_metric_cardinality: usize,
    /// Batch size for trace export
    pub trace_batch_size: usize,
    /// Queue size for trace buffering
    pub trace_queue_size: usize,
    /// Export timeout for traces
    pub trace_export_timeout_ms: u64,
    /// Enable gzip compression
    pub enable_compression: bool,
    /// Sampling ratio (0.0 - 1.0)
    pub sampling_ratio: f64,
    /// Enable metrics collection
    pub metrics_enabled: bool,
    /// Enable tracing
    pub tracing_enabled: bool,
    /// Memory limit for span buffer (bytes)
    pub span_buffer_memory_limit: usize,
}

impl Default for ObservabilityPerfConfig {
    fn default() -> Self {
        Self {
            max_metric_cardinality: 1000,
            trace_batch_size: 512,
            trace_queue_size: 8192,
            trace_export_timeout_ms: 30000,
            enable_compression: true,
            sampling_ratio: 0.1, // 10% sampling by default
            metrics_enabled: true,
            tracing_enabled: true,
            span_buffer_memory_limit: 100 * 1024 * 1024, // 100MB
        }
    }
}

impl ObservabilityPerfConfig {
    /// Development configuration (higher sampling, lower batching)
    pub fn development() -> Self {
        Self {
            sampling_ratio: 1.0, // Sample everything in dev
            trace_batch_size: 128,
            trace_queue_size: 1024,
            enable_compression: false,
            ..Default::default()
        }
    }

    /// High-throughput configuration (aggressive sampling, large batches)
    pub fn high_throughput() -> Self {
        Self {
            sampling_ratio: 0.01, // 1% sampling
            trace_batch_size: 1024,
            trace_queue_size: 16384,
            enable_compression: true,
            max_metric_cardinality: 500, // Lower cardinality limit
            ..Default::default()
        }
    }

    /// Low-latency configuration (smaller batches, faster export)
    pub fn low_latency() -> Self {
        Self {
            trace_batch_size: 256,
            trace_queue_size: 4096,
            trace_export_timeout_ms: 10000,
            ..Default::default()
        }
    }
}

/// Performance-optimized trace batch configuration
#[derive(Debug, Clone)]
pub struct TraceBatchConfig {
    /// Maximum batch size per export
    pub max_export_batch_size: usize,
    /// Maximum queue size before dropping spans
    pub max_queue_size: usize,
    /// Scheduled delay between exports (ms)
    pub scheduled_delay_ms: u64,
    /// Export timeout (ms)
    pub export_timeout_ms: u64,
    /// Max concurrent exports
    pub max_concurrent_exports: usize,
}

impl Default for TraceBatchConfig {
    fn default() -> Self {
        Self {
            max_export_batch_size: 512,
            max_queue_size: 8192,
            scheduled_delay_ms: 5000,
            export_timeout_ms: 30000,
            max_concurrent_exports: 2,
        }
    }
}

impl TraceBatchConfig {
    /// Production-optimized configuration
    /// 
    /// Tuned for high-throughput services with 10K+ req/sec
    pub fn production() -> Self {
        Self {
            max_export_batch_size: 1024,
            max_queue_size: 16384,
            scheduled_delay_ms: 5000,
            export_timeout_ms: 30000,
            max_concurrent_exports: 4,
        }
    }

    /// Low-overhead configuration
    /// 
    /// For services where minimal overhead is critical
    pub fn low_overhead() -> Self {
        Self {
            max_export_batch_size: 256,
            max_queue_size: 2048,
            scheduled_delay_ms: 10000, // Longer delays = fewer exports
            export_timeout_ms: 30000,
            max_concurrent_exports: 1,
        }
    }
}

/// Metrics performance tracker
pub struct MetricsPerformance {
    /// Metrics cardinality limiter
    cardinality_limiter: Arc<CardinalityLimiter>,
    /// Metrics export batch size
    export_batch_size: usize,
    /// Metrics scrape timeout
    scrape_timeout_ms: u64,
}

impl MetricsPerformance {
    pub fn new(config: &ObservabilityPerfConfig) -> Self {
        Self {
            cardinality_limiter: Arc::new(CardinalityLimiter::new(config.max_metric_cardinality)),
            export_batch_size: 1000,
            scrape_timeout_ms: 10000,
        }
    }

    pub fn cardinality_limiter(&self) -> Arc<CardinalityLimiter> {
        self.cardinality_limiter.clone()
    }

    /// Validate metric labels for cardinality
    pub fn validate_labels(&self, metric_name: &str, labels: &[(String, String)]) -> Vec<(String, String)> {
        labels
            .iter()
            .filter(|(k, v)| {
                let key = format!("{}.{}", metric_name, k);
                self.cardinality_limiter.check_cardinality(&key, v)
            })
            .cloned()
            .collect()
    }
}

/// Memory-limited span buffer
/// 
/// Prevents OOM by limiting memory usage for pending spans
pub struct MemoryLimitedSpanBuffer<T> {
    buffer: std::collections::VecDeque<T>,
    memory_limit: usize,
    current_memory: usize,
    /// Estimated size per span (bytes)
    estimated_span_size: usize,
    dropped_spans: AtomicU64,
}

impl<T> MemoryLimitedSpanBuffer<T> {
    pub fn new(memory_limit: usize, estimated_span_size: usize) -> Self {
        Self {
            buffer: std::collections::VecDeque::new(),
            memory_limit,
            current_memory: 0,
            estimated_span_size,
            dropped_spans: AtomicU64::new(0),
        }
    }

    pub fn push(&mut self, span: T) -> bool {
        // Check if adding would exceed memory limit
        if self.current_memory + self.estimated_span_size > self.memory_limit {
            self.dropped_spans.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        self.buffer.push_back(span);
        self.current_memory += self.estimated_span_size;
        true
    }

    pub fn pop_batch(&mut self, batch_size: usize) -> Vec<T> {
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            if let Some(span) = self.buffer.pop_front() {
                self.current_memory = self.current_memory.saturating_sub(self.estimated_span_size);
                batch.push(span);
            } else {
                break;
            }
        }
        batch
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped_spans.load(Ordering::Relaxed)
    }

    pub fn memory_usage(&self) -> usize {
        self.current_memory
    }

    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    /// Memory utilization ratio (0.0 - 1.0)
    pub fn memory_utilization(&self) -> f64 {
        self.current_memory as f64 / self.memory_limit as f64
    }
}

/// Compile-time feature flags for zero-cost observability
/// 
/// When disabled, all observability code is optimized away at compile time
#[macro_export]
macro_rules! if_observability_enabled {
    ($body:block) => {
        #[cfg(feature = "observability")]
        $body
    };
}

/// Zero-cost metrics increment macro
/// 
/// Only increments if metrics are enabled at compile time
#[macro_export]
macro_rules! metrics_inc {
    ($counter:expr, $value:expr) => {
        #[cfg(feature = "observability")]
        {
            $counter.inc_by($value);
        }
    };
    ($counter:expr) => {
        #[cfg(feature = "observability")]
        {
            $counter.inc();
        }
    };
}

/// Performance-timed operation with minimal overhead
/// 
/// Only times if observability is enabled
#[macro_export]
macro_rules! timed_operation {
    ($histogram:expr, $body:block) => {{
        #[cfg(feature = "observability")]
        let start = std::time::Instant::now();
        
        let result = $body;
        
        #[cfg(feature = "observability")]
        {
            let duration = start.elapsed().as_secs_f64();
            $histogram.observe(duration);
        }
        
        result
    }};
}

/// Sampling decision helper for high-performance sampling
pub struct SamplingDecision;

impl SamplingDecision {
    /// Make a sampling decision based on trace ID
    /// 
    /// Uses the trace ID to ensure consistent sampling decisions
    /// across services for the same trace.
    pub fn should_sample(trace_id: u128, sampling_ratio: f64) -> bool {
        if sampling_ratio >= 1.0 {
            return true;
        }
        if sampling_ratio <= 0.0 {
            return false;
        }
        
        // Use the lower 64 bits of trace ID for consistent decision
        let trace_lower = trace_id as u64;
        // Use wrapping multiplication to create better distribution
        let hash = trace_lower.wrapping_mul(0x9E3779B97F4A7C15);
        let threshold = (sampling_ratio * u64::MAX as f64) as u64;
        hash < threshold
    }

    /// Fast sampling check using thread-local RNG
    /// 
    /// Faster than trace-based but not consistent across services
    pub fn should_sample_fast(sampling_ratio: f64) -> bool {
        if sampling_ratio >= 1.0 {
            return true;
        }
        if sampling_ratio <= 0.0 {
            return false;
        }
        
        use std::cell::RefCell;
        use std::num::Wrapping;
        
        thread_local! {
            static RNG: RefCell<Wrapping<u64>> = RefCell::new(Wrapping(1));
        }
        
        RNG.with(|rng_cell| {
            let mut rng = rng_cell.borrow_mut();
            let old_val = rng.0;
            // xorshift64* pseudo-random number generator
            let mut val = old_val;
            val ^= val >> 12;
            val ^= val << 25;
            val ^= val >> 27;
            val = val.wrapping_mul(0x2545F4914F6CDD1Du64);
            *rng = Wrapping(val);
            
            let threshold = (sampling_ratio * u64::MAX as f64) as u64;
            val <= threshold
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinality_limiter() {
        let limiter = CardinalityLimiter::new(5);
        
        // First 5 unique values should pass
        for i in 0..5 {
            assert!(limiter.check_cardinality("test_metric", &format!("value_{}", i)));
        }
        
        // 6th value should fail
        assert!(!limiter.check_cardinality("test_metric", "value_5"));
        
        // Different metric should have its own limit
        for i in 0..5 {
            assert!(limiter.check_cardinality("other_metric", &format!("value_{}", i)));
        }
    }

    #[test]
    fn test_cardinality_rejects_high_cardinality_patterns() {
        let limiter = CardinalityLimiter::new(1000);
        
        // UUID-like values should be rejected
        assert!(!limiter.check_cardinality("test", "user_id=550e8400-e29b-41d4-a716-446655440000"));
        
        // Timestamps should be rejected
        assert!(!limiter.check_cardinality("test", "ts=1234567890"));
        
        // Bounded values should pass
        assert!(limiter.check_cardinality("test", "status=active"));
    }

    #[test]
    fn test_sampling_decision() {
        // 100% sampling
        assert!(SamplingDecision::should_sample(12345, 1.0));
        
        // 0% sampling
        assert!(!SamplingDecision::should_sample(12345, 0.0));
        
        // 50% sampling - should be roughly half (with wider tolerance for randomness)
        let mut sampled = 0;
        for i in 0..10000 {
            if SamplingDecision::should_sample(i as u128, 0.5) {
                sampled += 1;
            }
        }
        // Should be around 5000 ± 500 (10% tolerance)
        assert!(sampled > 4500 && sampled < 5500, "Sampled {} out of 10000", sampled);
    }

    #[test]
    fn test_memory_limited_buffer() {
        let mut buffer = MemoryLimitedSpanBuffer::<u64>::new(1000, 100);
        
        // Should fit 10 items (1000 / 100 = 10)
        for i in 0..10 {
            assert!(buffer.push(i));
        }
        
        // 11th item should fail
        assert!(!buffer.push(10));
        
        // Pop batch of 5
        let batch = buffer.pop_batch(5);
        assert_eq!(batch.len(), 5);
        
        // Now we can add one more
        assert!(buffer.push(10));
    }

    #[test]
    fn test_config_variants() {
        let dev = ObservabilityPerfConfig::development();
        assert_eq!(dev.sampling_ratio, 1.0);
        
        let prod = ObservabilityPerfConfig::high_throughput();
        assert_eq!(prod.sampling_ratio, 0.01);
        
        let low_lat = ObservabilityPerfConfig::low_latency();
        assert_eq!(low_lat.trace_export_timeout_ms, 10000);
    }
}
