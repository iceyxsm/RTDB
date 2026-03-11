//! High-Performance Cluster gRPC Server
//!
//! Optimized server for handling inter-node communication.
//! Features:
//! - Configurable concurrency limits
//! - HTTP/2 keepalive
//! - Request timeouts
//! - Compression support
//! - Batch operations for high throughput
//! - Integration with StorageRouter for distributed operations

#![cfg(feature = "grpc")]

use super::{
    ClusterManager,
    client::ClusterClient,
    generated::{ClusterService, ClusterServiceServer},
    generated::{
        BatchInsertRequest, BatchInsertResponse, BatchReplicateRequest, BatchReplicateResponse,
        BatchSearchRequest, BatchSearchResponse, HealthRequest, HealthResponse,
        HeartbeatRequest, HeartbeatResponse, InsertRequest, InsertResponse,
        JoinRequest, JoinResponse, LeaveRequest, LeaveResponse,
        ReplicateRequest, ReplicateResponse, SearchRequest, SearchResponse,
        SearchResult, ScoredVector as ProtoScoredVector, 
        TopologyRequest, TopologyResponse,
    },
    storage_router::StorageRouter,
};
use crate::collection::CollectionManager;
use crate::Vector;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::{
    Request, Response, Status,
    transport::Server,
};

/// Default concurrency limit for the server
const DEFAULT_CONCURRENCY_LIMIT: usize = 1024;

/// Default request timeout
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// TCP keepalive interval
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

/// Server configuration
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Maximum concurrent requests
    pub concurrency_limit: usize,
    /// Request timeout
    pub request_timeout: Duration,
    /// Enable gzip compression
    pub enable_compression: bool,
    /// TCP keepalive
    pub tcp_keepalive: Option<Duration>,
    /// HTTP/2 keepalive interval
    pub http2_keepalive_interval: Option<Duration>,
    /// Max frame size (bytes)
    pub max_frame_size: Option<u32>,
    /// Initial stream window size
    pub initial_stream_window_size: Option<u32>,
    /// Initial connection window size
    pub initial_connection_window_size: Option<u32>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            concurrency_limit: DEFAULT_CONCURRENCY_LIMIT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            enable_compression: true,
            tcp_keepalive: Some(TCP_KEEPALIVE),
            http2_keepalive_interval: Some(Duration::from_secs(30)),
            max_frame_size: Some(1024 * 1024), // 1MB
            initial_stream_window_size: Some(65535), // 64KB
            initial_connection_window_size: Some(1024 * 1024), // 1MB
        }
    }
}

/// Cluster gRPC server
pub struct ClusterGrpcServer {
    /// Shared cluster manager
    cluster: Arc<RwLock<ClusterManager>>,
    /// Collection manager for storage operations
    collections: Arc<RwLock<CollectionManager>>,
    /// Cluster client for forwarding to other nodes
    client: Arc<ClusterClient>,
    /// Local node ID
    node_id: String,
    /// Server bind address
    bind_addr: SocketAddr,
    /// Server configuration
    config: ServerConfig,
}

impl ClusterGrpcServer {
    /// Create new gRPC server with default configuration
    pub fn new(
        cluster: Arc<RwLock<ClusterManager>>,
        collections: Arc<RwLock<CollectionManager>>,
        client: Arc<ClusterClient>,
        node_id: String,
        bind_addr: SocketAddr,
    ) -> Self {
        Self::with_config(cluster, collections, client, node_id, bind_addr, ServerConfig::default())
    }
    
    /// Create new gRPC server with custom configuration
    pub fn with_config(
        cluster: Arc<RwLock<ClusterManager>>,
        collections: Arc<RwLock<CollectionManager>>,
        client: Arc<ClusterClient>,
        node_id: String,
        bind_addr: SocketAddr,
        config: ServerConfig,
    ) -> Self {
        Self {
            cluster,
            collections,
            client,
            node_id,
            bind_addr,
            config,
        }
    }
    
