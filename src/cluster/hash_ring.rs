//! Consistent Hashing Ring for Shard Distribution
//!
//! Provides virtual node-based consistent hashing for even distribution
//! and minimal rebalancing when nodes join/leave.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// Number of virtual nodes per physical node
const VIRTUAL_NODES: usize = 150;

/// Consistent Hash Ring
#[derive(Debug, Clone)]
pub struct HashRing {
    /// Ring mapping hash to node ID
    ring: BTreeMap<u64, String>,
    /// Physical nodes in the ring
    nodes: Vec<String>,
    /// Virtual node count
    virtual_nodes: usize,
}

impl Default for HashRing {
    fn default() -> Self {
        Self {
            ring: BTreeMap::new(),
            nodes: Vec::new(),
            virtual_nodes: VIRTUAL_NODES,
        }
    }
}

impl HashRing {
    /// Create new hash ring
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom virtual node count
    pub fn with_virtual_nodes(virtual_nodes: usize) -> Self {
        Self {
            ring: BTreeMap::new(),
            nodes: Vec::new(),
            virtual_nodes,
        }
    }

    /// Add node to ring
    pub fn add_node(&mut self, node_id: &str) {
        if self.nodes.contains(&node_id.to_string()) {
            return;
        }

        // Add virtual nodes
        for i in 0..self.virtual_nodes {
            let key = format!("{}-{}", node_id, i);
            let hash = Self::hash(&key);
            self.ring.insert(hash, node_id.to_string());
        }

        self.nodes.push(node_id.to_string());
    }

    /// Remove node from ring
    pub fn remove_node(&mut self, node_id: &str) {
        // Remove all virtual nodes
        for i in 0..self.virtual_nodes {
            let key = format!("{}-{}", node_id, i);
            let hash = Self::hash(&key);
            self.ring.remove(&hash);
        }

        self.nodes.retain(|n| n != node_id);
    }

    /// Get node for key
    pub fn get_node(&self, key: &str) -> Option<&String> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = Self::hash(key);
        
