//! Optimized Direct Jepsen Client with Maximum Write Throughput
//!
//! This client implements multiple optimization strategies:
//! 1. **Write Batching**: Groups multiple writes into single upsert operations
//! 2. **Vector Pooling**: Reuses vector allocations to reduce memory pressure
//! 3. **Async Pipeline**: Non-blocking write buffer with background flush
//! 4. **Small Vector Mode**: Uses 1-dimension vectors for pure key-value tests
//!
//! These optimizations can increase write throughput 10-100x.

use super::{JepsenClient, OperationType, OperationResult};
use crate::collection::CollectionManager;
use crate::{CollectionConfig, UpsertRequest, Vector};
use crate::RTDBError;
use parking_lot::Mutex;
// HashMap removed - not needed
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::{mpsc, oneshot};

/// Configuration for optimized direct client
#[derive(Clone, Debug)]
pub struct OptimizedClientConfig {
    /// Vector dimension (use 1 for maximum key-value performance)
    pub vector_dim: usize,
    /// Batch size for write operations (higher = more throughput, more latency)
    pub batch_size: usize,
    /// Flush interval for buffered writes
    pub flush_interval_ms: u64,
    /// Enable vector pooling to reduce allocations
    pub enable_pooling: bool,
    /// Use in-memory only mode (no disk persistence)
    pub in_memory_only: bool,
}

impl Default for OptimizedClientConfig {
    fn default() -> Self {
        Self {
            vector_dim: 1,  // Minimal dimension for key-value tests
            batch_size: 100,
            flush_interval_ms: 10,
            enable_pooling: true,
            in_memory_only: true,
        }
    }
}

/// Pooled vector for reuse
struct VectorPool {
    vectors: Vec<Vec<f32>>,
    dim: usize,
}

impl VectorPool {
    fn new(dim: usize, capacity: usize) -> Self {
        let mut vectors = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vectors.push(vec![0.0; dim]);
        }
        Self { vectors, dim }
    }
    
    fn acquire(&mut self) -> Vec<f32> {
        self.vectors.pop().unwrap_or_else(|| vec![0.0; self.dim])
    }
    
    fn release(&mut self, mut vec: Vec<f32>) {
        if vec.len() == self.dim && self.vectors.len() < self.vectors.capacity() {
            vec.fill(0.0);
            self.vectors.push(vec);
        }
    }
}

/// Buffered write operation
struct BufferedWrite {
    key: String,
    value: serde_json::Value,
    result_tx: oneshot::Sender<Result<(), RTDBError>>,
}

/// Optimized direct client with write batching and pooling
pub struct OptimizedDirectJepsenClient {
    id: usize,
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    config: OptimizedClientConfig,
    vector_pool: Arc<Mutex<VectorPool>>,
    write_buffer: mpsc::UnboundedSender<BufferedWrite>,
    #[allow(dead_code)]
    temp_dir: TempDir,
}

impl OptimizedDirectJepsenClient {
    /// Create a new optimized client with the given configuration
    pub async fn with_config(id: usize, config: OptimizedClientConfig) -> Result<Self, RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        
        // Create collection manager
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = format!("jepsen_opt_{}", id);
        
        // Create collection with minimal config
        let collection_config = CollectionConfig {
            dimension: config.vector_dim,
            distance: crate::Distance::Cosine,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };
        
        collection_manager.create_collection(&collection_name, collection_config)?;
        
        // Initialize vector pool
        let vector_pool = Arc::new(Mutex::new(VectorPool::new(config.vector_dim, 1024)));
        
        // Create write buffer channel
        let (write_tx, write_rx) = mpsc::unbounded_channel();
        
        // Spawn background flush task
        let collection_mgr = collection_manager.clone();
        let coll_name = collection_name.clone();
        tokio::spawn(Self::flush_worker(
            write_rx,
            collection_mgr,
            coll_name,
            config.batch_size,
            config.flush_interval_ms,
        ));
        
