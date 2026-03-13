//! Comprehensive integration tests for Weaviate API compatibility
//! 
//! Tests both GraphQL and REST API endpoints to ensure full compatibility
//! with Weaviate client libraries and workflows.

use rtdb::{
    api::weaviate_compat::{WeaviateState, create_weaviate_router, WeaviateClass, WeaviateProperty, WeaviateObject, GraphQLRequest},
    collection::CollectionManager,
    storage::snapshot::SnapshotManager,
    CollectionConfig,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

/// Create test state and router for Weaviate API testing
fn create_test_setup() -> (Router, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let collections = Arc::new(CollectionManager::new(temp_dir.path()).unwrap());
    let snapshot_config = rtdb::storage::snapshot::SnapshotConfig::default();
    let snapshots = Arc::new(SnapshotManager::new(snapshot_config).unwrap());
    let state = WeaviateState::new(collections, snapshots);
    let router = create_weaviate_router(state);
    (router, temp_dir)
}

/// Helper function to create a test class
fn create_test_class() -> WeaviateClass {
    WeaviateClass {
        class: "Article".to_string(),
        description: Some("A news article".to_string()),
        properties: vec![
            WeaviateProperty {
                name: "title".to_string(),
                data_type: vec!["text".to_string()],
                description: Some("Article title".to_string()),
                index_inverted: Some(true),
                index_filterable: Some(true),
                index_searchable: Some(true),
                tokenization: Some("word".to_string()),
                module_config: None,
            },
            WeaviateProperty {
                name: "content".to_string(),
                data_type: vec!["text".to_string()],
                description: Some("Article content".to_string()),
                index_inverted: Some(true),
                index_filterable: Some(false),
                index_searchable: Some(true),
                tokenization: Some("word".to_string()),
                module_config: None,
            },
        ],
        vectorizer: Some("text2vec-openai".to_string()),
        vector_index_type: Some("hnsw".to_string()),
        vector_index_config: Some(json!({
            "distance": "cosine",
            "efConstruction": 128,
            "maxConnections": 64
        })),
        inverted_index_config: None,
        module_config: Some(json!({
            "text2vec-openai": {
                "model": "text-embedding-ada-002",
                "dimensions": 1536
            }
        })),
    }
}

#[tokio::test]
async fn test_schema_management() {
    let (router, _temp_dir) = create_test_setup();
    
    // Test creating a class
    let test_class = create_test_class();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/schema")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_class).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test getting the schema
    let request = Request::builder()
        .method("GET")
        .uri("/v1/schema")
        .body(Body::empty())
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let schema: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(schema["classes"].as_array().unwrap().len() > 0);
    
    // Test getting specific class
    let request = Request::builder()
        .method("GET")
        .uri("/v1/schema/Article")
        .body(Body::empty())
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let class: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(class["class"], "Article");
    
    // Test deleting the class
    let request = Request::builder()
        .method("DELETE")
        .uri("/v1/schema/Article")
        .body(Body::empty())
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_object_management() {
    let (router, _temp_dir) = create_test_setup();
    
    // First create a class
    let test_class = create_test_class();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/schema")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_class).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Create an object
    let test_object = WeaviateObject {
        id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        class: "Article".to_string(),
        properties: json!({
            "title": "Test Article",
            "content": "This is a test article about vector databases."
        }),
        vector: Some(vec![0.1, 0.2, 0.3]), // Simple 3D vector for testing
        creation_time_unix: None,
        last_update_time_unix: None,
        additional: None,
    };
    
    let request = Request::builder()
        .method("POST")
        .uri("/v1/objects")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_object).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Get the object
    let request = Request::builder()
        .method("GET")
        .uri("/v1/objects/123e4567-e89b-12d3-a456-426614174000")
        .body(Body::empty())
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let object: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(object["id"], "123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(object["class"], "Article");
    
    // Update the object
    let mut updated_object = test_object.clone();
    updated_object.properties = json!({
        "title": "Updated Test Article",
        "content": "This is an updated test article about vector databases."
    });
    
    let request = Request::builder()
        .method("PUT")
        .uri("/v1/objects/123e4567-e89b-12d3-a456-426614174000")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&updated_object).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Delete the object
    let request = Request::builder()
        .method("DELETE")
        .uri("/v1/objects/123e4567-e89b-12d3-a456-426614174000")
        .body(Body::empty())
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_graphql_get_query() {
    let (router, _temp_dir) = create_test_setup();
    
    // Create a class and add some test data
    let test_class = create_test_class();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/schema")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_class).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Add test objects
    for i in 0..3 {
        let test_object = WeaviateObject {
            id: Some(format!("test-object-{}", i)),
            class: "Article".to_string(),
            properties: json!({
                "title": format!("Test Article {}", i),
                "content": format!("Content for article {}", i)
            }),
            vector: Some(vec![i as f32 * 0.1, (i + 1) as f32 * 0.1, (i + 2) as f32 * 0.1]),
            creation_time_unix: None,
            last_update_time_unix: None,
            additional: None,
        };
        
        let request = Request::builder()
            .method("POST")
            .uri("/v1/objects")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&test_object).unwrap()))
            .unwrap();
        
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    // Test GraphQL nearVector query
    let graphql_query = GraphQLRequest {
        query: r#"
        {
            Get {
                Article(nearVector: { vector: [0.1, 0.2, 0.3] }, limit: 2) {
                    title
                    content
                    _additional {
                        id
                        distance
                        certainty
                    }
                }
            }
        }
        "#.to_string(),
        variables: None,
        operation_name: None,
    };
    
    let request = Request::builder()
        .method("POST")
        .uri("/v1/graphql")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&graphql_query).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(result["data"].is_object());
    assert!(result["data"]["Get"]["Article"].is_array());
    let articles = result["data"]["Get"]["Article"].as_array().unwrap();
    assert!(articles.len() <= 2); // Respects limit
}

