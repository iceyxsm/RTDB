//! Batched Direct Jepsen Client - Optimized for Throughput with HNSW
//!
//! This client keeps HNSW indexing but optimizes by:
//! 1. Batch upserts (reduces lock contention)
//! 2. Async parallel execution
//! 3. Background flush
//! 4. Reduced atomic operations

use super::{JepsenClient, OperationType, OperationResult};
use crate::collection::CollectionManager;
use crate::{CollectionConfig, UpsertRequest, Vector};
use crate::RTDBError;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{mpsc, oneshot};

/// Optimized batched direct client
pub struct BatchedDirectJepsenClient {
    id: usize,
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    vector_dim: usize,
    write_buffer: mpsc::UnboundedSender<BufferedOp>,
    #[allow(dead_code)]
    temp_dir: TempDir,
}

/// Buffered operation with response channel
type BufferedOp = (String, serde_json::Value, oneshot::Sender<Result<(), RTDBError>>);

impl BatchedDirectJepsenClient {
    /// Create new batched client
    pub async fn new(id: usize, vector_dim: usize) -> Result<Self, RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = format!("jepsen_batched_{}", id);
        
        // Create collection with HNSW enabled (default config)
        let config = CollectionConfig {
            dimension: vector_dim,
            distance: crate::Distance::Cosine,
            hnsw_config: None, // Uses default HNSW
            quantization_config: None,
            optimizer_config: None,
        };
        
        collection_manager.create_collection(&collection_name, config)?;
        
        // Create write buffer channel
        let (write_tx, write_rx) = mpsc::unbounded_channel();
        
        // Spawn background batch processor
        let cm = collection_manager.clone();
        let cn = collection_name.clone();
        let dim = vector_dim;
        tokio::spawn(Self::batch_processor(write_rx, cm, cn, dim));
        
        Ok(Self {
            id,
            collection_manager,
            collection_name,
            vector_dim,
            write_buffer: write_tx,
            temp_dir,
        })
    }
    
    /// Background batch processor
    async fn batch_processor(
        mut rx: mpsc::UnboundedReceiver<BufferedOp>,
        collection_manager: Arc<CollectionManager>,
        collection_name: String,
        vector_dim: usize,
    ) {
        let mut batch: Vec<BufferedOp> = Vec::with_capacity(100);
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        
        loop {
            tokio::select! {
                Some(op) = rx.recv() => {
                    batch.push(op);
                    
                    // Flush when batch is full
                    if batch.len() >= 100 {
                        Self::flush_batch(&collection_manager, &collection_name, vector_dim, &mut batch);
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        Self::flush_batch(&collection_manager, &collection_name, vector_dim, &mut batch);
                    }
                }
                else => break,
            }
        }
        
        // Final flush
        if !batch.is_empty() {
            Self::flush_batch(&collection_manager, &collection_name, vector_dim, &mut batch);
        }
    }
    
    /// Flush a batch of operations
    fn flush_batch(
        collection_manager: &CollectionManager,
        collection_name: &str,
        vector_dim: usize,
        batch: &mut Vec<BufferedOp>,
    ) {
        if batch.is_empty() {
            return;
        }
        
        // Build vectors for batch upsert
        let vectors: Vec<(u64, Vector)> = batch
            .iter()
            .map(|(key, value, _)| {
                let vector_id = Self::key_to_vector_id(key);
                let mut vector = Vector::new(vec![0.0; vector_dim]);
                let mut payload = serde_json::Map::new();
                payload.insert("value".to_string(), value.clone());
                vector.payload = Some(payload);
                (vector_id, vector)
            })
            .collect();
        
        let upsert_request = UpsertRequest { vectors };
        
        // Single batch upsert (one lock acquisition, one HNSW update batch)
        let result = collection_manager
            .get_collection(collection_name)
            .and_then(|coll| coll.upsert(upsert_request));
        
        // Notify all waiters
        for (_, _, tx) in batch.drain(..) {
            let _ = tx.send(result.clone().map(|_| ()));
        }
    }
    
    /// Key to vector ID
    fn key_to_vector_id(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Execute batched write (high throughput)
    async fn execute_batched_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        let (tx, rx) = oneshot::channel();
        
        self.write_buffer
            .send((key, value, tx))
            .map_err(|_| RTDBError::Io("Write buffer closed".to_string()))?;
        
        rx.await
            .map_err(|_| RTDBError::Io("Channel closed".to_string()))??;
        
        Ok(OperationResult::WriteOk)
    }
    
    /// Execute direct write (lower latency)
    async fn execute_direct_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        let vector_id = Self::key_to_vector_id(&key);
        
        let mut vector = Vector::new(vec![0.0; self.vector_dim]);
        let mut payload = serde_json::Map::new();
        payload.insert("value".to_string(), value);
        vector.payload = Some(payload);
        
        collection.upsert(UpsertRequest {
            vectors: vec![(vector_id, vector)],
        })?;
        
        Ok(OperationResult::WriteOk)
    }
    
    /// Execute read
    async fn execute_read(&self, key: String) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        let vector_id = Self::key_to_vector_id(&key);
        
        match collection.get(vector_id) {
            Ok(Some(vector)) => {
                let value = vector.payload.as_ref()
                    .and_then(|p| p.get("value"))
                    .cloned();
                Ok(OperationResult::ReadOk { value })
            }
            Ok(None) => Ok(OperationResult::ReadOk { value: None }),
            Err(e) => Err(e),
        }
    }
    
    /// Flush pending writes
    pub async fn flush(&self) -> Result<(), RTDBError> {
        let (tx, rx) = oneshot::channel();
        self.write_buffer
            .send(("__flush__".to_string(), serde_json::Value::Null, tx))
            .map_err(|_| RTDBError::Io("Buffer closed".to_string()))?;
        rx.await.map_err(|_| RTDBError::Io("Channel closed".to_string()))??;
        Ok(())
    }
}

