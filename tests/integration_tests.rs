//! Integration Tests for RTDB
//!
//! Comprehensive end-to-end tests for the Qdrant-compatible API.
//! Run with: cargo test --test integration_tests

use std::time::Duration;

mod common;
use common::{generators, TestApp};

/// Collection Management Tests
mod collection_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_collection() {
        let app = TestApp::new().await;
        
        let response = app
            .put("/collections/test_collection", serde_json::json!({
                "dimension": 128,
                "distance": "Cosine"
            }))
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["result"], true);
    }

    #[tokio::test]
    async fn test_list_collections() {
        let app = TestApp::new().await;
        
        // Create a collection first
        app.create_collection("list_test", 128, "Cosine").await;
        
        let response = app.get("/collections").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        
        let collections = body["result"]["collections"].as_array().unwrap();
        assert!(!collections.is_empty());
    }

    #[tokio::test]
    async fn test_get_collection_info() {
        let app = TestApp::new().await;
        
        // Create a collection first
        app.create_collection("info_test", 256, "Euclidean").await;
        
        let response = app.get("/collections/info_test").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        
        let result = &body["result"];
        assert_eq!(result["status"]["status"], "green");
        assert_eq!(result["vectors_count"], 0);
    }

    #[tokio::test]
    async fn test_delete_collection() {
        let app = TestApp::new().await;
        
        // Create a collection first
        app.create_collection("delete_test", 128, "Cosine").await;
        
        let response = app.delete("/collections/delete_test").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["result"], true);
        
        // Verify collection is gone
        let check = app.get("/collections/delete_test").await;
        assert!(!check.status().is_success());
    }
}

/// Points Operations Tests
mod points_tests {
    use super::*;

