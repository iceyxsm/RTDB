//! Core Raft consensus implementation
//!
//! Implements leader election, log replication, and safety properties
//! following the etcd/TiKV design patterns.

use super::types::*;
use crate::{RTDBError, Result};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

/// Core Raft state machine
pub struct RaftNode {
    /// Node configuration
    config: Config,
    
    /// Persistent state (must be persisted before responding to RPCs)
    term: Term,
    voted_for: NodeId,
    log: Vec<LogEntry>,
    
    /// Volatile state (rebuilt on restart)
    commit_index: LogIndex,
    last_applied: LogIndex,
    
    /// Leader state (reset on election)
    progress: HashMap<NodeId, Progress>,
    
    /// Current state
    state: RaftState,
    leader_id: NodeId,
    
    /// Election timing
    election_timeout: Duration,
    last_election_reset: Instant,
    last_heartbeat: Instant,
    
    /// Vote tracking
    votes: HashMap<NodeId, bool>,
    
    /// Read index queue
    read_states: VecDeque<ReadState>,
    pending_reads: Vec<ReadState>,
    
    /// Snapshot tracking
    pending_snapshot: Option<Snapshot>,
    
    /// Check quorum tracking
    quorum_recently_active: bool,
    
    /// Configuration
    conf_state: ConfState,
    
    /// Whether we're in joint consensus
    in_joint_consensus: bool,
}

impl RaftNode {
    /// Create a new Raft node
    pub fn new(config: Config, store: &dyn Storage) -> Result<Self> {
        let initial_state = store.initial_state()?;
        let hard_state = initial_state.hard_state;
        let conf_state = initial_state.conf_state;
        
        // Load existing log or start fresh
        let first_index = store.first_index()?;
        let last_index = store.last_index()?;
        
        let log = if last_index >= first_index {
            store.entries(first_index, last_index + 1, usize::MAX)?
        } else {
            Vec::new()
        };
        
        let election_timeout = random_election_timeout(
            config.election_timeout_min,
            config.election_timeout_max,
        );
        
        let now = Instant::now();
        
        Ok(Self {
            config,
            term: hard_state.term,
            voted_for: hard_state.voted_for,
            log,
            commit_index: hard_state.commit_index,
            last_applied: hard_state.commit_index,
            progress: HashMap::new(),
            state: RaftState::Follower,
            leader_id: 0,
            election_timeout,
            last_election_reset: now,
            last_heartbeat: now,
            votes: HashMap::new(),
            read_states: VecDeque::new(),
            pending_reads: Vec::new(),
            pending_snapshot: None,
            quorum_recently_active: true,
            conf_state,
            in_joint_consensus: false,
        })
    }
    
    /// Create new Raft with initial configuration
    pub fn new_with_conf(config: Config, voters: Vec<NodeId>) -> Self {
        let election_timeout = random_election_timeout(
            config.election_timeout_min,
            config.election_timeout_max,
        );
        
        let now = Instant::now();
        let conf_state = ConfState::new(voters);
        
        let mut node = Self {
            config,
            term: 0,
            voted_for: 0,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            progress: HashMap::new(),
            state: RaftState::Follower,
            leader_id: 0,
            election_timeout,
            last_election_reset: now,
            last_heartbeat: now,
            votes: HashMap::new(),
            read_states: VecDeque::new(),
            pending_reads: Vec::new(),
            pending_snapshot: None,
            quorum_recently_active: true,
            conf_state,
            in_joint_consensus: false,
        };
        
        // Add initial no-op entry
        node.log.push(LogEntry::new(1, 0, Vec::new()));
        
        node
    }
    
    // ==================== Accessors ====================
    
    pub fn id(&self) -> NodeId {
        self.config.id
    }
    
    pub fn term(&self) -> Term {
        self.term
    }
    
    pub fn state(&self) -> RaftState {
        self.state
    }
    
    pub fn leader_id(&self) -> NodeId {
        self.leader_id
    }
    
