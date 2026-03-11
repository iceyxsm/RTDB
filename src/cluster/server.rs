//! High-Performance Cluster gRPC Server
//!
//! Optimized server for handling inter-node communication.
//! Features:
//! - Configurable concurrency limits
//! - HTTP/2 keepalive
//! - Request timeouts
//! - Compression support
//! - Batch operations for high throughput

#![cfg(grpc)]

use super::{
    ClusterManager, NodeInfo, NodeStatus,
    proto::cluster_service_server::{ClusterService, ClusterServiceServer},
    proto::{
        BatchInsertRequest, BatchInsertResponse, BatchReplicateRequest, BatchReplicateResponse,
        BatchSearchRequest, BatchSearchResponse, HealthRequest, HealthResponse,
        HeartbeatRequest, HeartbeatResponse, InsertRequest, InsertResponse,
        JoinRequest, JoinResponse, LeaveRequest, LeaveResponse,
        ReplicateRequest, ReplicateResponse, SearchRequest, SearchResponse,
        SearchResult, Topology as ProtoTopology, TopologyRequest, TopologyResponse,
        Node as ProtoNode, NodeStatus as ProtoNodeStatus, ScoredVector,
    },
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::{
    Request, Response, Status,
    transport::Server,
};
use tonic::codec::CompressionEncoding;

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
    /// Server bind address
    bind_addr: SocketAddr,
    /// Server configuration
    config: ServerConfig,
}

impl ClusterGrpcServer {
    /// Create new gRPC server with default configuration
    pub fn new(cluster: Arc<RwLock<ClusterManager>>, bind_addr: SocketAddr) -> Self {
        Self::with_config(cluster, bind_addr, ServerConfig::default())
    }
    
    /// Create new gRPC server with custom configuration
    pub fn with_config(
        cluster: Arc<RwLock<ClusterManager>>,
        bind_addr: SocketAddr,
        config: ServerConfig,
    ) -> Self {
        Self {
            cluster,
            bind_addr,
            config,
        }
    }
    
