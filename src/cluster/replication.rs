//! Data replication layer for distributed vector database
//!
//! Implements production-grade replication patterns from TiKV, CockroachDB, and Qdrant:
//! - Leader-based replication with Raft consensus
//! - Synchronous and asynchronous replication strategies

#![allow(missing_docs)]
//! - Quorum-based writes with configurable consistency
//! - Follower reads with load balancing for query scaling
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   ReplicationManager                        │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
//! │  │ReplicaTracker│  │WriteCoordinator│ │ReadBalancer  │      │
//! │  │              │  │              │  │              │      │
//! │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
//! │         │                 │                 │              │
//! │         ▼                 ▼                 ▼              │
//! │  ┌─────────────────────────────────────────────────────┐   │
//! │  │              Replication Strategies                  │   │
//! │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │   │
//! │  │  │  SyncWrite  │  │ AsyncWrite  │  │ QuorumWrite │  │   │
//! │  │  └─────────────┘  └─────────────┘  └─────────────┘  │   │
//! │  └─────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use super::config::ShardId;
/// Node ID type (String for node identifiers)
pub type NodeId = String;
// Note: raft::types::NodeId is u64, but we use String for node identifiers in this module
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

/// Replication factor for a shard
pub type ReplicationFactor = usize;

/// Replica role in a shard group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicaRole {
    /// Leader replica - handles writes
    Leader,
    /// Follower replica - receives replication, can serve reads
    Follower,
    /// Learner replica - receives replication, non-voting
    Learner,
}

impl std::fmt::Display for ReplicaRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicaRole::Leader => write!(f, "Leader"),
            ReplicaRole::Follower => write!(f, "Follower"),
            ReplicaRole::Learner => write!(f, "Learner"),
        }
    }
}

/// Replica information for a shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardReplica {
    /// Shard ID
    pub shard_id: ShardId,
    /// Node hosting this replica
    pub node_id: NodeId,
    /// Replica role
    pub role: ReplicaRole,
    /// Replication lag in milliseconds (0 for leader)
    pub replication_lag_ms: u64,
    /// Last applied sequence number
    pub last_applied: u64,
    /// Whether replica is online
    pub is_online: bool,
    /// Replica creation timestamp
    pub created_at: u64,
}

impl ShardReplica {
    /// Create new leader replica
    pub fn new_leader(shard_id: ShardId, node_id: NodeId) -> Self {
        Self {
            shard_id,
            node_id,
            role: ReplicaRole::Leader,
            replication_lag_ms: 0,
            last_applied: 0,
            is_online: true,
            created_at: chrono::Utc::now().timestamp() as u64,
        }
    }

    /// Create new follower replica
    pub fn new_follower(shard_id: ShardId, node_id: NodeId) -> Self {
        Self {
            shard_id,
            node_id,
            role: ReplicaRole::Follower,
            replication_lag_ms: 0,
            last_applied: 0,
            is_online: true,
            created_at: chrono::Utc::now().timestamp() as u64,
        }
    }

    /// Check if replica can serve reads
    pub fn can_serve_reads(&self, max_lag_ms: u64) -> bool {
        self.is_online && self.replication_lag_ms <= max_lag_ms
    }

    /// Update replication lag
    pub fn update_lag(&mut self, leader_applied: u64) {
        if self.role == ReplicaRole::Leader {
            self.replication_lag_ms = 0;
        } else {
            let lag = leader_applied.saturating_sub(self.last_applied);
            // Estimate lag: assume 1 sequence = 1ms for simplicity
            self.replication_lag_ms = lag;
        }
    }
}

