//! Client implementations for different vector databases
//!
//! Provides unified interfaces for reading from source databases and writing to RTDB.
//! Supports Qdrant, Milvus, Weaviate, Pinecone, LanceDB, and file formats.

use crate::migration::{AuthConfig, MigrationConfig, SourceType, VectorRecord};
use crate::{Result, RTDBError};
use async_trait::async_trait;
use reqwest::Client as HttpClient;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use base64::{Engine as _, engine::general_purpose};

/// Trait for source database clients
#[async_trait]
pub trait SourceClient: Send + Sync {
    /// Get total number of records (if available)
    async fn get_total_count(&mut self) -> Result<Option<u64>>;
    
    /// Fetch a batch of records starting from offset
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>>;
    
    /// Clone the client for use in different tasks
    fn clone_box(&self) -> Box<dyn SourceClient>;
}

/// Trait for target database clients
#[async_trait]
pub trait TargetClient: Send + Sync {
    /// Insert a batch of records
    async fn insert_batch(&self, records: &[VectorRecord]) -> Result<()>;

    /// Create collection if it doesn't exist
    async fn ensure_collection(&self, collection_name: &str, dimension: usize) -> Result<()>;

    /// Get collection info
    async fn get_collection_info(&self, collection_name: &str) -> Result<Option<CollectionInfo>>;

    /// Get total count of records (for consistency verification)
    async fn get_total_count(&self) -> Result<Option<u64>>;

    /// Fetch a batch of records (for consistency verification)
    async fn fetch_batch(&self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>>;

    /// Clone the client for use in different tasks
    fn clone_box(&self) -> Box<dyn TargetClient>;
}


/// Collection information for migration operations
#[derive(Debug, Clone)]
pub struct CollectionInfo {
    /// Collection name
    pub name: String,
    /// Vector dimension
    pub dimension: usize,
    /// Total number of vectors in collection
    pub vector_count: u64,
    /// Distance metric used (Cosine, L2, etc.)
    pub distance_metric: String,
}

/// Create source client based on configuration
pub async fn create_source_client(config: &MigrationConfig) -> Result<Box<dyn SourceClient>> {
    match config.source_type {
        SourceType::Qdrant => {
            Ok(Box::new(QdrantSourceClient::new(
                &config.source_url,
                config.source_collection.as_deref(),
                config.source_auth.as_ref(),
            ).await?))
        }
        SourceType::Milvus => {
            Ok(Box::new(MilvusSourceClient::new(
                &config.source_url,
                config.source_collection.as_deref(),
                config.source_auth.as_ref(),
            ).await?))
        }
        SourceType::Weaviate => {
            Ok(Box::new(WeaviateSourceClient::new(
                &config.source_url,
                config.source_collection.as_deref(),
                config.source_auth.as_ref(),
            ).await?))
        }
        SourceType::Pinecone => {
            Ok(Box::new(PineconeSourceClient::new(
                &config.source_url,
                config.source_collection.as_deref(),
                config.source_auth.as_ref(),
            ).await?))
        }
        SourceType::LanceDB => {
            Ok(Box::new(LanceDBSourceClient::new(
                &config.source_url,
                config.source_collection.as_deref(),
            ).await?))
        }
        SourceType::Jsonl => {
            Ok(Box::new(JsonlSourceClient::new(&config.source_url).await?))
        }
        SourceType::Parquet => {
            Ok(Box::new(ParquetSourceClient::new(&config.source_url).await?))
        }
        SourceType::Hdf5 => {
            Ok(Box::new(Hdf5SourceClient::new(&config.source_url).await?))
        }
        SourceType::Csv => {
            Ok(Box::new(CsvSourceClient::new(&config.source_url).await?))
        }
        SourceType::Binary => {
            Ok(Box::new(BinarySourceClient::new(&config.source_url).await?))
        }
    }
}

/// Create target client
pub async fn create_target_client(config: &MigrationConfig) -> Result<Box<dyn TargetClient>> {
    // For Parquet exports, we now support them with proper async handling
    if config.target_url.ends_with(".parquet") {
        tracing::info!("Creating Parquet target client with async support");
        // Note: ParquetTargetClient would need to be implemented if needed
        // For now, we recommend using the ParquetWriter directly through formats.rs
        return Err(RTDBError::Config(
            "Parquet export should use ParquetWriter through formats.rs for optimal performance".to_string()
        ));
    }
    
    // Default to RTDB target client
    Ok(Box::new(RTDBTargetClient::new(
        &config.target_url,
        config.target_auth.as_ref(),
    ).await?))
}

/// Qdrant source client
pub struct QdrantSourceClient {
    client: HttpClient,
    base_url: String,
    collection: String,
    current_offset: Option<String>,
}

impl QdrantSourceClient {
    async fn new(url: &str, collection: Option<&str>, auth: Option<&AuthConfig>) -> Result<Self> {
        let mut client_builder = HttpClient::builder();
        
        if let Some(auth_config) = auth {
            let mut headers = reqwest::header::HeaderMap::new();
            
            match auth_config {
                AuthConfig::ApiKey(api_key) => {
                    headers.insert("api-key", api_key.parse().map_err(|_| 
                        RTDBError::Config("Invalid API key format".to_string()))?);
                }
                AuthConfig::Bearer(token) => {
                    headers.insert("Authorization", format!("Bearer {}", token).parse()
                        .map_err(|_| RTDBError::Config("Invalid token format".to_string()))?);
                }
                AuthConfig::Basic { username, password } => {
                    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                    headers.insert("Authorization", format!("Basic {}", credentials).parse()
                        .map_err(|_| RTDBError::Config("Invalid credentials format".to_string()))?);
                }
                AuthConfig::Headers(header_map) => {
                    for (key, value) in header_map {
                        let header_name = reqwest::header::HeaderName::from_str(key)
                            .map_err(|_| RTDBError::Config(format!("Invalid header name: {}", key)))?;
                        let header_value = reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| RTDBError::Config(format!("Invalid header value: {}", value)))?;
                        headers.insert(header_name, header_value);
                    }
                }
            }
            
            client_builder = client_builder.default_headers(headers);
        }
        
