//! REST API implementation (Qdrant-compatible)

use crate::{
    auth::middleware::{auth_middleware, AuthState},
    auth::{ApiKeyStore, AuthConfig},
    CollectionConfig, Result, SearchRequest, UpsertRequest, Vector, VectorId,
    collection::CollectionManager,
};
use axum::{
    extract::{Path, State},
    middleware,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

/// REST API state
#[derive(Clone)]
pub struct RestState {
    /// Collection manager
    pub collections: Arc<CollectionManager>,
    /// Auth state
    pub auth_state: AuthState,
}

impl RestState {
    /// Create new REST state
    pub fn new(collections: Arc<CollectionManager>) -> Self {
        let key_store = Arc::new(ApiKeyStore::default());
        let auth_state = AuthState::new(AuthConfig::default(), key_store);
        Self { collections, auth_state }
    }
    
    /// Create with auth state
    pub fn with_auth(collections: Arc<CollectionManager>, auth_state: AuthState) -> Self {
        Self { collections, auth_state }
    }
}

/// Create REST router with auth middleware
pub fn create_router(state: RestState) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check));
    
    // Protected routes (auth required)
    let protected_routes = Router::new()
        .route("/collections", get(list_collections).put(create_collection))
        .route("/collections/:name", get(get_collection).delete(delete_collection))
        .route("/collections/:name/points", put(upsert_points))
        .route("/collections/:name/points/search", post(search_points))
        .route("/collections/:name/points/:id", get(get_point).delete(delete_point))
        .route_layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            auth_middleware
        ));
    
    public_routes
        .merge(protected_routes)
        .with_state(state)
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

/// Collections list response
#[derive(Serialize)]
#[allow(dead_code)]
struct CollectionsResponse {
    collections: Vec<CollectionDescription>,
}

#[derive(Serialize)]
struct CollectionDescription {
    name: String,
}

/// List all collections
async fn list_collections(State(state): State<RestState>) -> Json<serde_json::Value> {
    let collections = state.collections.list_collections();
    let descriptions: Vec<_> = collections
        .into_iter()
        .map(|name| CollectionDescription { name })
        .collect();

    Json(json!({
        "result": {
            "collections": descriptions
        },
        "status": "ok",
        "time": 0.0
    }))
}

/// Create collection request
#[derive(serde::Deserialize)]
struct CreateCollectionRequest {
    #[serde(flatten)]
    config: CollectionConfig,
}

