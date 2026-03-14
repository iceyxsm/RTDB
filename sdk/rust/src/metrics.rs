use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Client metrics collection
pub struct ClientMetrics {
    pub requests_total: AtomicU64,
    pub requests_success: AtomicU64,
    pub requests_error: AtomicU64,
    pub request_duration_total_ns: AtomicU64,
    pub vectors_processed: AtomicU64,
    pub search_results_total: AtomicU64,
    pub batch_queries_total: AtomicU64,
}

impl ClientMetrics {
    pub fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            requests_success: AtomicU64::new(0),
            requests_error: AtomicU64::new(0),
            request_duration_total_ns: AtomicU64::new(0),
            vectors_processed: AtomicU64::new(0),
            search_results_total: AtomicU64::new(0),
            batch_queries_total: AtomicU64::new(0),
        }
    }

    pub fn record_request_latency(&self, operation: &str, duration: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_success.fetch_add(1, Ordering::Relaxed);
        self.request_duration_total_ns.fetch_add(
            duration.as_nanos() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn record_error(&self, operation: &str) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_error.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_vectors_processed(&self, operation: &str, count: usize) {
        self.vectors_processed.fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn record_search_results(&self, count: usize) {
        self.search_results_total.fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn record_batch_queries(&self, count: usize) {
        self.batch_queries_total.fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn get_success_rate(&self) -> f64 {
        let total = self.requests_total.load(Ordering::Relaxed);
        if total == 0 {
            return 1.0;
        }
        let success = self.requests_success.load(Ordering::Relaxed);
        success as f64 / total as f64
    }

    pub fn get_average_latency_ms(&self) -> f64 {
        let total_requests = self.requests_total.load(Ordering::Relaxed);
        if total_requests == 0 {
            return 0.0;
        }
        let total_duration_ns = self.request_duration_total_ns.load(Ordering::Relaxed);
        (total_duration_ns as f64 / total_requests as f64) / 1_000_000.0
    }
}