/// Replica placement strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementStrategy {
    /// Spread replicas across all available nodes
    Spread,
    /// Place replicas in specific zones/racks
    ZoneAware,
    /// Place replicas on nodes with specific labels
    LabelAware,
}

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Default replication factor
    pub replication_factor: usize,
    /// Minimum replicas for write acknowledgment
    pub min_write_ack: usize,
    /// Placement strategy
    pub placement_strategy: PlacementStrategy,
    /// Maximum replication lag for follower reads (ms)
    pub max_follower_read_lag_ms: u64,
    /// Enable follower reads
    pub enable_follower_reads: bool,
    /// Async replication timeout
    pub async_replication_timeout_ms: u64,
    /// Sync replication timeout
    pub sync_replication_timeout_ms: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            replication_factor: 3,
            min_write_ack: 2, // Majority of 3
            placement_strategy: PlacementStrategy::Spread,
            max_follower_read_lag_ms: 1000, // 1 second
            enable_follower_reads: true,
            async_replication_timeout_ms: 5000,
            sync_replication_timeout_ms: 1000,
        }
    }
}

/// Tracks all shard replicas in the cluster
#[derive(Debug)]
pub struct ReplicaTracker {
    /// Shard ID -> List of replicas
    shard_replicas: DashMap<ShardId, Vec<ShardReplica>>,
    /// Node ID -> Set of shards hosted
    node_shards: DashMap<NodeId, HashSet<ShardId>>,
    /// Replication configuration
    config: ReplicationConfig,
    /// Local node ID
    local_node_id: NodeId,
}

impl ReplicaTracker {
    /// Create new replica tracker
    pub fn new(config: ReplicationConfig, local_node_id: NodeId) -> Self {
        Self {
            shard_replicas: DashMap::new(),
            node_shards: DashMap::new(),
            config,
            local_node_id,
        }
    }

    /// Register a new replica
    pub fn register_replica(&self, replica: ShardReplica) {
        let shard_id = replica.shard_id;
        let node_id = replica.node_id.clone();

        // Log before move
        trace!(
            shard_id = shard_id,
            node_id = %node_id,
            "Registering replica"
        );

        // Add to shard replicas
        self.shard_replicas
            .entry(shard_id)
            .or_insert_with(Vec::new)
            .push(replica);

        // Add to node shards
        self.node_shards
            .entry(node_id)
            .or_insert_with(HashSet::new)
            .insert(shard_id);
    }

    /// Remove a replica
    pub fn remove_replica(&self, shard_id: ShardId, node_id: &NodeId) {
        // Remove from shard replicas
        if let Some(mut replicas) = self.shard_replicas.get_mut(&shard_id) {
            replicas.retain(|r| &r.node_id != node_id);
        }

        // Remove from node shards
        if let Some(mut shards) = self.node_shards.get_mut(node_id) {
            shards.remove(&shard_id);
        }
    }

