//! RTDB Client Library
//!
//! This module provides a high-level client interface for interacting with RTDB instances.
//! It supports all major features including vector operations, quantization, cross-region
//! replication, and WebAssembly custom functions.

use crate::{
    quantization::advanced::QuantizationConfig,
    cross_region::SearchResult,
    Result,
};
use serde_json::Value;

/// Configuration for RTDB client connection and features
#[derive(Debug, Clone)]
pub struct Config {
    /// Hostname or IP address of the RTDB server
    pub host: String,
    /// Port number of the RTDB server
    pub port: u16,
    /// Enable advanced quantization features
    pub quantization_enabled: bool,
    /// Enable cross-region replication
    pub cross_region_enabled: bool,
    /// Enable WebAssembly custom functions
    pub wasm_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 6333,
            quantization_enabled: false,
            cross_region_enabled: false,
            wasm_enabled: false,
        }
    }
}

impl Config {
    /// Set the server hostname
    pub fn with_host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }
    
    /// Set the server port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    
    /// Enable or disable quantization features
    pub fn with_quantization_enabled(mut self, enabled: bool) -> Self {
        self.quantization_enabled = enabled;
        self
    }
    
    /// Enable or disable cross-region replication
    pub fn with_cross_region_enabled(mut self, enabled: bool) -> Self {
        self.cross_region_enabled = enabled;
        self
    }
    
    /// Enable or disable WebAssembly custom functions
    pub fn with_wasm_enabled(mut self, enabled: bool) -> Self {
        self.wasm_enabled = enabled;
        self
    }
}

/// RTDB client for interacting with the vector database
pub struct RtdbClient {
    config: Config,
    base_url: String,
    client: reqwest::Client,
}

impl RtdbClient {
    /// Create a new RTDB client with the given configuration
    pub async fn new(config: Config) -> Result<Self> {
        let base_url = format!("http://{}:{}", config.host, config.port);
        let client = reqwest::Client::new();
        
        Ok(Self {
            config,
            base_url,
            client,
        })
    }
    
    /// Create a new vector collection with optional quantization
    pub async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        quantization_config: Option<QuantizationConfig>,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "name": name,
            "dimension": dimension,
            "quantization": quantization_config
        });
        
        let response = self.client
            .post(format!("{}/collections", self.base_url))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(crate::RTDBError::ApiError(format!("Failed to create collection: {}", response.status())))
        }
    }
    
    /// Create a multimodal collection for text, image, and audio embeddings
    pub async fn create_multimodal_collection(&self, name: &str) -> Result<()> {
        let payload = serde_json::json!({
            "name": name,
            "dimension": 512, // Standard multimodal embedding dimension
            "multimodal": true,
            "metadata_schema": {
                "type": "object",
                "properties": {
                    "type": {"type": "string"},
                    "content": {"type": "string"},
                    "path": {"type": "string"},
                    "id": {"type": "integer"}
                }
            }
        });
        
        let response = self.client
            .post(format!("{}/collections", self.base_url))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(crate::RTDBError::ApiError(format!("Failed to create multimodal collection: {}", response.status())))
        }
    }
    
    /// Insert multiple vectors in a single batch operation
    pub async fn insert_batch(&self, collection_name: &str, vectors: Vec<Vec<f32>>) -> Result<()> {
        let points: Vec<serde_json::Value> = vectors.into_iter().enumerate().map(|(i, vector)| {
            serde_json::json!({
                "id": i,
                "vector": vector
            })
        }).collect();
        
        let payload = serde_json::json!({
            "points": points
        });
        
        let response = self.client
            .put(format!("{}/collections/{}/points", self.base_url, collection_name))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(crate::RTDBError::ApiError(format!("Failed to insert batch: {}", response.status())))
        }
    }
    
    /// Insert a vector with associated metadata
    pub async fn insert_with_metadata(
        &self,
        collection_name: &str,
        vector: Vec<f32>,
        metadata: Value,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "points": [{
                "id": uuid::Uuid::new_v4().to_string(),
                "vector": vector,
                "payload": metadata
            }]
        });
        
        let response = self.client
            .put(format!("{}/collections/{}/points", self.base_url, collection_name))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(crate::RTDBError::ApiError(format!("Failed to insert with metadata: {}", response.status())))
        }
    }
    
    /// Search for similar vectors in a collection
    pub async fn search(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let payload = serde_json::json!({
            "vector": query_vector,
            "limit": limit,
            "with_payload": true
        });
        
        let response = self.client
            .post(format!("{}/collections/{}/points/search", self.base_url, collection_name))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            let results: Vec<SearchResult> = response.json().await?;
            Ok(results)
        } else {
            Err(crate::RTDBError::ApiError(format!("Search failed: {}", response.status())))
        }
    }
    
    /// Search for similar vectors and return results with metadata
    pub async fn search_with_metadata(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<SearchResultWithMetadata>> {
        let payload = serde_json::json!({
            "vector": query_vector,
            "limit": limit,
            "with_payload": true
        });
        
        let response = self.client
            .post(format!("{}/collections/{}/points/search", self.base_url, collection_name))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            let results: Vec<SearchResultWithMetadata> = response.json().await?;
            Ok(results)
        } else {
            Err(crate::RTDBError::ApiError(format!("Search with metadata failed: {}", response.status())))
        }
    }
    
    /// Register a WebAssembly function for custom similarity calculations
    pub async fn register_wasm_function(
        &self,
        collection_name: &str,
        function_name: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "function_name": function_name,
            "collection": collection_name
        });
        
        let response = self.client
            .post(format!("{}/wasm/register", self.base_url))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(crate::RTDBError::ApiError(format!("Failed to register WASM function: {}", response.status())))
        }
    }
    
    /// Search using a custom similarity function
    pub async fn search_with_custom_similarity(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
        limit: usize,
        similarity_function: &str,
    ) -> Result<Vec<SearchResult>> {
        let payload = serde_json::json!({
            "vector": query_vector,
            "limit": limit,
            "similarity_function": similarity_function,
            "with_payload": true
        });
        
        let response = self.client
            .post(format!("{}/collections/{}/points/search/custom", self.base_url, collection_name))
            .json(&payload)
            .send()
            .await?;
            
        if response.status().is_success() {
            let results: Vec<SearchResult> = response.json().await?;
            Ok(results)
        } else {
            Err(crate::RTDBError::ApiError(format!("Custom similarity search failed: {}", response.status())))
        }
    }
}

/// Search result with metadata information
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchResultWithMetadata {
    /// Unique identifier of the result
    pub id: String,
    /// Similarity score (lower is more similar)
    pub score: f32,
    /// Optional metadata associated with the vector
    pub metadata: Option<Value>,
}

impl From<reqwest::Error> for crate::RTDBError {
    fn from(err: reqwest::Error) -> Self {
        crate::RTDBError::ApiError(err.to_string())
    }
}