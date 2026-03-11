//! Distributed clustering layer
//! 
//! Raft consensus, sharding, replication, and cluster coordination

pub mod config;
pub mod hash_ring;
pub mod proto;
pub mod raft;

// Pre-generated protobuf code - always available when grpc feature is enabled, no protoc required
#[cfg(feature = "grpc")]
pub mod generated;

#[cfg(feature = "grpc")]
pub mod client;
#[cfg(feature = "grpc")]
pub mod server;
#[cfg(feature = "grpc")]
pub mod storage_router;

#[cfg(feature = "grpc")]
pub use client::ClusterClient;
pub use config::{ClusterConfig, ClusterState, ClusterTopology, NodeInfo, NodeStatus, ShardId};
pub use hash_ring::{HashRing, ShardRouter};
#[cfg(feature = "grpc")]
pub use server::ClusterGrpcServer;
#[cfg(feature = "grpc")]
pub use storage_router::{StorageRouter, ScoredResult, BatchInsertResult};

use std::sync::Arc;
use parking_lot::RwLock;

/// Cluster manager - coordinates distributed operations
pub struct ClusterManager {
    /// Cluster state (shared across threads)
    state: Arc<RwLock<ClusterState>>,
    /// Shard router for distributed queries
    router: ShardRouter,
    /// Local Raft node (if in cluster mode)
    raft_node: Option<raft::RaftNode>,
}

impl ClusterManager {
    /// Create new cluster manager (standalone mode)
    pub fn new_standalone() -> Self {
        let config = ClusterConfig::default();
        let mut state = ClusterState::new(config);
        state.init_single_node();
        
        let shard_count = state.config.shard_count;
        let state = Arc::new(RwLock::new(state));
        
        // Initialize router with single node
        let mut router = ShardRouter::new(shard_count);
        router.add_node("standalone");
        
        Self {
            state,
            router,
            raft_node: None,
        }
    }

    /// Create new cluster manager with cluster configuration
    pub fn new_cluster(config: ClusterConfig) -> Self {
        let shard_count = config.shard_count;
        let local_node_id = config.node_id.clone();
        
        let state = Arc::new(RwLock::new(ClusterState::new(config)));
        
        // Initialize router with local node
        let mut router = ShardRouter::new(shard_count);
        router.add_node(&local_node_id);
        
        // Add seed nodes to router
        for seed in &state.read().config.seed_nodes {
            router.add_node(seed);
        }
        
        Self {
            state,
            router,
            raft_node: None, // TODO: Initialize Raft
        }
    }

    /// Initialize cluster and join if needed
    pub async fn init(&mut self) -> crate::Result<()> {
        let is_cluster_mode = !self.state.read().config.seed_nodes.is_empty();
        
        if is_cluster_mode {
            // Join existing cluster
            self.join_cluster().await?;
        } else {
            // Initialize as single-node cluster
            self.init_single_node();
        }
        
        Ok(())
    }

    /// Initialize single-node cluster
    fn init_single_node(&mut self) {
        let mut state = self.state.write();
        state.init_single_node();
        
        // Update router with all shards on local node
        let local_id = state.local_node_id.clone();
        drop(state);
        
        self.router.add_node(&local_id);
    }

    /// Join existing cluster
    async fn join_cluster(&mut self) -> crate::Result<()> {
        // TODO: Implement cluster join protocol
        // 1. Contact seed nodes
        // 2. Fetch current topology
        // 3. Register local node
        // 4. Sync data if needed
        
        tracing::info!("Joining cluster...");
        
        Ok(())
    }

    /// Check if in cluster mode
    pub fn is_cluster_mode(&self) -> bool {
        let state = self.state.read();
        !state.config.seed_nodes.is_empty() || state.topology.nodes.len() > 1
    }

    /// Check if running in single-node mode
    pub fn is_single_node(&self) -> bool {
        self.state.read().is_single_node()
    }

