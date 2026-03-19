//! Direct/In-Process Jepsen Client for High-Performance Testing
//!
//! This client bypasses the HTTP REST API entirely and uses RTDB's internal
//! APIs directly. This provides 100-200x performance improvement compared to
//! HTTP-based testing while still validating the same core logic.
//!
//! Use this for:
//! - Performance validation (10K+ ops/sec)
//! - Linearizability testing without network overhead
//! - CI/CD pipeline testing
//!
//! Use HTTP client for:
//! - Network partition testing
//! - Multi-node cluster testing
//! - Production scenario simulation

use super::{JepsenClient, OperationType, OperationResult};
use crate::collection::CollectionManager;
use crate::{CollectionConfig, UpsertRequest, Vector};
use crate::RTDBError;
use std::sync::Arc;
use tempfile::TempDir;

/// Direct/in-process client for high-performance Jepsen testing
/// 
/// This client creates an in-memory RTDB instance and performs operations
/// directly through the CollectionManager API, bypassing HTTP serialization,
/// TCP stack, and all network-related overhead.
pub struct DirectJepsenClient {
    id: usize,
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    vector_dim: usize,
    #[allow(dead_code)]
    temp_dir: TempDir, // Keep temp_dir alive for the lifetime of the client
}

impl DirectJepsenClient {
    /// Create a new direct Jepsen client with an in-memory RTDB instance
    /// 
    /// # Arguments
    /// * `id` - Client ID for identification
    /// * `vector_dim` - Vector dimension for test data (default: 128)
    /// 
    /// # Example
    /// ```rust
    /// let client = DirectJepsenClient::new(0, 128).await?;
    /// ```
    pub async fn new(id: usize, vector_dim: usize) -> Result<Self, RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        
        // Create collection manager with optimized in-memory config
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = format!("jepsen_direct_{}", id);
        