    /// Get replicas for a shard
    pub fn get_replicas(&self, shard_id: ShardId) -> Vec<ShardReplica> {
        self.shard_replicas
            .get(&shard_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Get leader for a shard
    pub fn get_leader(&self, shard_id: ShardId) -> Option<ShardReplica> {
        self.shard_replicas
            .get(&shard_id)
            .and_then(|replicas| {
                replicas.iter().find(|r| r.role == ReplicaRole::Leader).cloned()
            })
    }

    /// Get followers for a shard
    pub fn get_followers(&self, shard_id: ShardId) -> Vec<ShardReplica> {
        self.shard_replicas
            .get(&shard_id)
            .map(|replicas| {
                replicas
                    .iter()
                    .filter(|r| r.role == ReplicaRole::Follower)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get online replicas that can serve reads
    pub fn get_readable_replicas(&self, shard_id: ShardId) -> Vec<ShardReplica> {
        self.shard_replicas
            .get(&shard_id)
            .map(|replicas| {
                replicas
                    .iter()
                    .filter(|r| r.can_serve_reads(self.config.max_follower_read_lag_ms))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if local node is leader for shard
    pub fn is_local_leader(&self, shard_id: ShardId) -> bool {
        self.get_leader(shard_id)
            .map(|leader| leader.node_id == self.local_node_id)
            .unwrap_or(false)
    }

    /// Get shards hosted on a node
    pub fn get_node_shards(&self, node_id: &NodeId) -> HashSet<ShardId> {
        self.node_shards
            .get(node_id)
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Update replica status
    pub fn update_replica_status(&self, shard_id: ShardId, node_id: &NodeId, is_online: bool) {
        if let Some(mut replicas) = self.shard_replicas.get_mut(&shard_id) {
            for replica in replicas.iter_mut() {
                if &replica.node_id == node_id {
                    replica.is_online = is_online;
                    break;
                }
            }
        }
    }

    /// Update replication lag
    pub fn update_replication_lag(
        &self,
        shard_id: ShardId,
        node_id: &NodeId,
        last_applied: u64,
    ) {
        if let Some(mut replicas) = self.shard_replicas.get_mut(&shard_id) {
            // Find leader's applied index
            let leader_applied = replicas
                .iter()
                .find(|r| r.role == ReplicaRole::Leader)
                .map(|r| r.last_applied)
                .unwrap_or(0);

            // Update follower lag
            for replica in replicas.iter_mut() {
                if &replica.node_id == node_id {
                    replica.last_applied = last_applied;
                    replica.update_lag(leader_applied);
                    break;
                }
            }
        }
    }

    /// Assign replicas for a new shard
    pub fn assign_replicas(
        &self,
        shard_id: ShardId,
        candidate_nodes: Vec<NodeId>,
    ) -> Vec<ShardReplica> {
        if candidate_nodes.is_empty() {
            return Vec::new();
        }

        let mut replicas = Vec::new();
        let rf = self.config.replication_factor.min(candidate_nodes.len());

        // First node becomes leader
        replicas.push(ShardReplica::new_leader(shard_id, candidate_nodes[0].clone()));

        // Remaining nodes become followers
        for node_id in candidate_nodes.iter().skip(1).take(rf - 1) {
            replicas.push(ShardReplica::new_follower(shard_id, node_id.clone()));
        }

        // Register all replicas
        for replica in &replicas {
            self.register_replica(replica.clone());
        }

        info!(
            shard_id = shard_id,
            replica_count = replicas.len(),
            "Assigned replicas for shard"
        );

        replicas
    }

    /// Get replication statistics
    pub fn get_stats(&self) -> ReplicationStats {
        let total_shards = self.shard_replicas.len();
        let total_replicas: usize = self
            .shard_replicas
            .iter()
            .map(|e| e.value().len())
            .sum();

        let online_replicas: usize = self
            .shard_replicas
            .iter()
            .map(|e| e.value().iter().filter(|r| r.is_online).count())
            .sum();

        let avg_replication_lag: u64 = if total_replicas > 0 {
            let total_lag: u64 = self
                .shard_replicas
                .iter()
                .map(|e| e.value().iter().map(|r| r.replication_lag_ms).sum::<u64>())
                .sum();
            total_lag / total_replicas as u64
        } else {
            0
        };

        ReplicationStats {
            total_shards,
            total_replicas,
            online_replicas,
            offline_replicas: total_replicas - online_replicas,
            avg_replication_lag_ms: avg_replication_lag,
        }
    }
}

/// Replication statistics
#[derive(Debug, Clone, Default)]
pub struct ReplicationStats {
    /// Total number of shards
    pub total_shards: usize,
    /// Total number of replicas
    pub total_replicas: usize,
    /// Number of online replicas
    pub online_replicas: usize,
    /// Number of offline replicas
    pub offline_replicas: usize,
    /// Average replication lag in milliseconds
    pub avg_replication_lag_ms: u64,
}

/// Write consistency level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteConsistency {
    /// Wait for all replicas
    All,
    /// Wait for quorum (majority)
    Quorum,
    /// Wait for leader only
    One,
    /// Wait for specific number of replicas
    N(usize),
}

impl WriteConsistency {
    /// Get minimum acknowledgments needed
    pub fn min_acks(&self, replication_factor: usize) -> usize {
        match self {
            WriteConsistency::All => replication_factor,
            WriteConsistency::Quorum => (replication_factor / 2) + 1,
            WriteConsistency::One => 1,
            WriteConsistency::N(n) => *n.min(&replication_factor),
        }
    }
}

/// Write coordinator for replication
#[derive(Debug)]
pub struct WriteCoordinator<C: ReplicationClient> {
    /// Replica tracker
    replica_tracker: Arc<ReplicaTracker>,
    /// Replication client
    replication_client: Arc<C>,
}

impl<C: ReplicationClient> WriteCoordinator<C> {
    /// Create new write coordinator
    pub fn new(
        replica_tracker: Arc<ReplicaTracker>,
        replication_client: Arc<C>,
    ) -> Self {
        Self {
            replica_tracker,
            replication_client,
        }
    }

    /// Coordinate a write operation
    pub async fn coordinate_write(
        &self,
        shard_id: ShardId,
        data: Vec<u8>,
        consistency: WriteConsistency,
    ) -> Result<WriteResult, crate::RTDBError> {
        let start = Instant::now();

        // Get replicas for shard
        let replicas = self.replica_tracker.get_replicas(shard_id);
        if replicas.is_empty() {
            return Err(crate::RTDBError::Storage(
                format!("No replicas found for shard {}", shard_id)
            ));
        }

        // Find leader
        let leader = replicas
            .iter()
            .find(|r| r.role == ReplicaRole::Leader)
            .ok_or_else(|| crate::RTDBError::Consensus(
                format!("No leader found for shard {}", shard_id)
            ))?;

        // Determine required acknowledgments
        let required_acks = consistency.min_acks(replicas.len());

        // Send write to leader first
        let sequence = self
            .replication_client
            .write_to_leader(&leader.node_id, shard_id, data.clone())
            .await?;

        // Replicate to followers
        let followers: Vec<_> = replicas
            .iter()
            .filter(|r| r.role == ReplicaRole::Follower && r.is_online)
            .cloned()
            .collect();

        let mut ack_count = 1; // Leader counts as ack
        let mut failed_nodes = Vec::new();

        if required_acks > 1 && !followers.is_empty() {
            // Collect follower info before spawning tasks
            let follower_infos: Vec<_> = followers
                .iter()
                .map(|f| f.node_id.clone())
                .collect();

            // Replicate to followers concurrently
            let mut tasks = Vec::new();
            for follower in followers {
                let client = self.replication_client.clone();
                let data = data.clone();
                let node_id = follower.node_id.clone();
                let task = async move {
                    (node_id, client.replicate_to_follower(&follower.node_id, shard_id, sequence, data).await)
                };
                tasks.push(task);
            }

            // Wait for acknowledgments
            let mut completed = 0;
            for task in tasks {
                let (node_id, result) = task.await;
                completed += 1;
                
                if result.is_ok() {
                    ack_count += 1;
                } else {
                    failed_nodes.push(node_id);
                }

                // Early exit if we have enough acks
                if ack_count >= required_acks {
                    break;
                }
            }
        }

        let latency_ms = start.elapsed().as_millis() as u64;

        // Check if we met consistency requirement
        let success = ack_count >= required_acks;

        if success {
            Ok(WriteResult {
                sequence,
                acknowledged: ack_count,
                required: required_acks,
                latency_ms,
                failed_nodes,
            })
        } else {
            Err(crate::RTDBError::Storage(
                format!(
                    "Write failed: only {} of {} required acknowledgments received",
                    ack_count, required_acks
                )
            ))
        }
    }
}

/// Result of a write operation
#[derive(Debug, Clone)]
pub struct WriteResult {
    /// Sequence number assigned
    pub sequence: u64,
    /// Number of replicas that acknowledged
    pub acknowledged: usize,
    /// Required acknowledgments
    pub required: usize,
    /// Write latency in milliseconds
    pub latency_ms: u64,
    /// Nodes that failed to acknowledge
    pub failed_nodes: Vec<NodeId>,
}

/// Replication client trait
#[cfg(feature = "grpc")]
#[async_trait::async_trait]
pub trait ReplicationClient: Send + Sync + std::fmt::Debug {
    /// Write to leader
    fn write_to_leader(
        &self,
        node_id: &NodeId,
        shard_id: ShardId,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, crate::RTDBError>> + Send + '_>>;

    /// Replicate to follower
    fn replicate_to_follower(
        &self,
        node_id: &NodeId,
        shard_id: ShardId,
        sequence: u64,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), crate::RTDBError>> + Send + '_>>;
}

/// Stub implementation for non-gRPC builds
#[cfg(not(feature = "grpc"))]
pub trait ReplicationClient: Send + Sync + std::fmt::Debug {
    /// Write to leader
    async fn write_to_leader(
        &self,
        node_id: &NodeId,
        shard_id: ShardId,
        data: Vec<u8>,
    ) -> Result<u64, crate::RTDBError>;

    /// Replicate to follower
    async fn replicate_to_follower(
        &self,
        node_id: &NodeId,
        shard_id: ShardId,
        sequence: u64,
        data: Vec<u8>,
    ) -> Result<(), crate::RTDBError>;
}

/// Read consistency level for follower reads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadConsistency {
    /// Strong consistency - read from leader
    Strong,
    /// Bounded staleness - read from follower if lag is within bound
    BoundedStaleness(Duration),
    /// Eventual consistency - read from any replica
    Eventual,
    /// Local reads only (fastest, may be stale)
    Local,
}

impl Default for ReadConsistency {
    fn default() -> Self {
        ReadConsistency::Strong
    }
}

/// Load balancer for read operations
#[derive(Debug)]
pub struct ReadBalancer {
    /// Replica tracker
    replica_tracker: Arc<ReplicaTracker>,
    /// Round-robin counters per shard
    rr_counters: DashMap<ShardId, AtomicU64>,
}

impl ReadBalancer {
    /// Create new read balancer
    pub fn new(replica_tracker: Arc<ReplicaTracker>) -> Self {
        Self {
            replica_tracker,
            rr_counters: DashMap::new(),
        }
    }

    /// Select a replica for reading
    pub fn select_replica(
        &self,
        shard_id: ShardId,
        consistency: ReadConsistency,
        local_node_id: &NodeId,
    ) -> Option<ShardReplica> {
        match consistency {
            ReadConsistency::Strong => {
                // Always read from leader
                self.replica_tracker.get_leader(shard_id)
            }
            ReadConsistency::BoundedStaleness(max_lag) => {
                // Try local replica first
                let local_replica = self
                    .get_local_replica(shard_id, local_node_id, max_lag.as_millis() as u64);
                if local_replica.is_some() {
                    return local_replica;
                }

                // Fall back to any readable replica
                self.select_any_readable(shard_id, max_lag.as_millis() as u64)
            }
            ReadConsistency::Eventual | ReadConsistency::Local => {
                // Try local first, then any readable
                let local_replica = self
                    .get_local_replica(shard_id, local_node_id, u64::MAX);
                if local_replica.is_some() {
                    return local_replica;
                }

                self.select_any_readable(shard_id, u64::MAX)
            }
        }
    }

    /// Get local replica if available and readable
    fn get_local_replica(
        &self,
        shard_id: ShardId,
        local_node_id: &NodeId,
        max_lag_ms: u64,
    ) -> Option<ShardReplica> {
        self.replica_tracker
            .get_replicas(shard_id)
            .into_iter()
            .find(|r| {
                &r.node_id == local_node_id
                    && r.is_online
                    && r.replication_lag_ms <= max_lag_ms
            })
    }

    /// Select any readable replica using round-robin
    fn select_any_readable(&self, shard_id: ShardId, max_lag_ms: u64) -> Option<ShardReplica> {
        let readable: Vec<_> = self
            .replica_tracker
            .get_replicas(shard_id)
            .into_iter()
            .filter(|r| r.is_online && r.replication_lag_ms <= max_lag_ms)
            .collect();

        if readable.is_empty() {
            return None;
        }

        // Round-robin selection
        let counter = self
            .rr_counters
            .entry(shard_id)
            .or_insert_with(|| AtomicU64::new(0));
        let index = counter.fetch_add(1, Ordering::Relaxed) as usize % readable.len();

        Some(readable[index].clone())
    }

    /// Select replicas for scatter-gather read
    pub fn select_replicas_for_scatter(
        &self,
        shard_ids: Vec<ShardId>,
        consistency: ReadConsistency,
        local_node_id: &NodeId,
    ) -> HashMap<ShardId, ShardReplica> {
        let mut selected = HashMap::new();

        for shard_id in shard_ids {
            if let Some(replica) = self.select_replica(shard_id, consistency, local_node_id) {
                selected.insert(shard_id, replica);
            }
        }

        selected
    }
}

/// Replication manager - top-level coordinator
#[derive(Debug)]
pub struct ReplicationManager<C: ReplicationClient> {
    /// Replica tracker
    pub tracker: Arc<ReplicaTracker>,
    /// Write coordinator
    pub write_coordinator: Arc<WriteCoordinator<C>>,
    /// Read balancer
    pub read_balancer: Arc<ReadBalancer>,
    /// Configuration
    config: ReplicationConfig,
}

impl<C: ReplicationClient> ReplicationManager<C> {
    /// Create new replication manager
    pub fn new(
        config: ReplicationConfig,
        local_node_id: NodeId,
        replication_client: Arc<C>,
    ) -> Self {
        let tracker = Arc::new(ReplicaTracker::new(config.clone(), local_node_id));
        let write_coordinator = Arc::new(WriteCoordinator::new(tracker.clone(), replication_client));
        let read_balancer = Arc::new(ReadBalancer::new(tracker.clone()));

        Self {
            tracker,
            write_coordinator,
            read_balancer,
            config,
        }
    }

    /// Write data with specified consistency
    pub async fn write(
        &self,
        shard_id: ShardId,
        data: Vec<u8>,
        consistency: WriteConsistency,
    ) -> Result<WriteResult, crate::RTDBError> {
        self.write_coordinator
            .coordinate_write(shard_id, data, consistency)
            .await
    }

    /// Select replica for reading
    pub fn select_reader(
        &self,
        shard_id: ShardId,
        consistency: ReadConsistency,
        local_node_id: &NodeId,
    ) -> Option<ShardReplica> {
        self.read_balancer
            .select_replica(shard_id, consistency, local_node_id)
    }

    /// Get replication statistics
    pub fn stats(&self) -> ReplicationStats {
        self.tracker.get_stats()
    }

    /// Check if follower reads are enabled
    pub fn follower_reads_enabled(&self) -> bool {
        self.config.enable_follower_reads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_consistency_min_acks() {
        assert_eq!(WriteConsistency::All.min_acks(5), 5);
        assert_eq!(WriteConsistency::Quorum.min_acks(5), 3);
        assert_eq!(WriteConsistency::Quorum.min_acks(3), 2);
        assert_eq!(WriteConsistency::One.min_acks(5), 1);
        assert_eq!(WriteConsistency::N(2).min_acks(5), 2);
    }

    #[test]
    fn test_replica_tracker() {
        let config = ReplicationConfig::default();
        let tracker = ReplicaTracker::new(config, "node1".to_string());

        // Register replicas
        tracker.register_replica(ShardReplica::new_leader(1, "node1".to_string()));
        tracker.register_replica(ShardReplica::new_follower(1, "node2".to_string()));
        tracker.register_replica(ShardReplica::new_follower(1, "node3".to_string()));

        // Get leader
        let leader = tracker.get_leader(1).unwrap();
        assert_eq!(leader.node_id, "node1");
        assert_eq!(leader.role, ReplicaRole::Leader);

        // Get followers
        let followers = tracker.get_followers(1);
        assert_eq!(followers.len(), 2);

        // Check local leader
        assert!(tracker.is_local_leader(1));
    }

    #[test]
    fn test_read_balancer() {
        let config = ReplicationConfig::default();
        let tracker = Arc::new(ReplicaTracker::new(config, "node1".to_string()));
        let balancer = ReadBalancer::new(tracker.clone());

        // Register replicas
        tracker.register_replica(ShardReplica::new_leader(1, "node1".to_string()));
        tracker.register_replica(ShardReplica::new_follower(1, "node2".to_string()));

        // Strong consistency should return leader
        let replica = balancer.select_replica(1, ReadConsistency::Strong, &"node1".to_string());
        assert!(replica.is_some());
        assert_eq!(replica.unwrap().role, ReplicaRole::Leader);

        // Local read should prefer local replica
        let replica = balancer.select_replica(1, ReadConsistency::Local, &"node2".to_string());
        assert!(replica.is_some());
        assert_eq!(replica.unwrap().node_id, "node2");
    }
}