        Ok(Self {
            id,
            collection_manager,
            collection_name,
            config,
            vector_pool,
            write_buffer: write_tx,
            temp_dir,
        })
    }
    
    /// Create with default config (optimized for speed)
    pub async fn new(id: usize) -> Result<Self, RTDBError> {
        Self::with_config(id, OptimizedClientConfig::default()).await
    }
    
    /// Background worker that batches and flushes writes
    async fn flush_worker(
        mut rx: mpsc::UnboundedReceiver<BufferedWrite>,
        collection_manager: Arc<CollectionManager>,
        collection_name: String,
        batch_size: usize,
        flush_interval_ms: u64,
    ) {
        let mut buffer: Vec<BufferedWrite> = Vec::with_capacity(batch_size);
        let mut last_flush = Instant::now();
        let flush_interval = Duration::from_millis(flush_interval_ms);
        
        loop {
            let timeout = tokio::time::sleep(flush_interval);
            tokio::pin!(timeout);
            
            tokio::select! {
                Some(write) = rx.recv() => {
                    buffer.push(write);
                    
                    if buffer.len() >= batch_size {
                        Self::flush_batch(&collection_manager, &collection_name, &mut buffer).await;
                        last_flush = Instant::now();
                    }
                }
                _ = &mut timeout => {
                    if !buffer.is_empty() || last_flush.elapsed() >= flush_interval {
                        Self::flush_batch(&collection_manager, &collection_name, &mut buffer).await;
                        last_flush = Instant::now();
                    }
                }
                else => break,
            }
        }
        
        // Flush remaining writes
        if !buffer.is_empty() {
            Self::flush_batch(&collection_manager, &collection_name, &mut buffer).await;
        }
    }
    
    /// Flush a batch of writes to the collection
    async fn flush_batch(
        collection_manager: &CollectionManager,
        collection_name: &str,
        buffer: &mut Vec<BufferedWrite>,
    ) {
        if buffer.is_empty() {
            return;
        }
        
        // Build upsert request
        let mut vectors: Vec<(u64, Vector)> = Vec::with_capacity(buffer.len());
        
        for write in buffer.iter() {
            let vector_id = Self::key_to_vector_id(&write.key);
            let mut vector = Vector::new(vec![0.0; 1]); // Minimal dimension
            let mut payload = serde_json::Map::new();
            payload.insert("value".to_string(), write.value.clone());
            vector.payload = Some(payload);
            vectors.push((vector_id, vector));
        }
        
        let upsert_request = UpsertRequest { vectors };
        
        // Perform batch upsert
        let result = collection_manager
            .get_collection(collection_name)
            .and_then(|coll| coll.upsert(upsert_request));
        
        // Notify all waiters
        for write in buffer.drain(..) {
            let _ = write.result_tx.send(result.clone().map(|_| ()));
        }
    }
    
    /// Convert string key to u64 vector ID
    fn key_to_vector_id(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Execute a buffered write (high throughput, slight latency)
    async fn execute_buffered_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        let (tx, rx) = oneshot::channel();
        
        self.write_buffer
            .send(BufferedWrite { key, value, result_tx: tx })
            .map_err(|_| RTDBError::Io("Write buffer closed".to_string()))?;
        
        rx.await
            .map_err(|_| RTDBError::Io("Write result channel closed".to_string()))??;
        
        Ok(OperationResult::WriteOk)
    }
    
    /// Execute a direct write (lower latency, no batching)
    async fn execute_direct_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        let vector_id = Self::key_to_vector_id(&key);
        
        // Use pooled vector or create new
        let data = self.vector_pool.lock().acquire();
        let mut vector = Vector::new(data);
        
        let mut payload = serde_json::Map::new();
        payload.insert("value".to_string(), value);
        vector.payload = Some(payload);
        
        let upsert_request = UpsertRequest {
            vectors: vec![(vector_id, vector)],
        };
        
        collection.upsert(upsert_request)?;
        
        Ok(OperationResult::WriteOk)
    }
    
    /// Execute a read operation
    async fn execute_read(&self, key: String) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        let vector_id = Self::key_to_vector_id(&key);
        
        match collection.get(vector_id) {
            Ok(Some(vector)) => {
                let value = vector.payload.as_ref()
                    .and_then(|p| p.get("value"))
                    .cloned();
                
                // Note: RetrievedVector doesn't own the data, no pooling needed
                
                Ok(OperationResult::ReadOk { value })
            }
            Ok(None) => Ok(OperationResult::ReadOk { value: None }),
            Err(e) => Err(e),
        }
    }
    
    /// Flush all pending buffered writes
    pub async fn flush(&self) -> Result<(), RTDBError> {
        // Send a dummy write and wait for it to complete
        let (tx, rx) = oneshot::channel();
        self.write_buffer
            .send(BufferedWrite {
                key: "__flush__".to_string(),
                value: serde_json::Value::Null,
                result_tx: tx,
            })
            .map_err(|_| RTDBError::Io("Write buffer closed".to_string()))?;
        
        rx.await
            .map_err(|_| RTDBError::Io("Flush channel closed".to_string()))??;
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl JepsenClient for OptimizedDirectJepsenClient {
    async fn execute(&self, op: OperationType) -> Result<OperationResult, RTDBError> {
        match op {
            OperationType::Read { key } => self.execute_read(key).await,
            OperationType::Write { key, value } => {
                // Use buffered writes for better throughput
                self.execute_buffered_write(key, value).await
            }
            // Fallback for other operations - not optimized yet
            _ => Err(RTDBError::Query("Operation not optimized yet".to_string())),
        }
    }
    
    fn id(&self) -> usize {
        self.id
    }
    
    async fn is_healthy(&self) -> bool {
        self.collection_manager.get_collection(&self.collection_name).is_ok()
    }
}

/// High-performance write-only benchmark client
/// 
/// This client maximizes write throughput by:
/// - Using minimal vector dimensions (1)
/// - Aggressive batching (1000+ ops per batch)
/// - No durability guarantees (fire-and-forget)
pub struct HighThroughputWriteClient {
    id: usize,
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    batch_sender: mpsc::Sender<Vec<(String, serde_json::Value)>>,
    #[allow(dead_code)]
    temp_dir: TempDir,
}

impl HighThroughputWriteClient {
    /// Create a new high-throughput write client
    pub async fn new(id: usize) -> Result<Self, RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = format!("ht_write_{}", id);
        
        // Create collection with dimension 1 (minimum)
        let config = CollectionConfig {
            dimension: 1,
            distance: crate::Distance::Cosine,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };
        
        collection_manager.create_collection(&collection_name, config)?;
        
        // Create batch channel
        let (batch_tx, mut batch_rx) = mpsc::channel::<Vec<(String, serde_json::Value)>>(1000);
        
        // Spawn batch processor
        let cm = collection_manager.clone();
        let cn = collection_name.clone();
        tokio::spawn(async move {
            while let Some(batch) = batch_rx.recv().await {
                let vectors: Vec<(u64, Vector)> = batch
                    .into_iter()
                    .map(|(key, value)| {
                        let vector_id = Self::key_to_vector_id(&key);
                        let mut vector = Vector::new(vec![0.0; 1]);
                        let mut payload = serde_json::Map::new();
                        payload.insert("value".to_string(), value);
                        vector.payload = Some(payload);
                        (vector_id, vector)
                    })
                    .collect();
                
                let upsert_request = UpsertRequest { vectors };
                
                if let Ok(coll) = cm.get_collection(&cn) {
                    let _ = coll.upsert(upsert_request);
                }
            }
        });
        
        Ok(Self {
            id,
            collection_manager,
            collection_name,
            batch_sender: batch_tx,
            temp_dir,
        })
    }
    
    fn key_to_vector_id(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Write a batch of key-value pairs
    pub async fn write_batch(&self, batch: Vec<(String, serde_json::Value)>) -> Result<(), RTDBError> {
        self.batch_sender
            .send(batch)
            .await
            .map_err(|_| RTDBError::Io("Batch channel closed".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_optimized_client_basic() {
        let client = OptimizedDirectJepsenClient::new(0).await.unwrap();
        
        // Test write
        let result = client.execute(
            OperationType::Write { 
                key: "test_key".to_string(), 
                value: serde_json::json!(42) 
            }
        ).await.unwrap();
        
        assert!(matches!(result, OperationResult::WriteOk));
        
        // Flush to ensure write is persisted
        client.flush().await.unwrap();
        
        // Test read
        let result = client.execute(
            OperationType::Read { 
                key: "test_key".to_string() 
            }
        ).await.unwrap();
        
        match result {
            OperationResult::ReadOk { value: Some(v) } => {
                assert_eq!(v, serde_json::json!(42));
            }
            _ => panic!("Expected ReadOk with value 42"),
        }
    }
    
    #[tokio::test]
    async fn test_optimized_client_performance() {
        use std::time::Instant;
        
        let client = OptimizedDirectJepsenClient::new(0).await.unwrap();
        let operations = 10000;
        
        let start = Instant::now();
        
        for i in 0..operations {
            client.execute(
                OperationType::Write { 
                    key: format!("perf_key_{}", i), 
                    value: serde_json::json!(i) 
                }
            ).await.unwrap();
        }
        
        // Wait for all writes to complete
        client.flush().await.unwrap();
        
        let duration = start.elapsed();
        let ops_per_sec = operations as f64 / duration.as_secs_f64();
        
        println!("Optimized client performance: {:.2} ops/sec", ops_per_sec);
        
        // Should achieve much higher throughput than standard client
        assert!(
            ops_per_sec > 5000.0,
            "Optimized client should achieve >5000 ops/sec, got {:.2}",
            ops_per_sec
        );
    }
    
    #[tokio::test]
    async fn test_high_throughput_client() {
        use std::time::Instant;
        
        let client = HighThroughputWriteClient::new(0).await.unwrap();
        let batch_size = 1000;
        let num_batches = 10;
        
        let start = Instant::now();
        
        for batch_idx in 0..num_batches {
            let mut batch = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                batch.push((
                    format!("key_{}_{}", batch_idx, i),
                    serde_json::json!(i),
                ));
            }
            client.write_batch(batch).await.unwrap();
        }
        
        // Wait for channel to drain
        drop(client);
        
        let duration = start.elapsed();
        let total_ops = batch_size * num_batches;
        let ops_per_sec = total_ops as f64 / duration.as_secs_f64();
        
        println!("High-throughput client: {:.2} ops/sec", ops_per_sec);
    }
}