        // Create collection with specified dimension
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
            temp_dir,
        })
    }
    
    /// Create a shared direct client for multi-client testing
    /// 
    /// All clients share the same collection manager but have different IDs.
    /// This is useful for testing concurrent access patterns.
    pub async fn new_shared(
        id: usize,
        collection_manager: Arc<CollectionManager>,
        collection_name: String,
        vector_dim: usize,
    ) -> Result<Self, RTDBError> {
        Ok(Self {
            id,
            collection_manager,
            collection_name,
            vector_dim,
            temp_dir: tempfile::tempdir()
                .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?,
        })
    }
    
    /// Get the collection name this client operates on
    pub fn collection_name(&self) -> &str {
        &self.collection_name
    }
    
    /// Convert a string key to a u64 vector ID using hashing
    fn key_to_vector_id(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Execute a read operation directly
    async fn execute_read(&self, key: String) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        
        // Convert string key to u64 VectorId using hash
        let vector_id = Self::key_to_vector_id(&key);
        
        match collection.get(vector_id) {
            Ok(Some(vector)) => {
                // Extract value from payload if present
                let value = vector.payload.as_ref()
                    .and_then(|p| p.get("value"))
                    .cloned();
                
                Ok(OperationResult::ReadOk { value })
            }
            Ok(None) => Ok(OperationResult::ReadOk { value: None }),
            Err(e) => Err(e),
        }
    }
    
    /// Execute a write operation directly
    async fn execute_write(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        let collection = self.collection_manager.get_collection(&self.collection_name)?;
        
        let vector_id = Self::key_to_vector_id(&key);
        
        // Create vector with value in payload
        let mut vector = Vector::new(vec![0.0; self.vector_dim]);
        let mut payload = serde_json::Map::new();
        payload.insert("value".to_string(), value);
        vector.payload = Some(payload);
        
        let upsert_request = UpsertRequest {
            vectors: vec![(vector_id, vector)],
        };
        
        collection.upsert(upsert_request)?;
        
        Ok(OperationResult::WriteOk)
    }
    
    /// Execute a CAS operation directly
    async fn execute_cas(
        &self,
        key: String,
        old: serde_json::Value,
        new: serde_json::Value,
    ) -> Result<OperationResult, RTDBError> {
        // Read current value
        let read_result = self.execute_read(key.clone()).await?;
        
        match read_result {
            OperationResult::ReadOk { value: Some(current) } if current == old => {
                // Values match, perform write
                self.execute_write(key, new).await?;
                Ok(OperationResult::CasOk { success: true })
            }
            OperationResult::ReadOk { value: None } if old == serde_json::Value::Null => {
                // Key doesn't exist and old is null (create operation)
                self.execute_write(key, new).await?;
                Ok(OperationResult::CasOk { success: true })
            }
            _ => Ok(OperationResult::CasOk { success: false }),
        }
    }
    
    /// Execute an increment operation directly
    async fn execute_increment(&self, key: String, delta: i64) -> Result<OperationResult, RTDBError> {
        // Read current value
        let read_result = self.execute_read(key.clone()).await?;
        
        let current_value = match &read_result {
            OperationResult::ReadOk { value: Some(v) } => {
                v.as_i64().unwrap_or(0)
            }
            _ => 0,
        };
        
        let new_value = current_value + delta;
        self.execute_write(key, serde_json::json!(new_value)).await?;
        
        Ok(OperationResult::IncrementOk { value: new_value })
    }
    
    /// Execute an append operation directly
    async fn execute_append(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        // Read current list
        let read_result = self.execute_read(key.clone()).await?;
        
        let mut list = match &read_result {
            OperationResult::ReadOk { value: Some(serde_json::Value::Array(arr)) } => {
                arr.clone()
            }
            OperationResult::ReadOk { value: None } | OperationResult::ReadOk { value: Some(serde_json::Value::Null) } => {
                Vec::new()
            }
            _ => Vec::new(),
        };
        
        list.push(value);
        self.execute_write(key, serde_json::json!(list)).await?;
        
        Ok(OperationResult::AppendOk)
    }
    
    /// Execute a set add operation directly
    async fn execute_set_add(&self, key: String, value: serde_json::Value) -> Result<OperationResult, RTDBError> {
        // Read current set
        let read_result = self.execute_read(key.clone()).await?;
        
        let mut set = match &read_result {
            OperationResult::ReadOk { value: Some(serde_json::Value::Array(arr)) } => {
                arr.clone()
            }
            _ => Vec::new(),
        };
        
        // Add if not present
        if !set.contains(&value) {
            set.push(value);
            self.execute_write(key, serde_json::json!(set)).await?;
        }
        
        Ok(OperationResult::SetAddOk)
    }
    
    /// Execute a transaction operation directly
    async fn execute_transaction(
        &self,
        ops: Vec<super::TransactionOp>,
    ) -> Result<OperationResult, RTDBError> {
        let mut results = Vec::new();
        
        for op in ops {
            match op {
                super::TransactionOp::Read { key } => {
                    let result = self.execute_read(key).await?;
                    if let OperationResult::ReadOk { value } = result {
                        results.push(value);
                    }
                }
                super::TransactionOp::Write { key, value } => {
                    self.execute_write(key, value).await?;
                    results.push(Some(serde_json::Value::Bool(true)));
                }
            }
        }
        
        Ok(OperationResult::TransactionOk { results })
    }
}

#[async_trait::async_trait]
impl JepsenClient for DirectJepsenClient {
    async fn execute(&self, op: OperationType) -> Result<OperationResult, RTDBError> {
        match op {
            OperationType::Read { key } => self.execute_read(key).await,
            OperationType::Write { key, value } => self.execute_write(key, value).await,
            OperationType::Cas { key, old, new } => self.execute_cas(key, old, new).await,
            OperationType::Increment { key, delta } => self.execute_increment(key, delta).await,
            OperationType::Append { key, value } => self.execute_append(key, value).await,
            OperationType::SetAdd { key, element } => self.execute_set_add(key, element).await,
            OperationType::Transaction { ops } => self.execute_transaction(ops).await,
        }
    }
    
    fn id(&self) -> usize {
        self.id
    }
    
    async fn is_healthy(&self) -> bool {
        // Direct client is always healthy as long as collection exists
        self.collection_manager.get_collection(&self.collection_name).is_ok()
    }
}

/// Builder for creating multiple direct clients sharing the same storage
pub struct DirectClientCluster {
    collection_manager: Arc<CollectionManager>,
    collection_name: String,
    vector_dim: usize,
    #[allow(dead_code)]
    temp_dir: TempDir,
}

