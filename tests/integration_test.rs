//! Integration tests for RTDB

use rtdb::{
    collection::CollectionManager,
    CollectionConfig, Distance, SearchRequest, UpsertRequest, Vector,
};
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn test_full_workflow() {
    // Create temporary directory
    let temp_dir = TempDir::new().unwrap();
    
    // Create collection manager
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );

    // Create collection
    let config = CollectionConfig {
        dimension: 128,
        distance: Distance::Cosine,
        hnsw_config: None,
        quantization_config: None,
        optimizer_config: None,
    };
    
    manager.create_collection("test_collection", config).unwrap();
    
    // Get collection
    let collection = manager.get_collection("test_collection").unwrap();
    
    // Insert vectors
    let mut vectors = Vec::new();
    for i in 0..100 {
        let v = Vector::new(vec![i as f32; 128]);
        vectors.push((i as u64, v));
    }
    
    let upsert_request = UpsertRequest { vectors };
    collection.upsert(upsert_request).unwrap();
    
    // Search
    let search_request = SearchRequest::new(vec![50.0; 128], 10);
    let results = collection.search(search_request).unwrap();
    
    assert!(!results.is_empty());
    // First result should be close to our query
    assert!(results[0].id >= 45 && results[0].id <= 55);
}

#[test]
fn test_collection_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    
    // Create and populate collection
    {
        let manager = Arc::new(
            CollectionManager::new(&path).unwrap()
        );
        
        let config = CollectionConfig::new(64);
        manager.create_collection("persistent", config).unwrap();
        
        let collection = manager.get_collection("persistent").unwrap();
        
        // Insert data
        let vectors: Vec<(u64, Vector)> = (0..50)
            .map(|i| (i, Vector::new(vec![i as f32; 64])))
            .collect();
        
        collection.upsert(UpsertRequest { vectors }).unwrap();
    }
    
    // Reopen and verify
    {
        let manager = Arc::new(
            CollectionManager::new(&path).unwrap()
        );
        
        assert!(manager.has_collection("persistent"));
        
        let collection = manager.get_collection("persistent").unwrap();
        assert_eq!(collection.vector_count(), 50);
        
        // Verify search works
        let results = collection.search(SearchRequest::new(vec![25.0; 64], 5)).unwrap();
        assert!(!results.is_empty());
    }
}

#[test]
fn test_multiple_collections() {
    let temp_dir = TempDir::new().unwrap();
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );
    
    // Create multiple collections
    for i in 0..5 {
        let name = format!("collection_{}", i);
        let config = CollectionConfig::new(32);
        manager.create_collection(&name, config).unwrap();
        
        let collection = manager.get_collection(&name).unwrap();
        
        // Insert some data
        let vectors: Vec<(u64, Vector)> = (0..10)
            .map(|j| (j, Vector::new(vec![j as f32; 32])))
            .collect();
        
        collection.upsert(UpsertRequest { vectors }).unwrap();
    }
    
    // List collections
    let collections = manager.list_collections();
    assert_eq!(collections.len(), 5);
    
    // Delete one
    manager.delete_collection("collection_2").unwrap();
    assert!(!manager.has_collection("collection_2"));
    assert_eq!(manager.list_collections().len(), 4);
}

#[test]
fn test_vector_operations() {
    let temp_dir = TempDir::new().unwrap();
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );
    
    let config = CollectionConfig::new(3);
    manager.create_collection("ops", config).unwrap();
    
    let collection = manager.get_collection("ops").unwrap();
    
    // Insert
    collection.upsert(UpsertRequest {
        vectors: vec![
            (1, Vector::new(vec![1.0, 0.0, 0.0])),
            (2, Vector::new(vec![0.0, 1.0, 0.0])),
            (3, Vector::new(vec![0.0, 0.0, 1.0])),
        ],
    }).unwrap();
    
    // Get
    let v = collection.get(1).unwrap().unwrap();
    assert_eq!(v.vector, vec![1.0, 0.0, 0.0]);
    
    // Search
    let results = collection.search(SearchRequest::new(vec![1.0, 0.0, 0.0], 2)).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, 1);
    
    // Delete
    collection.delete(&[1]).unwrap();
    assert!(collection.get(1).unwrap().is_none());
}

#[test]
fn test_different_distances() {
    let temp_dir = TempDir::new().unwrap();
    
    // Test Euclidean distance
    {
        let manager = Arc::new(
            CollectionManager::new(temp_dir.path().join("euclid")).unwrap()
        );
        
        let mut config = CollectionConfig::new(2);
        config.distance = Distance::Euclidean;
        manager.create_collection("test", config).unwrap();
        
        let collection = manager.get_collection("test").unwrap();
        
        collection.upsert(UpsertRequest {
            vectors: vec![
                (1, Vector::new(vec![0.0, 0.0])),
                (2, Vector::new(vec![3.0, 4.0])), // Distance 5 from origin
                (3, Vector::new(vec![6.0, 8.0])), // Distance 10 from origin
            ],
        }).unwrap();
        
        let results = collection.search(SearchRequest::new(vec![0.0, 0.0], 3)).unwrap();
        // Nearest should be ID 1 (origin), then 2, then 3
        assert_eq!(results[0].id, 1);
    }
    
    // Test Cosine similarity
    {
        let manager = Arc::new(
            CollectionManager::new(temp_dir.path().join("cosine")).unwrap()
        );
        
        let mut config = CollectionConfig::new(2);
        config.distance = Distance::Cosine;
        manager.create_collection("test", config).unwrap();
        
        let collection = manager.get_collection("test").unwrap();
        
        collection.upsert(UpsertRequest {
            vectors: vec![
                (1, Vector::new(vec![1.0, 0.0])), // Along x-axis
                (2, Vector::new(vec![0.0, 1.0])), // Along y-axis
                (3, Vector::new(vec![1.0, 1.0])), // 45 degrees
            ],
        }).unwrap();
        
        // Search for x-axis direction
        let results = collection.search(SearchRequest::new(vec![1.0, 0.0], 2)).unwrap();
        // Most similar should be ID 1 (same direction)
        assert_eq!(results[0].id, 1);
    }
}

#[test]
fn test_payload_handling() {
    let temp_dir = TempDir::new().unwrap();
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );
    
    let config = CollectionConfig::new(4);
    manager.create_collection("payload_test", config).unwrap();
    
    let collection = manager.get_collection("payload_test").unwrap();
    
    // Create vector with payload
    let mut vector = Vector::new(vec![1.0, 2.0, 3.0, 4.0]);
    let mut payload = serde_json::Map::new();
    payload.insert("name".to_string(), serde_json::json!("test_item"));
    payload.insert("category".to_string(), serde_json::json!("test"));
    vector.payload = Some(payload);
    
    collection.upsert(UpsertRequest {
        vectors: vec![(1, vector)],
    }).unwrap();
    
    // Retrieve and verify payload
    let retrieved = collection.get(1).unwrap().unwrap();
    assert!(retrieved.payload.is_some());
    
    let payload = retrieved.payload.unwrap();
    assert_eq!(payload.get("name").unwrap().as_str().unwrap(), "test_item");
}