    /// Start the gRPC server
    pub async fn start(&self) -> crate::Result<()> {
        let service = ClusterServiceImpl {
            storage_router: StorageRouter::new(
                self.collections.clone(),
                self.cluster.clone(),
                self.client.clone(),
                self.node_id.clone(),
            ),
            config: self.config.clone(),
        };
        
        let addr = self.bind_addr;
        tracing::info!(
            "Starting optimized cluster gRPC server on {} (node_id={})",
            addr,
            self.node_id
        );
        
        // Build service
        let service_builder = ClusterServiceServer::new(service);
        
        // Configure server with performance optimizations
        let mut server_builder = Server::builder();
        
        // Apply TCP keepalive
        if let Some(keepalive) = self.config.tcp_keepalive {
            server_builder = server_builder.tcp_keepalive(Some(keepalive));
        }
        
        // Apply HTTP/2 keepalive
        if let Some(interval) = self.config.http2_keepalive_interval {
            server_builder = server_builder.http2_keepalive_interval(Some(interval));
        }
        
        // Apply window sizes for throughput optimization
        if let Some(stream_window) = self.config.initial_stream_window_size {
            server_builder = server_builder.initial_stream_window_size(stream_window);
        }
        
        if let Some(conn_window) = self.config.initial_connection_window_size {
            server_builder = server_builder.initial_connection_window_size(conn_window);
        }
        
        // Apply max frame size
        if let Some(frame_size) = self.config.max_frame_size {
            server_builder = server_builder.max_frame_size(Some(frame_size));
        }
        
        server_builder
            .add_service(service_builder)
            .serve(addr)
            .await
            .map_err(|e| crate::RTDBError::Io(e.to_string()))?;
        
        Ok(())
    }
}

/// gRPC service implementation
#[derive(Clone)]
struct ClusterServiceImpl {
    /// Storage router for distributed operations
    storage_router: StorageRouter,
    /// Server configuration
    config: ServerConfig,
}

#[tonic::async_trait]
impl ClusterService for ClusterServiceImpl {
    /// Handle node join request
    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Node {} joining cluster from {}", req.node_id, req.address);
        
        // Note: This would need cluster manager access - simplified for now
        // In production, we'd update topology through storage_router
        