    #[tokio::test]
    async fn test_upsert_points() {
        let app = TestApp::new().await;
        app.create_collection("upsert_test", 4, "Cosine").await;
        
        let response = app
            .put("/collections/upsert_test/points", serde_json::json!({
                "points": [
                    {
                        "id": 1,
                        "vector": [0.1, 0.2, 0.3, 0.4],
                        "payload": {"color": "red"}
                    },
                    {
                        "id": 2,
                        "vector": [0.5, 0.6, 0.7, 0.8],
                        "payload": {"color": "blue"}
                    }
                ]
            }))
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn test_search_points() {
        let app = TestApp::new().await;
        app.create_collection("search_test", 4, "Cosine").await;
        
        // Insert points
        app.upsert_points("search_test", vec![
            (1, vec![0.1, 0.2, 0.3, 0.4], None),
            (2, vec![0.5, 0.6, 0.7, 0.8], None),
        ]).await;
        
        let response = app
            .post("/collections/search_test/points/search", serde_json::json!({
                "vector": [0.1, 0.2, 0.3, 0.4],
                "limit": 10,
                "with_payload": true
            }))
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        
        let results = body["result"].as_array().unwrap();
        assert!(!results.is_empty());
        
        // First result should be the closest
        let first = &results[0];
        assert_eq!(first["id"], 1);
        assert!(first["score"].as_f64().unwrap() > 0.9);
    }

    #[tokio::test]
    async fn test_get_point() {
        let app = TestApp::new().await;
        app.create_collection("get_test", 4, "Cosine").await;
        
        // Insert a point
        app.upsert_points("get_test", vec![
            (42, vec![0.1, 0.2, 0.3, 0.4], Some(serde_json::json!({"key": "value"}))),
        ]).await;
        
        let response = app.get("/collections/get_test/points/42").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        
        let result = &body["result"];
        assert_eq!(result["id"], 42);
        assert!(result["vector"].as_array().unwrap().len() == 4);
    }

    #[tokio::test]
    async fn test_delete_point() {
        let app = TestApp::new().await;
        app.create_collection("delete_point_test", 4, "Cosine").await;
        
        // Insert a point
        app.upsert_points("delete_point_test", vec![
            (99, vec![0.1, 0.2, 0.3, 0.4], None),
        ]).await;
        
        let response = app.delete("/collections/delete_point_test/points/99").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn test_batch_search() {
        let app = TestApp::new().await;
        app.create_collection("batch_search_test", 4, "Cosine").await;
        
        // Insert points
        app.upsert_points("batch_search_test", vec![
            (1, vec![0.1, 0.2, 0.3, 0.4], None),
            (2, vec![0.5, 0.6, 0.7, 0.8], None),
        ]).await;
        
        let response = app
            .post("/collections/batch_search_test/points/search/batch", serde_json::json!({
                "searches": [
                    {
                        "vector": [0.1, 0.2, 0.3, 0.4],
                        "limit": 5
                    },
                    {
                        "vector": [0.5, 0.6, 0.7, 0.8],
                        "limit": 5
                    }
                ]
            }))
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        
        let results = body["result"].as_array().unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_count_points() {
        let app = TestApp::new().await;
        app.create_collection("count_test", 4, "Cosine").await;
        
        // Insert points
        app.upsert_points("count_test", vec![
            (1, vec![0.1, 0.2, 0.3, 0.4], None),
            (2, vec![0.5, 0.6, 0.7, 0.8], None),
            (3, vec![0.9, 0.1, 0.2, 0.3], None),
        ]).await;
        
        let response = app
            .post("/collections/count_test/points/count", serde_json::json!({}))
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["result"]["count"], 3);
    }
}

/// Health and Monitoring Tests
mod health_tests {
    use super::*;

    #[tokio::test]
    async fn test_root_endpoint() {
        let app = TestApp::new().await;
        
        let response = app.get("/").await;
        
        assert_eq!(response.status(), 200);
        
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["result"]["title"].as_str().unwrap().contains("RTDB"));
    }

    #[tokio::test]
    async fn test_healthz() {
        let app = TestApp::new().await;
        
        let response = app.get("/healthz").await;
        
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let app = TestApp::new().await;
        
        let response = app.client()
            .get("http://localhost:9090/metrics")
            .send()
            .await
            .expect("Request failed");
        
        assert_eq!(response.status(), 200);
        
        let body = response.text().await.unwrap();
        assert!(body.contains("rtdb_"));
    }
}

/// Performance Tests
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_latency() {
        let app = TestApp::new().await;
        app.create_collection("perf_test", 128, "Cosine").await;
        
        // Insert 1000 points
        let mut points = Vec::new();
        for i in 0..1000 {
            points.push((i as u64, generators::random_vector(128), None));
        }
        app.upsert_points_batch("perf_test", points).await;
        
        // Measure search latency
        let start = std::time::Instant::now();
        
        let response = app
            .post("/collections/perf_test/points/search", serde_json::json!({
                "vector": generators::random_vector(128),
                "limit": 10
            }))
            .await;
        
        let elapsed = start.elapsed();
        
        assert_eq!(response.status(), 200);
        
        // Assert response time is reasonable (< 1 second for 1000 points)
        assert!(elapsed < Duration::from_secs(1), 
            "Search took too long: {:?}", elapsed);
    }
}

/// Error Handling Tests
mod error_tests {
    use super::*;

    #[tokio::test]
    async fn test_collection_not_found() {
        let app = TestApp::new().await;
        
        let response = app.get("/collections/nonexistent").await;
        
        assert!(!response.status().is_success());
    }

    #[tokio::test]
    async fn test_invalid_vector_dimension() {
        let app = TestApp::new().await;
        app.create_collection("dim_test", 4, "Cosine").await;
        
        // Try to insert with wrong dimension
        let response = app
            .put("/collections/dim_test/points", serde_json::json!({
                "points": [
                    {
                        "id": 1,
                        "vector": [0.1, 0.2, 0.3]  // Wrong dimension
                    }
                ]
            }))
            .await;
        
        assert!(!response.status().is_success());
    }
}