#[async_trait::async_trait]
impl JepsenClient for BatchedDirectJepsenClient {
    async fn execute(&self, op: OperationType) -> Result<OperationResult, RTDBError> {
        match op {
            OperationType::Read { key } => self.execute_read(key).await,
            OperationType::Write { key, value } => {
                self.execute_batched_write(key, value).await
            }
            _ => Err(RTDBError::Query("Operation not supported".to_string())),
        }
    }
    
    fn id(&self) -> usize {
        self.id
    }
    
    async fn is_healthy(&self) -> bool {
        self.collection_manager.get_collection(&self.collection_name).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_batched_performance() {
        println!("\n=== BATCHED DIRECT CLIENT PERFORMANCE ===\n");
        
        let operations = 1000;
        
        // Batched client
        println!("Testing Batched Client (with HNSW)...");
        let batched = BatchedDirectJepsenClient::new(0, 128).await.unwrap();
        
        let start = Instant::now();
        for i in 0..operations {
            batched.execute(OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }).await.unwrap();
        }
        batched.flush().await.unwrap();
        let batched_duration = start.elapsed();
        let batched_ops = operations as f64 / batched_duration.as_secs_f64();
        
        println!("  Operations: {}", operations);
        println!("  Duration: {:?}", batched_duration);
        println!("  Throughput: {:.2} ops/sec", batched_ops);
        
        // Standard client for comparison
        println!("\nTesting Standard Client (with HNSW)...");
        let standard = crate::jepsen::direct_client::DirectJepsenClient::new(1, 128).await.unwrap();
        
        let start = Instant::now();
        for i in 0..100 { // Fewer ops for standard
            standard.execute(OperationType::Write { 
                key: format!("std_key_{}", i), 
                value: serde_json::json!(i) 
            }).await.unwrap();
        }
        let std_duration = start.elapsed();
        let std_ops = 100.0 / std_duration.as_secs_f64();
        
        println!("  Operations: 100");
        println!("  Duration: {:?}", std_duration);
        println!("  Throughput: {:.2} ops/sec", std_ops);
        
        println!("\n=== RESULT ===");
        println!("Batched:  {:.2} ops/sec", batched_ops);
        println!("Standard: {:.2} ops/sec", std_ops);
        println!("Speedup:  {:.1}x", batched_ops / std_ops);
    }
}