#[tokio::test]
async fn test_graphql_aggregate_query() {
    let (router, _temp_dir) = create_test_setup();
    
    // Create a class
    let test_class = create_test_class();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/schema")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_class).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test GraphQL aggregate query
    let graphql_query = GraphQLRequest {
        query: r#"
        {
            Aggregate {
                Article {
                    meta {
                        count
                    }
                }
            }
        }
        "#.to_string(),
        variables: None,
        operation_name: None,
    };
    
    let request = Request::builder()
        .method("POST")
        .uri("/v1/graphql")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&graphql_query).unwrap()))
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(result["data"].is_object());
    assert!(result["data"]["Aggregate"]["Article"].is_array());
}

#[tokio::test]
async fn test_batch_operations() {
    let (router, _temp_dir) = create_test_setup();
    
    // Create a class
    let test_class = create_test_class();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/schema")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_class).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test batch create
    let batch_objects = json!({
        "objects": [
            {
                "class": "Article",
                "properties": {
                    "title": "Batch Article 1",
                    "content": "Content 1"
                },
                "vector": [0.1, 0.2, 0.3]
            },
            {
                "class": "Article", 
                "properties": {
                    "title": "Batch Article 2",
                    "content": "Content 2"
                },
                "vector": [0.4, 0.5, 0.6]
            }
        ]
    });
    
    let request = Request::builder()
        .method("POST")
        .uri("/v1/batch/objects")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&batch_objects).unwrap()))
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(result["results"].is_array());
    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    
    // Check that both objects were created successfully
    for result in results {
        assert!(result["result"].is_object());
        assert!(result["errors"].is_null());
    }
}

#[tokio::test]
async fn test_health_endpoints() {
    let (router, _temp_dir) = create_test_setup();
    
    // Test ready endpoint
    let request = Request::builder()
        .method("GET")
        .uri("/v1/.well-known/ready")
        .body(Body::empty())
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["status"], "ready");
    
    // Test live endpoint
    let request = Request::builder()
        .method("GET")
        .uri("/v1/.well-known/live")
        .body(Body::empty())
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["status"], "live");
    
    // Test meta endpoint
    let request = Request::builder()
        .method("GET")
        .uri("/v1/meta")
        .body(Body::empty())
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(result["hostname"].is_string());
    assert!(result["version"].is_string());
}

#[tokio::test]
async fn test_error_handling() {
    let (router, _temp_dir) = create_test_setup();
    
    // Test GraphQL query with non-existent class
    let graphql_query = GraphQLRequest {
        query: r#"
        {
            Get {
                NonExistentClass(nearVector: { vector: [0.1, 0.2, 0.3] }) {
                    title
                }
            }
        }
        "#.to_string(),
        variables: None,
        operation_name: None,
    };
    
    let request = Request::builder()
        .method("POST")
        .uri("/v1/graphql")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&graphql_query).unwrap()))
        .unwrap();
    
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should have errors
    assert!(result["errors"].is_array());
    assert!(result["data"].is_null());
    
    // Test getting non-existent object
    let request = Request::builder()
        .method("GET")
        .uri("/v1/objects/non-existent-id")
        .body(Body::empty())
        .unwrap();
    
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(result["error"].is_array());
}