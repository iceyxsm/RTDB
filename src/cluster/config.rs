//! Cluster configuration and node management
//!
//! Defines cluster topology, node discovery, and shard assignment.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Configuration settings for RTDB cluster operation.
/// 
/// Defines node identity, networking, replication, and failure detection
/// parameters for distributed cluster deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Unique node identifier
    pub node_id: String,
    /// Node address for inter-node communication
    pub bind_addr: SocketAddr,
    /// List of seed nodes for joining cluster
    pub seed_nodes: Vec<String>,
    /// Number of shards (virtual buckets)
    pub shard_count: usize,
    /// Replication factor
    pub replication_factor: usize,
    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    /// Failure detection timeout in milliseconds
    pub failure_timeout_ms: u64,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: format!("node-{}", uuid::Uuid::new_v4()),
            bind_addr: "0.0.0.0:7000".parse().unwrap(),
            seed_nodes: vec![],
            shard_count: 256, // Default 256 virtual shards
            replication_factor: 3,
            heartbeat_interval_ms: 1000,
            failure_timeout_ms: 5000,
        }
    }
}

/// Information about a cluster node including identity, status, and load metrics.
/// 
/// Tracks node state, shard assignments, capacity, and health information
/// for cluster management and load balancing decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: String,
    /// Node address
    pub address: SocketAddr,
    /// Node status
    pub status: NodeStatus,
    /// Shards owned by this node
    pub shards: Vec<ShardId>,
    /// Node capacity (max vectors)
    pub capacity: usize,
    /// Current load (vectors stored)
    pub load: usize,
    /// Last heartbeat timestamp
    pub last_heartbeat: u64,
}

/// Status of a cluster node in the distributed system.
/// 
/// Represents the current operational state of a node for cluster
/// management, failure detection, and load balancing decisions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeStatus {
    /// Node is in the process of joining the cluster
    Joining,
    /// Node is active and serving requests
    Active,
    /// Node is suspected to have failed (missed heartbeats)
    Suspect,
    /// Node is confirmed offline and not serving requests
    Offline,
    /// Node is gracefully leaving the cluster
    Leaving,
}

/// Shard identifier (virtual bucket)
pub type ShardId = u32;

/// Cluster topology mapping shards to nodes for distributed data placement.
/// 
/// Maintains the current assignment of virtual shards to cluster nodes,
/// including primary and replica assignments with versioning for consistency.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterTopology {
    /// Version number for topology changes
    pub version: u64,
    /// Map of shard to primary node
    pub shard_to_node: HashMap<ShardId, String>,
    /// Map of shard to replica nodes
    pub shard_replicas: HashMap<ShardId, Vec<String>>,
    /// All known nodes
    pub nodes: HashMap<String, NodeInfo>,
}

impl ClusterTopology {
    /// Create new topology
    pub fn new(shard_count: usize) -> Self {
        Self {
            version: 1,
            shard_to_node: HashMap::with_capacity(shard_count),
            shard_replicas: HashMap::with_capacity(shard_count),
            nodes: HashMap::new(),
        }
    }

    /// Add node to topology
    pub fn add_node(&mut self, node: NodeInfo) {
        self.nodes.insert(node.id.clone(), node);
        self.version += 1;
    }

    /// Remove node from topology
    pub fn remove_node(&mut self, node_id: &str) {
        self.nodes.remove(node_id);
        
        // Remove node from shard assignments
        self.shard_to_node.retain(|_, id| id != node_id);
        for replicas in self.shard_replicas.values_mut() {
            replicas.retain(|id| id != node_id);
        }
        
        self.version += 1;
    }

    /// Get node for shard
    pub fn get_node_for_shard(&self, shard: ShardId) -> Option<&NodeInfo> {
        self.shard_to_node
            .get(&shard)
            .and_then(|node_id| self.nodes.get(node_id))
    }

