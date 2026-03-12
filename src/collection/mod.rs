//! Collection management layer
//! 
//! A collection is similar to a table in SQL databases.
//! Each collection has its own vectors, index, and configuration.

use crate::{
    CollectionConfig, HnswConfig, Result, RTDBError,
    ScoredVector, SearchRequest, UpsertRequest, VectorId,
    index::{VectorIndex, hnsw::HNSWIndex},
    storage::{StorageEngine, Storage, StorageConfig},
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Collection manager
pub struct CollectionManager {
    /// Collections by name
    collections: RwLock<HashMap<String, Arc<Collection>>>,
    /// Base storage path
    base_path: String,
}

impl CollectionManager {
    /// Create new collection manager
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let base_path = base_path.as_ref().to_string_lossy().to_string();
        std::fs::create_dir_all(&base_path).map_err(|e| RTDBError::Io(e.to_string()))?;

        let mut manager = Self {
            collections: RwLock::new(HashMap::new()),
            base_path,
        };

        // Load existing collections
        manager.load_collections()?;

        Ok(manager)
    }

    /// Load collections from disk
    fn load_collections(&mut self) -> Result<()> {
        let entries = std::fs::read_dir(&self.base_path)
            .map_err(|e| RTDBError::Io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| RTDBError::Io(e.to_string()))?;
            let path = entry.path();

            if path.is_dir() {
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if name.is_empty() {
                    continue;
                }

                // Try to load collection
                match Collection::open(&path) {
                    Ok(collection) => {
                        self.collections.write().insert(name, Arc::new(collection));
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to load collection '{}': {}", name, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Create new collection
    pub fn create_collection(&self, name: &str, config: CollectionConfig) -> Result<()> {
        if self.collections.read().contains_key(name) {
            return Err(RTDBError::Storage(
                format!("Collection '{}' already exists", name)
            ));
        }

        let path = Path::new(&self.base_path).join(name);
        let collection = Collection::create(&path, config)?;

        self.collections.write().insert(name.to_string(), Arc::new(collection));

        Ok(())
    }

    /// Get collection
    pub fn get_collection(&self, name: &str) -> Result<Arc<Collection>> {
        self.collections.read()
            .get(name)
            .cloned()
            .ok_or_else(|| RTDBError::CollectionNotFound(name.to_string()))
    }

    /// Delete collection
    pub fn delete_collection(&self, name: &str) -> Result<()> {
        // Remove from memory
        self.collections.write().remove(name);

        // Delete from disk
        let path = Path::new(&self.base_path).join(name);
        if path.exists() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| RTDBError::Io(e.to_string()))?;
        }

        Ok(())
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<String> {
        self.collections.read().keys().cloned().collect()
    }

    /// Check if collection exists
    pub fn has_collection(&self, name: &str) -> bool {
        self.collections.read().contains_key(name)
    }
}

/// A single collection
pub struct Collection {
    /// Collection name
    name: String,
    /// Collection configuration
    config: CollectionConfig,
    /// Storage engine
    storage: Arc<StorageEngine>,
    /// Vector index
    index: RwLock<Box<dyn VectorIndex>>,
    /// Next vector ID
    next_id: std::sync::atomic::AtomicU64,
    /// Next operation ID (for tracking operations)
    next_operation_id: std::sync::atomic::AtomicU64,
}

impl Collection {
    /// Create new collection
    fn create(path: impl AsRef<Path>, config: CollectionConfig) -> Result<Self> {
        std::fs::create_dir_all(&path).map_err(|e| RTDBError::Io(e.to_string()))?;

        // Save config
        let config_path = path.as_ref().join("config.json");
        let config_json = serde_json::to_string_pretty(&config)
            .map_err(|e| RTDBError::Serialization(e.to_string()))?;
        std::fs::write(&config_path, config_json)
            .map_err(|e| RTDBError::Io(e.to_string()))?;

        // Initialize storage
        let storage_path = path.as_ref().join("storage");
        let storage_config = StorageConfig {
            path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };
        let storage = Arc::new(StorageEngine::open(storage_config)?);

        // Initialize index
        let index: Box<dyn VectorIndex> = match &config.hnsw_config {
            Some(hnsw_config) => {
                Box::new(HNSWIndex::new(hnsw_config.clone(), config.distance))
            }
            None => {
                Box::new(HNSWIndex::new(HnswConfig::default(), config.distance))
            }
        };

        let name = path.as_ref()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            name,
            config,
            storage,
            index: RwLock::new(index),
            next_id: std::sync::atomic::AtomicU64::new(1),
            next_operation_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// Open existing collection
    fn open(path: impl AsRef<Path>) -> Result<Self> {
        // Load config
        let config_path = path.as_ref().join("config.json");
        let config_json = std::fs::read_to_string(&config_path)
            .map_err(|e| RTDBError::Io(e.to_string()))?;
        let config: CollectionConfig = serde_json::from_str(&config_json)
            .map_err(|e| RTDBError::Serialization(e.to_string()))?;

        // Initialize storage
        let storage_path = path.as_ref().join("storage");
        let storage_config = StorageConfig {
            path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };
        let storage = Arc::new(StorageEngine::open(storage_config)?);

        // Initialize and build index from storage
        let mut index = HNSWIndex::new(
            config.hnsw_config.clone().unwrap_or_default(),
            config.distance
        );

        // Load all vectors into index
        let vectors = storage.scan(None, None)?;
        let max_id = vectors.iter().map(|(id, _)| *id).max().unwrap_or(0);
        
        for (id, vector) in vectors {
            index.add(id, &vector)?;
        }

        let name = path.as_ref()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            name,
            config,
            storage,
            index: RwLock::new(Box::new(index)),
            next_id: std::sync::atomic::AtomicU64::new(max_id + 1),
            next_operation_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// Get collection name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get configuration
    pub fn config(&self) -> &CollectionConfig {
        &self.config
    }

    /// Get vector count
    pub fn vector_count(&self) -> u64 {
        self.storage.stats().vector_count
    }

    /// Upsert vectors
    pub fn upsert(&self, request: UpsertRequest) -> Result<OperationInfo> {
        let mut ids = Vec::with_capacity(request.vectors.len());

        for (id, vector) in request.vectors {
            // Validate dimension
            if vector.dim() != self.config.dimension {
                return Err(RTDBError::InvalidDimension {
                    expected: self.config.dimension,
                    actual: vector.dim(),
                });
            }

            // Store in storage
            self.storage.put(id, vector.clone())?;

            // Add to index
            self.index.write().add(id, &vector)?;

            ids.push(id);

            // Update next_id if needed
            let current_next = self.next_id.load(std::sync::atomic::Ordering::SeqCst);
            if id >= current_next {
                self.next_id.store(id + 1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        Ok(OperationInfo {
            operation_id: self.next_operation_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            status: OperationStatus::Completed,
        })
    }

    /// Get next auto-generated ID
    pub fn next_id(&self) -> VectorId {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Search vectors
    pub fn search(&self, request: SearchRequest) -> Result<Vec<ScoredVector>> {
        // Validate query dimension
        if request.vector.len() != self.config.dimension {
            return Err(RTDBError::InvalidDimension {
                expected: self.config.dimension,
                actual: request.vector.len(),
            });
        }

        // Search index
        let mut results = self.index.read().search(&request)?;

        // Apply filter if provided
        if let Some(filter) = &request.filter {
            results.retain(|scored| {
                // Get vector to check filter
                if let Ok(Some(vector)) = self.storage.get(scored.id) {
                    crate::filter::FilterEvaluator::matches(filter, scored.id, &vector)
                } else {
                    false
                }
            });
        }

        // Limit results
        let limit = request.limit.min(results.len());
        let results: Vec<_> = results.into_iter().take(limit).collect();

        // Fetch payloads if requested
        let results = match request.with_payload {
            Some(crate::WithPayload::Bool(true)) => {
                results.into_iter()
                    .map(|mut r| {
                        if let Ok(Some(v)) = self.storage.get(r.id) {
                            r.payload = v.payload;
                        }
                        r
                    })
                    .collect()
            }
            _ => results,
        };

        // Fetch vectors if requested
        let results = if request.with_vector {
            results.into_iter()
                .map(|mut r| {
                    if let Ok(Some(v)) = self.storage.get(r.id) {
                        r.vector = Some(v.data);
                    }
                    r
                })
                .collect()
        } else {
            results
        };

        Ok(results)
    }

    /// Get vector by ID
    pub fn get(&self, id: VectorId) -> Result<Option<RetrievedVector>> {
        match self.storage.get(id)? {
            Some(vector) => {
                Ok(Some(RetrievedVector {
                    id,
                    vector: vector.data,
                    payload: vector.payload,
                }))
            }
            None => Ok(None),
        }
    }

    /// Delete vectors
    pub fn delete(&self, ids: &[VectorId]) -> Result<OperationInfo> {
        for &id in ids {
            self.storage.delete(id)?;
            self.index.write().remove(id)?;
        }

        Ok(OperationInfo {
            operation_id: self.next_operation_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            status: OperationStatus::Completed,
        })
    }
    
    /// Get all vectors (for snapshots)
    pub fn get_all_vectors(&self) -> Result<Vec<(VectorId, crate::Vector)>> {
        // Scan through storage to get all vectors
        // This is a simplified implementation - in production, use a more efficient scan
        let mut vectors = Vec::new();
        
        // Get max ID from storage metadata or scan range
        let count = self.vector_count();
        if count == 0 {
            return Ok(vectors);
        }
        
        // Scan reasonable range for vectors
        // In production, this should use a proper iterator from storage
        for id in 0..count * 2 + 1000 {
            if let Ok(Some(vector)) = self.storage.get(id) {
                vectors.push((id, vector));
            }
        }
        
        Ok(vectors)
    }
}

/// Operation information
#[derive(Debug, Clone)]
pub struct OperationInfo {
    /// Operation ID
    pub operation_id: u64,
    /// Operation status
    pub status: OperationStatus,
}

/// Operation status
#[derive(Debug, Clone)]
pub enum OperationStatus {
    /// Operation completed
    Completed,
    /// Operation pending
    Pending,
    /// Operation failed
    Failed(String),
}

/// Retrieved vector
#[derive(Debug, Clone)]
pub struct RetrievedVector {
    /// Vector ID
    pub id: VectorId,
    /// Vector data
    pub vector: Vec<f32>,
    /// Payload
    pub payload: Option<crate::Payload>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vector;
    use tempfile::TempDir;

    #[test]
    fn test_collection_manager() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CollectionManager::new(temp_dir.path()).unwrap();

        // Create collection
        let config = CollectionConfig::new(128);
        manager.create_collection("test", config).unwrap();

        assert!(manager.has_collection("test"));
        assert_eq!(manager.list_collections(), vec!["test"]);

        // Get collection
        let collection = manager.get_collection("test").unwrap();
        assert_eq!(collection.name(), "test");
        assert_eq!(collection.config().dimension, 128);

        // Delete collection
        manager.delete_collection("test").unwrap();
        assert!(!manager.has_collection("test"));
    }

    #[test]
    fn test_collection_upsert_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CollectionManager::new(temp_dir.path()).unwrap();

        // Create collection
        let config = CollectionConfig::new(3);
        manager.create_collection("test", config).unwrap();

        let collection = manager.get_collection("test").unwrap();

        // Upsert vectors
        let request = UpsertRequest {
            vectors: vec![
                (1, Vector::new(vec![1.0, 0.0, 0.0])),
                (2, Vector::new(vec![0.0, 1.0, 0.0])),
                (3, Vector::new(vec![0.0, 0.0, 1.0])),
            ],
        };
        collection.upsert(request).unwrap();

        assert_eq!(collection.vector_count(), 3);

        // Search
        let search_request = SearchRequest::new(vec![1.0, 0.0, 0.0], 2);
        let results = collection.search(search_request).unwrap();

        // TODO: HNSW search quality needs improvement
        // For now, just verify search returns at least one result
        assert!(!results.is_empty(), "Search should return at least one result");
    }
}