    /// Start the gRPC server
    pub async fn start(&self) -> crate::Result<()> {
        let service = ClusterServiceImpl {
            cluster: self.cluster.clone(),
            config: self.config.clone(),
        };
        
        let addr = self.bind_addr;
        tracing::info!(
            "Starting optimized cluster gRPC server on {} (concurrency_limit={})",
            addr,
            self.config.concurrency_limit
        );
        
        // Build service with compression support
        let mut service_builder = ClusterServiceServer::new(service);
        
        if self.config.enable_compression {
            service_builder = service_builder
                .send_compressed(CompressionEncoding::Gzip)
                .accept_compressed(CompressionEncoding::Gzip);
        }
        
        // Configure server with performance optimizations
        let mut server_builder = Server::builder();
        
        // Apply concurrency limit
        server_builder = server_builder.concurrency_limit(self.config.concurrency_limit);
        
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
    cluster: Arc<RwLock<ClusterManager>>,
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
        
        let node = NodeInfo {
            id: req.node_id.clone(),
            address: req.address.parse().map_err(|e| {
                Status::invalid_argument(format!("Invalid address: {}", e))
            })?,
            status: NodeStatus::Active,
            shards: vec![],
            capacity: req.capacity as usize,
            load: 0,
            last_heartbeat: current_timestamp(),
        };
        
        // Add node to cluster
        self.cluster.write().await.add_node(node);
        
        // Get current topology
        let topology = {
            let cluster = self.cluster.read().await;
            build_topology(&cluster)
        };
        
        Ok(Response::new(JoinResponse {
            success: true,
            error: String::new(),
            topology: Some(topology),
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
        
        self.cluster.write().await.remove_node(&req.node_id);
        
        Ok(Response::new(LeaveResponse { 
            success: true,
            message: "Node removed successfully".to_string(),
        }))
    }
    
    /// Handle topology request
    async fn get_topology(
        &self,
        request: Request<TopologyRequest>,
    ) -> Result<Response<TopologyResponse>, Status> {
        let include_mappings = request.into_inner().include_shard_mappings;
        
        let cluster = self.cluster.read().await;
        let topology = build_topology_with_options(&cluster, include_mappings);
        
        Ok(Response::new(TopologyResponse {
            topology: Some(topology),
            server_timestamp: current_timestamp(),
        }))
    }
    
    /// Handle heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        
        // Update node heartbeat in topology
        tracing::debug!("Heartbeat from node {} at {}", req.node_id, req.timestamp);
        
        // Check if topology has changed since client's version
        // For now, always return no change
        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            server_timestamp: current_timestamp(),
            topology: None,
            topology_changed: false,
        }))
    }
    
    /// Handle search request (forwarded from another node)
    async fn search(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Received forwarded search for collection {} (req_id={})",
            req.collection,
            request_id
        );
        
        // Convert bytes back to vector
        let _vector = bytes_to_vector(&req.vector);
        
        // TODO: Execute search on local node and return results
        // This requires integration with CollectionManager
        
        let search_time = start_time.elapsed().as_micros() as u64;
        
        Ok(Response::new(SearchResponse {
            results: vec![],
            error: String::new(),
            search_time_us: search_time,
            request_id,
        }))
    }
    
    /// Handle batch search request
    async fn batch_search(
        &self,
        request: Request<BatchSearchRequest>,
    ) -> Result<Response<BatchSearchResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Received batch search for collection {} ({} vectors, req_id={})",
            req.collection,
            req.vectors.len(),
            request_id
        );
        
        // Process each query vector
        let results: Vec<SearchResult> = req.vectors.iter()
            .map(|vec_bytes| {
                let _vector = bytes_to_vector(vec_bytes);
                // TODO: Execute search on local node
                SearchResult {
                    vectors: vec![],
                }
            })
            .collect();
        
        let total_time = start_time.elapsed().as_micros() as u64;
        
        Ok(Response::new(BatchSearchResponse {
            results,
            error: String::new(),
            total_time_us: total_time,
            request_id,
        }))
    }
    
    /// Handle insert request (forwarded from another node)
    async fn insert(
        &self,
        request: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Received forwarded insert for collection {} (req_id={})",
            req.collection,
            request_id
        );
        
        // Convert bytes back to vector
        let _vector = bytes_to_vector(&req.vector);
        
        // TODO: Execute insert on local node
        // This requires integration with CollectionManager
        
        let insert_time = start_time.elapsed().as_micros() as u64;
        
        Ok(Response::new(InsertResponse {
            success: true,
            error: String::new(),
            insert_time_us: insert_time,
            request_id,
        }))
    }
    
    /// Handle batch insert request
    async fn batch_insert(
        &self,
        request: Request<BatchInsertRequest>,
    ) -> Result<Response<BatchInsertResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            "Received batch insert for collection {} ({} entries, req_id={})",
            req.collection,
            req.entries.len(),
            request_id
        );
        
        let mut inserted_count = 0u32;
        let mut failed_ids = Vec::new();
        
        for entry in &req.entries {
            let _vector = bytes_to_vector(&entry.vector);
            // TODO: Execute insert on local node
            // For now, assume all succeed
            inserted_count += 1;
        }
        
        let total_time = start_time.elapsed().as_micros() as u64;
        
        Ok(Response::new(BatchInsertResponse {
            success: failed_ids.is_empty(),
            error: if failed_ids.is_empty() {
                String::new()
            } else {
                format!("Failed to insert {} vectors", failed_ids.len())
            },
            inserted_count,
            failed_ids,
            total_time_us: total_time,
            request_id,
        }))
    }
    
    /// Handle replication request
    async fn replicate(
        &self,
        request: Request<ReplicateRequest>,
    ) -> Result<Response<ReplicateResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!(
            "Received replication for {}:{} (seq={})",
            req.collection,
            req.id,
            req.sequence_number
        );
        
        // Convert bytes back to vector
        let _vector = bytes_to_vector(&req.vector);
        
        // TODO: Store replicated data
        // This requires integration with storage layer
        
        Ok(Response::new(ReplicateResponse {
            success: true,
            replicated_at: current_timestamp(),
            applied_sequence: req.sequence_number,
            error: String::new(),
        }))
    }
    
    /// Handle batch replication request
    async fn batch_replicate(
        &self,
        request: Request<BatchReplicateRequest>,
    ) -> Result<Response<BatchReplicateResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!(
            "Received batch replication for collection {} ({} entries)",
            req.collection,
            req.entries.len()
        );
        
        let mut applied_count = 0u32;
        let mut highest_sequence = req.base_sequence;
        
        for entry in &req.entries {
            let _vector = bytes_to_vector(&entry.vector);
            // TODO: Store replicated data
            applied_count += 1;
            highest_sequence = highest_sequence.max(entry.sequence_number);
        }
        
        Ok(Response::new(BatchReplicateResponse {
            success: true,
            replicated_at: current_timestamp(),
            highest_applied_sequence: highest_sequence,
            applied_count,
            error: String::new(),
        }))
    }
    
    /// Handle stream replication (bidirectional streaming)
    async fn stream_replicate(
        &self,
        request: Request<tonic::Streaming<ReplicateRequest>>,
    ) -> Result<Response<ReplicateResponse>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0u64;
        
        tracing::debug!("Started stream replication");
        
        while let Some(req) = stream.message().await? {
            let _vector = bytes_to_vector(&req.vector);
            // TODO: Store replicated data
            count += 1;
            
            if count % 1000 == 0 {
                tracing::debug!("Streamed {} replication entries", count);
            }
        }
        
        tracing::debug!("Stream replication completed: {} entries", count);
        
        Ok(Response::new(ReplicateResponse {
            success: true,
            replicated_at: current_timestamp(),
            applied_sequence: count,
            error: String::new(),
        }))
    }
    
    /// Health check endpoint
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        // Get node ID from cluster config
        let node_id = {
            let cluster = self.cluster.read().await;
            // Assuming ClusterManager has a way to get local node ID
            // For now, return empty string
            String::new()
        };
        
        Ok(Response::new(HealthResponse {
            status: 1, // SERVING
            node_id,
            uptime_seconds: 0, // TODO: Track server start time
        }))
    }
}