        let client = client_builder.build()
            .map_err(|e| RTDBError::Config(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            client,
            base_url: url.trim_end_matches('/').to_string(),
            collection: collection.unwrap_or("default").to_string(),
            current_offset: None,
        })
    }
}

#[async_trait]
impl SourceClient for QdrantSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let response = self.client.get(&url).send().await
            .map_err(|e| RTDBError::Network(format!("Failed to get collection info: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let info: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let count = info["result"]["points_count"].as_u64();
        Ok(count)
    }
    
    async fn fetch_batch(&mut self, _offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        let url = format!("{}/collections/{}/points/scroll", self.base_url, self.collection);
        
        let mut request_body = serde_json::json!({
            "limit": limit,
            "with_payload": true,
            "with_vector": true
        });
        
        if let Some(offset_id) = &self.current_offset {
            request_body["offset"] = Value::String(offset_id.clone());
        }
        
        let response = self.client.post(&url)
            .json(&request_body)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to fetch batch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let result: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let points = result["result"]["points"].as_array()
            .ok_or_else(|| RTDBError::Serialization("Invalid response format".to_string()))?;
        
        let mut records = Vec::new();
        
        for point in points {
            let id = point["id"].as_str()
                .or_else(|| point["id"].as_u64().map(|n| Box::leak(n.to_string().into_boxed_str()) as &str))
                .ok_or_else(|| RTDBError::Serialization("Missing point ID".to_string()))?;
            
            let vector = point["vector"].as_array()
                .ok_or_else(|| RTDBError::Serialization("Missing vector data".to_string()))?
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            
            let metadata = point["payload"].as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            
            records.push(VectorRecord {
                id: id.to_string(),
                vector,
                metadata,
            });
        }
        
        // Update offset for next batch
        if let Some(next_offset) = result["result"]["next_page_offset"].as_str() {
            self.current_offset = Some(next_offset.to_string());
        } else {
            self.current_offset = None;
        }
        
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            collection: self.collection.clone(),
            current_offset: self.current_offset.clone(),
        })
    }
}

/// Milvus source client
pub struct MilvusSourceClient {
    client: HttpClient,
    base_url: String,
    collection: String,
}

impl MilvusSourceClient {
    async fn new(url: &str, collection: Option<&str>, auth: Option<&AuthConfig>) -> Result<Self> {
        let mut client_builder = HttpClient::builder();
        
        if let Some(auth_config) = auth {
            let mut headers = reqwest::header::HeaderMap::new();
            
            match auth_config {
                AuthConfig::Bearer(token) => {
                    headers.insert("Authorization", format!("Bearer {}", token).parse()
                        .map_err(|_| RTDBError::Config("Invalid token format".to_string()))?);
                }
                AuthConfig::ApiKey(api_key) => {
                    headers.insert("Authorization", format!("Bearer {}", api_key).parse()
                        .map_err(|_| RTDBError::Config("Invalid API key format".to_string()))?);
                }
                AuthConfig::Basic { username, password } => {
                    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                    headers.insert("Authorization", format!("Basic {}", credentials).parse()
                        .map_err(|_| RTDBError::Config("Invalid credentials format".to_string()))?);
                }
                AuthConfig::Headers(header_map) => {
                    for (key, value) in header_map {
                        let header_name = reqwest::header::HeaderName::from_str(key)
                            .map_err(|_| RTDBError::Config(format!("Invalid header name: {}", key)))?;
                        let header_value = reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| RTDBError::Config(format!("Invalid header value: {}", value)))?;
                        headers.insert(header_name, header_value);
                    }
                }
            }
            
            client_builder = client_builder.default_headers(headers);
        }
        
        let client = client_builder.build()
            .map_err(|e| RTDBError::Config(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            client,
            base_url: url.trim_end_matches('/').to_string(),
            collection: collection.unwrap_or("default").to_string(),
        })
    }
}

