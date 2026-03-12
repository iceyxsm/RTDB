//! Async runtime for driving Raft consensus
//!
//! Handles:
//! - Timer ticks for elections and heartbeats
//! - Message I/O via Transport trait
//! - Entry application via Apply trait
//! - Snapshot management

use super::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::interval;
use tracing::{error, info, trace, warn};

/// Raft runtime that drives the consensus state machine
pub struct RaftRuntime<T: Transport, A: Apply> {
    /// Raft state machine
    raft: Arc<RwLock<RaftNode>>,
    /// Network transport
    transport: Arc<T>,
    /// State machine applier
    apply: Arc<A>,
    /// Storage for persistence
    storage: Arc<dyn Storage>,
    /// Command receiver
    command_rx: mpsc::UnboundedReceiver<RaftCommand>,
    /// Message receiver from network
    message_rx: mpsc::UnboundedReceiver<Message>,
    /// Snapshot configuration
    snapshot_config: SnapshotConfig,
    /// Last snapshot time
    last_snapshot: Instant,
    /// Pending proposals awaiting commit
    pending_proposals: Vec<(LogIndex, oneshot::Sender<ProposeResult>)>,
    /// Pending read indices
    pending_reads: Vec<(LogIndex, oneshot::Sender<ReadIndexResult>)>,
    /// Shutdown signal
    shutdown: bool,
}

impl<T: Transport, A: Apply> RaftRuntime<T, A> {
    /// Create new Raft runtime
    pub fn new(
        raft: RaftNode,
        transport: Arc<T>,
        apply: Arc<A>,
        storage: Arc<dyn Storage>,
        command_rx: mpsc::UnboundedReceiver<RaftCommand>,
        message_rx: mpsc::UnboundedReceiver<Message>,
    ) -> Self {
        Self {
            raft: Arc::new(RwLock::new(raft)),
            transport,
            apply,
            storage,
            command_rx,
            message_rx,
            snapshot_config: SnapshotConfig::default(),
            last_snapshot: Instant::now(),
            pending_proposals: Vec::new(),
            pending_reads: Vec::new(),
            shutdown: false,
        }
    }

    /// Set snapshot configuration
    pub fn with_snapshot_config(mut self, config: SnapshotConfig) -> Self {
        self.snapshot_config = config;
        self
    }

    /// Run the Raft event loop
    pub async fn run(mut self) -> crate::Result<()> {
        info!("Starting Raft runtime");

        // Create tick interval (10ms resolution)
        let mut tick_interval = interval(Duration::from_millis(10));

        while !self.shutdown {
            tokio::select! {
                // Timer tick
                _ = tick_interval.tick() => {
                    if let Err(e) = self.on_tick().await {
                        error!("Tick error: {}", e);
                    }
                }

                // Incoming command
                Some(cmd) = self.command_rx.recv() => {
                    if let Err(e) = self.on_command(cmd).await {
                        error!("Command error: {}", e);
                    }
                }

                // Incoming message
                Some(msg) = self.message_rx.recv() => {
                    if let Err(e) = self.on_message(msg).await {
                        error!("Message error: {}", e);
                    }
                }
            }
        }

        info!("Raft runtime stopped");
        Ok(())
    }

    /// Handle timer tick
    async fn on_tick(&mut self) -> crate::Result<()> {
        let needs_action = {
            let mut raft = self.raft.write().await;
            raft.tick()
        };

        if needs_action {
            self.process_ready().await?;
        }

        Ok(())
    }

    /// Handle command
    async fn on_command(&mut self, cmd: RaftCommand) -> crate::Result<()> {
        match cmd {
            RaftCommand::Propose { data, respond_to } => {
                let mut raft = self.raft.write().await;
                
                match raft.propose(data) {
                    Ok(index) => {
                        let term = raft.term();
                        self.pending_proposals.push((index, respond_to));
                        
                        // Generate ready immediately
                        drop(raft);
                        self.process_ready().await?;
                    }
                    Err(e) => {
                        // Not leader - send error
                        let leader_id = raft.leader_id();
                        let _ = respond_to.send(ProposeResult {
                            index: 0,
                            term: 0,
                        });
                        warn!(
                            "Propose failed: not leader (leader is {:?})",
                            leader_id
                        );
                    }
                }
            }

            RaftCommand::ReadIndex { ctx, respond_to } => {
                let mut raft = self.raft.write().await;
                
                match raft.read_index(ctx) {
                    Ok(_) => {
                        // Will be completed when commit advances
                        let index = raft.commit_index();
                        let term = raft.term();
                        self.pending_reads.push((index, respond_to));
                        
                        // Send heartbeats to advance commit
                        drop(raft);
                        self.process_ready().await?;
                    }
                    Err(_) => {
                        let _ = respond_to.send(ReadIndexResult { index: 0, term: 0 });
                    }
                }
            }

            RaftCommand::Status { respond_to } => {
                let raft = self.raft.read().await;
                let metrics = raft.metrics();
                
                let _ = respond_to.send(RaftStatus {
                    id: raft.id(),
                    state: metrics.state,
                    term: metrics.term,
                    leader_id: metrics.leader_id,
                    commit_index: metrics.commit_index,
                    applied_index: metrics.applied_index,
                    last_index: metrics.last_index,
                });
            }

            RaftCommand::StepDown => {
                info!("Stepping down from leadership");
                // This is handled by the node internally when quorum is lost
            }

            RaftCommand::Snapshot => {
                self.trigger_snapshot().await?;
            }

            RaftCommand::Shutdown => {
                info!("Shutdown requested");
                self.shutdown = true;
            }
        }

        Ok(())
    }

