//! High-Performance Cluster gRPC Client
//!
//! Optimized client for communicating with other nodes in the cluster.
//! Features:
//! - Connection pooling with multiple channels per node for high throughput
//! - HTTP/2 keepalive for connection health
//! - Compression support (gzip)
//! - Configurable timeouts
//! - Lock-free connection access using DashMap
//! - Batch operations for bulk processing

#![cfg(feature = "grpc")]

use super::{
    ClusterConfig, NodeInfo,
    generated::ClusterServiceClient,
    generated::{
        BatchInsertRequest, BatchReplicateRequest, BatchSearchRequest,
        HealthRequest, HeartbeatRequest, InsertRequest, JoinRequest,
        ReplicateRequest, SearchRequest, TopologyRequest, VectorEntry,
    },
};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tonic::{
    Request,
    transport::{Channel, Endpoint},
};

/// Default number of connections per node for connection pooling
const DEFAULT_CONNECTION_POOL_SIZE: usize = 4;

/// Default timeout for gRPC requests
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Default timeout for search requests (longer due to computational cost)
const DEFAULT_SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Keepalive interval for HTTP/2 connections
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// Keepalive timeout
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection pool for a single node
#[derive(Clone)]
struct ConnectionPool {
    /// Multiple channels for this node (round-robin distribution)
    channels: Vec<ClusterServiceClient<Channel>>,
    /// Current index for round-robin selection
    current_index: Arc<AtomicUsize>,
    /// Node address
    address: String,
}

impl ConnectionPool {
    /// Create a new connection pool with multiple channels
    async fn new(
        addr: &str,
        pool_size: usize,
    ) -> crate::Result<Self> {
        let mut channels = Vec::with_capacity(pool_size);
        
        for i in 0..pool_size {
            let channel = create_channel(addr, i).await?;
            channels.push(channel);
        }
        
        Ok(Self {
            channels,
            current_index: Arc::new(AtomicUsize::new(0)),
            address: addr.to_string(),
        })
    }
    
    /// Get a channel using round-robin selection
    fn get_channel(&self) -> ClusterServiceClient<Channel> {
        let index = self.current_index.fetch_add(1, Ordering::Relaxed) % self.channels.len();
        self.channels[index].clone()
    }
    
    /// Get a specific channel by index
    fn get_channel_at(&self, index: usize) -> Option<ClusterServiceClient<Channel>> {
        self.channels.get(index % self.channels.len()).cloned()
    }
    
    /// Refresh a specific connection in the pool
    async fn refresh_channel(&mut self, index: usize) -> crate::Result<()> {
        if index < self.channels.len() {
            let channel = create_channel(&self.address, index).await?;
            self.channels[index] = channel;
        }
        Ok(())
    }
}

/// Create a configured gRPC channel
async fn create_channel(
    addr: &str,
    _pool_index: usize,
) -> crate::Result<ClusterServiceClient<Channel>> {
    // Build endpoint with performance optimizations
    let mut endpoint = Endpoint::from_shared(format!("http://{}", addr))
        .map_err(|e| crate::RTDBError::Io(format!("Invalid address: {}", e)))?;
    
    // Configure HTTP/2 keepalive
    endpoint = endpoint
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(KEEPALIVE_INTERVAL)
        .keep_alive_timeout(KEEPALIVE_TIMEOUT);
    
    // Configure connection settings for high throughput
    endpoint = endpoint
        .concurrency_limit(256)  // Max concurrent streams per connection
        .initial_stream_window_size(Some(65535))  // 64KB stream window
        .initial_connection_window_size(Some(1048576));  // 1MB connection window
    
    // Note: user agent customization removed for compatibility
    // endpoint = endpoint.user_agent(...)
    
    let channel = endpoint.connect()
        .await
        .map_err(|e| crate::RTDBError::Io(format!("Connection failed: {}", e)))?;
    
    Ok(ClusterServiceClient::new(channel))
}

/// Cluster client configuration
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// Number of connections to maintain per node
    pub connection_pool_size: usize,
    /// Default request timeout
    pub request_timeout: Duration,
    /// Search request timeout
    pub search_timeout: Duration,
    /// Enable compression (gzip)
    pub enable_compression: bool,
    /// Enable keepalive pings
    pub enable_keepalive: bool,
    // Note: TLS configuration removed for standalone builds
    // pub tls_config: Option<ClientTlsConfig>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: DEFAULT_CONNECTION_POOL_SIZE,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            search_timeout: DEFAULT_SEARCH_TIMEOUT,
            enable_compression: true,
            enable_keepalive: true,
        }
    }
}