#[async_trait]
impl SourceClient for MilvusSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        let url = format!("{}/v1/vector/collections/{}/entities/stats", self.base_url, self.collection);
        let response = self.client.get(&url).send().await
            .map_err(|e| RTDBError::Network(format!("Failed to get collection stats: {}", e)))?;
        
        if response.status().is_success() {
            let stats: Value = response.json().await
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
            Ok(stats["row_count"].as_u64())
        } else {
            Ok(None)
        }
    }
    
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        // Milvus uses query API for batch retrieval
        let url = format!("{}/v1/vector/query", self.base_url);
        
        let request_body = serde_json::json!({
            "collection_name": self.collection,
            "output_fields": ["*"],
            "limit": limit,
            "offset": offset,
            "expr": "" // Empty expression to get all records
        });
        
        let response = self.client.post(&url)
            .json(&request_body)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to fetch batch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let result: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let mut records = Vec::new();
        
        if let Some(data) = result.get("data") {
            if let Some(entities) = data.as_array() {
                for entity in entities {
                    if let Some(entity_obj) = entity.as_object() {
                        // Extract ID (could be in different fields)
                        let id = entity_obj.get("id")
                            .or_else(|| entity_obj.get("pk"))
                            .or_else(|| entity_obj.get("primary_key"))
                            .and_then(|v| v.as_str().or_else(|| v.as_i64().map(|i| Box::leak(i.to_string().into_boxed_str()) as &str)))
                            .unwrap_or("unknown")
                            .to_string();
                        
                        // Extract vector (try common field names)
                        let vector = entity_obj.get("vector")
                            .or_else(|| entity_obj.get("embedding"))
                            .or_else(|| entity_obj.get("embeddings"))
                            .or_else(|| entity_obj.get("vec"))
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                            .unwrap_or_default();
                        
                        // Extract metadata (all other fields)
                        let mut metadata = HashMap::new();
                        for (key, value) in entity_obj {
                            if !["id", "pk", "primary_key", "vector", "embedding", "embeddings", "vec"].contains(&key.as_str()) {
                                metadata.insert(key.clone(), value.clone());
                            }
                        }
                        
                        records.push(VectorRecord {
                            id,
                            vector,
                            metadata,
                        });
                    }
                }
            }
        }
        
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            collection: self.collection.clone(),
        })
    }
}

/// Weaviate source client
pub struct WeaviateSourceClient {
    client: HttpClient,
    base_url: String,
    class_name: String,
}

impl WeaviateSourceClient {
    async fn new(url: &str, class_name: Option<&str>, auth: Option<&AuthConfig>) -> Result<Self> {
        let mut client_builder = HttpClient::builder();
        
        if let Some(auth_config) = auth {
            let mut headers = reqwest::header::HeaderMap::new();
            
            match auth_config {
                AuthConfig::ApiKey(api_key) => {
                    headers.insert("X-OpenAI-Api-Key", api_key.parse()
                        .map_err(|_| RTDBError::Config("Invalid API key format".to_string()))?);
                }
                AuthConfig::Bearer(token) => {
                    headers.insert("Authorization", format!("Bearer {}", token).parse()
                        .map_err(|_| RTDBError::Config("Invalid token format".to_string()))?);
                }
                AuthConfig::Basic { username, password } => {
                    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                    headers.insert("Authorization", format!("Basic {}", credentials).parse()
                        .map_err(|_| RTDBError::Config("Invalid credentials format".to_string()))?);
                }
                AuthConfig::Headers(header_map) => {
                    for (key, value) in header_map {
                        let header_name = reqwest::header::HeaderName::from_str(key)
                            .map_err(|_| RTDBError::Config(format!("Invalid header name: {}", key)))?;
                        let header_value = reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| RTDBError::Config(format!("Invalid header value: {}", value)))?;
                        headers.insert(header_name, header_value);
                    }
                }
            }
            
            client_builder = client_builder.default_headers(headers);
        }
        
        let client = client_builder.build()
            .map_err(|e| RTDBError::Config(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            client,
            base_url: url.trim_end_matches('/').to_string(),
            class_name: class_name.unwrap_or("Document").to_string(),
        })
    }
}

#[async_trait]
impl SourceClient for WeaviateSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        let query = format!(r#"
        {{
            Aggregate {{
                {}(groupBy: []) {{
                    meta {{
                        count
                    }}
                }}
            }}
        }}
        "#, self.class_name);
        
        let url = format!("{}/v1/graphql", self.base_url);
        let response = self.client.post(&url)
            .json(&serde_json::json!({"query": query}))
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to get count: {}", e)))?;
        
        if response.status().is_success() {
            let result: Value = response.json().await
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
            
            let count = result["data"]["Aggregate"][&self.class_name][0]["meta"]["count"].as_u64();
            Ok(count)
        } else {
            Ok(None)
        }
    }
    
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        let query = format!(r#"
        {{
            Get {{
                {}(limit: {}, offset: {}) {{
                    _additional {{
                        id
                        vector
                    }}
                    # Add other fields as needed
                }}
            }}
        }}
        "#, self.class_name, limit, offset);
        
        let url = format!("{}/v1/graphql", self.base_url);
        let response = self.client.post(&url)
            .json(&serde_json::json!({"query": query}))
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to fetch batch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let result: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let mut records = Vec::new();
        
        if let Some(objects) = result["data"]["Get"][&self.class_name].as_array() {
            for obj in objects {
                if let Some(additional) = obj["_additional"].as_object() {
                    let id = additional["id"].as_str()
                        .ok_or_else(|| RTDBError::Serialization("Missing object ID".to_string()))?;
                    
                    let vector = additional["vector"].as_array()
                        .ok_or_else(|| RTDBError::Serialization("Missing vector data".to_string()))?
                        .iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect();
                    
                    // Extract other properties as metadata
                    let mut metadata = HashMap::new();
                    for (key, value) in obj.as_object().unwrap_or(&serde_json::Map::new()) {
                        if key != "_additional" {
                            metadata.insert(key.clone(), value.clone());
                        }
                    }
                    
                    records.push(VectorRecord {
                        id: id.to_string(),
                        vector,
                        metadata,
                    });
                }
            }
        }
        
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            class_name: self.class_name.clone(),
        })
    }
}

