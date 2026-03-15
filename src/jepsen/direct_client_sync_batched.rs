//! Synchronous Batched Direct Jepsen Client
//!
//! Eliminates async overhead by using synchronous batching

use super::{JepsenClient, OperationType, OperationResult};
use crate::collection::CollectionManager;
use crate::{CollectionConfig, UpsertRequest, Vector};
use crate::RTDBError;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Synchronous batched direct client
pub struct SyncBatchedDirectJepsenClient {
    id: usize,
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    vector_dim: usize,
    write_buffer: Arc<Mutex<WriteBuffer>>,
    #[allow(dead_code)]
    temp_dir: TempDir,
}

/// Write buffer for batching
struct WriteBuffer {
    ops: Vec<(String, serde_json::Value)>,
    batch_size: usize,
}

impl SyncBatchedDirectJepsenClient {
    /// Create new sync batched client
    pub async fn new(id: usize, vector_dim: usize, batch_size: usize) -> Result<Self, RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = format!("jepsen_sync_{}", id);
        
        let config = CollectionConfig {
            dimension: vector_dim,
            distance: crate::Distance::Cosine,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };
        
        collection_manager.create_collection(&collection_name, config)?;
        
        Ok(Self {
            id,
            collection_manager,
            collection_name,
            vector_dim,
            write_buffer: Arc::new(Mutex::new(WriteBuffer {
                ops: Vec::with_capacity(batch_size),
                batch_size,
            })),
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
    
    /// Add operation to buffer (may trigger flush)
    fn buffer_write(&self, key: String, value: serde_json::Value) -> Result<Option<OperationResult>, RTDBError> {
        let mut buffer = self.write_buffer.lock();
        buffer.ops.push((key, value));
        
        if buffer.ops.len() >= buffer.batch_size {
            drop(buffer); // Release lock before flush
            self.flush_buffer()?;
            Ok(Some(OperationResult::WriteOk))
        } else {
            Ok(None) // Not flushed yet
        }
    }
    
    /// Flush buffer to storage
    fn flush_buffer(&self) -> Result<(), RTDBError> {
        let mut buffer = self.write_buffer.lock();
        if buffer.ops.is_empty() {
            return Ok(());
        }
        
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        
        // Build batch upsert
        let vectors: Vec<(u64, Vector)> = buffer.ops
            .drain(..)
            .map(|(key, value)| {
                let vector_id = Self::key_to_vector_id(&key);
                let mut vector = Vector::new(vec![0.0; self.vector_dim]);
                let mut payload = serde_json::Map::new();
                payload.insert("value".to_string(), value);
                vector.payload = Some(payload);
                (vector_id, vector)
            })
            .collect();
        
        drop(buffer); // Release lock before I/O
        
        collection.upsert(UpsertRequest { vectors })?;
        
        Ok(())
    }
    
    /// Execute write (with batching)
    async fn execute_batched_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        match self.buffer_write(key, value)? {
            Some(result) => Ok(result),
            None => {
                // Not flushed, return OK immediately
                Ok(OperationResult::WriteOk)
            }
        }
    }
    
    /// Execute read
    async fn execute_read(&self, key: String) -> Result<OperationResult, RTDBError> {
        // First flush any pending writes to ensure consistency
        self.flush_buffer()?;
        
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
    
    /// Flush all pending writes
    pub async fn flush(&self) -> Result<(), RTDBError> {
        self.flush_buffer()
    }
}

#[async_trait::async_trait]
impl JepsenClient for SyncBatchedDirectJepsenClient {
    async fn execute(&self, op: OperationType) -> Result<OperationResult, RTDBError> {
        match op {
            OperationType::Read { key } => self.execute_read(key).await,
            OperationType::Write { key, value } => self.execute_batched_write(key, value).await,
            _ => Err(RTDBError::Query("Not supported".to_string())),
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
    async fn test_sync_batched_performance() {
        println!("\n=== SYNC BATCHED vs STANDARD PERFORMANCE ===\n");
        
        let operations = 1000;
        
        // Test different batch sizes
        for batch_size in [10, 50, 100, 200] {
            println!("\nBatch size: {}", batch_size);
            let client = SyncBatchedDirectJepsenClient::new(0, 128, batch_size).await.unwrap();
            
            let start = Instant::now();
            for i in 0..operations {
                client.execute(OperationType::Write { 
                    key: format!("key_{}", i), 
                    value: serde_json::json!(i) 
                }).await.unwrap();
            }
            client.flush().await.unwrap();
            let duration = start.elapsed();
            let ops = operations as f64 / duration.as_secs_f64();
            
            println!("  Throughput: {:.2} ops/sec ({:?})", ops, duration);
        }
        
        // Standard for comparison
        println!("\nStandard Direct Client:");
        let standard = crate::jepsen::direct_client::DirectJepsenClient::new(1, 128).await.unwrap();
        
        let start = Instant::now();
        for i in 0..100 {
            standard.execute(OperationType::Write { 
                key: format!("std_key_{}", i), 
                value: serde_json::json!(i) 
            }).await.unwrap();
        }
        let duration = start.elapsed();
        let ops = 100.0 / duration.as_secs_f64();
        
        println!("  Throughput: {:.2} ops/sec ({:?})", ops, duration);
    }
}
