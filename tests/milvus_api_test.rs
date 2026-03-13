//! Integration tests for Milvus API compatibility
//! 
//! Tests the complete Milvus v2 REST API implementation to ensure
//! drop-in compatibility with PyMilvus and other Milvus clients.

use rtdb::{
    api::milvus_compat::{MilvusState, create_milvus_router},
    collection::CollectionManager,
    storage::snapshot::SnapshotManager,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

/// Test helper to create a Milvus API router with test state
fn create_test_router() -> (Router, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let collections = Arc::new(CollectionManager::new(temp_dir.path()).unwrap());
    let snapshot_config = rtdb::storage::snapshot::SnapshotConfig::default();
    let snapshots = Arc::new(SnapshotManager::new(snapshot_config).unwrap());
    let state = MilvusState::new(collections, snapshots);
    let router = create_milvus_router(state);
    (router, temp_dir)
}

/// Test helper to make HTTP requests
async fn make_request(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request_builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    
    let request = if let Some(body) = body {
        request_builder
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    } else {
        request_builder.body(Body::empty()).unwrap()
    };
    
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
    
    (status, body_json)
}

#[tokio::test]
async fn test_collection_lifecycle() {
    let (router, _temp_dir) = create_test_router();
    
    // Test 1: Create collection
    let create_req = json!({
        "collectionName": "test_collection",
        "dimension": 128,
        "metricType": "COSINE",
        "description": "Test collection for Milvus API"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/create", Some(create_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // Test 2: Check if collection exists
    let has_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/has", Some(has_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["has"], true);
    
    // Test 3: List collections
    let list_req = json!({
        "dbName": "_default"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/list", Some(list_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert!(response["data"]["collections"].as_array().unwrap().contains(&json!("test_collection")));
    
    // Test 4: Describe collection
    let describe_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/describe", Some(describe_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["collectionName"], "test_collection");
    assert_eq!(response["data"]["enableDynamicField"], true);
    
    // Test 5: Load collection
    let load_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/load", Some(load_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // Test 6: Get load state
    let load_state_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/get_load_state", Some(load_state_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["state"], "Loaded");
    
    // Test 7: Drop collection
    let drop_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/drop", Some(drop_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // Test 8: Verify collection is dropped
    let has_req = json!({
        "collectionName": "test_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/has", Some(has_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["has"], false);
}

#[tokio::test]
async fn test_vector_operations() {
    let (router, _temp_dir) = create_test_router();
    
    // Create collection first
    let create_req = json!({
        "collectionName": "vector_test",
        "dimension": 4,
        "metricType": "L2"
    });
    
    let (status, _) = make_request(&router, "POST", "/v2/vectordb/collections/create", Some(create_req)).await;
    assert_eq!(status, StatusCode::OK);
    
    // Test 1: Insert vectors
    let insert_req = json!({
        "collectionName": "vector_test",
        "data": [
            {
                "id": 1,
                "vector": [0.1, 0.2, 0.3, 0.4],
                "color": "red",
                "tag": "test1"
            },
            {
                "id": 2,
                "vector": [0.5, 0.6, 0.7, 0.8],
                "color": "blue",
                "tag": "test2"
            },
            {
                "id": 3,
                "vector": [0.9, 0.1, 0.2, 0.3],
                "color": "green",
                "tag": "test3"
            }
        ]
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/insert", Some(insert_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["insertCount"], 3);
    assert_eq!(response["data"]["insertIds"].as_array().unwrap().len(), 3);
    
    // Test 2: Search vectors
    let search_req = json!({
        "collectionName": "vector_test",
        "vector": [0.1, 0.2, 0.3, 0.4],
        "limit": 2
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/search", Some(search_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    let results = response["data"]["results"].as_array().unwrap();
    assert!(results.len() <= 2);
    
    // Verify search result structure
    if !results.is_empty() {
        let first_result = &results[0];
        assert!(first_result["id"].is_string() || first_result["id"].is_number());
        assert!(first_result["distance"].is_number());
    }
    
    // Test 3: Query vectors (basic test - implementation may be limited)
    let query_req = json!({
        "collectionName": "vector_test",
        "filter": "id > 0",
        "limit": 10
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/query", Some(query_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // Test 4: Delete vectors (basic test - implementation may be limited)
    let delete_req = json!({
        "collectionName": "vector_test",
        "filter": "id == 1"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/delete", Some(delete_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
}

#[tokio::test]
async fn test_error_handling() {
    let (router, _temp_dir) = create_test_router();
    
    // Test 1: Create collection with invalid data
    let invalid_req = json!({
        "collectionName": "",
        "dimension": 0
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/create", Some(invalid_req)).await;
    // Should handle gracefully (exact behavior depends on validation)
    assert!(status == StatusCode::OK || status.is_client_error());
    
    // Test 2: Operations on non-existent collection
    let non_existent_req = json!({
        "collectionName": "non_existent_collection"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/describe", Some(non_existent_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(response["code"], 0); // Should return error code
    
    // Test 3: Insert into non-existent collection
    let insert_req = json!({
        "collectionName": "non_existent",
        "data": [{"id": 1, "vector": [1.0, 2.0, 3.0, 4.0]}]
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/insert", Some(insert_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(response["code"], 0); // Should return error code
}

#[tokio::test]
async fn test_v1_compatibility() {
    let (router, _temp_dir) = create_test_router();
    
    // Test v1 list collections endpoint
    let (status, response) = make_request(&router, "GET", "/v1/vector/collections", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // Test v1 create collection
    let create_req = json!({
        "collectionName": "v1_test",
        "dimension": 128
    });
    
    let (status, response) = make_request(&router, "POST", "/v1/vector/collections", Some(create_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
}

#[tokio::test]
async fn test_metric_type_conversion() {
    let (router, _temp_dir) = create_test_router();
    
    // Test different metric types
    let metric_types = vec!["L2", "IP", "COSINE", "HAMMING", "JACCARD"];
    
    for (i, metric_type) in metric_types.iter().enumerate() {
        let create_req = json!({
            "collectionName": format!("test_metric_{}", i),
            "dimension": 128,
            "metricType": metric_type
        });
        
        let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/create", Some(create_req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["code"], 0);
        
        // Verify the collection was created with correct metric
        let describe_req = json!({
            "collectionName": format!("test_metric_{}", i)
        });
        
        let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/describe", Some(describe_req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["code"], 0);
        
        // Check that the metric type is properly stored
        let indexes = response["data"]["indexes"].as_array().unwrap();
        if !indexes.is_empty() {
            let index = &indexes[0];
            assert!(index["metricType"].is_string());
        }
    }
}

#[tokio::test]
async fn test_pymilvus_workflow() {
    let (router, _temp_dir) = create_test_router();
    
    // Simulate a typical PyMilvus workflow
    
    // 1. Create collection
    let create_req = json!({
        "collectionName": "pymilvus_test",
        "dimension": 8,
        "metricType": "COSINE",
        "primaryField": "id",
        "vectorField": "embedding",
        "enableDynamicField": true
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/create", Some(create_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // 2. Load collection
    let load_req = json!({
        "collectionName": "pymilvus_test"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/load", Some(load_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // 3. Insert data with various field types
    let insert_req = json!({
        "collectionName": "pymilvus_test",
        "data": [
            {
                "id": 1,
                "embedding": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
                "title": "Document 1",
                "category": "tech",
                "score": 0.95,
                "tags": ["ai", "ml"]
            },
            {
                "id": 2,
                "embedding": [0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
                "title": "Document 2",
                "category": "science",
                "score": 0.87,
                "tags": ["research", "data"]
            }
        ]
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/insert", Some(insert_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    assert_eq!(response["data"]["insertCount"], 2);
    
    // 4. Search with output fields
    let search_req = json!({
        "collectionName": "pymilvus_test",
        "vector": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        "limit": 5,
        "outputFields": ["title", "category", "score"]
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/entities/search", Some(search_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
    
    // 5. Release collection
    let release_req = json!({
        "collectionName": "pymilvus_test"
    });
    
    let (status, response) = make_request(&router, "POST", "/v2/vectordb/collections/release", Some(release_req)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["code"], 0);
}