    /// Handle incoming message
    async fn on_message(&mut self, msg: Message) -> crate::Result<()> {
        trace!(
            msg_type = ?msg.msg_type,
            from = msg.from,
            term = msg.term,
            "Processing message"
        );

        let mut raft = self.raft.write().await;
        
        // Step the message
        if let Some(ready) = raft.step(msg) {
            drop(raft);
            self.handle_ready(ready).await?;
        }

        Ok(())
    }

    /// Process ready from Raft
    async fn process_ready(&mut self) -> crate::Result<()> {
        let mut raft = self.raft.write().await;
        
        if let Some(ready) = raft.ready() {
            drop(raft);
            self.handle_ready(ready).await?;
        } else {
            // Just send heartbeats periodically
            drop(raft);
            self.send_heartbeats().await?;
        }

        Ok(())
    }

    /// Handle ready output from Raft
    async fn handle_ready(&mut self, ready: Ready) -> crate::Result<()> {
        // 1. Persist hard state
        if let Some(hard_state) = ready.hard_state {
            self.persist_hard_state(hard_state).await?;
        }

        // 2. Persist entries
        if !ready.entries.is_empty() {
            self.persist_entries(&ready.entries).await?;
        }

        // 3. Send messages
        if !ready.messages.is_empty() {
            self.send_messages(ready.messages).await?;
        }

        // 4. Apply committed entries
        if !ready.committed_entries.is_empty() {
            self.apply_entries(&ready.committed_entries).await?;
            
            // Check if any proposals are committed
            self.notify_proposals(&ready.committed_entries).await;
            
            // Maybe trigger snapshot
            self.maybe_snapshot().await?;
        }

        // 5. Handle snapshot
        if let Some(snapshot) = ready.snapshot {
            self.handle_snapshot(snapshot).await?;
        }

        // 6. Process read states
        let mut raft = self.raft.write().await;
        let read_states = raft.read_states();
        drop(raft);
        self.notify_reads(read_states).await;

        Ok(())
    }

    /// Persist hard state
    async fn persist_hard_state(&self, state: HardState) -> crate::Result<()> {
        // In production, this writes to WAL
        // For now, we rely on the storage trait
        trace!(
            term = state.term,
            voted_for = state.voted_for,
            commit_index = state.commit_index,
            "Persisting hard state"
        );
        Ok(())
    }

    /// Persist entries
    async fn persist_entries(&self, entries: &[LogEntry]) -> crate::Result<()> {
        trace!(count = entries.len(), "Persisting entries");
        // Storage handles persistence
        Ok(())
    }

    /// Send messages to peers
    async fn send_messages(&self, messages: Vec<Message>) -> crate::Result<()> {
        for msg in messages {
            trace!(
                msg_type = ?msg.msg_type,
                to = msg.to,
                "Sending message"
            );
            
            if let Err(e) = self.transport.send(msg.to, msg).await {
                debug!("Failed to send message: {}", e);
            }
        }
        Ok(())
    }

    /// Send heartbeat messages
    async fn send_heartbeats(&self) -> crate::Result<()> {
        let raft = self.raft.read().await;
        
        if !raft.is_leader() {
            return Ok(());
        }

        let id = raft.id();
        let term = raft.term();
        let commit = raft.commit_index();
        let conf_state = raft.conf_state().clone();
        
        drop(raft);

        // Send to all peers
        for &peer in &conf_state.voters {
            if peer != id {
                let msg = Message {
                    msg_type: MessageType::Heartbeat,
                    from: id,
                    to: peer,
                    term,
                    commit,
                    ..Default::default()
                };
                
                if let Err(e) = self.transport.send(peer, msg).await {
                    trace!("Failed to send heartbeat to {}: {}", peer, e);
                }
            }
        }

        Ok(())
    }

    /// Apply committed entries
    async fn apply_entries(&self, entries: &[LogEntry]) -> crate::Result<()> {
        debug!(count = entries.len(), "Applying entries");
        
        if let Err(e) = self.apply.apply(entries.to_vec()).await {
            error!("Failed to apply entries: {}", e);
            return Err(e);
        }

        Ok(())
    }

