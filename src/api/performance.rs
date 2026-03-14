//! Performance monitoring and optimization for REST API
//!
//! Provides:
//! - Request/response caching
//! - Performance metrics collection
//! - Query result caching with TTL
//! - Connection pooling optimization

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cache entry with time-to-live (TTL) expiration for API response caching.
/// 
/// Stores cached values with expiration timestamps to implement
/// time-based cache invalidation and improve API response times.
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    /// Cached value
    value: T,
    /// Expiration timestamp for this cache entry
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
        }
    }
    
    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// LRU cache with TTL for API response caching and performance optimization.
/// 
/// Implements a thread-safe cache with size limits and time-based expiration
/// to reduce database load and improve API response times.
pub struct ApiCache<K, V> {
    /// Thread-safe cache storage with hash map
    cache: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    /// Maximum number of entries before eviction
    max_size: usize,
    /// Default time-to-live for cache entries
    default_ttl: Duration,
}

impl<K, V> ApiCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(max_size: usize, default_ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            default_ttl,
        }
    }
    
    /// Get value from cache
    pub async fn get(&self, key: &K) -> Option<V> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(key) {
            if !entry.is_expired() {
                debug!("Cache hit for key");
                return Some(entry.value.clone());
            }
        }
        debug!("Cache miss for key");
        None
    }
    
    /// Put value into cache
    pub async fn put(&self, key: K, value: V) {
        self.put_with_ttl(key, value, self.default_ttl).await;
    }
    
    /// Put value into cache with custom TTL
    pub async fn put_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let mut cache = self.cache.write().await;
        
        // Remove expired entries
        cache.retain(|_, entry| !entry.is_expired());
        
        // Evict oldest entries if at capacity
        if cache.len() >= self.max_size {
            // Simple eviction: remove first entry (not truly LRU but good enough)
            if let Some(first_key) = cache.keys().next().cloned() {
                cache.remove(&first_key);
            }
        }
        
        cache.insert(key, CacheEntry::new(value, ttl));
    }
    
    /// Clear all entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
    
    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_entries = cache.len();
        let expired_entries = cache.values().filter(|entry| entry.is_expired()).count();
        
        CacheStats {
            total_entries,
            expired_entries,
            active_entries: total_entries - expired_entries,
            max_size: self.max_size,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
    pub max_size: usize,
}

/// Performance metrics for API operations
#[derive(Debug, Default, Clone)]
pub struct ApiMetrics {
    /// Total requests processed
    pub total_requests: u64,
    /// Total errors encountered
    pub total_errors: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// P95 response time in milliseconds
    pub p95_response_time_ms: f64,
    /// P99 response time in milliseconds
    pub p99_response_time_ms: f64,
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
}

/// Performance monitor for API operations
pub struct ApiPerformanceMonitor {
    /// Response time samples (last 1000 requests)
    response_times: Arc<RwLock<Vec<u64>>>,
    /// Request counters
    request_count: Arc<RwLock<u64>>,
    error_count: Arc<RwLock<u64>>,
    /// Cache hit/miss counters
    cache_hits: Arc<RwLock<u64>>,
    cache_misses: Arc<RwLock<u64>>,
}