/// Pinecone source client (placeholder)
pub struct PineconeSourceClient {
    client: HttpClient,
    base_url: String,
    index_name: String,
}

impl PineconeSourceClient {
    async fn new(url: &str, index_name: Option<&str>, auth: Option<&AuthConfig>) -> Result<Self> {
        let mut client_builder = HttpClient::builder();
        
        if let Some(auth_config) = auth {
            let mut headers = reqwest::header::HeaderMap::new();
            
            match auth_config {
                AuthConfig::ApiKey(api_key) => {
                    headers.insert("Api-Key", api_key.parse()
                        .map_err(|_| RTDBError::Config("Invalid API key format".to_string()))?);
                }
                AuthConfig::Bearer(token) => {
                    headers.insert("Authorization", format!("Bearer {}", token).parse()
                        .map_err(|_| RTDBError::Config("Invalid token format".to_string()))?);
                }
                AuthConfig::Basic { username, password } => {
                    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                    headers.insert("Authorization", format!("Basic {}", credentials).parse()
                        .map_err(|_| RTDBError::Config("Invalid credentials format".to_string()))?);
                }
                AuthConfig::Headers(header_map) => {
                    for (key, value) in header_map {
                        let header_name = reqwest::header::HeaderName::from_str(key)
                            .map_err(|_| RTDBError::Config(format!("Invalid header name: {}", key)))?;
                        let header_value = reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| RTDBError::Config(format!("Invalid header value: {}", value)))?;
                        headers.insert(header_name, header_value);
                    }
                }
            }
            
            client_builder = client_builder.default_headers(headers);
        }
        
        let client = client_builder.build()
            .map_err(|e| RTDBError::Config(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            client,
            base_url: url.to_string(),
            index_name: index_name.unwrap_or("default").to_string(),
        })
    }
}

#[async_trait]
impl SourceClient for PineconeSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        // Pinecone doesn't provide easy count API
        Ok(None)
    }
    
    async fn fetch_batch(&mut self, _offset: u64, _limit: usize) -> Result<Vec<VectorRecord>> {
        // Pinecone uses list/query operations for batch retrieval
        let url = format!("{}/query", self.base_url);
        
        let request_body = serde_json::json!({
            "vector": vec![0.0; 1536], // Dummy vector for query
            "topK": _limit,
            "includeValues": true,
            "includeMetadata": true
        });
        
        let response = self.client.post(&url)
            .json(&request_body)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to fetch batch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let result: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let mut records = Vec::new();
        
        if let Some(matches) = result.get("matches").and_then(|v| v.as_array()) {
            for match_obj in matches {
                if let Some(match_data) = match_obj.as_object() {
                    let id = match_data.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    
                    let vector = match_data.get("values")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                        .unwrap_or_default();
                    
                    let metadata = match_data.get("metadata")
                        .and_then(|v| v.as_object())
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();
                    
                    records.push(VectorRecord {
                        id,
                        vector,
                        metadata,
                    });
                }
            }
        }
        
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            index_name: self.index_name.clone(),
        })
    }
}

/// LanceDB source client (file-based)
pub struct LanceDBSourceClient {
    path: String,
    table_name: String,
    current_offset: u64,
}

impl LanceDBSourceClient {
    async fn new(path: &str, table_name: Option<&str>) -> Result<Self> {
        // Verify path exists
        if !std::path::Path::new(path).exists() {
            return Err(RTDBError::Config(format!("LanceDB path does not exist: {}", path)));
        }
        
        Ok(Self {
            path: path.to_string(),
            table_name: table_name.unwrap_or("vectors").to_string(),
            current_offset: 0,
        })
    }
}

