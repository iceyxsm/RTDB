//! Integration tests for REST API endpoints
//!
//! Tests the production-grade REST API implementation with proper error handling,
//! rate limiting, and Qdrant compatibility.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rtdb::api::qdrant_compat::{create_qdrant_router, QdrantState};
use rtdb::collection::CollectionManager;
use rtdb::storage::snapshot::{SnapshotManager, SnapshotConfig};
use std::sync::Arc;
use tower::ServiceExt;

fn create_test_state() -> QdrantState {
    let temp_dir = tempfile::tempdir().unwrap();
    let collections = Arc::new(CollectionManager::new(temp_dir.path()).unwrap());
    
    // Create snapshot config
    let snapshot_config = rtdb::storage::snapshot::SnapshotConfig {
        local_path: temp_dir.path().to_path_buf(),
        s3_endpoint: None,
        s3_bucket: None,
        s3_access_key: None,
        s3_secret_key: None,
        compression_level: 6,
        max_incremental: 10,
        retention_days: 30,
    };
    
    let snapshot_manager = Arc::new(SnapshotManager::new(snapshot_config).unwrap());
    QdrantState::new(collections, snapshot_manager)
}

#[tokio::test]
async fn test_service_endpoints() {
    let state = create_test_state();
    let app = create_qdrant_router(state);

    // Test root info
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test health endpoints
    let health_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_response.status(), StatusCode::OK);

    // Test readiness endpoint
    let ready_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready_response.status(), StatusCode::OK);

    // Test liveness endpoint
    let live_response = app
        .oneshot(
            Request::builder()
                .uri("/livez")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_collections_api() {
    let state = create_test_state();
    let app = create_qdrant_router(state);

    // Test list collections (should be empty initially)
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/collections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    // Test collection exists (should return false for non-existent collection)
    let exists_response = app
        .oneshot(
            Request::builder()
                .uri("/collections/nonexistent/exists")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exists_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_error_handling() {
    let state = create_test_state();
    let app = create_qdrant_router(state);

    // Test getting non-existent collection (should return error)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Should return OK with error in response body (Qdrant format)
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_middleware_headers() {
    let state = create_test_state();
    let app = create_qdrant_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let headers = response.headers();
    
    // Check security headers are present
    assert!(headers.contains_key("x-content-type-options"));
    assert!(headers.contains_key("x-frame-options"));
    assert!(headers.contains_key("x-xss-protection"));
    assert!(headers.contains_key("referrer-policy"));
    assert!(headers.contains_key("content-security-policy"));
    
    // Check CORS headers
    assert!(headers.contains_key("access-control-allow-origin"));
    assert!(headers.contains_key("access-control-allow-methods"));
    assert!(headers.contains_key("access-control-allow-headers"));
    
    // Check rate limit headers
    assert!(headers.contains_key("x-ratelimit-limit"));
    assert!(headers.contains_key("x-ratelimit-remaining"));
}

#[tokio::test]
async fn test_request_validation() {
    let state = create_test_state();
    let app = create_qdrant_router(state);

    // Test invalid JSON should be handled gracefully
    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/test")
                .method("PUT")
                .header("Content-Type", "application/json")
                .body(Body::from("invalid json"))
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Should return 422 Unprocessable Entity or 400 Bad Request
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY 
        || response.status() == StatusCode::BAD_REQUEST
    );
}