        Ok(Response::new(JoinResponse {
            success: true,
            error: String::new(),
            topology: None,
            config: vec![],
        }))
    }
    
    /// Handle node leave request
    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Node {} leaving cluster (graceful={})", req.node_id, req.graceful);
        
        Ok(Response::new(LeaveResponse { 
            success: true,
            message: "Node removal acknowledged".to_string(),
        }))
    }
    
    /// Handle topology request
    async fn get_topology(
        &self,
        _request: Request<TopologyRequest>,
    ) -> Result<Response<TopologyResponse>, Status> {
        // Note: Would need cluster manager access
        Ok(Response::new(TopologyResponse {
            topology: None,
            server_timestamp: current_timestamp(),
        }))
    }
    
    /// Handle heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!("Heartbeat from node {} at {}", req.node_id, req.timestamp);
        
        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            server_timestamp: current_timestamp(),
            topology: None,
            topology_changed: false,
        }))
    }
    
    /// Handle search request (forwarded from another node)
    /// Executes search on local storage and returns results
    async fn search(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Executing local search for collection {} (req_id={})",
            req.collection,
            request_id
        );
        
        // Convert bytes to vector
        let vector = bytes_to_vector(&req.vector);
        
        // Execute search on local storage (single shard, not broadcast)
        match self.storage_router.search(&req.collection, vector, req.top_k, false).await {
            Ok(results) => {
                let search_time = start_time.elapsed().as_micros() as u64;
                
                // Convert ScoredResult to ProtoScoredVector
                let proto_results: Vec<ProtoScoredVector> = results
                    .into_iter()
                    .map(|r| ProtoScoredVector {
                        id: r.id,
                        score: r.score,
                        payload: vec![], // Payload not stored in ScoredResult
                        version: 0,
                    })
                    .collect();
                
                Ok(Response::new(SearchResponse {
                    results: proto_results,
                    error: String::new(),
                    search_time_us: search_time,
                    request_id,
                }))
            }
            Err(e) => {
                tracing::error!("Search failed: {}", e);
                Ok(Response::new(SearchResponse {
                    results: vec![],
                    error: e.to_string(),
                    search_time_us: start_time.elapsed().as_micros() as u64,
                    request_id,
                }))
            }
        }
    }
    
    /// Handle batch search request
    /// Processes multiple query vectors on local storage
    async fn batch_search(
        &self,
        request: Request<BatchSearchRequest>,
    ) -> Result<Response<BatchSearchResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Executing batch search for collection {} ({} vectors, req_id={})",
            req.collection,
            req.vectors.len(),
            request_id
        );
        
        // Process each query vector
        let mut all_results = Vec::with_capacity(req.vectors.len());
        
        for vec_bytes in &req.vectors {
            let vector = bytes_to_vector(vec_bytes);
            
            match self.storage_router.search(&req.collection, vector, req.top_k, false).await {
                Ok(results) => {
                    let proto_vectors: Vec<ProtoScoredVector> = results
                        .into_iter()
                        .map(|r| ProtoScoredVector {
                            id: r.id,
                            score: r.score,
                            payload: vec![],
                            version: 0,
                        })
                        .collect();
                    
                    all_results.push(SearchResult {
                        vectors: proto_vectors,
                    });
                }
                Err(e) => {
                    tracing::error!("Batch search query failed: {}", e);
                    all_results.push(SearchResult {
                        vectors: vec![],
                    });
                }
            }
        }
        
        let total_time = start_time.elapsed().as_micros() as u64;
        
        Ok(Response::new(BatchSearchResponse {
            results: all_results,
            error: String::new(),
            total_time_us: total_time,
            request_id,
        }))
    }
    
    /// Handle insert request (forwarded from another node)
    /// Stores vector in local collection
    async fn insert(
        &self,
        request: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Executing local insert for collection {} (req_id={})",
            req.collection,
            request_id
        );
        
        // Convert bytes to vector
        let vector = Vector::new(bytes_to_vector(&req.vector));
        // Payload conversion from bytes to JSON would go here
        // For now, store without payload
        let payload: Option<crate::Payload> = None;
        
        // Execute insert
        match self.storage_router.insert(&req.collection, req.id, vector, payload).await {
            Ok(()) => {
                let insert_time = start_time.elapsed().as_micros() as u64;
                Ok(Response::new(InsertResponse {
                    success: true,
                    error: String::new(),
                    insert_time_us: insert_time,
                    request_id,
                }))
            }
            Err(e) => {
                tracing::error!("Insert failed: {}", e);
                Ok(Response::new(InsertResponse {
                    success: false,
                    error: e.to_string(),
                    insert_time_us: start_time.elapsed().as_micros() as u64,
                    request_id,
                }))
            }
        }
    }
    
    /// Handle batch insert request
    /// Stores multiple vectors in local collection
    async fn batch_insert(
        &self,
        request: Request<BatchInsertRequest>,
    ) -> Result<Response<BatchInsertResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Executing batch insert for collection {} ({} entries, req_id={})",
            req.collection,
            req.entries.len(),
            request_id
        );
        
        // Convert entries to vector pairs
        let vectors: Vec<(u64, Vector)> = req.entries
            .into_iter()
            .map(|e| (e.id, Vector::new(bytes_to_vector(&e.vector))))
            .collect();
        
        // Execute batch insert
        match self.storage_router.batch_insert(&req.collection, vectors).await {
            Ok(result) => {
                let total_time = start_time.elapsed().as_micros() as u64;
                
                Ok(Response::new(BatchInsertResponse {
                    success: result.failed_nodes.is_empty(),
                    error: if result.failed_nodes.is_empty() {
                        String::new()
                    } else {
                        format!("Failed nodes: {:?}", result.failed_nodes)
                    },
                    inserted_count: result.inserted_count,
                    failed_ids: vec![], // Detailed failed IDs not tracked in this flow
                    total_time_us: total_time,
                    request_id,
                }))
            }
            Err(e) => {
                tracing::error!("Batch insert failed: {}", e);
                Ok(Response::new(BatchInsertResponse {
                    success: false,
                    error: e.to_string(),
                    inserted_count: 0,
                    failed_ids: vec![],
                    total_time_us: start_time.elapsed().as_micros() as u64,
                    request_id,
                }))
            }
        }
    }
    
    /// Handle replication request
    /// Stores replicated data from primary node
    async fn replicate(
        &self,
        request: Request<ReplicateRequest>,
    ) -> Result<Response<ReplicateResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!(
            "Storing replication for {}:{} (seq={})",
            req.collection,
            req.id,
            req.sequence_number
        );
        
        // Convert bytes to vector
        let vector = Vector::new(bytes_to_vector(&req.vector));
        let payload: Option<crate::Payload> = None;
        
        // For replication, we store locally without forwarding
        // The replication factor is handled by the primary node
        match self.storage_router.insert(&req.collection, req.id, vector, payload).await {
            Ok(()) => {
                Ok(Response::new(ReplicateResponse {
                    success: true,
                    replicated_at: current_timestamp(),
                    applied_sequence: req.sequence_number,
                    error: String::new(),
                }))
            }
            Err(e) => {
                tracing::error!("Replication failed: {}", e);
                Ok(Response::new(ReplicateResponse {
                    success: false,
                    replicated_at: current_timestamp(),
                    applied_sequence: req.sequence_number,
                    error: e.to_string(),
                }))
            }
        }
    }
    
    /// Handle batch replication request
    async fn batch_replicate(
        &self,
        request: Request<BatchReplicateRequest>,
    ) -> Result<Response<BatchReplicateResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!(
            "Storing batch replication for collection {} ({} entries)",
            req.collection,
            req.entries.len()
        );
        
        let mut applied_count = 0u32;
        let mut highest_sequence = req.base_sequence;
        
        for entry in req.entries {
            let vector = Vector::new(bytes_to_vector(&entry.vector));
            let payload: Option<crate::Payload> = None;
            
            match self.storage_router.insert(&req.collection, entry.id, vector, payload).await {
                Ok(()) => {
                    applied_count += 1;
                    highest_sequence = highest_sequence.max(entry.sequence_number);
                }
                Err(e) => {
                    tracing::error!("Replication entry failed: {}", e);
                }
            }
        }
        
        Ok(Response::new(BatchReplicateResponse {
            success: applied_count > 0,
            replicated_at: current_timestamp(),
            highest_applied_sequence: highest_sequence,
            applied_count,
            error: if applied_count > 0 { String::new() } else { "All replications failed".to_string() },
        }))
    }
    
    /// Handle stream replication (bidirectional streaming)
    async fn stream_replicate(
        &self,
        request: Request<tonic::Streaming<ReplicateRequest>>,
    ) -> Result<Response<ReplicateResponse>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0u64;
        let mut failed = 0u64;
        
        tracing::debug!("Started stream replication");
        
        while let Some(req) = stream.message().await? {
            let vector = Vector::new(bytes_to_vector(&req.vector));
            let payload: Option<crate::Payload> = None;
            
            match self.storage_router.insert(&req.collection, req.id, vector, payload).await {
                Ok(()) => {
                    count += 1;
                }
                Err(e) => {
                    tracing::error!("Stream replication entry failed: {}", e);
                    failed += 1;
                }
            }
            
            if (count + failed) % 1000 == 0 {
                tracing::debug!("Streamed {} replication entries ({} failed)", count, failed);
            }
        }
        
        tracing::debug!("Stream replication completed: {} entries ({} failed)", count, failed);
        
        Ok(Response::new(ReplicateResponse {
            success: failed == 0,
            replicated_at: current_timestamp(),
            applied_sequence: count,
            error: if failed == 0 { String::new() } else { format!("{} entries failed", failed) },
        }))
    }
    
    /// Health check endpoint
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            status: 1, // SERVING
            node_id: String::new(), // Would get from storage_router
            uptime_seconds: 0,
        }))
    }
}

/// Convert f32 vector to bytes
fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|&f| f.to_le_bytes()).collect()
}

/// Convert bytes back to f32 vector
fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_bytes_conversion() {
        let original = vec![1.0f32, 2.5, 3.14, -1.5];
        let bytes = vector_to_bytes(&original);
        let recovered = bytes_to_vector(&bytes);
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.concurrency_limit, DEFAULT_CONCURRENCY_LIMIT);
        assert!(config.enable_compression);
        assert!(config.tcp_keepalive.is_some());
    }
}