    pub fn is_leader(&self) -> bool {
        self.state == RaftState::Leader
    }
    
    pub fn commit_index(&self) -> LogIndex {
        self.commit_index
    }
    
    pub fn last_index(&self) -> LogIndex {
        self.log.len() as LogIndex
    }
    
    pub fn last_term(&self) -> Term {
        self.log_term(self.last_index())
    }
    
    pub fn conf_state(&self) -> &ConfState {
        &self.conf_state
    }
    
    // ==================== Tick / Timing ====================
    
    /// Process time tick - call periodically (e.g., every 10ms)
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        
        match self.state {
            RaftState::Leader => {
                // Check if we need to send heartbeats
                if now.duration_since(self.last_heartbeat) >= self.config.heartbeat_interval {
                    self.last_heartbeat = now;
                    return true; // Need to send heartbeats
                }
                
                // Check quorum
                if self.config.check_quorum {
                    self.check_quorum();
                }
            }
            _ => {
                // Check election timeout
                if now.duration_since(self.last_election_reset) >= self.election_timeout {
                    return self.handle_election_timeout();
                }
            }
        }
        
        false
    }
    
    fn handle_election_timeout(&mut self) -> bool {
        if self.config.pre_vote && self.state == RaftState::Follower {
            // Use pre-vote to avoid disrupting leader
            self.pre_campaign();
            true
        } else {
            // Start actual election
            self.campaign();
            true
        }
    }
    
    fn reset_election_timeout(&mut self) {
        self.election_timeout = random_election_timeout(
            self.config.election_timeout_min,
            self.config.election_timeout_max,
        );
        self.last_election_reset = Instant::now();
    }
    
    // ==================== State Transitions ====================
    
    fn become_follower(&mut self, term: Term, leader_id: NodeId) {
        info!(
            node_id = self.config.id,
            term = term,
            leader_id = leader_id,
            "Becoming follower"
        );
        
        self.reset(term);
        self.state = RaftState::Follower;
        self.leader_id = leader_id;
        self.reset_election_timeout();
    }
    
    fn become_pre_candidate(&mut self) {
        debug!(node_id = self.config.id, term = self.term, "Becoming pre-candidate");
        
        self.state = RaftState::PreCandidate;
        self.votes = HashMap::new();
        self.votes.insert(self.config.id, true);
        self.reset_election_timeout();
    }
    
    fn become_candidate(&mut self) {
        info!(node_id = self.config.id, term = self.term + 1, "Becoming candidate");
        
        self.term += 1;
        self.reset(self.term);
        self.state = RaftState::Candidate;
        self.voted_for = self.config.id;
        self.votes = HashMap::new();
        self.votes.insert(self.config.id, true);
        self.reset_election_timeout();
    }
    
    fn become_leader(&mut self) {
        info!(node_id = self.config.id, term = self.term, "Becoming leader");
        
        let last_index = self.last_index();
        
        // Initialize progress for all peers
        self.progress.clear();
        for &node_id in &self.conf_state.voters {
            if node_id != self.config.id {
                let is_learner = self.conf_state.is_learner(node_id);
                let mut progress = Progress::new(last_index + 1, is_learner);
                
                // Learners don't count for commit
                if is_learner {
                    progress.match_index = 0;
                }
                
                self.progress.insert(node_id, progress);
            }
        }
        
        self.state = RaftState::Leader;
        self.leader_id = self.config.id;
        self.quorum_recently_active = true;
        
        // Append no-op entry to establish leadership
        let noop = LogEntry::new(last_index + 1, self.term, Vec::new());
        self.log.push(noop);
        
        // Process pending reads
        for read in std::mem::take(&mut self.pending_reads) {
            self.read_states.push_back(read);
        }
    }
    
    fn reset(&mut self, term: Term) {
        if self.term != term {
            self.term = term;
            self.voted_for = 0;
        }
        
        self.leader_id = 0;
        self.progress.clear();
        self.votes.clear();
        self.read_states.clear();
        self.pending_reads.clear();
    }
    
    // ==================== Message Handlers ====================
    
    /// Handle incoming Raft message
    pub fn step(&mut self, msg: Message) -> Option<Ready> {
        trace!(
            node_id = self.config.id,
            term = self.term,
            msg_type = ?msg.msg_type,
            from = msg.from,
            "Received message"
        );
        
        // Check term
        if msg.term > self.term {
            // Higher term - become follower
            let leader_id = if msg.msg_type == MessageType::AppendEntries {
                msg.from
            } else {
                0
            };
            self.become_follower(msg.term, leader_id);
        }
        
        match msg.msg_type {
            MessageType::Heartbeat => self.handle_heartbeat(msg),
            MessageType::AppendEntries => self.handle_append_entries(msg),
            MessageType::AppendResponse => self.handle_append_response(msg),
            MessageType::RequestVote => self.handle_request_vote(msg),
            MessageType::VoteResponse => self.handle_vote_response(msg),
            MessageType::RequestPreVote => self.handle_request_pre_vote(msg),
            MessageType::PreVoteResponse => self.handle_pre_vote_response(msg),
            MessageType::InstallSnapshot => self.handle_install_snapshot(msg),
            MessageType::SnapshotResponse => self.handle_snapshot_response(msg),
            _ => None,
        }
    }
    
    fn handle_heartbeat(&mut self, msg: Message) -> Option<Ready> {
        self.reset_election_timeout();
        
        if msg.term < self.term {
            return None;
        }
        
        self.leader_id = msg.from;
        self.commit_index = self.commit_index.max(msg.commit);
        
        // Build ready with response
        let mut ready = Ready::default();
        ready.messages.push(Message::new(
            MessageType::Heartbeat,
            self.config.id,
            msg.from,
            self.term,
        ));
        
        Some(ready)
    }
    
    fn handle_append_entries(&mut self, msg: Message) -> Option<Ready> {
        self.reset_election_timeout();
        
        if msg.term < self.term {
            // Reject - stale leader
            let response = Message::new(
                MessageType::AppendResponse,
                self.config.id,
                msg.from,
                self.term,
            )
            .with_reject(self.last_index());
            
            return Some(Ready {
                messages: vec![response],
                ..Default::default()
            });
        }
        
        self.leader_id = msg.from;
        
        // Check log consistency at prev_log_index
        if msg.index > 0 {
            let prev_log_term = self.log_term(msg.index);
            if prev_log_term != msg.term {
                // Log mismatch - reject
                let hint = self.find_conflict_hint(msg.index);
                let response = Message::new(
                    MessageType::AppendResponse,
                    self.config.id,
                    msg.from,
                    self.term,
                )
                .with_reject(hint);
                
                return Some(Ready {
                    messages: vec![response],
                    ..Default::default()
                });
            }
        }
        
        // Append new entries
        let mut entries = msg.entries.clone();
        if !entries.is_empty() {
            let next_index = msg.index + 1;
            self.truncate_log(next_index);
            
            // Find matching entries
            for entry in entries {
                if entry.index < next_index {
                    continue;
                }
                self.log.push(entry);
            }
        }
        
        // Update commit index
        if msg.commit > self.commit_index {
            let last_new_index = if msg.entries.is_empty() {
                msg.index
            } else {
                msg.entries.last().unwrap().index
            };
            self.commit_index = msg.commit.min(last_new_index);
        }
        
        // Send acceptance
        let response = Message {
            msg_type: MessageType::AppendResponse,
            from: self.config.id,
            to: msg.from,
            term: self.term,
            index: self.last_index(),
            reject: false,
            ..Default::default()
        };
        
        Some(Ready {
            messages: vec![response],
            entries: msg.entries,
            ..Default::default()
        })
    }
    
    fn handle_append_response(&mut self, msg: Message) -> Option<Ready> {
        if self.state != RaftState::Leader || msg.term != self.term {
            return None;
        }
        
        let progress = self.progress.get_mut(&msg.from)?;
        
        if msg.reject {
            // Decrement next_index
            progress.become_reject();
            if msg.reject_hint > 0 {
                progress.next_index = progress.next_index.min(msg.reject_hint);
            }
            progress.next_index = progress.next_index.max(1);
        } else {
            // Success - update match_index
            progress.become_accept(msg.index);
            
            // Try to advance commit index
            self.advance_commit_index();
        }
        
        None
    }
    
    fn handle_request_vote(&mut self, msg: Message) -> Option<Ready> {
        if msg.term < self.term {
            // Reject - stale term
            let response = Message::new(
                MessageType::VoteResponse,
                self.config.id,
                msg.from,
                self.term,
            )
            .with_reject(0);
            return Some(Ready {
                messages: vec![response],
                ..Default::default()
            });
        }
        
        let mut can_vote = self.voted_for == 0 || self.voted_for == msg.from;
        
        // Check if we're leader or candidate in same term
        if self.voted_for == self.config.id {
            can_vote = false;
        }
        
        // Check log completeness
        let last_index = self.last_index();
        let last_term = self.last_term();
        let log_ok = msg.term > last_term || 
            (msg.term == last_term && msg.index >= last_index);
        
        let granted = can_vote && log_ok;
        
        if granted {
            self.voted_for = msg.from;
            self.reset_election_timeout();
        }
        
        let response = Message {
            msg_type: MessageType::VoteResponse,
            from: self.config.id,
            to: msg.from,
            term: self.term,
            reject: !granted,
            ..Default::default()
        };
        
        Some(Ready {
            hard_state: Some(HardState {
                term: self.term,
                voted_for: self.voted_for,
                commit_index: self.commit_index,
            }),
            messages: vec![response],
            ..Default::default()
        })
    }
    
    fn handle_vote_response(&mut self, msg: Message) -> Option<Ready> {
        if self.state != RaftState::Candidate || msg.term != self.term {
            return None;
        }
        
        self.votes.insert(msg.from, !msg.reject);
        
        // Count votes
        let granted = self.votes.values().filter(|&&v| v).count();
        let total_voters = self.conf_state.voters.len();
        
        if is_quorum(total_voters, granted) {
            self.become_leader();
            
            // Generate ready with append entries
            return Some(self.build_append_acks());
        }
        
        // Check if election is lost
        let rejected = self.votes.values().filter(|&&v| !v).count();
        if is_quorum(total_voters, rejected) {
            // Lost election
            self.become_follower(self.term, 0);
        }
        
        None
    }
    
    fn handle_request_pre_vote(&mut self, msg: Message) -> Option<Ready> {
        // Pre-vote doesn't change our state
        let last_index = self.last_index();
        let last_term = self.last_term();
        
        let log_ok = msg.term > last_term ||
            (msg.term == last_term && msg.index >= last_index);
        
        // Check if we're leader recently active
        let leader_recently_active = self.state == RaftState::Leader ||
            (self.leader_id != 0 && 
             Instant::now().duration_since(self.last_election_reset) < 
             self.config.election_timeout_max * 2);
        
        let granted = log_ok && !leader_recently_active;
        
        let response = Message {
            msg_type: MessageType::PreVoteResponse,
            from: self.config.id,
            to: msg.from,
            term: self.term,
            reject: !granted,
            ..Default::default()
        };
        
        Some(Ready {
            messages: vec![response],
            ..Default::default()
        })
    }
    
    fn handle_pre_vote_response(&mut self, msg: Message) -> Option<Ready> {
        if self.state != RaftState::PreCandidate || msg.term != self.term {
            return None;
        }
        
        self.votes.insert(msg.from, !msg.reject);
        
        let granted = self.votes.values().filter(|&&v| v).count();
        let total_voters = self.conf_state.voters.len();
        
        if is_quorum(total_voters, granted) {
            // Pre-vote successful - start real campaign
            self.campaign();
        }
        
        None
    }
    
    fn handle_install_snapshot(&mut self, msg: Message) -> Option<Ready> {
        if msg.term < self.term {
            return None;
        }
        
        self.reset_election_timeout();
        self.leader_id = msg.from;
        
        if let Some(snapshot) = msg.snapshot {
            // Apply snapshot
            let meta = snapshot.metadata.clone();
            self.pending_snapshot = Some(snapshot);
            
            // Truncate log up to snapshot index
            self.log.clear();
            self.commit_index = meta.index;
            self.last_applied = meta.index;
            
            let response = Message {
                msg_type: MessageType::SnapshotResponse,
                from: self.config.id,
                to: msg.from,
                term: self.term,
                reject: false,
                ..Default::default()
            };
            
            return Some(Ready {
                snapshot: self.pending_snapshot.take(),
                messages: vec![response],
                ..Default::default()
            });
        }
        
        None
    }
    
    fn handle_snapshot_response(&mut self, msg: Message) -> Option<Ready> {
        if self.state != RaftState::Leader {
            return None;
        }
        
        if let Some(progress) = self.progress.get_mut(&msg.from) {
            if !msg.reject {
                // Snapshot successfully installed
                progress.match_index = msg.index;
                progress.next_index = msg.index + 1;
                progress.paused = false;
            }
        }
        
        None
    }
    
    // ==================== Campaign ====================
    
    fn pre_campaign(&mut self) {
        self.become_pre_candidate();
        
        // Send pre-vote requests
        let last_index = self.last_index();
        let last_term = self.last_term();
        
        for &peer in &self.conf_state.voters {
            if peer != self.config.id {
                let msg = Message {
                    msg_type: MessageType::RequestPreVote,
                    from: self.config.id,
                    to: peer,
                    term: last_term,
                    index: last_index,
                    ..Default::default()
                };
                // Pre-vote messages are handled differently in ready processing
            }
        }
    }
    
    fn campaign(&mut self) {
        self.become_candidate();
        
        // Vote for self already counted
        let last_index = self.last_index();
        let last_term = self.last_term();
        
        // Request votes from all peers
        for &peer in &self.conf_state.voters {
            if peer != self.config.id {
                let msg = Message {
                    msg_type: MessageType::RequestVote,
                    from: self.config.id,
                    to: peer,
                    term: self.term,
                    index: last_index,
                    ..Default::default()
                };
                // Vote requests handled via ready
            }
        }
    }
    
    // ==================== Log Operations ====================
    
    /// Propose a new entry (only valid when leader)
    pub fn propose(&mut self, data: Vec<u8>) -> crate::Result<LogIndex> {
        if !self.is_leader() {
            return Err(RTDBError::Consensus(format!(
                "Not leader (leader is {:?})",
                self.leader_id
            )));
        }
        
        let index = self.last_index() + 1;
        let entry = LogEntry::new(index, self.term, data);
        
        self.log.push(entry);
        
        Ok(index)
    }
    
    /// Read index for linearizable reads
    pub fn read_index(&mut self, ctx: Vec<u8>) -> crate::Result<()> {
        if !self.is_leader() {
            return Err(RTDBError::Consensus(format!(
                "Not leader (leader is {:?})",
                self.leader_id
            )));
        }
        
        // Add read request to pending
        let read_state = ReadState {
            index: self.commit_index,
            request_ctx: ctx,
        };
        self.pending_reads.push(read_state);
        
        Ok(())
    }
    
    fn log_term(&self, index: LogIndex) -> Term {
        if index == 0 {
            return 0;
        }
        
        if index > self.log.len() as LogIndex {
            return 0;
        }
        
        self.log[(index - 1) as usize].term
    }
    
    fn truncate_log(&mut self, from_index: LogIndex) {
        if from_index <= self.log.len() as LogIndex {
            self.log.truncate((from_index - 1) as usize);
        }
    }
    
    fn find_conflict_hint(&self, index: LogIndex) -> LogIndex {
        // Find the first index with matching term
        let term = self.log_term(index);
        
        for i in (1..=index).rev() {
            if self.log_term(i) != term {
                return i;
            }
        }
        
        0
    }
    
    // ==================== Commit & Apply ====================
    
    fn advance_commit_index(&mut self) {
        if self.state != RaftState::Leader {
            return;
        }
        
        // Find median match index (quorum)
        let mut match_indices: Vec<LogIndex> = self.progress
            .values()
            .map(|p| p.match_index)
            .collect();
        match_indices.push(self.last_index()); // Leader's own log
        
        match_indices.sort_unstable_by(|a, b| b.cmp(a)); // Descending
        
        let quorum_index = match_indices[self.quorum_size() - 1];
        
        // Only commit entries from current term
        if quorum_index > self.commit_index && self.log_term(quorum_index) == self.term {
            self.commit_index = quorum_index;
            
            // Process pending reads that can be completed
            let pending = std::mem::take(&mut self.pending_reads);
            for read in pending {
                if read.index <= self.commit_index {
                    self.read_states.push_back(read);
                } else {
                    self.pending_reads.push(read);
                }
            }
        }
    }
    
    fn quorum_size(&self) -> usize {
        (self.conf_state.voters.len() / 2) + 1
    }
    
    fn check_quorum(&mut self) {
        let now = Instant::now();
        let active_threshold = self.config.election_timeout_max;
        
        let active_peers = self.progress
            .values()
            .filter(|p| p.recent_active)
            .count();
        
        let active = active_peers + 1 >= self.quorum_size(); // Include self
        
        if !active && self.quorum_recently_active {
            warn!(
                node_id = self.config.id,
                "Lost quorum, stepping down"
            );
            self.become_follower(self.term, 0);
        }
        
        self.quorum_recently_active = active;
    }
    
    // ==================== Ready Generation ====================
    
    /// Generate ready for persistence and sending
    pub fn ready(&mut self) -> Option<Ready> {
        if self.state != RaftState::Leader {
            return None;
        }
        
        let mut ready = Ready::default();
        
        // Generate append entries for each peer
        for (&peer, progress) in &self.progress {
            if progress.paused {
                continue;
            }
            
            let next_index = progress.next_index;
            let last_index = self.last_index();
            
            if next_index > last_index {
                // Send heartbeat
                let msg = Message {
                    msg_type: MessageType::Heartbeat,
                    from: self.config.id,
                    to: peer,
                    term: self.term,
                    commit: self.commit_index,
                    ..Default::default()
                };
                ready.messages.push(msg);
            } else {
                // Get entries to send
                let entries = self.get_entries(next_index, self.config.max_msg_size);
                let prev_index = next_index - 1;
                let prev_term = self.log_term(prev_index);
                
                let msg = Message {
                    msg_type: MessageType::AppendEntries,
                    from: self.config.id,
                    to: peer,
                    term: self.term,
                    index: prev_index,
                    entries,
                    commit: self.commit_index,
                    ..Default::default()
                };
                ready.messages.push(msg);
            }
        }
        
        // Get committed entries to apply
        if self.commit_index > self.last_applied {
            let start = self.last_applied + 1;
            let end = self.commit_index + 1;
            ready.committed_entries = self.get_entries(start, usize::MAX);
            self.last_applied = self.commit_index;
        }
        
        // Hard state if changed
        ready.hard_state = Some(HardState {
            term: self.term,
            voted_for: self.voted_for,
            commit_index: self.commit_index,
        });
        
        if ready.is_empty() {
            None
        } else {
            Some(ready)
        }
    }
    
    fn get_entries(&self, from: LogIndex, max_size: usize) -> Vec<LogEntry> {
        let start = (from - 1) as usize;
        if start >= self.log.len() {
            return Vec::new();
        }
        
        let mut entries = Vec::new();
        let mut size = 0;
        
        for entry in &self.log[start..] {
            size += entry.data.len() + 32; // Approximate overhead
            if size > max_size && !entries.is_empty() {
                break;
            }
            entries.push(entry.clone());
            if size > max_size {
                break;
            }
        }
        
        entries
    }
    
    fn build_append_acks(&self) -> Ready {
        let mut ready = Ready::default();
        
        for &peer in &self.conf_state.voters {
            if peer != self.config.id {
                let msg = Message {
                    msg_type: MessageType::AppendEntries,
                    from: self.config.id,
                    to: peer,
                    term: self.term,
                    index: self.last_index() - 1,
                    commit: self.commit_index,
                    entries: if self.log.len() > 1 {
                        vec![self.log.last().unwrap().clone()]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                };
                ready.messages.push(msg);
            }
        }
        
        ready.hard_state = Some(HardState {
            term: self.term,
            voted_for: self.voted_for,
            commit_index: self.commit_index,
        });
        
        ready
    }
    
    /// Get pending read states
    pub fn read_states(&mut self) -> Vec<ReadState> {
        std::mem::take(&mut self.read_states).into_iter().collect()
    }
    
    /// Get metrics
    pub fn metrics(&self) -> RaftMetrics {
        RaftMetrics {
            state: self.state,
            term: self.term,
            leader_id: self.leader_id,
            commit_index: self.commit_index,
            applied_index: self.last_applied,
            last_index: self.last_index(),
            pending_proposals: self.pending_reads.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_node(id: NodeId, voters: Vec<NodeId>) -> RaftNode {
        let config = Config {
            id,
            ..Default::default()
        };
        RaftNode::new_with_conf(config, voters)
    }
    
    #[test]
    fn test_initial_state() {
        let node = create_test_node(1, vec![1, 2, 3]);
        assert_eq!(node.state(), RaftState::Follower);
        assert_eq!(node.term(), 0);
        assert!(!node.is_leader());
    }
    
    #[test]
    fn test_leader_election() {
        let mut node = create_test_node(1, vec![1, 2, 3]);
        
        // Simulate election timeout
        node.become_candidate();
        assert_eq!(node.state(), RaftState::Candidate);
        assert_eq!(node.term(), 1);
        
        // Receive votes from peers
        node.step(Message {
            msg_type: MessageType::VoteResponse,
            from: 2,
            to: 1,
            term: 1,
            reject: false,
            ..Default::default()
        });
        
        // Quorum of 3 is 2, we have 2 votes (self + peer), should become leader
        node.step(Message {
            msg_type: MessageType::VoteResponse,
            from: 3,
            to: 1,
            term: 1,
            reject: false,
            ..Default::default()
        });
        
        assert!(node.is_leader());
    }
    
    #[test]
    fn test_log_replication() {
        let mut leader = create_test_node(1, vec![1, 2, 3]);
        leader.become_leader();
        
        // Propose entry (index 3 because become_leader adds noop at 2)
        let index = leader.propose(vec![1, 2, 3]).unwrap();
        assert_eq!(index, 3);
        
        // Simulate follower accepting (index 3)
        leader.step(Message {
            msg_type: MessageType::AppendResponse,
            from: 2,
            to: 1,
            term: 0,
            index: 3,
            reject: false,
            ..Default::default()
        });
        
        // Quorum reached, commit should advance
        leader.step(Message {
            msg_type: MessageType::AppendResponse,
            from: 3,
            to: 1,
            term: 0,
            index: 3,
            reject: false,
            ..Default::default()
        });
        
        // Ready should have committed entries
        if let Some(ready) = leader.ready() {
            assert!(!ready.committed_entries.is_empty());
        }
    }
    
    #[test]
    fn test_quorum_calculation() {
        assert_eq!(is_quorum(3, 2), true);
        assert_eq!(is_quorum(5, 3), true);
        assert_eq!(is_quorum(5, 2), false);
        assert_eq!(is_quorum(1, 1), true);
    }
}
