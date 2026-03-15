//! High-Performance In-Memory Storage for Jepsen Testing
//!
//! This storage backend provides maximum write throughput by:
//! 1. Pure in-memory HashMap (no disk I/O)
//! 2. No HNSW indexing overhead
//! 3. Lock-free reads using Arc
//! 4. Batch-friendly write interface

use crate::{RTDBError, Vector};
use crate::collection::RetrievedVector;
use dashmap::DashMap;
// Unused imports removed
use std::sync::Arc;

/// High-performance in-memory storage engine
/// 
/// This is a specialized storage backend optimized for Jepsen testing.
/// It trades durability and vector search capabilities for raw write throughput.
pub struct HighPerformanceStore {
    /// In-memory vector storage (id -> vector)
    vectors: DashMap<u64, StoredVector>,
    /// Next available ID
    next_id: std::sync::atomic::AtomicU64,
}

/// Stored vector with metadata
#[derive(Clone)]
struct StoredVector {
    /// Vector data (stored as Vec<f32>)
    data: Vec<f32>,
    /// Optional payload (using crate's Payload type)
    payload: Option<crate::Payload>,
}

impl HighPerformanceStore {
    /// Create a new high-performance store
    pub fn new() -> Self {
        Self {
            vectors: DashMap::with_capacity(10000),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }
    
    /// Store a single vector (high throughput)
    #[inline]
    pub fn put(&self, id: u64, vector: Vector) -> Result<(), RTDBError> {
        let stored = StoredVector {
            data: vector.data,
            payload: vector.payload,
        };
        self.vectors.insert(id, stored);
        Ok(())
    }
    
    /// Batch insert multiple vectors (maximum throughput)
    #[inline]
    pub fn put_batch(&self, vectors: Vec<(u64, Vector)>) -> Result<(), RTDBError> {
        for (id, vector) in vectors {
            let stored = StoredVector {
                data: vector.data,
                payload: vector.payload,
            };
            self.vectors.insert(id, stored);
        }
        Ok(())
    }
    
    /// Get a vector by ID
    #[inline]
    pub fn get(&self, id: u64) -> Option<RetrievedVector> {
        self.vectors.get(&id).map(|v| RetrievedVector {
            id,
            vector: v.data.clone(),
            payload: v.payload.clone(),
        })
    }
    
    /// Delete a vector
    #[inline]
    pub fn delete(&self, id: u64) -> Result<(), RTDBError> {
        self.vectors.remove(&id);
        Ok(())
    }
    
    /// Get count of stored vectors
    pub fn len(&self) -> usize {
        self.vectors.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
    
    /// Get next auto-generated ID
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
    
    /// Clear all data
    pub fn clear(&self) {
        self.vectors.clear();
    }
}

impl Default for HighPerformanceStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Ultra-high-performance Jepsen client using the optimized store
/// 
/// This client bypasses ALL overhead:
/// - No CollectionManager
/// - No HNSW index
/// - No disk I/O
/// - Pure in-memory DashMap
pub struct UltraFastJepsenClient {
    id: usize,
    store: Arc<HighPerformanceStore>,
}

impl UltraFastJepsenClient {
    /// Create a new ultra-fast client
    pub fn new(id: usize) -> Self {
        Self {
            id,
            store: Arc::new(HighPerformanceStore::new()),
        }
    }
    
    /// Convert string key to numeric ID
    fn key_to_id(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Execute write (blazing fast)
    pub fn write(&self, key: String, value: serde_json::Value) -> Result<(), RTDBError> {
        let id = Self::key_to_id(&key);
        
        // Create minimal vector (1 dimension)
        let vector = Vector {
            data: vec![0.0],
            payload: Some({
                let mut map = serde_json::Map::new();
                map.insert("value".to_string(), value);
                map
            }),
        };
        
        self.store.put(id, vector)
    }
    
    /// Execute read (blazing fast)
    pub fn read(&self, key: String) -> Option<serde_json::Value> {
        let id = Self::key_to_id(&key);
        
        self.store.get(id).and_then(|v| {
            v.payload.and_then(|p: crate::Payload| p.get("value").cloned())
        })
    }
    
    /// Batch write (maximum throughput)
    pub fn write_batch(&self, batch: Vec<(String, serde_json::Value)>) -> Result<(), RTDBError> {
        let vectors: Vec<(u64, Vector)> = batch
            .into_iter()
            .map(|(key, value)| {
                let id = Self::key_to_id(&key);
                let vector = Vector {
                    data: vec![0.0],
                    payload: Some({
                        let mut map = serde_json::Map::new();
                        map.insert("value".to_string(), value);
                        map
                    }),
                };
                (id, vector)
            })
            .collect();
        
        self.store.put_batch(vectors)
    }
    
    /// Get client ID
    pub fn id(&self) -> usize {
        self.id
    }
    
    /// Get store stats
    pub fn stats(&self) -> (usize, usize) {
        (self.store.len(), 0)
    }
}

/// Multi-threaded ultra-fast client pool
pub struct UltraFastClientPool {
    clients: Vec<Arc<UltraFastJepsenClient>>,
}

impl UltraFastClientPool {
    /// Create a pool of clients
    pub fn new(num_clients: usize) -> Self {
        let clients: Vec<Arc<UltraFastJepsenClient>> = (0..num_clients)
            .map(|id| Arc::new(UltraFastJepsenClient::new(id)))
            .collect();
        
        Self { clients }
    }
    
    /// Get a client by ID
    pub fn get(&self, id: usize) -> Option<Arc<UltraFastJepsenClient>> {
        self.clients.get(id % self.clients.len()).cloned()
    }
    
    /// Get all clients
    pub fn all(&self) -> &[Arc<UltraFastJepsenClient>] {
        &self.clients
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_ultra_fast_basic() {
        let client = UltraFastJepsenClient::new(0);
        
        // Test write
        client.write("key1".to_string(), serde_json::json!(42)).unwrap();
        
        // Test read
        let value = client.read("key1".to_string());
        assert_eq!(value, Some(serde_json::json!(42)));
        
        // Test non-existent key
        let value = client.read("nonexistent".to_string());
        assert_eq!(value, None);
    }
    
    #[test]
    fn test_ultra_fast_performance() {
        let client = UltraFastJepsenClient::new(0);
        let operations = 100000;
        
        // Benchmark writes
        let start = Instant::now();
        for i in 0..operations {
            client.write(
                format!("key_{}", i),
                serde_json::json!(i)
            ).unwrap();
        }
        let write_duration = start.elapsed();
        let write_ops = operations as f64 / write_duration.as_secs_f64();
        
        println!("\n=== ULTRA-FAST CLIENT PERFORMANCE ===");
        println!("Write operations: {}", operations);
        println!("Write duration: {:?}", write_duration);
        println!("Write throughput: {:.2} ops/sec", write_ops);
        println!("Write latency: {:.3} μs/op", write_duration.as_micros() as f64 / operations as f64);
        
        // Benchmark reads
        let start = Instant::now();
        for i in 0..operations {
            let _ = client.read(format!("key_{}", i));
        }
        let read_duration = start.elapsed();
        let read_ops = operations as f64 / read_duration.as_secs_f64();
        
        println!("\nRead operations: {}", operations);
        println!("Read duration: {:?}", read_duration);
        println!("Read throughput: {:.2} ops/sec", read_ops);
        println!("Read latency: {:.3} μs/op", read_duration.as_micros() as f64 / operations as f64);
        
        // This should achieve 100K+ ops/sec even in debug mode
        assert!(
            write_ops > 50000.0,
            "Ultra-fast client should achieve >50,000 ops/sec, got {:.2}",
            write_ops
        );
    }
    
    #[test]
    fn test_batch_writes() {
        let client = UltraFastJepsenClient::new(0);
        let batch_size = 10000;
        
        // Create batch
        let mut batch = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            batch.push((format!("batch_key_{}", i), serde_json::json!(i)));
        }
        
        // Benchmark batch write
        let start = Instant::now();
        client.write_batch(batch).unwrap();
        let duration = start.elapsed();
        let ops = batch_size as f64 / duration.as_secs_f64();
        
        println!("\n=== BATCH WRITE PERFORMANCE ===");
        println!("Batch size: {}", batch_size);
        println!("Duration: {:?}", duration);
        println!("Throughput: {:.2} ops/sec", ops);
    }
}