#[async_trait]
impl SourceClient for LanceDBSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        // For LanceDB, we would need to read parquet metadata or scan the directory
        // This is a simplified implementation
        let table_path = std::path::Path::new(&self.path).join(&self.table_name);
        
        if table_path.exists() {
            // Try to estimate from directory size or file count
            let metadata = tokio::fs::metadata(&table_path).await
                .map_err(|e| RTDBError::Io(format!("Failed to read table metadata: {}", e)))?;
            
            // Rough estimate based on file size (assuming ~1KB per vector)
            let estimated_count = metadata.len() / 1024;
            Ok(Some(estimated_count))
        } else {
            Ok(None)
        }
    }
    
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        // For a real implementation, this would use the LanceDB Rust SDK
        // or read parquet files directly. For now, we'll simulate reading
        // from a JSONL file in the LanceDB directory
        
        let jsonl_path = std::path::Path::new(&self.path)
            .join(&self.table_name)
            .with_extension("jsonl");
        
        if !jsonl_path.exists() {
            tracing::warn!("LanceDB JSONL file not found: {:?}", jsonl_path);
            return Ok(Vec::new());
        }
        
        let file = File::open(&jsonl_path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open LanceDB file: {}", e)))?;
        
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let mut records = Vec::new();
        let mut current_line = 0u64;
        
        // Skip to offset
        while current_line < offset {
            line.clear();
            if reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? == 0 {
                break; // EOF
            }
            current_line += 1;
        }
        
        // Read batch
        for _ in 0..limit {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
            
            if bytes_read == 0 {
                break; // EOF
            }
            
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            let record: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse JSON line: {}", e)))?;
            
            if let Some(obj) = record.as_object() {
                let id = obj.get("id")
                    .or_else(|| obj.get("_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                
                let vector = obj.get("vector")
                    .or_else(|| obj.get("embedding"))
                    .or_else(|| obj.get("embeddings"))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                    .unwrap_or_default();
                
                let mut metadata = HashMap::new();
                for (key, value) in obj {
                    if !["id", "_id", "vector", "embedding", "embeddings"].contains(&key.as_str()) {
                        metadata.insert(key.clone(), value.clone());
                    }
                }
                
                records.push(VectorRecord {
                    id,
                    vector,
                    metadata,
                });
            }
        }
        
        self.current_offset = offset + records.len() as u64;
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            path: self.path.clone(),
            table_name: self.table_name.clone(),
            current_offset: self.current_offset,
        })
    }
}

/// JSONL source client
pub struct JsonlSourceClient {
    path: String,
    current_offset: u64,
}

impl JsonlSourceClient {
    async fn new(path: &str) -> Result<Self> {
        // Verify file exists
        if !std::path::Path::new(path).exists() {
            return Err(RTDBError::Config(format!("JSONL file does not exist: {}", path)));
        }
        
        Ok(Self { 
            path: path.to_string(),
            current_offset: 0,
        })
    }
    
    async fn count_lines(&self) -> Result<u64> {
        let file = File::open(&self.path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open file for counting: {}", e)))?;
        
        let mut reader = BufReader::new(file);
        let mut count = 0u64;
        let mut line = String::new();
        
        while reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? > 0 {
            count += 1;
            line.clear();
        }
        
        Ok(count)
    }
}

#[async_trait]
impl SourceClient for JsonlSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        match self.count_lines().await {
            Ok(count) => Ok(Some(count)),
            Err(_) => Ok(None), // Don't fail if we can't count
        }
    }
    
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        let file = File::open(&self.path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open JSONL file: {}", e)))?;
        
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let mut records = Vec::new();
        let mut current_line = 0u64;
        
        // Skip to offset
        while current_line < offset {
            line.clear();
            if reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? == 0 {
                break; // EOF
            }
            current_line += 1;
        }
        
        // Read batch
        for _ in 0..limit {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
            
            if bytes_read == 0 {
                break; // EOF
            }
            
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            let record: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse JSON line {}: {}", current_line + 1, e)))?;
            
            if let Some(obj) = record.as_object() {
                let id = obj.get("id")
                    .or_else(|| obj.get("_id"))
                    .and_then(|v| v.as_str().or_else(|| v.as_i64().map(|i| Box::leak(i.to_string().into_boxed_str()) as &str)))
                    .unwrap_or("unknown")
                    .to_string();
                
                let vector = obj.get("vector")
                    .or_else(|| obj.get("embedding"))
                    .or_else(|| obj.get("embeddings"))
                    .or_else(|| obj.get("vec"))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                    .unwrap_or_default();
                
                let mut metadata = HashMap::new();
                for (key, value) in obj {
                    if !["id", "_id", "vector", "embedding", "embeddings", "vec"].contains(&key.as_str()) {
                        metadata.insert(key.clone(), value.clone());
                    }
                }
                
                records.push(VectorRecord {
                    id,
                    vector,
                    metadata,
                });
            }
            
            current_line += 1;
        }
        
        self.current_offset = current_line;
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self { 
            path: self.path.clone(),
            current_offset: self.current_offset,
        })
    }
}

/// Parquet source client with Send/Sync workaround
pub struct ParquetSourceClient {
    path: String,
    total_rows: Option<u64>,
    #[allow(dead_code)]
    current_offset: u64,
}

impl ParquetSourceClient {
    /// Create a new Parquet source client for reading from a Parquet file.
    /// 
    /// # Arguments
    /// * `path` - Path to the Parquet file to read from
    /// 
    /// # Returns
    /// A new ParquetSourceClient instance
    pub async fn new(path: &str) -> Result<Self> {
        // Get file metadata without creating the stream (which isn't Send/Sync)
        let file = tokio::fs::File::open(path).await
            .map_err(|e| RTDBError::Migration(format!("Failed to open Parquet file: {}", e)))?;
        
        let builder = parquet::arrow::ParquetRecordBatchStreamBuilder::new(file).await
            .map_err(|e| RTDBError::Migration(format!("Failed to read Parquet metadata: {}", e)))?;
        
        let total_rows = Some(builder.metadata().file_metadata().num_rows() as u64);
        
        tracing::info!("Initialized Parquet source: {} ({} rows)", path, total_rows.unwrap_or(0));
        
        Ok(Self {
            path: path.to_string(),
            total_rows,
            current_offset: 0,
        })
    }
    
