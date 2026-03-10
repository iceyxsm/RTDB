//! REST API implementation (Qdrant-compatible)

use crate::{CollectionConfig, Result, SearchRequest, UpsertRequest};
use axum::{
    extract::Path,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde::Serialize;
use serde_json::json;

/// REST API state
pub struct RestState {
    // TODO: Add storage engine reference
}

/// Start REST server
pub async fn start_server(port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(health_check))
        .route("/collections", get(list_collections).put(create_collection))
        .route("/collections/:name", get(get_collection).delete(delete_collection))
        .route("/collections/:name/points", put(upsert_points))
        .route("/collections/:name/points/search", post(search_points))
        .route("/collections/:name/points/:id", get(get_point).delete(delete_point));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await
        .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

    axum::serve(listener, app).await
        .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

    Ok(())
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    title: String,
    version: String,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        title: "rtdb - vector search engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// List collections
async fn list_collections() -> Json<serde_json::Value> {
    Json(json!({
        "result": {
            "collections": []
        },
        "status": "ok",
        "time": 0.0
    }))
}

/// Create collection
async fn create_collection(
    Path(_name): Path<String>,
    Json(_config): Json<CollectionConfig>,
) -> Json<serde_json::Value> {
    // TODO: Implement
    Json(json!({
        "result": true,
        "status": "ok",
        "time": 0.0
    }))
}

/// Get collection info
async fn get_collection(Path(_name): Path<String>) -> Json<serde_json::Value> {
    Json(json!({
        "result": {
            "status": "green",
            "vectors_count": 0,
            "segments_count": 1,
            "config": {}
        },
        "status": "ok",
        "time": 0.0
    }))
}

/// Delete collection
async fn delete_collection(Path(_name): Path<String>) -> Json<serde_json::Value> {
    Json(json!({
        "result": true,
        "status": "ok",
        "time": 0.0
    }))
}

/// Upsert points
async fn upsert_points(
    Path(_name): Path<String>,
    Json(_request): Json<UpsertRequest>,
) -> Json<serde_json::Value> {
    Json(json!({
        "result": {
            "operation_id": 1,
            "status": "completed"
        },
        "status": "ok",
        "time": 0.0
    }))
}

/// Search points
async fn search_points(
    Path(_name): Path<String>,
    Json(_request): Json<SearchRequest>,
) -> Json<serde_json::Value> {
    Json(json!({
        "result": [],
        "status": "ok",
        "time": 0.0
    }))
}

/// Get point by ID
async fn get_point(
    Path((_name, _id)): Path<(String, u64)>,
) -> Json<serde_json::Value> {
    Json(json!({
        "result": null,
        "status": "ok",
        "time": 0.0
    }))
}

/// Delete point
async fn delete_point(
    Path((_name, _id)): Path<(String, u64)>,
) -> Json<serde_json::Value> {
    Json(json!({
        "result": true,
        "status": "ok",
        "time": 0.0
    }))
}
