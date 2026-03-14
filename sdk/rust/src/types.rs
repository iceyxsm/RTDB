use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Vector data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Vector {
    /// Create a new vector
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            vector,
            metadata: None,
        }
    }

    /// Create a vector with metadata
    pub fn with_metadata(
        id: impl Into<String>,
        vector: Vec<f32>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: id.into(),
            vector,
            metadata: Some(metadata),
        }
    }
}

/// Search request parameters
#[derive(Debug, Clone, Serialize)]
pub struct SearchRequest {
    pub vector: Vec<f32>,
    pub limit: usize,
    pub filter: Option<serde_json::Value>,
    pub with_payload: bool,
    pub with_vector: bool,
}

/// Search response
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub time: f64,
}

/// Individual search result
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub payload: Option<HashMap<String, serde_json::Value>>,
    pub vector: Option<Vec<f32>>,
}

/// Collection information
#[derive(Debug, Clone, Deserialize)]
pub struct Collection {
    pub name: String,
    pub status: String,
    pub vectors_count: Option<u64>,
    pub config: CollectionConfig,
}

/// Collection configuration
#[derive(Debug, Clone, Deserialize)]
pub struct CollectionConfig {
    pub params: CollectionParams,
}

/// Collection parameters
#[derive(Debug, Clone, Deserialize)]
pub struct CollectionParams {
    pub vectors: VectorParams,
}

/// Vector parameters
#[derive(Debug, Clone, Deserialize)]
pub struct VectorParams {
    pub size: usize,
    pub distance: String,
}

/// Collection information for listing
#[derive(Debug, Clone, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub status: String,
    pub vectors_count: Option<u64>,
}