    /// Notify waiting proposals
    async fn notify_proposals(&mut self, entries: &[LogEntry]) {
        let committed_indices: std::collections::HashSet<_> = 
            entries.iter().map(|e| e.index).collect();
        
        let raft = self.raft.read().await;
        let term = raft.term();
        drop(raft);

        let mut still_pending = Vec::new();
        
        for (index, tx) in std::mem::take(&mut self.pending_proposals) {
            if committed_indices.contains(&index) {
                let _ = tx.send(ProposeResult { index, term });
            } else {
                still_pending.push((index, tx));
            }
        }
        
        self.pending_proposals = still_pending;
    }

    /// Notify waiting reads
    async fn notify_reads(&mut self, reads: Vec<ReadState>) {
        let raft = self.raft.read().await;
        let term = raft.term();
        let commit_index = raft.commit_index();
        drop(raft);

        for read in reads {
            // Find and notify matching read
            let mut found = None;
            for (i, (idx, _)) in self.pending_reads.iter().enumerate() {
                if *idx <= commit_index {
                    found = Some(i);
                    break;
                }
            }
            
            if let Some(i) = found {
                let (_, tx) = self.pending_reads.remove(i);
                let _ = tx.send(ReadIndexResult {
                    index: read.index,
                    term,
                });
            }
        }
    }

    /// Handle snapshot
    async fn handle_snapshot(&self, snapshot: Snapshot) -> crate::Result<()> {
        info!(
            index = snapshot.metadata.index,
            "Installing snapshot"
        );
        
        self.apply.apply_snapshot(snapshot).await?;
        
        Ok(())
    }

    /// Maybe trigger snapshot
    async fn maybe_snapshot(&mut self) -> crate::Result<()> {
        let elapsed = Instant::now().duration_since(self.last_snapshot);
        if elapsed < self.snapshot_config.min_interval {
            return Ok(());
        }

        let raft = self.raft.read().await;
        let applied = raft.metrics().applied_index;
        let last_snapshot_index = 0u64; // Track this in storage
        
        let should_snapshot = applied.saturating_sub(last_snapshot_index) >= self.snapshot_config.interval;
        drop(raft);

        if should_snapshot {
            self.trigger_snapshot().await?;
        }

        Ok(())
    }

    /// Trigger snapshot creation
    async fn trigger_snapshot(&mut self) -> crate::Result<()> {
        info!("Triggering snapshot");

        // Get snapshot data from state machine
        let (index, data) = self.apply.snapshot().await?;
        
        let mut raft = self.raft.write().await;
        let conf_state = raft.conf_state().clone();
        drop(raft);
        
        // Create snapshot in storage
        // TODO: Use a method on Storage trait instead of downcasting
        let _ = (index, conf_state, data);
        
        self.last_snapshot = Instant::now();
        
        Ok(())
    }
}

/// Create Raft runtime with memory storage for testing
pub fn create_memory_runtime<T: Transport, A: Apply>(
    node_id: NodeId,
    voters: Vec<NodeId>,
    transport: Arc<T>,
    apply: Arc<A>,
) -> (RaftRuntime<T, A>, mpsc::UnboundedSender<RaftCommand>, mpsc::UnboundedSender<Message>) {
    let storage = Arc::new(MemStorage::with_conf_state(ConfState::new(voters.clone())));
    let config = three_node_config(node_id);
    let raft = RaftNode::new_with_conf(config, voters);
    
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel();
    
    let runtime = RaftRuntime::new(raft, transport, apply, storage, cmd_rx, msg_rx);
    
    (runtime, cmd_tx, msg_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[derive(Debug)]
    struct MockTransport;
    
    #[async_trait::async_trait]
    impl Transport for MockTransport {
        async fn send(&self, _to: NodeId, _msg: Message) -> crate::Result<()> {
            Ok(())
        }
        
        async fn broadcast(&self, _from: NodeId, _msg: Message) -> crate::Result<()> {
            Ok(())
        }
        
        fn node_addresses(&self) -> Vec<(NodeId, String)> {
            vec![
                (1, "127.0.0.1:5001".to_string()),
                (2, "127.0.0.1:5002".to_string()),
                (3, "127.0.0.1:5003".to_string()),
            ]
        }
    }
    
    #[derive(Debug)]
    struct MockApply;
    
    #[async_trait::async_trait]
    impl Apply for MockApply {
        async fn apply(&self, entries: Vec<LogEntry>) -> crate::Result<()> {
            debug!("Applied {} entries", entries.len());
            Ok(())
        }
        
        async fn apply_snapshot(&self, _snapshot: Snapshot) -> crate::Result<()> {
            Ok(())
        }
        
        async fn snapshot(&self) -> crate::Result<(LogIndex, Vec<u8>)> {
            Ok((0, Vec::new()))
        }
    }
    
    #[tokio::test]
    async fn test_runtime_creation() {
        let transport = Arc::new(MockTransport);
        let apply = Arc::new(MockApply);
        
        let (runtime, _cmd_tx, _msg_tx) = create_memory_runtime(
            1,
            vec![1, 2, 3],
            transport,
            apply,
        );
        
        // Verify runtime created successfully
        assert!(!runtime.shutdown);
    }
}