    /// Check if local node owns shard
    pub fn owns_shard(&self, shard: ShardId, node_id: &str) -> bool {
        self.shard_to_node
            .get(&shard)
            .map(|id| id == node_id)
            .unwrap_or(false)
    }

    /// Get all active nodes
    pub fn active_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .collect()
    }

    /// Calculate shard assignment for balanced distribution
    pub fn rebalance_shards(&mut self, _local_node_id: &str) {
        let active_nodes: Vec<_> = self.active_nodes().into_iter().cloned().collect();
        if active_nodes.is_empty() {
            return;
        }

        let node_count = active_nodes.len();
        let shard_count = 256; // TODO: Get from config
        self.shard_to_node.clear();
        
        // Simple round-robin assignment
        for shard_id in 0..shard_count as ShardId {
            let node_idx = shard_id as usize % node_count;
            let node_id = active_nodes[node_idx].id.clone();
            self.shard_to_node.insert(shard_id, node_id);
        }

        self.version += 1;
    }
}

/// Cluster state manager
pub struct ClusterState {
    /// Local node ID
    pub local_node_id: String,
    /// Current topology
    pub topology: ClusterTopology,
    /// Cluster configuration
    pub config: ClusterConfig,
}

impl ClusterState {
    /// Create new cluster state
    pub fn new(config: ClusterConfig) -> Self {
        let local_node_id = config.node_id.clone();
        let topology = ClusterTopology::new(config.shard_count);
        
        Self {
            local_node_id,
            topology,
            config,
        }
    }

    /// Initialize as single-node cluster
    pub fn init_single_node(&mut self) {
        let local_node = NodeInfo {
            id: self.local_node_id.clone(),
            address: self.config.bind_addr,
            status: NodeStatus::Active,
            shards: (0..self.config.shard_count as ShardId).collect(),
            capacity: 100_000_000, // 100M vectors
            load: 0,
            last_heartbeat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        self.topology.add_node(local_node);
        
        // Assign all shards to local node
        for shard_id in 0..self.config.shard_count as ShardId {
            self.topology.shard_to_node.insert(
                shard_id,
                self.local_node_id.clone(),
            );
        }
    }

    /// Check if cluster is in single-node mode
    pub fn is_single_node(&self) -> bool {
        self.topology.nodes.len() == 1
    }

    /// Get local node info
    pub fn local_node(&self) -> Option<&NodeInfo> {
        self.topology.nodes.get(&self.local_node_id)
    }

    /// Get shard for vector ID
    pub fn get_shard_for_vector(&self, vector_id: crate::VectorId) -> ShardId {
        // Simple hash-based sharding
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        vector_id.hash(&mut hasher);
        let hash = hasher.finish();
        
        (hash % self.config.shard_count as u64) as ShardId
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_assignment() {
        let config = ClusterConfig {
            node_id: "test-node".to_string(),
            shard_count: 256,
            ..Default::default()
        };
        
        let mut state = ClusterState::new(config);
        state.init_single_node();
        
        // Test vector ID to shard mapping
        let shard1 = state.get_shard_for_vector(1);
        let shard2 = state.get_shard_for_vector(2);
        let shard1_again = state.get_shard_for_vector(1);
        
        // Same ID should map to same shard
        assert_eq!(shard1, shard1_again);
        // Different IDs likely map to different shards
        // (could collide but very unlikely with 256 shards)
        assert!(shard1 < 256);
        assert!(shard2 < 256);
    }

    #[test]
    fn test_single_node_topology() {
        let config = ClusterConfig {
            node_id: "node-1".to_string(),
            shard_count: 16,
            ..Default::default()
        };
        
        let mut state = ClusterState::new(config);
        state.init_single_node();
        
        assert!(state.is_single_node());
        assert_eq!(state.topology.nodes.len(), 1);
        assert_eq!(state.topology.shard_to_node.len(), 16);
        
        // All shards should be assigned to local node
        for shard_id in 0..16 as ShardId {
            assert!(state.topology.owns_shard(shard_id, "node-1"));
        }
    }
}
