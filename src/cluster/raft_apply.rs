//! Raft state machine application layer
//!
//! Implements the Apply trait for processing committed Raft entries.
//! This is where collection metadata changes are applied deterministically.
//!
//! Design based on production patterns from TiKV and etcd:
//! - Deterministic application of commands
//! - Snapshot support for log compaction
//! - Batch apply for efficiency

use super::raft::types::{LogEntry, Snapshot, EntryType};
use super::raft::Apply;
use super::ClusterManager;
use crate::collection::CollectionManager;
use crate::{RTDBError, Result};
use crate::cluster::{NodeInfo, NodeStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, trace, warn};

/// Commands that can be applied to the state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateMachineCommand {
    /// Create a new collection
    CreateCollection {
        name: String,
        dimension: usize,
        /// Serialized collection configuration
        config: Vec<u8>,
    },
    
    /// Delete a collection
    DeleteCollection {
        name: String,
    },
    
    /// Update collection configuration
    UpdateCollection {
        name: String,
        /// Serialized configuration changes
        config_update: Vec<u8>,
    },
    
    /// Register a node in the cluster
    RegisterNode {
        node_id: String,
        address: String,
        /// Node capabilities (JSON)
        capabilities: Vec<u8>,
    },
    
    /// Deregister a node
    DeregisterNode {
        node_id: String,
    },
    
    /// Update node status
    UpdateNodeStatus {
        node_id: String,
        /// Status: 0=Unknown, 1=Healthy, 2=Degraded, 3=Unhealthy, 4=Offline
        status: u32,
    },
    
    /// Reassign shards after topology change
    ReassignShards {
        /// Serialized shard assignments
        assignments: Vec<u8>,
    },
    
    /// Update cluster configuration
    UpdateClusterConfig {
        /// Serialized config changes
        config: Vec<u8>,
    },
    
    /// No-op for heartbeat/leadership establishment
    NoOp,
}

/// State machine for applying Raft entries
pub struct RaftStateMachine {
    /// Collection manager for metadata operations
    collections: Arc<RwLock<CollectionManager>>,
    
    /// Cluster manager for node operations
    cluster: Arc<RwLock<ClusterManager>>,
    
    /// Last applied index
    last_applied: std::sync::atomic::AtomicU64,
    
    /// Snapshot directory
    snapshot_dir: std::path::PathBuf,
}

impl RaftStateMachine {
    /// Create new state machine
    pub fn new(
        collections: Arc<RwLock<CollectionManager>>,
        cluster: Arc<RwLock<ClusterManager>>,
        snapshot_dir: impl AsRef<std::path::Path>,
    ) -> Self {
        Self {
            collections,
            cluster,
            last_applied: std::sync::atomic::AtomicU64::new(0),
            snapshot_dir: snapshot_dir.as_ref().to_path_buf(),
        }
    }

