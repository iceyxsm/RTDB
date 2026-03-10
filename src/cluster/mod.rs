//! Distributed clustering layer
//! 
//! Raft consensus, sharding, replication

pub mod raft;

/// Cluster manager
pub struct ClusterManager {
    /// Local Raft node (if in cluster mode)
    raft_node: Option<raft::RaftNode>,
}

impl ClusterManager {
    /// Create new cluster manager (standalone mode)
    pub fn new_standalone() -> Self {
        Self {
            raft_node: None,
        }
    }

    /// Create new cluster manager with Raft
    pub fn new_cluster(node_id: String, peers: Vec<String>) -> Self {
        let raft_node = raft::RaftNode::new(node_id, peers);
        
        Self {
            raft_node: Some(raft_node),
        }
    }

    /// Check if in cluster mode
    pub fn is_cluster_mode(&self) -> bool {
        self.raft_node.is_some()
    }

    /// Get Raft node if in cluster mode
    pub fn raft_node(&self) -> Option<&raft::RaftNode> {
        self.raft_node.as_ref()
    }

    /// Get mutable Raft node
    pub fn raft_node_mut(&mut self) -> Option<&mut raft::RaftNode> {
        self.raft_node.as_mut()
    }
}
