use crate::{RTDBConfig, RTDBError, RTDBResult};
use crate::types::{Vector, SearchRequest, SearchResponse, Collection, CollectionInfo};
use crate::resilience::CircuitBreakerClient;
use crate::metrics::ClientMetrics;
use reqwest::{Client, Response};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error, instrument};
use uuid::Uuid;

/// High-performance RTDB client with production-grade features
#[derive(Clone)]
pub struct RTDBClient {
    config: RTDBConfig,
    http_client: Client,
    circuit_breaker: Arc<CircuitBreakerClient>,
    metrics: Arc<ClientMetrics>,
    base_url: String,
}

impl RTDBClient {
    /// Create a new RTDB client with the given configuration
    #[instrument(skip(config))]
    pub async fn new(config: RTDBConfig) -> RTDBResult<Self> {
        info!("Initializing RTDB client with endpoint: {}", config.endpoint);
        
        let http_client = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.max_idle_connections)
            .pool_idle_timeout(config.idle_timeout)
            .user_agent(format!("rtdb-rust-client/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| RTDBError::ClientInitialization(e.to_string()))?;
        
        let circuit_breaker = Arc::new(CircuitBreakerClient::new(config.circuit_breaker_config.clone()));
        let metrics = Arc::new(ClientMetrics::new());
        
        let client = Self {
            base_url: config.endpoint.clone(),
            config,
            http_client,
            circuit_breaker,
            metrics,
        };
        
        // Health check
        client.health_check().await?;
        
        info!("RTDB client initialized successfully");
        Ok(client)
    }

    /// Perform a health check against the RTDB server
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> RTDBResult<()> {
        let start = Instant::now();
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .get(&format!("{}/health", self.base_url))
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("health_check", latency);
        
        if response.status().is_success() {
            debug!("Health check successful in {:?}", latency);
            Ok(())
        } else {
            let error = RTDBError::ServerError(response.status().as_u16(), "Health check failed".to_string());
            self.metrics.record_error("health_check");
            Err(error)
        }
    }

    /// Create a new collection
    #[instrument(skip(self))]
    pub async fn create_collection(&self, name: &str, dimension: usize) -> RTDBResult<Collection> {
        let start = Instant::now();
        
        let request_body = serde_json::json!({
            "name": name,
            "config": {
                "params": {
                    "vectors": {
                        "size": dimension,
                        "distance": "Cosine"
                    }
                }
            }
        });
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .put(&format!("{}/collections/{}", self.base_url, name))
                .json(&request_body)
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("create_collection", latency);
        
        if response.status().is_success() {
            let collection: Collection = response.json().await
                .map_err(|e| RTDBError::DeserializationError(e.to_string()))?;
            
            info!("Created collection '{}' with dimension {}", name, dimension);
            Ok(collection)
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("create_collection");
            Err(error)
        }
    }

    /// List all collections
    #[instrument(skip(self))]
    pub async fn list_collections(&self) -> RTDBResult<Vec<CollectionInfo>> {
        let start = Instant::now();
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .get(&format!("{}/collections", self.base_url))
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("list_collections", latency);
        
        if response.status().is_success() {
            let collections: Vec<CollectionInfo> = response.json().await
                .map_err(|e| RTDBError::DeserializationError(e.to_string()))?;
            
            debug!("Listed {} collections", collections.len());
            Ok(collections)
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("list_collections");
            Err(error)
        }
    }
    /// Insert vectors into a collection
    #[instrument(skip(self, vectors))]
    pub async fn insert_vectors(&self, collection: &str, vectors: Vec<Vector>) -> RTDBResult<()> {
        let start = Instant::now();
        let vector_count = vectors.len();
        
        let request_body = serde_json::json!({
            "points": vectors.into_iter().map(|v| serde_json::json!({
                "id": v.id,
                "vector": v.vector,
                "payload": v.metadata
            })).collect::<Vec<_>>()
        });
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .put(&format!("{}/collections/{}/points", self.base_url, collection))
                .json(&request_body)
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("insert_vectors", latency);
        self.metrics.record_vectors_processed("insert", vector_count);
        
        if response.status().is_success() {
            info!("Inserted {} vectors into collection '{}'", vector_count, collection);
            Ok(())
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("insert_vectors");
            Err(error)
        }
    }

    /// Search for similar vectors
    #[instrument(skip(self, query_vector))]
    pub async fn search(
        &self,
        collection: &str,
        query_vector: Vec<f32>,
        limit: usize,
    ) -> RTDBResult<SearchResponse> {
        let search_request = SearchRequest {
            vector: query_vector,
            limit,
            filter: None,
            with_payload: true,
            with_vector: false,
        };
        
        self.search_with_request(collection, search_request).await
    }

    /// Search with detailed request parameters
    #[instrument(skip(self, request))]
    pub async fn search_with_request(
        &self,
        collection: &str,
        request: SearchRequest,
    ) -> RTDBResult<SearchResponse> {
        let start = Instant::now();
        
        let request_body = serde_json::json!({
            "vector": request.vector,
            "limit": request.limit,
            "filter": request.filter,
            "with_payload": request.with_payload,
            "with_vector": request.with_vector
        });
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .post(&format!("{}/collections/{}/points/search", self.base_url, collection))
                .json(&request_body)
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("search", latency);
        
        if response.status().is_success() {
            let search_response: SearchResponse = response.json().await
                .map_err(|e| RTDBError::DeserializationError(e.to_string()))?;
            
            debug!("Search completed in {:?}, found {} results", latency, search_response.results.len());
            self.metrics.record_search_results(search_response.results.len());
            
            Ok(search_response)
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("search");
            Err(error)
        }
    }

    /// Batch search for multiple queries
    #[instrument(skip(self, queries))]
    pub async fn batch_search(
        &self,
        collection: &str,
        queries: Vec<Vec<f32>>,
        limit: usize,
    ) -> RTDBResult<Vec<SearchResponse>> {
        let start = Instant::now();
        let query_count = queries.len();
        
        // Process queries in parallel batches for optimal performance
        let batch_size = self.config.batch_size;
        let mut results = Vec::with_capacity(query_count);
        
        for chunk in queries.chunks(batch_size) {
            let batch_futures: Vec<_> = chunk.iter().map(|query| {
                self.search(collection, query.clone(), limit)
            }).collect();
            
            let batch_results = futures::future::try_join_all(batch_futures).await?;
            results.extend(batch_results);
        }
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("batch_search", latency);
        self.metrics.record_batch_queries(query_count);
        
        info!("Batch search completed: {} queries in {:?}", query_count, latency);
        Ok(results)
    }

    /// Delete vectors by IDs
    #[instrument(skip(self, ids))]
    pub async fn delete_vectors(&self, collection: &str, ids: Vec<String>) -> RTDBResult<()> {
        let start = Instant::now();
        let id_count = ids.len();
        
        let request_body = serde_json::json!({
            "points": ids
        });
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .post(&format!("{}/collections/{}/points/delete", self.base_url, collection))
                .json(&request_body)
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("delete_vectors", latency);
        
        if response.status().is_success() {
            info!("Deleted {} vectors from collection '{}'", id_count, collection);
            Ok(())
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("delete_vectors");
            Err(error)
        }
    }

    /// Delete a collection
    #[instrument(skip(self))]
    pub async fn delete_collection(&self, name: &str) -> RTDBResult<()> {
        let start = Instant::now();
        
        let response = self.circuit_breaker.call(|| async {
            self.http_client
                .delete(&format!("{}/collections/{}", self.base_url, name))
                .send()
                .await
        }).await?;
        
        let latency = start.elapsed();
        self.metrics.record_request_latency("delete_collection", latency);
        
        if response.status().is_success() {
            info!("Deleted collection '{}'", name);
            Ok(())
        } else {
            let status_code = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let error = RTDBError::ServerError(status_code, error_text);
            self.metrics.record_error("delete_collection");
            Err(error)
        }
    }

    /// Get client metrics
    pub fn get_metrics(&self) -> &ClientMetrics {
        &self.metrics
    }

    /// Get client configuration
    pub fn get_config(&self) -> &RTDBConfig {
        &self.config
    }
}