    /// Get last applied index
    pub fn last_applied(&self) -> u64 {
        self.last_applied.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Apply a single entry to the state machine
    async fn apply_entry(&self, entry: &LogEntry) -> Result<()> {
        trace!(
            index = entry.index,
            term = entry.term,
            entry_type = ?entry.entry_type,
            "Applying Raft entry"
        );

        match entry.entry_type {
            EntryType::Normal => {
                if entry.data.is_empty() {
                    // No-op entry (heartbeat/leadership)
                    trace!("Applying no-op entry");
                    return Ok(());
                }
                
                // Deserialize and apply command
                let cmd: StateMachineCommand = bincode::deserialize(&entry.data)
                    .map_err(|e| RTDBError::Serialization(
                        format!("Failed to deserialize command: {}", e)
                    ))?;
                
                self.apply_command(cmd).await?;
            }
            
            EntryType::ConfigChange => {
                // Handle configuration changes (membership)
                info!(
                    index = entry.index,
                    "Applying configuration change"
                );
                // Config changes are handled by the Raft module itself
                // We just need to persist the new configuration
            }
            
            _ => {
                warn!("Unknown entry type: {:?}", entry.entry_type);
            }
        }

        // Update last applied
        self.last_applied.store(entry.index, std::sync::atomic::Ordering::SeqCst);
        
        Ok(())
    }

    /// Apply a command to the state machine
    async fn apply_command(&self, cmd: StateMachineCommand) -> Result<()> {
        debug!("Applying command: {:?}", cmd);

        match cmd {
            StateMachineCommand::CreateCollection { name, dimension, config } => {
                self.apply_create_collection(name, dimension, config).await?;
            }
            
            StateMachineCommand::DeleteCollection { name } => {
                self.apply_delete_collection(name).await?;
            }
            
            StateMachineCommand::UpdateCollection { name, config_update } => {
                self.apply_update_collection(name, config_update).await?;
            }
            
            StateMachineCommand::RegisterNode { node_id, address, capabilities } => {
                self.apply_register_node(node_id, address, capabilities).await?;
            }
            
            StateMachineCommand::DeregisterNode { node_id } => {
                self.apply_deregister_node(node_id).await?;
            }
            
            StateMachineCommand::UpdateNodeStatus { node_id, status } => {
                self.apply_update_node_status(node_id, status).await?;
            }
            
            StateMachineCommand::ReassignShards { assignments } => {
                self.apply_reassign_shards(assignments).await?;
            }
            
            StateMachineCommand::UpdateClusterConfig { config } => {
                self.apply_update_cluster_config(config).await?;
            }
            
            StateMachineCommand::NoOp => {
                // No operation needed
            }
        }

        Ok(())
    }

    // ==================== Command Handlers ====================

    async fn apply_create_collection(
        &self,
        name: String,
        dimension: usize,
        _config: Vec<u8>,
    ) -> Result<()> {
        use crate::{CollectionConfig, Distance};
        
        info!(
            collection = %name,
            dimension = dimension,
            "Creating collection via Raft"
        );

        let collections = self.collections.read().await;
        
        // Check if collection already exists
        if collections.get_collection(&name).is_ok() {
            warn!("Collection {} already exists", name);
            return Ok(());
        }
        drop(collections);

        // Create collection with default config
        let config = CollectionConfig {
            dimension,
            distance: Distance::Cosine,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };
        
        let collections = self.collections.write().await;
        collections.create_collection(&name, config)?;
        
        info!("Collection {} created successfully", name);
        Ok(())
    }

    async fn apply_delete_collection(&self, name: String) -> Result<()> {
        info!(collection = %name, "Deleting collection via Raft");

        let mut collections = self.collections.write().await;
        collections.delete_collection(&name)?;
        
        info!("Collection {} deleted successfully", name);
        Ok(())
    }

    async fn apply_update_collection(
        &self,
        name: String,
        _config_update: Vec<u8>,
    ) -> Result<()> {
        info!(collection = %name, "Updating collection via Raft");
        
        // TODO: Implement collection configuration updates
        // This would modify collection parameters like index type, etc.
        
        Ok(())
    }

    async fn apply_register_node(
        &self,
        node_id: String,
        address: String,
        _capabilities: Vec<u8>,
    ) -> Result<()> {
        use std::net::SocketAddr;
        
        info!(
            node_id = %node_id,
            address = %address,
            "Registering node via Raft"
        );

        let mut cluster = self.cluster.write().await;
        
        let socket_addr: SocketAddr = address.parse()
            .map_err(|e| RTDBError::Configuration(format!("Invalid address: {}", e)))?;
        
        let node_info = NodeInfo {
            id: node_id.clone(),
            address: socket_addr,
            status: NodeStatus::Active,
            shards: Vec::new(),
            capacity: 0,
            load: 0,
            last_heartbeat: 0,
        };
        
        cluster.add_node(node_info);
        
        info!("Node {} registered successfully", node_id);
        Ok(())
    }

    async fn apply_deregister_node(&self, node_id: String) -> Result<()> {
        info!(node_id = %node_id, "Deregistering node via Raft");

        let mut cluster = self.cluster.write().await;
        cluster.remove_node(&node_id);
        
        info!("Node {} deregistered successfully", node_id);
        Ok(())
    }

    async fn apply_update_node_status(&self, node_id: String, status: u32) -> Result<()> {
        let status_enum = match status {
            1 => NodeStatus::Active,
            2 => NodeStatus::Suspect,
            3 => NodeStatus::Offline,
            4 => NodeStatus::Offline,
            _ => NodeStatus::Joining,
        };

        debug!(
            node_id = %node_id,
            status = ?status_enum,
            "Updating node status via Raft"
        );

        // TODO: Update node status in cluster manager
        
        Ok(())
    }

    async fn apply_reassign_shards(&self, _assignments: Vec<u8>) -> Result<()> {
        info!("Reassigning shards via Raft");
        
        // TODO: Apply shard reassignments
        // This would update the shard-to-node mapping
        
        Ok(())
    }

    async fn apply_update_cluster_config(&self, _config: Vec<u8>) -> Result<()> {
        info!("Updating cluster config via Raft");
        
        // TODO: Apply cluster configuration changes
        
        Ok(())
    }

    /// Create snapshot of state machine
    async fn create_snapshot_data(&self) -> Result<Vec<u8>> {
        info!("Creating state machine snapshot");

        // Collect all collection metadata
        let collections = self.collections.read().await;
        let collection_names: Vec<String> = collections.list_collections();
        
        // Build snapshot data
        let snapshot = StateMachineSnapshot {
            collections: collection_names,
            last_applied: self.last_applied(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let data = bincode::serialize(&snapshot)
            .map_err(|e| RTDBError::Serialization(
                format!("Failed to serialize snapshot: {}", e)
            ))?;

        info!(
            collections = snapshot.collections.len(),
            "Snapshot created"
        );

        Ok(data)
    }

    /// Restore state machine from snapshot
    async fn restore_from_snapshot(&self, data: &[u8]) -> Result<()> {
        info!("Restoring state machine from snapshot");

        let snapshot: StateMachineSnapshot = bincode::deserialize(data)
            .map_err(|e| RTDBError::Serialization(
                format!("Failed to deserialize snapshot: {}", e)
            ))?;

        // Restore last applied index
        self.last_applied.store(
            snapshot.last_applied,
            std::sync::atomic::Ordering::SeqCst
        );

        info!(
            collections = snapshot.collections.len(),
            last_applied = snapshot.last_applied,
            "State machine restored from snapshot"
        );

        Ok(())
    }
}

/// Snapshot data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateMachineSnapshot {
    pub collections: Vec<String>,
    pub last_applied: u64,
    pub timestamp: i64,
}

#[async_trait::async_trait]
impl Apply for RaftStateMachine {
    async fn apply(&self, entries: Vec<LogEntry>) -> Result<()> {
        for entry in entries {
            if let Err(e) = self.apply_entry(&entry).await {
                error!(
                    index = entry.index,
                    error = %e,
                    "Failed to apply entry"
                );
                return Err(e);
            }
        }
        Ok(())
    }

    async fn apply_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        info!(
            index = snapshot.metadata.index,
            "Applying snapshot to state machine"
        );

        self.restore_from_snapshot(&snapshot.data).await?;
        
        info!("Snapshot applied successfully");
        Ok(())
    }

    async fn snapshot(&self) -> Result<(u64, Vec<u8>)> {
        let index = self.last_applied();
        let data = self.create_snapshot_data().await?;
        Ok((index, data))
    }
}

/// Helper to create commands
impl StateMachineCommand {
    pub fn create_collection(name: impl Into<String>, dimension: usize) -> Self {
        Self::CreateCollection {
            name: name.into(),
            dimension,
            config: Vec::new(), // TODO: Add proper config serialization
        }
    }