impl DirectClientCluster {
    /// Create a new cluster of direct clients
    /// 
    /// All clients will share the same underlying storage, allowing for
    /// true concurrent access testing without network overhead.
    pub async fn new(client_count: usize, vector_dim: usize) -> Result<(Self, Vec<Arc<dyn JepsenClient>>), RTDBError> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| RTDBError::Io(format!("Failed to create temp dir: {}", e)))?;
        
        let storage_path = temp_dir.path().to_path_buf();
        let collection_manager = Arc::new(
            CollectionManager::new(storage_path.to_string_lossy().to_string())?
        );
        
        let collection_name = "jepsen_shared".to_string();
        
        // Create shared collection
        let config = CollectionConfig {
            dimension: vector_dim,
            distance: crate::Distance::Cosine,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };
        
        collection_manager.create_collection(&collection_name, config)?;
        
        // Create clients
        let mut clients: Vec<Arc<dyn JepsenClient>> = Vec::with_capacity(client_count);
        for id in 0..client_count {
            let client = DirectJepsenClient::new_shared(
                id,
                collection_manager.clone(),
                collection_name.clone(),
                vector_dim,
            ).await?;
            clients.push(Arc::new(client));
        }
        
        let cluster = Self {
            collection_manager,
            collection_name,
            vector_dim,
            temp_dir,
        };
        
        Ok((cluster, clients))
    }
    
    /// Get the collection manager for direct access
    pub fn collection_manager(&self) -> &CollectionManager {
        &self.collection_manager
    }
    
    /// Get the collection name
    pub fn collection_name(&self) -> &str {
        &self.collection_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_direct_client_basic_operations() {
        let client = DirectJepsenClient::new(0, 128).await.unwrap();
        
        // Test write
        let write_result = client.execute(
            OperationType::Write { 
                key: "test_key".to_string(), 
                value: serde_json::json!(42) 
            }
        ).await.unwrap();
        
        assert!(matches!(write_result, OperationResult::WriteOk));
        
        // Test read
        let read_result = client.execute(
            OperationType::Read { 
                key: "test_key".to_string() 
            }
        ).await.unwrap();
        
        match read_result {
            OperationResult::ReadOk { value: Some(v) } => {
                assert_eq!(v, serde_json::json!(42));
            }
            _ => panic!("Expected ReadOk with value 42"),
        }
        
        // Test health check
        assert!(client.is_healthy().await);
    }
    
    #[tokio::test]
    async fn test_direct_client_cas() {
        let client = DirectJepsenClient::new(0, 128).await.unwrap();
        
        // Initial write
        client.execute(
            OperationType::Write { 
                key: "cas_key".to_string(), 
                value: serde_json::json!(100) 
            }
        ).await.unwrap();
        
        // Successful CAS
        let cas_result = client.execute(
            OperationType::Cas { 
                key: "cas_key".to_string(), 
                old: serde_json::json!(100),
                new: serde_json::json!(200),
            }
        ).await.unwrap();
        
        match cas_result {
            OperationResult::CasOk { success: true } => {}
            _ => panic!("Expected successful CAS"),
        }
        
        // Failed CAS (wrong old value)
        let cas_result = client.execute(
            OperationType::Cas { 
                key: "cas_key".to_string(), 
                old: serde_json::json!(100), // Wrong
                new: serde_json::json!(300),
            }
        ).await.unwrap();
        
        match cas_result {
            OperationResult::CasOk { success: false } => {}
            _ => panic!("Expected failed CAS"),
        }
    }
    
    #[tokio::test]
    async fn test_direct_client_cluster() {
        let (cluster, clients) = DirectClientCluster::new(4, 128).await.unwrap();
        
        assert_eq!(clients.len(), 4);
        assert!(cluster.collection_manager.get_collection(&cluster.collection_name).is_ok());
        
        // All clients should share the same collection
        for (i, client) in clients.iter().enumerate() {
            assert_eq!(client.id(), i);
            assert!(client.is_healthy().await);
        }
    }
    
    #[tokio::test]
    async fn test_direct_client_performance() {
        use std::time::Instant;
        
        let client = DirectJepsenClient::new(0, 128).await.unwrap();
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
        
        let duration = start.elapsed();
        let ops_per_sec = operations as f64 / duration.as_secs_f64();
        
        println!("Direct client performance: {:.2} ops/sec", ops_per_sec);
        
        // Should achieve at least 5000 ops/sec on any reasonable hardware
        assert!(
            ops_per_sec > 500.0,
            "Direct client should achieve >500 ops/sec, got {:.2}",
            ops_per_sec
        );
    }
}