        // Find first node with hash >= key hash
        self.ring
            .range(hash..)
            .next()
            .map(|(_, node)| node)
            .or_else(|| {
                // Wrap around to first node
                self.ring.values().next()
            })
    }

    /// Get nodes for replication (primary + replicas)
    pub fn get_nodes(&self, key: &str, count: usize) -> Vec<&String> {
        if self.ring.is_empty() || count == 0 {
            return vec![];
        }

        let hash = Self::hash(key);
        let mut result = Vec::with_capacity(count);
        let mut seen = std::collections::HashSet::with_capacity(count);

        // Start from hash position
        for (_, node) in self.ring.range(hash..) {
            if seen.insert(node) && result.len() < count {
                result.push(node);
            }
            if result.len() >= count {
                break;
            }
        }

        // Wrap around if needed
        if result.len() < count {
            for (_, node) in self.ring.iter() {
                if seen.insert(node) && result.len() < count {
                    result.push(node);
                }
                if result.len() >= count {
                    break;
                }
            }
        }

        result
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    /// Get node count
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Calculate distribution statistics
    pub fn distribution_stats(&self) -> DistributionStats {
        if self.nodes.is_empty() {
            return DistributionStats::default();
        }

        // Count virtual nodes per physical node
        let mut counts: std::collections::HashMap<&String, usize> = 
            std::collections::HashMap::new();
        
        for node in self.ring.values() {
            *counts.entry(node).or_insert(0) += 1;
        }

        let values: Vec<usize> = counts.values().copied().collect();
        let min = *values.iter().min().unwrap_or(&0);
        let max = *values.iter().max().unwrap_or(&0);
        let avg = values.iter().sum::<usize>() as f64 / values.len() as f64;
        
        // Calculate standard deviation
        let variance: f64 = values
            .iter()
            .map(|&v| {
                let diff = v as f64 - avg;
                diff * diff
            })
            .sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        DistributionStats {
            node_count: self.nodes.len(),
            virtual_node_count: self.ring.len(),
            min_virtual_nodes: min,
            max_virtual_nodes: max,
            avg_virtual_nodes: avg,
            std_deviation: std_dev,
        }
    }

    /// Hash function using FNV-1a
    fn hash<T: Hash>(key: T) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

/// Distribution statistics
#[derive(Debug, Clone, Default)]
pub struct DistributionStats {
    /// Number of physical nodes
    pub node_count: usize,
    /// Total virtual nodes
    pub virtual_node_count: usize,
    /// Minimum virtual nodes per physical node
    pub min_virtual_nodes: usize,
    /// Maximum virtual nodes per physical node
    pub max_virtual_nodes: usize,
    /// Average virtual nodes per physical node
    pub avg_virtual_nodes: f64,
    /// Standard deviation
    pub std_deviation: f64,
}

/// Shard router using consistent hashing
pub struct ShardRouter {
    /// Hash ring for shard assignment
    ring: HashRing,
    /// Total number of shards
    shard_count: usize,
}

impl ShardRouter {
    /// Create new shard router
    pub fn new(shard_count: usize) -> Self {
        Self {
            ring: HashRing::new(),
            shard_count,
        }
    }

    /// Add node to cluster
    pub fn add_node(&mut self, node_id: &str) {
        self.ring.add_node(node_id);
    }

    /// Remove node from cluster
    pub fn remove_node(&mut self, node_id: &str) {
        self.ring.remove_node(node_id);
    }

    /// Get node for vector ID
    pub fn get_node_for_vector(&self, vector_id: crate::VectorId) -> Option<&String> {
        let key = format!("vector-{}", vector_id);
        self.ring.get_node(&key)
    }

    /// Get primary and replica nodes for vector
    pub fn get_nodes_for_vector(
        &self,
        vector_id: crate::VectorId,
        replication_factor: usize,
    ) -> Vec<&String> {
        let key = format!("vector-{}", vector_id);
        self.ring.get_nodes(&key, replication_factor)
    }

    /// Get node for shard
    pub fn get_node_for_shard(&self, shard_id: u32) -> Option<&String> {
        let key = format!("shard-{}", shard_id);
        self.ring.get_node(&key)
    }

    /// Calculate shard-to-node mapping
    pub fn calculate_shard_mapping(&self) -> Vec<(u32, String)> {
        let mut mapping = Vec::with_capacity(self.shard_count);
        
        for shard_id in 0..self.shard_count as u32 {
            if let Some(node) = self.get_node_for_shard(shard_id) {
                mapping.push((shard_id, node.clone()));
            }
        }
        
        mapping
    }

    /// Get distribution statistics
    pub fn distribution_stats(&self) -> DistributionStats {
        self.ring.distribution_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_ring_basic() {
        let mut ring = HashRing::new();
        
        ring.add_node("node-1");
        ring.add_node("node-2");
        ring.add_node("node-3");
        
        assert_eq!(ring.len(), 3);
        
        // Test key distribution
        let node = ring.get_node("test-key-1");
        assert!(node.is_some());
        
        // Test replication
        let nodes = ring.get_nodes("test-key-1", 2);
        assert_eq!(nodes.len(), 2);
        assert_ne!(nodes[0], nodes[1]); // Should be different nodes
    }

    #[test]
    fn test_hash_ring_consistency() {
        let mut ring = HashRing::new();
        
        ring.add_node("node-1");
        ring.add_node("node-2");
        
        // Same key should always map to same node
        let node1 = ring.get_node("consistent-key").cloned();
        let node2 = ring.get_node("consistent-key").cloned();
        assert_eq!(node1, node2);
    }

    #[test]
    fn test_hash_ring_node_removal() {
        let mut ring = HashRing::new();
        
        ring.add_node("node-1");
        ring.add_node("node-2");
        
        // Get initial assignment
        let _initial_node = ring.get_node("test-key").cloned();
        
        // Remove a node
        ring.remove_node("node-1");
        
        // Should still have one node
        assert_eq!(ring.len(), 1);
        
        // Key should still be assigned (possibly to different node)
        let new_node = ring.get_node("test-key");
        assert!(new_node.is_some());
    }

    #[test]
    fn test_distribution_stats() {
        let mut ring = HashRing::with_virtual_nodes(100);
        
        ring.add_node("node-1");
        ring.add_node("node-2");
        ring.add_node("node-3");
        
        let stats = ring.distribution_stats();
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.virtual_node_count, 300); // 3 nodes × 100 vnodes
        
        // Distribution should be fairly even
        assert!(stats.std_deviation < 10.0, "Distribution too uneven");
    }

    #[test]
    fn test_shard_router() {
        let mut router = ShardRouter::new(256);
        
        router.add_node("node-1");
        router.add_node("node-2");
        router.add_node("node-3");
        
        // Test vector routing
        let node = router.get_node_for_vector(1);
        assert!(node.is_some());
        
        // Test with replication
        let nodes = router.get_nodes_for_vector(1, 3);
        assert_eq!(nodes.len(), 3);
        
        // Test shard mapping
        let mapping = router.calculate_shard_mapping();
        assert_eq!(mapping.len(), 256);
    }
}