    /// Get local node ID
    pub fn local_node_id(&self) -> String {
        self.state.read().local_node_id.clone()
    }

    /// Get shard for vector ID
    pub fn get_shard_for_vector(&self, vector_id: crate::VectorId) -> ShardId {
        self.state.read().get_shard_for_vector(vector_id)
    }

    /// Get node for vector (for routing queries)
    pub fn get_node_for_vector(&self, vector_id: crate::VectorId) -> Option<String> {
        let state = self.state.read();
        let shard = state.get_shard_for_vector(vector_id);
        
        state.topology.get_node_for_shard(shard)
            .map(|n| n.id.clone())
    }

    /// Check if local node owns this vector
    pub fn owns_vector(&self, vector_id: crate::VectorId) -> bool {
        let state = self.state.read();
        let shard = state.get_shard_for_vector(vector_id);
        
        state.topology.owns_shard(shard, &state.local_node_id)
    }

    /// Get cluster topology version
    pub fn topology_version(&self) -> u64 {
        self.state.read().topology.version
    }

    /// Add node to cluster (called when new node joins)
    pub fn add_node(&mut self, node: NodeInfo) {
        let mut state = self.state.write();
        let node_id = node.id.clone();
        state.topology.add_node(node);
        
        // Rebalance shards
        let local_id = state.local_node_id.clone();
        state.topology.rebalance_shards(&local_id);
        drop(state);
        
        // Update router
        self.router.add_node(&node_id);
        
        tracing::info!("Added node {} to cluster", node_id);
    }

    /// Remove node from cluster
    pub fn remove_node(&mut self, node_id: &str) {
        let mut state = self.state.write();
        state.topology.remove_node(node_id);
        
        // Rebalance remaining shards
        let local_id = state.local_node_id.clone();
        state.topology.rebalance_shards(&local_id);
        drop(state);
        
        // Update router
        self.router.remove_node(node_id);
        
        tracing::info!("Removed node {} from cluster", node_id);
    }

    /// Get all active nodes
    pub fn active_nodes(&self) -> Vec<NodeInfo> {
        self.state.read().topology.active_nodes()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get cluster statistics
    pub fn stats(&self) -> ClusterStats {
        let state = self.state.read();
        let topology = &state.topology;
        
        ClusterStats {
            node_count: topology.nodes.len(),
            active_node_count: topology.active_nodes().len(),
            shard_count: state.config.shard_count,
            topology_version: topology.version,
            is_single_node: topology.nodes.len() == 1,
        }
    }

    /// Get mutable access to Raft node
    pub fn raft_node_mut(&mut self) -> Option<&mut raft::RaftNode> {
        self.raft_node.as_mut()
    }
}

/// Cluster statistics
#[derive(Debug, Clone)]
pub struct ClusterStats {
    /// Total number of nodes
    pub node_count: usize,
    /// Number of active nodes
    pub active_node_count: usize,
    /// Number of shards
    pub shard_count: usize,
    /// Topology version
    pub topology_version: u64,
    /// Whether running in single-node mode
    pub is_single_node: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_manager_standalone() {
        let manager = ClusterManager::new_standalone();
        
        assert!(manager.is_single_node());
        assert!(!manager.is_cluster_mode());
        assert_eq!(manager.active_nodes().len(), 1);
    }

    #[test]
    fn test_vector_ownership() {
        let manager = ClusterManager::new_standalone();
        
        // In single-node mode, all vectors are owned locally
        assert!(manager.owns_vector(1));
        assert!(manager.owns_vector(100));
        assert!(manager.owns_vector(1000));
    }

    #[test]
    fn test_shard_routing() {
        let manager = ClusterManager::new_standalone();
        
        let shard1 = manager.get_shard_for_vector(1);
        let shard2 = manager.get_shard_for_vector(2);
        
        // Different vectors likely map to different shards
        // (could be same but unlikely with 256 shards)
        assert!(shard1 < 256);
        assert!(shard2 < 256);
    }
}