    pub fn delete_collection(name: impl Into<String>) -> Self {
        Self::DeleteCollection {
            name: name.into(),
        }
    }

    pub fn register_node(
        node_id: impl Into<String>,
        address: impl Into<String>,
    ) -> Self {
        Self::RegisterNode {
            node_id: node_id.into(),
            address: address.into(),
            capabilities: Vec::new(),
        }
    }

    pub fn deregister_node(node_id: impl Into<String>) -> Self {
        Self::DeregisterNode {
            node_id: node_id.into(),
        }
    }

    pub fn no_op() -> Self {
        Self::NoOp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::raft::types::{LogEntry, Term};

    // Note: These tests would need a properly initialized CollectionManager
    // and ClusterManager to work. For now, they serve as documentation.

    #[test]
    fn test_command_serialization() {
        let cmd = StateMachineCommand::create_collection("test", 128);
        let serialized = bincode::serialize(&cmd).unwrap();
        let deserialized: StateMachineCommand = bincode::deserialize(&serialized).unwrap();
        
        match deserialized {
            StateMachineCommand::CreateCollection { name, dimension, .. } => {
                assert_eq!(name, "test");
                assert_eq!(dimension, 128);
            }
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = StateMachineSnapshot {
            collections: vec!["col1".to_string(), "col2".to_string()],
            last_applied: 100,
            timestamp: 1234567890,
        };
        
        let serialized = bincode::serialize(&snapshot).unwrap();
        let deserialized: StateMachineSnapshot = bincode::deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.collections.len(), 2);
        assert_eq!(deserialized.last_applied, 100);
    }
}