    // Create a new stream each time to avoid Send/Sync issues
    async fn create_stream(&self) -> Result<parquet::arrow::async_reader::ParquetRecordBatchStream<tokio::fs::File>> {
        let file = tokio::fs::File::open(&self.path).await
            .map_err(|e| RTDBError::Migration(format!("Failed to open Parquet file: {}", e)))?;
        
        let builder = parquet::arrow::ParquetRecordBatchStreamBuilder::new(file).await
            .map_err(|e| RTDBError::Migration(format!("Failed to create stream builder: {}", e)))?;
        
        let stream = builder
            .with_batch_size(1000)
            .build()
            .map_err(|e| RTDBError::Migration(format!("Failed to build stream: {}", e)))?;
        
        Ok(stream)
    }
}

#[async_trait]
impl SourceClient for ParquetSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        Ok(self.total_rows)
    }
    
    async fn fetch_batch(&mut self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        use futures::StreamExt;
        
        // Create a fresh stream each time to avoid Send/Sync issues
        let mut stream = self.create_stream().await?;
        let mut records = Vec::new();
        let mut current_pos = 0u64;
        
        // Skip to the desired offset
        while current_pos < offset {
            match stream.next().await {
                Some(Ok(batch)) => {
                    let batch_size = batch.num_rows() as u64;
                    if current_pos + batch_size <= offset {
                        // Skip entire batch
                        current_pos += batch_size;
                    } else {
                        // Partial skip within batch
                        let skip_in_batch = (offset - current_pos) as usize;
                        let batch_records = self.convert_batch_to_records(&batch)?;
                        
                        // Take remaining records from this batch
                        let remaining: Vec<VectorRecord> = batch_records.into_iter()
                            .skip(skip_in_batch)
                            .take(limit)
                            .collect();
                        
                        return Ok(remaining);
                    }
                }
                Some(Err(e)) => return Err(RTDBError::Migration(format!("Parquet stream error: {}", e))),
                None => break, // End of stream
            }
        }
        
        // Fetch the requested batch
        let mut collected = 0;
        while collected < limit {
            match stream.next().await {
                Some(Ok(batch)) => {
                    let batch_records = self.convert_batch_to_records(&batch)?;
                    let remaining_needed = limit - collected;
                    let to_take = std::cmp::min(batch_records.len(), remaining_needed);
                    
                    records.extend(batch_records.into_iter().take(to_take));
                    collected += to_take;
                }
                Some(Err(e)) => return Err(RTDBError::Migration(format!("Parquet stream error: {}", e))),
                None => break, // End of stream
            }
        }
        
        Ok(records)
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self {
            path: self.path.clone(),
            total_rows: self.total_rows,
            current_offset: 0,
        })
    }
}

impl ParquetSourceClient {
    fn convert_batch_to_records(&self, batch: &arrow::array::RecordBatch) -> Result<Vec<VectorRecord>> {
        use arrow::array::{StringArray, ListArray, Float32Array, Array};
        use std::collections::HashMap;
        
        let mut records = Vec::new();
        let num_rows = batch.num_rows();
        
        // Extract columns
        let id_column = batch.column_by_name("id")
            .ok_or_else(|| RTDBError::Migration("Missing 'id' column in Parquet".to_string()))?;
        let vector_column = batch.column_by_name("vector")
            .or_else(|| batch.column_by_name("embedding"))
            .or_else(|| batch.column_by_name("embeddings"))
            .or_else(|| batch.column_by_name("vec"))
            .ok_or_else(|| RTDBError::Migration("Missing vector column".to_string()))?;
        let metadata_column = batch.column_by_name("metadata");
        
        let id_array = id_column.as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| RTDBError::Migration("Invalid 'id' column type".to_string()))?;
        let vector_array = vector_column.as_any().downcast_ref::<ListArray>()
            .ok_or_else(|| RTDBError::Migration("Invalid 'vector' column type".to_string()))?;
        let metadata_array = metadata_column.and_then(|col| 
            col.as_any().downcast_ref::<StringArray>()
        );
        
        for i in 0..num_rows {
            let id = id_array.value(i).to_string();
            
            // Extract vector
            let vector_list = vector_array.value(i);
            let float_array = vector_list.as_any().downcast_ref::<Float32Array>()
                .ok_or_else(|| RTDBError::Migration("Invalid vector data type".to_string()))?;
            let vector: Vec<f32> = (0..float_array.len())
                .map(|j| float_array.value(j))
                .collect();
            
            // Extract metadata
            let metadata = if let Some(meta_array) = metadata_array {
                if !meta_array.is_null(i) {
                    let meta_str = meta_array.value(i);
                    serde_json::from_str(meta_str).unwrap_or_default()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            };
            
            records.push(VectorRecord {
                id,
                vector,
                metadata,
            });
        }
        
        Ok(records)
    }
}