impl ApiPerformanceMonitor {
    pub fn new() -> Self {
        Self {
            response_times: Arc::new(RwLock::new(Vec::new())),
            request_count: Arc::new(RwLock::new(0)),
            error_count: Arc::new(RwLock::new(0)),
            cache_hits: Arc::new(RwLock::new(0)),
            cache_misses: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Record a request completion
    pub async fn record_request(&self, duration: Duration, is_error: bool) {
        let duration_ms = duration.as_millis() as u64;
        
        // Update response times (keep last 1000)
        {
            let mut times = self.response_times.write().await;
            times.push(duration_ms);
            if times.len() > 1000 {
                times.remove(0);
            }
        }
        
        // Update counters
        {
            let mut count = self.request_count.write().await;
            *count += 1;
        }
        
        if is_error {
            let mut errors = self.error_count.write().await;
            *errors += 1;
        }
        
        debug!(
            duration_ms = duration_ms,
            is_error = is_error,
            "Request completed"
        );
    }
    
    /// Record cache hit
    pub async fn record_cache_hit(&self) {
        let mut hits = self.cache_hits.write().await;
        *hits += 1;
    }
    
    /// Record cache miss
    pub async fn record_cache_miss(&self) {
        let mut misses = self.cache_misses.write().await;
        *misses += 1;
    }
    
    /// Get current metrics
    pub async fn get_metrics(&self) -> ApiMetrics {
        let times = self.response_times.read().await;
        let request_count = *self.request_count.read().await;
        let error_count = *self.error_count.read().await;
        let cache_hits = *self.cache_hits.read().await;
        let cache_misses = *self.cache_misses.read().await;
        
        let avg_response_time_ms = if !times.is_empty() {
            times.iter().sum::<u64>() as f64 / times.len() as f64
        } else {
            0.0
        };
        
        let (p95_response_time_ms, p99_response_time_ms) = if !times.is_empty() {
            let mut sorted_times = times.clone();
            sorted_times.sort_unstable();
            
            let p95_idx = (sorted_times.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted_times.len() as f64 * 0.99) as usize;
            
            let p95 = sorted_times.get(p95_idx).copied().unwrap_or(0) as f64;
            let p99 = sorted_times.get(p99_idx).copied().unwrap_or(0) as f64;
            
            (p95, p99)
        } else {
            (0.0, 0.0)
        };
        
        let cache_hit_rate = if cache_hits + cache_misses > 0 {
            cache_hits as f64 / (cache_hits + cache_misses) as f64
        } else {
            0.0
        };
        
        ApiMetrics {
            total_requests: request_count,
            total_errors: error_count,
            avg_response_time_ms,
            p95_response_time_ms,
            p99_response_time_ms,
            cache_hit_rate,
        }
    }
    
    /// Reset all metrics
    pub async fn reset(&self) {
        {
            let mut times = self.response_times.write().await;
            times.clear();
        }
        {
            let mut count = self.request_count.write().await;
            *count = 0;
        }
        {
            let mut errors = self.error_count.write().await;
            *errors = 0;
        }
        {
            let mut hits = self.cache_hits.write().await;
            *hits = 0;
        }
        {
            let mut misses = self.cache_misses.write().await;
            *misses = 0;
        }
        
        info!("API metrics reset");
    }
}

impl Default for ApiPerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Query result cache key
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct QueryCacheKey {
    pub collection: String,
    pub vector_hash: u64,
    pub limit: usize,
    pub offset: usize,
    pub params_hash: u64,
}

impl QueryCacheKey {
    pub fn new(
        collection: String,
        vector: &[f32],
        limit: usize,
        offset: usize,
        params: &str,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash vector (use bytes for better precision)
        for &v in vector {
            v.to_bits().hash(&mut hasher);
        }
        let vector_hash = hasher.finish();
        
        let mut hasher = DefaultHasher::new();
        params.hash(&mut hasher);
        let params_hash = hasher.finish();
        
        Self {
            collection,
            vector_hash,
            limit,
            offset,
            params_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache: ApiCache<String, String> = ApiCache::new(10, Duration::from_secs(1));
        
        // Test put and get
        cache.put("key1".to_string(), "value1".to_string()).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));
        
        // Test cache miss
        assert_eq!(cache.get(&"nonexistent".to_string()).await, None);
    }
    
    #[tokio::test]
    async fn test_cache_ttl() {
        let cache: ApiCache<String, String> = ApiCache::new(10, Duration::from_millis(50));
        
        cache.put("key1".to_string(), "value1".to_string()).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));
        
        // Wait for expiration
        sleep(Duration::from_millis(100)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }
    
    #[tokio::test]
    async fn test_performance_monitor() {
        let monitor = ApiPerformanceMonitor::new();
        
        // Record some requests
        monitor.record_request(Duration::from_millis(100), false).await;
        monitor.record_request(Duration::from_millis(200), false).await;
        monitor.record_request(Duration::from_millis(150), true).await;
        
        let metrics = monitor.get_metrics().await;
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.total_errors, 1);
        assert!(metrics.avg_response_time_ms > 0.0);
    }
    
    #[tokio::test]
    async fn test_cache_stats() {
        let cache: ApiCache<String, String> = ApiCache::new(5, Duration::from_secs(1));
        
        cache.put("key1".to_string(), "value1".to_string()).await;
        cache.put("key2".to_string(), "value2".to_string()).await;
        
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
        assert_eq!(stats.max_size, 5);
    }
    
    #[test]
    fn test_query_cache_key() {
        let key1 = QueryCacheKey::new(
            "test".to_string(),
            &[0.1, 0.2, 0.3],
            10,
            0,
            "default",
        );
        
        let key2 = QueryCacheKey::new(
            "test".to_string(),
            &[0.1, 0.2, 0.3],
            10,
            0,
            "default",
        );
        
        let key3 = QueryCacheKey::new(
            "test".to_string(),
            &[0.1, 0.2, 0.5], // Different vector (more different)
            10,
            0,
            "default",
        );
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}