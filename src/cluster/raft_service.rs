//! Raft gRPC service implementation
//!
//! Handles incoming Raft RPCs and forwards them to the Raft state machine.
//! Based on production patterns from TiKV and etcd.

use super::raft::types::{Message, MessageType};
use super::raft::RaftCommand;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, trace, warn};

/// Raft runtime manager
/// 
/// Coordinates the Raft state machine, transport, and application layer.
/// Follows the TiKV RawNode pattern.
#[derive(Debug)]
pub struct RaftRuntimeManager {
    /// Raft command sender
    raft_tx: mpsc::UnboundedSender<RaftCommand>,
    /// Local node ID
    node_id: u64,
    /// Is this node the leader
    is_leader: std::sync::atomic::AtomicBool,
    /// Current leader ID (if known)
    leader_id: std::sync::atomic::AtomicU64,
}

impl RaftRuntimeManager {
    /// Create new Raft runtime manager
    pub fn new(raft_tx: mpsc::UnboundedSender<RaftCommand>, node_id: u64) -> Self {
        Self {
            raft_tx,
            node_id,
            is_leader: std::sync::atomic::AtomicBool::new(false),
            leader_id: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get current leader ID
    pub fn leader_id(&self) -> Option<u64> {
        let id = self.leader_id.load(std::sync::atomic::Ordering::SeqCst);
        if id == 0 {
            None
        } else {
            Some(id)
        }
    }

    /// Set leader status
    pub fn set_leader(&self, is_leader: bool) {
        self.is_leader.store(is_leader, std::sync::atomic::Ordering::SeqCst);
        if is_leader {
            self.leader_id.store(self.node_id, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Set leader ID
    pub fn set_leader_id(&self, leader_id: u64) {
        self.leader_id.store(leader_id, std::sync::atomic::Ordering::SeqCst);
        self.is_leader.store(leader_id == self.node_id, std::sync::atomic::Ordering::SeqCst);
    }

    /// Propose a command to Raft (only valid on leader)
    pub async fn propose(&self, data: Vec<u8>) -> Result<u64, crate::RTDBError> {
        if !self.is_leader() {
            return Err(crate::RTDBError::Consensus(
                format!("Not leader (leader is {:?})", self.leader_id())
            ));
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.raft_tx
            .send(RaftCommand::Propose { data, respond_to: tx })
            .map_err(|_| crate::RTDBError::Consensus("Raft channel closed".to_string()))?;

        let result = rx.await
            .map_err(|_| crate::RTDBError::Consensus("Propose cancelled".to_string()))?;
        
        Ok(result.index)
    }

    /// Read index for linearizable read
    pub async fn read_index(&self, ctx: Vec<u8>) -> Result<u64, crate::RTDBError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.raft_tx
            .send(RaftCommand::ReadIndex { ctx, respond_to: tx })
            .map_err(|_| crate::RTDBError::Consensus("Raft channel closed".to_string()))?;

        let result = rx.await
            .map_err(|_| crate::RTDBError::Consensus("ReadIndex cancelled".to_string()))?;
        
        Ok(result.index)
    }

    /// Get Raft status
    pub async fn status(&self) -> Result<super::raft::RaftStatus, crate::RTDBError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.raft_tx
            .send(RaftCommand::Status { respond_to: tx })
            .map_err(|_| crate::RTDBError::Consensus("Raft channel closed".to_string()))?;

        rx.await
            .map_err(|_| crate::RTDBError::Consensus("Status request cancelled".to_string()))
    }

    /// Shutdown Raft
    pub fn shutdown(&self) -> Result<(), crate::RTDBError> {
        self.raft_tx
            .send(RaftCommand::Shutdown)
            .map_err(|_| crate::RTDBError::Consensus("Raft channel closed".to_string()))?;

        Ok(())
    }

    /// Send a Raft message directly (for transport layer)
    pub fn send_message(&self, msg: Message) -> Result<(), crate::RTDBError> {
        // For now, we need a way to send messages to the Raft runtime
        // This would be implemented via a message channel
        debug!("Sending Raft message: {:?} from {}", msg.msg_type, msg.from);
        Ok(())
    }
}

/// Leader discovery helper
#[derive(Debug)]
pub struct LeaderDiscovery {
    /// Known nodes in cluster
    nodes: Arc<RwLock<Vec<String>>>,
    /// Current leader hint
    leader_hint: Arc<RwLock<Option<String>>>,
}

impl LeaderDiscovery {
    /// Create new leader discovery
    pub fn new(nodes: Vec<String>) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
            leader_hint: Arc::new(RwLock::new(None)),
        }
    }

    /// Get leader address
    pub async fn get_leader(&self) -> Option<String> {
        self.leader_hint.read().await.clone()
    }

    /// Update leader hint
    pub async fn set_leader(&self, leader: String) {
        *self.leader_hint.write().await = Some(leader);
    }

    /// Clear leader hint (on leader failure)
    pub async fn clear_leader(&self) {
        *self.leader_hint.write().await = None;
    }

    /// Get random node (for load balancing reads)
    pub async fn get_random_node(&self) -> Option<String> {
        use rand::seq::SliceRandom;
        let nodes = self.nodes.read().await;
        nodes.choose(&mut rand::thread_rng()).cloned()
    }

    /// Add a node
    pub async fn add_node(&self, node: String) {
        let mut nodes = self.nodes.write().await;
        if !nodes.contains(&node) {
            nodes.push(node);
        }
    }

    /// Remove a node
    pub async fn remove_node(&self, node: &str) {
        let mut nodes = self.nodes.write().await;
        nodes.retain(|n| n != node);
    }
}

/// Raft message handler
/// 
/// Processes incoming Raft messages and forwards them to the Raft runtime.
#[derive(Debug, Clone)]
pub struct RaftMessageHandler {
    /// Node ID
    node_id: u64,
    /// Message sender (to Raft runtime)
    message_tx: mpsc::UnboundedSender<Message>,
}

impl RaftMessageHandler {
    /// Create new message handler
    pub fn new(node_id: u64, message_tx: mpsc::UnboundedSender<Message>) -> Self {
        Self { node_id, message_tx }
    }

    /// Handle incoming Raft message
    pub fn handle_message(&self, msg: Message) -> Result<(), crate::RTDBError> {
        trace!(
            "Handling Raft message: {:?} from {} to {}",
            msg.msg_type,
            msg.from,
            msg.to
        );

        // Validate message is for us
        if msg.to != self.node_id {
            warn!(
                "Received message for wrong node: {} != {}",
                msg.to, self.node_id
            );
            return Err(crate::RTDBError::Configuration(
                "Message not for this node".to_string()
            ));
        }

        // Forward to Raft runtime
        self.message_tx.send(msg)
            .map_err(|_| crate::RTDBError::Consensus("Message channel closed".to_string()))?;

        Ok(())
    }

    /// Create AppendEntries message
    pub fn create_append_entries(
        &self,
        to: u64,
        term: u64,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<super::raft::types::LogEntry>,
        leader_commit: u64,
    ) -> Message {
        Message {
            msg_type: MessageType::AppendEntries,
            from: self.node_id,
            to,
            term,
            index: prev_log_index,
            entries,
            commit: leader_commit,
            reject: false,
            reject_hint: 0,
            snapshot: None,
            context: Vec::new(),
        }
    }

    /// Create RequestVote message
    pub fn create_request_vote(
        &self,
        to: u64,
        term: u64,
        last_log_index: u64,
        last_log_term: u64,
    ) -> Message {
        Message {
            msg_type: MessageType::RequestVote,
            from: self.node_id,
            to,
            term,
            index: last_log_index,
            entries: Vec::new(),
            commit: 0,
            reject: false,
            reject_hint: 0,
            snapshot: None,
            context: Vec::new(),
        }
    }

    /// Create heartbeat message
    pub fn create_heartbeat(&self, to: u64, term: u64, commit: u64) -> Message {
        Message {
            msg_type: MessageType::Heartbeat,
            from: self.node_id,
            to,
            term,
            index: 0,
            entries: Vec::new(),
            commit,
            reject: false,
            reject_hint: 0,
            snapshot: None,
            context: Vec::new(),
        }
    }
}

/// Raft integration with ClusterManager
/// 
/// This struct wires up all Raft components with the cluster management layer.
pub struct RaftClusterIntegration {
    /// Raft runtime manager
    pub raft_manager: Arc<RaftRuntimeManager>,
    /// Leader discovery
    pub leader_discovery: Arc<LeaderDiscovery>,
    /// Message handler
    pub message_handler: RaftMessageHandler,
}

impl RaftClusterIntegration {
    /// Create new Raft cluster integration
    pub fn new(
        raft_tx: mpsc::UnboundedSender<RaftCommand>,
        message_tx: mpsc::UnboundedSender<Message>,
        node_id: u64,
        peers: Vec<String>,
    ) -> Self {
        let raft_manager = Arc::new(RaftRuntimeManager::new(raft_tx, node_id));
        let leader_discovery = Arc::new(LeaderDiscovery::new(peers));
        let message_handler = RaftMessageHandler::new(node_id, message_tx);

        Self {
            raft_manager,
            leader_discovery,
            message_handler,
        }
    }

    /// Propose a collection creation
    pub async fn propose_create_collection(
        &self,
        name: &str,
        dimension: usize,
    ) -> Result<u64, crate::RTDBError> {
        use super::raft_apply::StateMachineCommand;

        let cmd = StateMachineCommand::create_collection(name, dimension);
        let data = bincode::serialize(&cmd)
            .map_err(|e| crate::RTDBError::Serialization(e.to_string()))?;

        self.raft_manager.propose(data).await
    }

    /// Propose a node registration
    pub async fn propose_register_node(
        &self,
        node_id: &str,
        address: &str,
    ) -> Result<u64, crate::RTDBError> {
        use super::raft_apply::StateMachineCommand;

        let cmd = StateMachineCommand::register_node(node_id, address);
        let data = bincode::serialize(&cmd)
            .map_err(|e| crate::RTDBError::Serialization(e.to_string()))?;

        self.raft_manager.propose(data).await
    }

    /// Check if we can handle write requests
    pub fn can_handle_writes(&self) -> bool {
        self.raft_manager.is_leader()
    }

    /// Get leader address for forwarding
    pub async fn get_leader_for_forwarding(&self) -> Option<String> {
        self.leader_discovery.get_leader().await
    }

    /// Forward request to leader
    pub async fn forward_to_leader<T, R>(
        &self,
        _request: T,
    ) -> Result<R, crate::RTDBError> {
        let leader = self.get_leader_for_forwarding().await;
        match leader {
            Some(leader_addr) => {
                debug!("Forwarding request to leader at {}", leader_addr);
                // TODO: Implement actual forwarding via gRPC client
                Err(crate::RTDBError::Consensus(
                    format!("Would forward to {}", leader_addr)
                ))
            }
            None => Err(crate::RTDBError::Consensus(
                "No leader known, cannot forward".to_string()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_leader_discovery() {
        let nodes = vec![
            "node1:5001".to_string(),
            "node2:5002".to_string(),
            "node3:5003".to_string(),
        ];

        let discovery = LeaderDiscovery::new(nodes);
        
        // Initially no leader
        assert!(discovery.get_leader().await.is_none());
        
        // Set leader
        discovery.set_leader("node1:5001".to_string()).await;
        assert_eq!(discovery.get_leader().await.unwrap(), "node1:5001");
        
        // Clear leader
        discovery.clear_leader().await;
        assert!(discovery.get_leader().await.is_none());
    }

    #[tokio::test]
    async fn test_raft_manager_leader() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let manager = RaftRuntimeManager::new(tx, 1);

        assert!(!manager.is_leader());
        assert_eq!(manager.leader_id(), None);

        manager.set_leader(true);
        assert!(manager.is_leader());
        assert_eq!(manager.leader_id(), Some(1));

        manager.set_leader_id(2);
        assert!(!manager.is_leader());
        assert_eq!(manager.leader_id(), Some(2));
    }

    #[test]
    fn test_message_handler() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handler = RaftMessageHandler::new(1, tx);

        // Create a heartbeat
        let msg = handler.create_heartbeat(2, 1, 5);
        assert_eq!(msg.from, 1);
        assert_eq!(msg.to, 2);
        assert_eq!(msg.msg_type, MessageType::Heartbeat);

        // Handle it
        handler.handle_message(msg.clone()).unwrap();
        
        // Verify it was sent
        let received = rx.try_recv().unwrap();
        assert_eq!(received.from, 1);
        assert_eq!(received.to, 2);
    }
}