/// Create new collection
async fn create_collection(
    Path(name): Path<String>,
    State(state): State<RestState>,
    Json(request): Json<CreateCollectionRequest>,
) -> Json<serde_json::Value> {
    match state.collections.create_collection(&name, request.config) {
        Ok(_) => Json(json!({
            "result": true,
            "status": "ok",
            "time": 0.0
        })),
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Collection info response
#[derive(Serialize)]
struct CollectionInfo {
    status: String,
    vectors_count: u64,
    indexed_vectors_count: u64,
    points_count: u64,
    segments_count: usize,
    config: CollectionConfig,
}

/// Get collection info
async fn get_collection(
    Path(name): Path<String>,
    State(state): State<RestState>,
) -> Json<serde_json::Value> {
    match state.collections.get_collection(&name) {
        Ok(collection) => {
            let info = CollectionInfo {
                status: "green".to_string(),
                vectors_count: collection.vector_count(),
                indexed_vectors_count: collection.vector_count(),
                points_count: collection.vector_count(),
                segments_count: 1,
                config: collection.config().clone(),
            };

            Json(json!({
                "result": info,
                "status": "ok",
                "time": 0.0
            }))
        }
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Delete collection
async fn delete_collection(
    Path(name): Path<String>,
    State(state): State<RestState>,
) -> Json<serde_json::Value> {
    match state.collections.delete_collection(&name) {
        Ok(_) => Json(json!({
            "result": true,
            "status": "ok",
            "time": 0.0
        })),
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Point struct for API
#[derive(serde::Deserialize)]
struct Point {
    id: u64,
    vector: Vec<f32>,
    #[serde(default)]
    payload: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Upsert points request
#[derive(serde::Deserialize)]
struct UpsertPointsRequest {
    points: Vec<Point>,
}

/// Upsert operation response
#[derive(Serialize)]
#[allow(dead_code)]
struct UpsertResponse {
    operation_id: u64,
    status: String,
}

/// Upsert points into collection
async fn upsert_points(
    Path(name): Path<String>,
    State(state): State<RestState>,
    Json(request): Json<UpsertPointsRequest>,
) -> Json<serde_json::Value> {
    match state.collections.get_collection(&name) {
        Ok(collection) => {
            let vectors: Vec<(VectorId, Vector)> = request
                .points
                .into_iter()
                .map(|p| {
                    let mut vector = Vector::new(p.vector);
                    vector.payload = p.payload;
                    (p.id, vector)
                })
                .collect();

            let upsert_request = UpsertRequest { vectors };

            match collection.upsert(upsert_request) {
                Ok(info) => Json(json!({
                    "result": {
                        "operation_id": info.operation_id,
                        "status": format!("{:?}", info.status).to_lowercase()
                    },
                    "status": "ok",
                    "time": 0.0
                })),
                Err(e) => Json(json!({
                    "status": {"error": e.to_string()},
                    "time": 0.0
                })),
            }
        }
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Search response
#[derive(Serialize)]
struct SearchResponse {
    id: u64,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    vector: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Search points in collection
async fn search_points(
    Path(name): Path<String>,
    State(state): State<RestState>,
    Json(request): Json<SearchRequest>,
) -> Json<serde_json::Value> {
    match state.collections.get_collection(&name) {
        Ok(collection) => {
            match collection.search(request) {
                Ok(results) => {
                    let response: Vec<SearchResponse> = results
                        .into_iter()
                        .map(|r| SearchResponse {
                            id: r.id,
                            score: r.score,
                            vector: r.vector,
                            payload: r.payload,
                        })
                        .collect();

                    Json(json!({
                        "result": response,
                        "status": "ok",
                        "time": 0.0
                    }))
                }
                Err(e) => Json(json!({
                    "status": {"error": e.to_string()},
                    "time": 0.0
                })),
            }
        }
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Get point by ID
async fn get_point(
    Path((name, id)): Path<(String, u64)>,
    State(state): State<RestState>,
) -> Json<serde_json::Value> {
    match state.collections.get_collection(&name) {
        Ok(collection) => {
            match collection.get(id) {
                Ok(Some(vector)) => Json(json!({
                    "result": {
                        "id": id,
                        "vector": vector.vector,
                        "payload": vector.payload
                    },
                    "status": "ok",
                    "time": 0.0
                })),
                Ok(None) => Json(json!({
                    "result": null,
                    "status": "ok",
                    "time": 0.0
                })),
                Err(e) => Json(json!({
                    "status": {"error": e.to_string()},
                    "time": 0.0
                })),
            }
        }
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Delete point by ID
async fn delete_point(
    Path((name, id)): Path<(String, u64)>,
    State(state): State<RestState>,
) -> Json<serde_json::Value> {
    match state.collections.get_collection(&name) {
        Ok(collection) => {
            match collection.delete(&[id]) {
                Ok(_) => Json(json!({
                    "result": true,
                    "status": "ok",
                    "time": 0.0
                })),
                Err(e) => Json(json!({
                    "status": {"error": e.to_string()},
                    "time": 0.0
                })),
            }
        }
        Err(e) => Json(json!({
            "status": {"error": e.to_string()},
            "time": 0.0
        })),
    }
}

/// Start REST server
pub async fn start_server(port: u16, collections: Arc<CollectionManager>) -> Result<()> {
    let state = RestState::new(collections);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await
        .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

    axum::serve(listener, app).await
        .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

    Ok(())
}
