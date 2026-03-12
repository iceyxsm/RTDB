//! Raft Consensus Implementation
//!
//! A production-grade Raft consensus module for RTDB cluster coordination.
//!
//! ## Architecture

#![allow(missing_docs)]
//!
//! The implementation follows the TiKV/etcd design patterns:
//!
//! - **Core State Machine** (`RaftNode`): Handles leader election, log replication,
//!   and commit processing.
//! - **Storage Trait** (`Storage`): Pluggable persistence layer (memory/file/rocksdb).
//! - **Async Runtime** (`RaftRuntime`): Drives the Raft state machine with timers,
//!   handles I/O, and applies committed entries.
//!
//! ## Key Features
//!
//! - **Pre-vote**: Prevents disrupted leaders from causing term explosions
//! - **Check Quorum**: Leader steps down when quorum is lost
//! - **Read Index**: Linearizable reads without log entries
//! - **Batch Apply**: Multiple entries applied together for efficiency
//! - **Joint Consensus**: Safe membership changes
//!
//! ## Usage
//!
//! ```ignore
//! // Create storage and configuration
//! let storage = Arc::new(MemStorage::new());
//! let config = Config { id: 1, ..Default::default() };
//!
//! // Create Raft node
//! let raft = RaftNode::new(config, storage.as_ref())?;
//!
//! // Drive with runtime
//! let runtime = RaftRuntime::new(raft, network, apply_tx);
//! runtime.run().await?;
//! ```

pub mod types;
pub mod node;
pub mod storage;

#[cfg(feature = "grpc")]
pub mod runtime;

pub use types::*;
pub use node::RaftNode;
pub use storage::{MemStorage, FileStorage};

#[cfg(feature = "grpc")]
pub use runtime::RaftRuntime;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

/// Propose a command to the Raft cluster
#[cfg(feature = "grpc")]
pub async fn propose(
    sender: mpsc::UnboundedSender<RaftCommand>,
    data: Vec<u8>,
) -> crate::Result<oneshot::Receiver<ProposeResult>> {
    let (tx, rx) = oneshot::channel();
    sender.send(RaftCommand::Propose { data, respond_to: tx })
        .map_err(|_| crate::RTDBError::Consensus("Failed to send propose command".to_string()))?;
    Ok(rx)
}

/// Read index for linearizable reads
#[cfg(feature = "grpc")]
pub async fn read_index(
    sender: mpsc::UnboundedSender<RaftCommand>,
    ctx: Vec<u8>,
) -> crate::Result<oneshot::Receiver<ReadIndexResult>> {
    let (tx, rx) = oneshot::channel();
    sender.send(RaftCommand::ReadIndex { ctx, respond_to: tx })
        .map_err(|_| crate::RTDBError::Consensus("Failed to send read index command".to_string()))?;
    Ok(rx)
}

/// Commands sent to Raft runtime
#[derive(Debug)]
pub enum RaftCommand {
    /// Propose a new entry
    Propose {
        data: Vec<u8>,
        respond_to: oneshot::Sender<ProposeResult>,
    },
    /// Request read index
    ReadIndex {
        ctx: Vec<u8>,
        respond_to: oneshot::Sender<ReadIndexResult>,
    },
    /// Get current status
    Status {
        respond_to: oneshot::Sender<RaftStatus>,
    },
    /// Step down from leadership
    StepDown,
    /// Trigger snapshot
    Snapshot,
    /// Shutdown
    Shutdown,
}

/// Result of a propose operation
#[derive(Debug, Clone)]
pub struct ProposeResult {
    pub index: LogIndex,
    pub term: Term,
}

/// Result of read index
#[derive(Debug, Clone)]
pub struct ReadIndexResult {
    pub index: LogIndex,
    pub term: Term,
}

/// Raft node status
#[derive(Debug, Clone)]
pub struct RaftStatus {
    pub id: NodeId,
    pub state: RaftState,
    pub term: Term,
    pub leader_id: NodeId,
    pub commit_index: LogIndex,
    pub applied_index: LogIndex,
    pub last_index: LogIndex,
}

/// Network transport trait for sending Raft messages
#[cfg(feature = "grpc")]
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Send message to specific node
    async fn send(&self, to: NodeId, msg: Message) -> crate::Result<()>;
    
    /// Broadcast message to all nodes except self
    async fn broadcast(&self, from: NodeId, msg: Message) -> crate::Result<()>;
    
    /// Get addresses of all nodes
    fn node_addresses(&self) -> Vec<(NodeId, String)>;
}

/// Apply trait for committed entries
#[cfg(feature = "grpc")]
#[async_trait::async_trait]
pub trait Apply: Send + Sync {
    /// Apply committed entries to state machine
    async fn apply(&self, entries: Vec<LogEntry>) -> crate::Result<()>;
    
    /// Apply snapshot to state machine
    async fn apply_snapshot(&self, snapshot: Snapshot) -> crate::Result<()>;
    
    /// Generate snapshot
    async fn snapshot(&self) -> crate::Result<(LogIndex, Vec<u8>)>;
}

/// Snapshot trigger configuration
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Create snapshot every N entries
    pub interval: u64,
    /// Max retained log entries
    pub max_retained: u64,
    /// Min interval between snapshots
    pub min_interval: std::time::Duration,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            interval: 10000,
            max_retained: 1000,
            min_interval: std::time::Duration::from_secs(60),
        }
    }
}

/// Helper to create standard 3-node configuration
pub fn three_node_config(node_id: NodeId) -> Config {
    Config {
        id: node_id,
        election_timeout_min: std::time::Duration::from_millis(150),
        election_timeout_max: std::time::Duration::from_millis(300),
        heartbeat_interval: std::time::Duration::from_millis(50),
        max_msg_size: 1024 * 1024,
        max_inflight: 256,
        pre_vote: true,
        check_quorum: true,
        batch_apply: true,
    }
}

/// Helper to create 5-node configuration for higher availability
pub fn five_node_config(node_id: NodeId) -> Config {
    Config {
        id: node_id,
        election_timeout_min: std::time::Duration::from_millis(200),
        election_timeout_max: std::time::Duration::from_millis(400),
        heartbeat_interval: std::time::Duration::from_millis(50),
        max_msg_size: 1024 * 1024,
        max_inflight: 256,
        pre_vote: true,
        check_quorum: true,
        batch_apply: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_helpers() {
        let config = three_node_config(1);
        assert_eq!(config.id, 1);
        assert!(config.pre_vote);
        assert!(config.check_quorum);
        
        let config5 = five_node_config(2);
        assert_eq!(config5.id, 2);
    }
}