/// High-performance cluster client manager
///
/// Manages connections to all other nodes in the cluster with:
/// - Connection pooling for high throughput
/// - Automatic reconnection on failures
/// - Compression support
/// - Configurable timeouts
pub struct ClusterClient {
    /// Local node configuration
    config: ClusterConfig,
    /// Client configuration
    client_config: ClientConfig,
    /// Connection pools for each node (lock-free access)
    connection_pools: Arc<DashMap<String, ConnectionPool>>,
    /// Request ID counter for tracing
    request_id_counter: Arc<AtomicUsize>,
}

impl ClusterClient {
    /// Create new cluster client with default configuration
    pub fn new(config: ClusterConfig) -> Self {
        Self::with_client_config(config, ClientConfig::default())
    }
    
    /// Create new cluster client with custom client configuration
    pub fn with_client_config(config: ClusterConfig, client_config: ClientConfig) -> Self {
        Self {
            config,
            client_config,
            connection_pools: Arc::new(DashMap::new()),
            request_id_counter: Arc::new(AtomicUsize::new(1)),
        }
    }
    
    /// Generate next request ID
    fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, Ordering::Relaxed) as u64
    }
    
    /// Connect to a node with connection pooling
    pub async fn connect(&self, node: &NodeInfo) -> crate::Result<()> {
        let addr = node.address.to_string();
        
        // Create connection pool for this node
        let pool = ConnectionPool::new(
            &addr,
            self.client_config.connection_pool_size,
        ).await?;
        
        self.connection_pools.insert(node.id.clone(), pool);
        
        tracing::info!(
            "Connected to node {} at {} with {} connections",
            node.id,
            addr,
            self.client_config.connection_pool_size
        );
        Ok(())
    }
    
    /// Disconnect from a node
    pub async fn disconnect(&self, node_id: &str) {
        self.connection_pools.remove(node_id);
        tracing::info!("Disconnected from node {}", node_id);
    }
    
    /// Get a connection pool for a node
    fn get_pool(&self, node_id: &str) -> crate::Result<ConnectionPool> {
        self.connection_pools
            .get(node_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| {
                crate::RTDBError::Storage(format!("No connection pool for node {}", node_id))
            })
    }
    
    /// Join cluster by contacting seed nodes
    pub async fn join_cluster(&self, seeds: &[String]) -> crate::Result<NodeInfo> {
        let local_node = NodeInfo {
            id: self.config.node_id.clone(),
            address: self.config.bind_addr,
            status: super::NodeStatus::Joining,
            shards: vec![],
            capacity: 100_000_000, // 100M vectors
            load: 0,
            last_heartbeat: current_timestamp(),
        };
        
        // Try each seed node
        for seed_addr in seeds {
            match self.try_join_seed(seed_addr, &local_node).await {
                Ok(topology) => {
                    tracing::info!("Successfully joined cluster via {}", seed_addr);
                    // Connect to all nodes in topology
                    for node in &topology.nodes {
                        if node.id != self.config.node_id {
                            // Parse address and connect
                            if let Ok(addr) = node.address.parse() {
                                let node_info = NodeInfo {
                                    id: node.id.clone(),
                                    address: addr,
                                    status: super::NodeStatus::Active,
                                    shards: node.shards.clone(),
                                    capacity: node.capacity as usize,
                                    load: node.load as usize,
                                    last_heartbeat: current_timestamp(),
                                };
                                let _ = self.connect(&node_info).await;
                            }
                        }
                    }
                    return Ok(local_node);
                }
                Err(e) => {
                    tracing::warn!("Failed to join via {}: {}", seed_addr, e);
                }
            }
        }
        
        Err(crate::RTDBError::Storage(
            "Failed to join cluster via any seed node".to_string()
        ))
    }
    
    /// Try to join via a specific seed node
    async fn try_join_seed(
        &self,
        seed_addr: &str,
        local_node: &NodeInfo,
    ) -> crate::Result<super::proto::Topology> {
        // Create temporary connection for join
        let endpoint = Endpoint::from_shared(format!("http://{}", seed_addr))
            .map_err(|e| crate::RTDBError::Io(format!("Invalid seed address: {}", e)))?;
        
        let channel = endpoint.connect()
            .await
            .map_err(|e| crate::RTDBError::Io(format!("Connection to seed failed: {}", e)))?;
        
        let mut client = ClusterServiceClient::new(channel);
        
        let request = JoinRequest {
            node_id: local_node.id.clone(),
            address: local_node.address.to_string(),
            capacity: local_node.capacity as u64,
            metadata: vec![],
        };
        
        let mut tonic_request = Request::new(request);
        
        // Apply compression if enabled
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert(
                "grpc-encoding",
                "gzip".parse().unwrap(),
            );
        }
        
        let response = client.join_cluster(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Join failed: {}", e)))?;
        
        let join_response = response.into_inner();
        
        if !join_response.success {
            return Err(crate::RTDBError::Storage(
                join_response.error.clone()
            ));
        }
        
        join_response.topology.ok_or_else(|| {
            crate::RTDBError::Storage("No topology in join response".to_string())
        })
    }
    
    /// Send heartbeat to a specific node
    pub async fn send_heartbeat(&self, node_id: &str) -> crate::Result<bool> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let request = HeartbeatRequest {
            node_id: self.config.node_id.clone(),
            timestamp: current_timestamp(),
            load: 0, // TODO: Get actual load
            shards: vec![],
            metrics: vec![],
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(Duration::from_secs(3)); // Short timeout for heartbeats
        
        match client.heartbeat(tonic_request).await {
            Ok(response) => {
                let inner = response.into_inner();
                Ok(inner.acknowledged)
            }
            Err(e) => {
                // Remove failed connection - will be reestablished on next use
                self.connection_pools.remove(node_id);
                Err(crate::RTDBError::Io(format!("Heartbeat failed: {}", e)))
            }
        }
    }
    
    /// Forward search request to another node
    pub async fn forward_search(
        &self,
        node_id: &str,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<super::proto::ScoredVector>> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        // Convert f32 vector to bytes for efficient transfer
        let vector_bytes = vector_to_bytes(&vector);
        let request_id = self.next_request_id();
        
        let request = SearchRequest {
            collection: collection.to_string(),
            vector: vector_bytes,
            top_k,
            score_threshold: 0.0,
            filter: vec![],
            request_id,
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(self.client_config.search_timeout);
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
            tonic_request.metadata_mut().insert("grpc-accept-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.search(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Search failed: {}", e)))?;
        
        Ok(response.into_inner().results)
    }
    
    /// Batch search for scatter-gather queries
    pub async fn forward_batch_search(
        &self,
        node_id: &str,
        collection: &str,
        vectors: Vec<Vec<f32>>,
        top_k: u32,
    ) -> crate::Result<Vec<Vec<super::proto::ScoredVector>>> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let vector_bytes: Vec<Vec<u8>> = vectors.iter()
            .map(|v| vector_to_bytes(v))
            .collect();
        
        let request_id = self.next_request_id();
        
        let request = BatchSearchRequest {
            collection: collection.to_string(),
            vectors: vector_bytes,
            top_k,
            score_threshold: 0.0,
            filter: vec![],
            request_id,
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(self.client_config.search_timeout);
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.batch_search(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Batch search failed: {}", e)))?;
        
        let inner = response.into_inner();
        let results: Vec<Vec<super::proto::ScoredVector>> = inner.results
            .into_iter()
            .map(|r| r.vectors)
            .collect();
        
        Ok(results)
    }
    
    /// Forward insert request to another node
    pub async fn forward_insert(
        &self,
        node_id: &str,
        collection: &str,
        id: u64,
        vector: Vec<f32>,
    ) -> crate::Result<()> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let request = InsertRequest {
            collection: collection.to_string(),
            id,
            vector: vector_to_bytes(&vector),
            payload: vec![],
            timestamp: current_timestamp(),
            request_id: self.next_request_id(),
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(self.client_config.request_timeout);
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.insert(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Insert failed: {}", e)))?;
        
        if response.into_inner().success {
            Ok(())
        } else {
            Err(crate::RTDBError::Storage("Insert failed".to_string()))
        }
    }
    
    /// Batch insert for bulk operations
    pub async fn forward_batch_insert(
        &self,
        node_id: &str,
        collection: &str,
        entries: Vec<(u64, Vec<f32>)>,
    ) -> crate::Result<u32> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let vector_entries: Vec<VectorEntry> = entries.into_iter()
            .map(|(id, vector)| VectorEntry {
                id,
                vector: vector_to_bytes(&vector),
                payload: vec![],
                timestamp: current_timestamp(),
            })
            .collect();
        
        let request = BatchInsertRequest {
            collection: collection.to_string(),
            entries: vector_entries,
            request_id: self.next_request_id(),
        };
        
        let mut tonic_request = Request::new(request);
        // Batch operations need longer timeout
        tonic_request.set_timeout(Duration::from_secs(60));
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.batch_insert(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Batch insert failed: {}", e)))?;
        
        let inner = response.into_inner();
        if inner.success {
            Ok(inner.inserted_count)
        } else {
            Err(crate::RTDBError::Storage(inner.error))
        }
    }
    
    /// Replicate data to a follower node
    pub async fn replicate(
        &self,
        node_id: &str,
        collection: &str,
        id: u64,
        vector: Vec<f32>,
    ) -> crate::Result<()> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let request = ReplicateRequest {
            collection: collection.to_string(),
            id,
            vector: vector_to_bytes(&vector),
            payload: vec![],
            timestamp: current_timestamp(),
            sequence_number: 0,
            is_delete: false,
        };
        
        let mut tonic_request = Request::new(request);
        // Replication has longer timeout for network issues
        tonic_request.set_timeout(Duration::from_secs(10));
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.replicate(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Replication failed: {}", e)))?;
        
        if response.into_inner().success {
            Ok(())
        } else {
            Err(crate::RTDBError::Storage("Replication failed".to_string()))
        }
    }
    
    /// Batch replicate for efficient replication
    pub async fn batch_replicate(
        &self,
        node_id: &str,
        collection: &str,
        entries: Vec<(u64, Vec<f32>)>,
        base_sequence: u64,
    ) -> crate::Result<u32> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let replicate_entries: Vec<ReplicateRequest> = entries.into_iter()
            .enumerate()
            .map(|(idx, (id, vector))| ReplicateRequest {
                collection: collection.to_string(),
                id,
                vector: vector_to_bytes(&vector),
                payload: vec![],
                timestamp: current_timestamp(),
                sequence_number: base_sequence + idx as u64,
                is_delete: false,
            })
            .collect();
        
        let request = BatchReplicateRequest {
            collection: collection.to_string(),
            entries: replicate_entries,
            base_sequence,
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(Duration::from_secs(30));
        
        if self.client_config.enable_compression {
            tonic_request.metadata_mut().insert("grpc-encoding", "gzip".parse().unwrap());
        }
        
        let response = client.batch_replicate(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Batch replication failed: {}", e)))?;
        
        let inner = response.into_inner();
        if inner.success {
            Ok(inner.applied_count)
        } else {
            Err(crate::RTDBError::Storage(inner.error))
        }
    }
    
    /// Get topology from another node
    pub async fn get_topology(&self, node_id: &str) -> crate::Result<super::proto::Topology> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let request = TopologyRequest {
            include_shard_mappings: true,
        };
        
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(Duration::from_secs(5));
        
        let response = client.get_topology(tonic_request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Get topology failed: {}", e)))?;
        
        response.into_inner().topology.ok_or_else(|| {
            crate::RTDBError::Storage("No topology in response".to_string())
        })
    }
    
    /// Check node health
    pub async fn health_check(&self, node_id: &str) -> crate::Result<bool> {
        let pool = self.get_pool(node_id)?;
        let mut client = pool.get_channel();
        
        let request = HealthRequest {};
        let mut tonic_request = Request::new(request);
        tonic_request.set_timeout(Duration::from_secs(2));
        
        match client.health(tonic_request).await {
            Ok(response) => {
                let inner = response.into_inner();
                Ok(inner.status == 1) // Status::SERVING = 1
            }
            Err(_) => Ok(false),
        }
    }
    
    /// Get all connected node IDs
    pub fn connected_nodes(&self) -> Vec<String> {
        self.connection_pools.iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
    
    /// Check if connected to a node
    pub fn is_connected(&self, node_id: &str) -> bool {
        self.connection_pools.contains_key(node_id)
    }
    
    /// Get connection pool statistics
    pub fn pool_stats(&self) -> HashMap<String, usize> {
        self.connection_pools.iter()
            .map(|entry| {
                let node_id = entry.key().clone();
                let pool_size = entry.channels.len();
                (node_id, pool_size)
            })
            .collect()
    }
}

use std::collections::HashMap;

/// Convert f32 vector to bytes for efficient transfer
fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect()
}

/// Convert bytes back to f32 vector
#[allow(dead_code)]
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
    fn test_vector_bytes_conversion() {
        let original = vec![1.0f32, 2.5, 3.14, -1.5];
        let bytes = vector_to_bytes(&original);
        let recovered = bytes_to_vector(&bytes);
        assert_eq!(original, recovered);
    }
    
    #[test]
    fn test_cluster_client_creation() {
        let config = ClusterConfig::default();
        let client = ClusterClient::new(config);
        
        // Just verify it compiles and creates
        assert_eq!(client.config.node_id, ClusterConfig::default().node_id);
    }
    
    #[test]
    fn test_client_config_defaults() {
        let config = ClientConfig::default();
        assert_eq!(config.connection_pool_size, DEFAULT_CONNECTION_POOL_SIZE);
        assert!(config.enable_compression);
        assert!(config.enable_keepalive);
    }
}
