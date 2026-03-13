//! Milvus API compatibility layer
//! 
//! Provides drop-in compatibility with Milvus REST and gRPC APIs
//! Supports both v1 and v2 endpoints for maximum compatibility

#![allow(missing_docs)]

use crate::{
    collection::CollectionManager,
    storage::snapshot::SnapshotManager,
    CollectionConfig, Distance as RTDBDistance, SearchRequest, UpsertRequest, Vector, WithPayload,
    Result,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tracing::{info, warn, error};

/// Milvus API state
pub struct MilvusState {
    collections: Arc<CollectionManager>,
    snapshots: Arc<SnapshotManager>,
}

impl MilvusState {
    pub fn new(collections: Arc<CollectionManager>, snapshots: Arc<SnapshotManager>) -> Self {
        Self {
            collections,
            snapshots,
        }
    }
}

/// Create Milvus-compatible router
pub fn create_milvus_router(state: MilvusState) -> Router {
    Router::new()
        // v2 Collection endpoints (recommended)
        .route("/v2/vectordb/collections/create", post(create_collection_v2))
        .route("/v2/vectordb/collections/drop", post(drop_collection_v2))
        .route("/v2/vectordb/collections/list", get(list_collections_v2))
        .route("/v2/vectordb/collections/describe", post(describe_collection_v2))
        .route("/v2/vectordb/collections/has", post(has_collection_v2))
        .route("/v2/vectordb/collections/load", post(load_collection_v2))
        .route("/v2/vectordb/collections/release", post(release_collection_v2))
        .route("/v2/vectordb/collections/get_load_state", post(get_load_state_v2))
        
        // v2 Vector/Entity endpoints
        .route("/v2/vectordb/entities/insert", post(insert_entities_