/// HDF5 source client (placeholder)
pub struct Hdf5SourceClient {
    path: String,
}

impl Hdf5SourceClient {
    async fn new(path: &str) -> Result<Self> {
        Ok(Self { path: path.to_string() })
    }
}

#[async_trait]
impl SourceClient for Hdf5SourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        Ok(None)
    }
    
    async fn fetch_batch(&mut self, _offset: u64, _limit: usize) -> Result<Vec<VectorRecord>> {
        tracing::warn!("HDF5 migration not fully implemented");
        Ok(Vec::new())
    }
    
    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self { path: self.path.clone() })
    }
}

/// Parquet target client for exporting data to Parquet format
/// Note: This client is not used through the TargetClient trait due to Send/Sync issues
/// with the underlying Parquet writer. Use ParquetExporter directly instead.
pub struct ParquetTargetClient {
    path: String,
    writer: Option<crate::migration::parquet_streaming::ParquetStreamWriter>,
    #[allow(dead_code)]
    collection_name: String,
}

impl ParquetTargetClient {
    /// Create a new Parquet target client for writing to a Parquet file.
    /// 
    /// # Arguments
    /// * `path` - Path where the Parquet file will be written
    /// 
    /// # Returns
    /// A new ParquetTargetClient instance
    pub async fn new(path: &str) -> Result<Self> {
        tracing::info!("Initialized Parquet target: {}", path);
        
        Ok(Self {
            path: path.to_string(),
            writer: None,
            collection_name: "default".to_string(),
        })
    }
    
    /// Ensure the Parquet writer is initialized and return a mutable reference to it.
    /// 
    /// # Returns
    /// A mutable reference to the ParquetStreamWriter
    pub async fn ensure_writer(&mut self) -> Result<&mut crate::migration::parquet_streaming::ParquetStreamWriter> {
        if self.writer.is_none() {
            use crate::migration::parquet_streaming::{ParquetStreamConfig, ParquetStreamWriter};
            use tokio::time::Duration;
            use parquet::basic::Compression;
            use parquet::file::properties::EnabledStatistics;
            
            let config = ParquetStreamConfig {
                batch_size: 1000,
                row_group_size: 10000,
                compression: Compression::SNAPPY,
                dictionary_enabled: true,
                statistics_enabled: EnabledStatistics::Chunk,
                buffer_size: 8192,
                operation_timeout: Duration::from_secs(300),
                max_memory_usage: 512 * 1024 * 1024, // 512MB
            };
            
            let writer = ParquetStreamWriter::new(std::path::Path::new(&self.path), config).await?;
            self.writer = Some(writer);
        }
        
        Ok(self.writer.as_mut().unwrap())
    }
    
    /// Write vector records to the Parquet file.
    /// 
    /// # Arguments
    /// * `records` - Slice of vector records to write
    /// 
    /// # Returns
    /// Result indicating success or failure
    pub async fn write_records(&mut self, records: &[VectorRecord]) -> Result<()> {
        let writer = self.ensure_writer().await?;
        writer.write_records(records).await
    }
    
    /// Finalize the Parquet file and flush all remaining data.
    /// 
    /// # Returns
    /// Result indicating success or failure
    pub async fn finalize(mut self) -> Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.finalize().await?;
        }
        Ok(())
    }
}

/// RTDB target client
pub struct RTDBTargetClient {
    client: HttpClient,
    base_url: String,
}

impl RTDBTargetClient {
    async fn new(url: &str, auth: Option<&AuthConfig>) -> Result<Self> {
        use std::str::FromStr;
        let mut client_builder = reqwest::Client::builder();
        
        if let Some(auth_config) = auth {
            let mut headers = reqwest::header::HeaderMap::new();
            
            match auth_config {
                AuthConfig::ApiKey(api_key) => {
                    headers.insert("api-key", api_key.parse()
                        .map_err(|_| RTDBError::Config("Invalid API key format".to_string()))?);
                }
                AuthConfig::Bearer(token) => {
                    headers.insert("authorization", format!("Bearer {}", token).parse()
                        .map_err(|_| RTDBError::Config("Invalid bearer token format".to_string()))?);
                }
                AuthConfig::Basic { username, password } => {
                    let credentials = general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                    headers.insert("authorization", format!("Basic {}", credentials).parse()
                        .map_err(|_| RTDBError::Config("Invalid basic auth format".to_string()))?);
                }
                AuthConfig::Headers(header_map) => {
                    for (key, value) in header_map {
                        let header_name = reqwest::header::HeaderName::from_str(key)
                            .map_err(|_| RTDBError::Config(format!("Invalid header name: {}", key)))?;
                        let header_value = reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| RTDBError::Config(format!("Invalid header value: {}", value)))?;
                        headers.insert(header_name, header_value);
                    }
                }
            }
            
            client_builder = client_builder.default_headers(headers);
        }
        
        let client = client_builder.build()
            .map_err(|e| RTDBError::Config(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            client,
            base_url: url.trim_end_matches('/').to_string(),
        })
    }
}