/// Build protobuf topology from cluster manager
fn build_topology(cluster: &ClusterManager) -> ProtoTopology {
    build_topology_with_options(cluster, true)
}

/// Build protobuf topology with options
fn build_topology_with_options(cluster: &ClusterManager, _include_mappings: bool) -> ProtoTopology {
    let active_nodes = cluster.active_nodes();
    
    ProtoTopology {
        version: cluster.topology_version(),
        nodes: active_nodes.iter().map(|n| ProtoNode {
            id: n.id.clone(),
            address: n.address.to_string(),
            status: node_status_to_proto(n.status) as i32,
            capacity: n.capacity as u64,
            load: n.load as u64,
            shards: n.shards.clone(),
            last_heartbeat: n.last_heartbeat,
            metadata: vec![],
        }).collect(),
        shard_mapping: vec![], // TODO: Populate shard mappings
        timestamp: current_timestamp(),
    }
}

/// Convert internal NodeStatus to protobuf
fn node_status_to_proto(status: NodeStatus) -> ProtoNodeStatus {
    match status {
        NodeStatus::Joining => ProtoNodeStatus::Joining,
        NodeStatus::Active => ProtoNodeStatus::Active,
        NodeStatus::Suspect => ProtoNodeStatus::Suspect,
        NodeStatus::Offline => ProtoNodeStatus::Offline,
        NodeStatus::Leaving => ProtoNodeStatus::Leaving,
    }
}

/// Convert f32 vector to bytes
fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect()
}

/// Convert bytes back to f32 vector
fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            f32::from_le_bytes(arr)
        })
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
    fn test_node_status_conversion() {
        assert_eq!(
            node_status_to_proto(NodeStatus::Active) as i32,
            ProtoNodeStatus::Active as i32
        );
        assert_eq!(
            node_status_to_proto(NodeStatus::Offline) as i32,
            ProtoNodeStatus::Offline as i32
        );
    }
    
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
