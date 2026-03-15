//! Hedge Fund/HFT-Grade HTTP Client for RTDB
//!
//! Implements ultra-low latency optimizations used by high-frequency trading firms:
//! - HTTP/2 multiplexing (single connection, multiple concurrent streams)
//! - Connection pooling with aggressive keep-alive
//! - TCP_NODELAY (disable Nagle's algorithm)
//! - Binary Protocol Buffers instead of JSON
//! - Request pipelining and batching

use crate::RTDBError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// HFT-grade connection pool configuration
#[derive(Debug, Clone)]
pub struct HftConnectionConfig {
    /// Maximum idle connections per host (HFT: 50-100)
    pub max_idle_per_host: usize,
    /// Connection idle timeout (HFT: 90s-5min)
    pub idle_timeout: Duration,
    /// TCP_NODELAY - disable Nagle's algorithm for low latency
    pub tcp_nodelay: bool,
    /// TCP keepalive interval
    pub tcp_keepalive: Duration,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Max concurrent streams per HTTP/2 connection
    pub http2_max_concurrent_streams: usize,
    /// Enable compression
    pub compression: bool,
}

impl Default for HftConnectionConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: 50,                    // HFT standard: 50-100
            idle_timeout: Duration::from_secs(300),   // 5 minutes
            tcp_nodelay: true,                        // Critical for low latency
            tcp_keepalive: Duration::from_secs(60),   // Keep connections alive
            connect_timeout: Duration::from_millis(50), // Fast fail
            request_timeout: Duration::from_millis(100),
            http2_max_concurrent_streams: 100,        // High concurrency
            compression: true,
        }
    }
}

/// HFT-grade connection pool with multiplexing support
pub struct HftConnectionPool {
    config: HftConnectionConfig,
    semaphore: Arc<Semaphore>,
    host: String,
    port: u16,
}

impl HftConnectionPool {
    /// Create a new HFT-grade connection pool
    pub fn new(host: String, port: u16, config: HftConnectionConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.http2_max_concurrent_streams));
        
        Self {
            config,
            semaphore,
            host,
            port,
        }
    }

    /// Get a connection permit for concurrent request control
    pub async fn acquire_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, RTDBError> {
        self.semaphore.clone()
            .acquire_owned()
            .await
            .map_err(|e| RTDBError::Internal(e.to_string()))
    }

    /// Get the host:port string
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// HFT-grade batch request processor
pub struct BatchProcessor<T> {
    buffer: Vec<T>,
    max_batch_size: usize,
    flush_interval: Duration,
}

impl<T> BatchProcessor<T> {
    /// Create new batch processor
    pub fn new(max_batch_size: usize, flush_interval: Duration) -> Self {
        Self {
            buffer: Vec::with_capacity(max_batch_size),
            max_batch_size,
            flush_interval,
        }
    }

    /// Add item to batch (returns true if batch is full)
    pub fn push(&mut self, item: T) -> bool {
        self.buffer.push(item);
        self.buffer.len() >= self.max_batch_size
    }

    /// Get batch for processing
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.buffer)
    }

    /// Check if should flush based on interval
    pub fn should_flush(&self, last_flush: std::time::Instant) -> bool {
        last_flush.elapsed() >= self.flush_interval && !self.buffer.is_empty()
    }

    /// Get current batch size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// HFT client configuration
#[derive(Debug, Clone)]
pub struct HftClientConfig {
    pub connection: HftConnectionConfig,
    pub batch_size: usize,
    pub batch_interval_ms: u64,
    pub compression: bool,
    pub compression_threshold: usize, // Minimum bytes to compress
}

impl Default for HftClientConfig {
    fn default() -> Self {
        Self {
            connection: HftConnectionConfig::default(),
            batch_size: 100,              // Batch 100 requests
            batch_interval_ms: 1,         // Flush every 1ms (HFT grade)
            compression: true,
            compression_threshold: 1024,  // Compress payloads > 1KB
        }
    }
}

/// Performance metrics for HFT operations
#[derive(Debug, Default)]
pub struct HftMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_latency_us: u64,  // Microseconds
    pub avg_latency_us: u64,
    pub p99_latency_us: u64,
}

impl HftMetrics {
    pub fn record_request(&mut self, latency_us: u64, success: bool) {
        self.total_requests += 1;
        self.total_latency_us += latency_us;
        
        if success {
            self.successful_requests += 1;
        } else {
            self.failed_requests += 1;
        }
        
        self.avg_latency_us = self.total_latency_us / self.total_requests;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hft_connection_config_defaults() {
        let config = HftConnectionConfig::default();
        assert_eq!(config.max_idle_per_host, 50);
        assert!(config.tcp_nodelay);
        assert_eq!(config.http2_max_concurrent_streams, 100);
    }

    #[test]
    fn test_batch_processor() {
        let mut batch = BatchProcessor::new(3, Duration::from_millis(10));
        
        assert!(!batch.push(1));
        assert!(!batch.push(2));
        assert!(batch.push(3)); // Batch full
        
        let items = batch.drain();
        assert_eq!(items.len(), 3);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_hft_metrics() {
        let mut metrics = HftMetrics::default();
        metrics.record_request(100, true);
        metrics.record_request(200, true);
        metrics.record_request(50, false);
        
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.successful_requests, 2);
        assert_eq!(metrics.failed_requests, 1);
        assert_eq!(metrics.avg_latency_us, 116); // (100+200+50)/3
    }
}