#[async_trait]
impl TargetClient for RTDBTargetClient {
    async fn insert_batch(&self, records: &[VectorRecord]) -> Result<()> {
        let url = format!("{}/collections/{{collection_name}}/points", self.base_url);
        
        let points: Vec<Value> = records.iter().map(|record| {
            serde_json::json!({
                "id": record.id,
                "vector": record.vector,
                "payload": record.metadata
            })
        }).collect();
        
        let request_body = serde_json::json!({
            "points": points
        });
        
        let response = self.client.put(&url)
            .json(&request_body)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to insert batch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        Ok(())
    }
    
    async fn ensure_collection(&self, collection_name: &str, dimension: usize) -> Result<()> {
        let url = format!("{}/collections/{}", self.base_url, collection_name);
        
        let collection_config = serde_json::json!({
            "vectors": {
                "size": dimension,
                "distance": "Cosine"
            }
        });
        
        let response = self.client.put(&url)
            .json(&collection_config)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to create collection: {}", e)))?;
        
        if !response.status().is_success() && response.status().as_u16() != 409 {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        Ok(())
    }
    
    async fn get_collection_info(&self, collection_name: &str) -> Result<Option<CollectionInfo>> {
        let url = format!("{}/collections/{}", self.base_url, collection_name);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| RTDBError::Network(format!("Failed to get collection info: {}", e)))?;
        
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        
        if !response.status().is_success() {
            return Err(RTDBError::Network(format!("HTTP error: {}", response.status())));
        }
        
        let info: Value = response.json().await
            .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
        
        let result = info["result"].as_object()
            .ok_or_else(|| RTDBError::Serialization("Invalid response format".to_string()))?;
        
        let config = result["config"].as_object()
            .ok_or_else(|| RTDBError::Serialization("Missing config in response".to_string()))?;
        
        let vectors_config = config["params"]["vectors"].as_object()
            .ok_or_else(|| RTDBError::Serialization("Missing vectors config".to_string()))?;
        
        let dimension = vectors_config["size"].as_u64()
            .ok_or_else(|| RTDBError::Serialization("Missing vector size".to_string()))? as usize;
        
        let distance_metric = vectors_config["distance"].as_str()
            .unwrap_or("Cosine").to_string();
        
        let vector_count = result["points_count"].as_u64().unwrap_or(0);
        
        Ok(Some(CollectionInfo {
            name: collection_name.to_string(),
            dimension,
            vector_count,
            distance_metric,
        }))
    }
    
    /// Get the total count of vectors in the collection.
    /// 
    /// # Returns
    /// Optional total count (None if not available)
    async fn get_total_count(&self) -> Result<Option<u64>> {
        // This would typically query the collection info to get the total count
        // For now, return None to indicate count is not available
        Ok(None)
    }
    
    async fn fetch_batch(&self, offset: u64, limit: usize) -> Result<Vec<VectorRecord>> {
        // This would typically implement pagination to fetch records
        // For now, return empty vector as this is primarily used for consistency verification
        let _ = (offset, limit); // Suppress unused warnings
        Ok(Vec::new())
    }
    
    fn clone_box(&self) -> Box<dyn TargetClient> {
        Box::new(Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
        })
    }
}

/// CSV source client
pub struct CsvSourceClient {
    path: String,
}

impl CsvSourceClient {
    /// Create a new CSV source client for reading from a CSV file.
    /// 
    /// # Arguments
    /// * `path` - Path to the CSV file to read from
    /// 
    /// # Returns
    /// A new CsvSourceClient instance
    pub async fn new(path: &str) -> Result<Self> {
        Ok(Self { path: path.to_string() })
    }
}

#[async_trait]
impl SourceClient for CsvSourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        // TODO: Implement CSV record counting
        Ok(None)
    }

    async fn fetch_batch(&mut self, _offset: u64, _limit: usize) -> Result<Vec<VectorRecord>> {
        // TODO: Implement CSV reading
        Ok(Vec::new())
    }

    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self { path: self.path.clone() })
    }
}

/// Binary source client
pub struct BinarySourceClient {
    path: String,
}

impl BinarySourceClient {
    /// Create a new binary source client
    pub async fn new(path: &str) -> Result<Self> {
        Ok(Self { path: path.to_string() })
    }
}

#[async_trait]
impl SourceClient for BinarySourceClient {
    async fn get_total_count(&mut self) -> Result<Option<u64>> {
        // TODO: Implement binary record counting
        Ok(None)
    }

    async fn fetch_batch(&mut self, _offset: u64, _limit: usize) -> Result<Vec<VectorRecord>> {
        // TODO: Implement binary reading
        Ok(Vec::new())
    }

    fn clone_box(&self) -> Box<dyn SourceClient> {
        Box::new(Self { path: self.path.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_info() {
        let info = CollectionInfo {
            name: "test".to_string(),
            dimension: 128,
            vector_count: 1000,
            distance_metric: "Cosine".to_string(),
        };
        
        assert_eq!(info.name, "test");
        assert_eq!(info.dimension, 128);
        assert_eq!(info.vector_count, 1000);
    }
}
