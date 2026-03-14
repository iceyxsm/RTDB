//! Raft consensus core types
//!
//! Based on the Raft consensus algorithm design from etcd and TiKV,
//! optimized for high-performance distributed vector database workloads.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Unique identifier for a Raft node
pub type NodeId = u64;

/// Index in the Raft log
pub type LogIndex = u64;

/// Raft term number
pub type Term = u64;

/// Raft node states
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaftState {
    /// Follower - accepts log entries from leader (default state)
    #[default]
    Follower,
    /// Candidate - seeking votes to become leader
    Candidate,
    /// Leader - actively replicating log entries
    Leader,
    /// PreCandidate - used with PreVote to avoid disrupting leader
    PreCandidate,
}

impl fmt::Display for RaftState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RaftState::Follower => write!(f, "Follower"),
            RaftState::Candidate => write!(f, "Candidate"),
            RaftState::Leader => write!(f, "Leader"),
            RaftState::PreCandidate => write!(f, "PreCandidate"),
        }
    }
}

/// Single entry in the Raft log
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log index (1-based, monotonically increasing)
    pub index: LogIndex,
    /// Term when this entry was created
    pub term: Term,
    /// The command data to apply to state machine
    pub data: Vec<u8>,
    /// Entry type (normal or configuration change)
    pub entry_type: EntryType,
}

impl LogEntry {
    pub fn new(index: LogIndex, term: Term, data: Vec<u8>) -> Self {
        Self {
            index,
            term,
            data,
            entry_type: EntryType::Normal,
        }
    }

    pub fn new_config_change(index: LogIndex, term: Term, data: Vec<u8>) -> Self {
        Self {
            index,
            term,
            data,
            entry_type: EntryType::ConfigChange,
        }
    }
}

/// Type of log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    /// Normal application command
    Normal,
    /// Configuration change (membership modification)
    ConfigChange,
}

/// Hard state - must be persisted atomically
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub struct HardState {
    /// Current term
    pub term: Term,
    /// Who we voted for in current term (0 = none)
    pub voted_for: NodeId,
    /// Highest committed log index
    pub commit_index: LogIndex,
}


/// Configuration state - cluster membership
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfState {
    /// Current voting members
    pub voters: Vec<NodeId>,
    /// Learner nodes (non-voting, receiving log replication)
    pub learners: Vec<NodeId>,
    /// Nodes being removed (joint consensus)
    pub voters_outgoing: Vec<NodeId>,
    /// Auto transition on commit (joint consensus)
    pub auto_leave: bool,
}

impl ConfState {
    pub fn new(voters: Vec<NodeId>) -> Self {
        Self {
            voters,
            learners: Vec::new(),
            voters_outgoing: Vec::new(),
            auto_leave: false,
        }
    }

    pub fn is_voter(&self, node_id: NodeId) -> bool {
        self.voters.contains(&node_id)
    }

    pub fn is_learner(&self, node_id: NodeId) -> bool {
        self.learners.contains(&node_id)
    }

    pub fn all_nodes(&self) -> Vec<NodeId> {
        let mut nodes = self.voters.clone();
        nodes.extend(&self.learners);
        nodes
    }
}

/// Combined persistent Raft state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistentState {
    pub hard_state: HardState,
    pub conf_state: ConfState,
}

/// Progress of log replication to a follower
#[derive(Debug, Clone)]
pub struct Progress {
    pub next_index: LogIndex,
    pub match_index: LogIndex,
    pub paused: bool,
    pub inflight: usize,
    pub recent_active: bool,
    pub is_learner: bool,
}

impl Progress {
    pub fn new(next_index: LogIndex, is_learner: bool) -> Self {
        Self {
            next_index,
            match_index: 0,
            paused: false,
            inflight: 0,
            recent_active: false,
            is_learner,
        }
    }

    pub fn become_reject(&mut self) {
        if self.next_index > 1 {
            self.next_index -= 1;
        }
        self.paused = false;
    }

    pub fn become_accept(&mut self, index: LogIndex) {
        if index > self.match_index {
            self.match_index = index;
            self.next_index = index + 1;
        }
        self.paused = false;
        self.recent_active = true;
    }

    pub fn optimistic_update(&mut self, last_index: LogIndex) {
        self.next_index = last_index + 1;
    }
}

/// Snapshot metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub index: LogIndex,
    pub term: Term,
    pub conf_state: ConfState,
}

/// Snapshot data and metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub metadata: SnapshotMetadata,
    pub data: Vec<u8>,
}

/// Raft message types
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageType {
    #[default]
    Heartbeat,
    AppendEntries,
    AppendResponse,
    RequestVote,
    VoteResponse,
    RequestPreVote,
    PreVoteResponse,
    InstallSnapshot,
    SnapshotResponse,
    TimeoutNow,
}

/// Raft message exchanged between nodes
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub msg_type: MessageType,
    pub from: NodeId,
    pub to: NodeId,
    pub term: Term,
    pub index: LogIndex,
    pub entries: Vec<LogEntry>,
    pub commit: LogIndex,
    pub reject: bool,
    pub reject_hint: LogIndex,
    pub snapshot: Option<Snapshot>,
    pub context: Vec<u8>,
}

impl Message {
    pub fn new(msg_type: MessageType, from: NodeId, to: NodeId, term: Term) -> Self {
        Self {
            msg_type,
            from,
            to,
            term,
            index: 0,
            entries: Vec::new(),
            commit: 0,
            reject: false,
            reject_hint: 0,
            snapshot: None,
            context: Vec::new(),
        }
    }

    pub fn with_entries(mut self, entries: Vec<LogEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub fn with_reject(mut self, hint: LogIndex) -> Self {
        self.reject = true;
        self.reject_hint = hint;
        self
    }
}

/// Commands that can be proposed to Raft
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Normal(Vec<u8>),
    ConfChange(ConfChange),
    ReadIndex(Vec<u8>),
}

/// Configuration change types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfChange {
    AddNode { node_id: NodeId, address: String },
    RemoveNode { node_id: NodeId },
    AddLearner { node_id: NodeId, address: String },
}

/// Ready contains changes that need to be persisted/applied
#[derive(Debug, Default)]
pub struct Ready {
    pub hard_state: Option<HardState>,
    pub entries: Vec<LogEntry>,
    pub committed_entries: Vec<LogEntry>,
    pub messages: Vec<Message>,
    pub snapshot: Option<Snapshot>,
}

impl Ready {
    pub fn is_empty(&self) -> bool {
        self.hard_state.is_none()
            && self.entries.is_empty()
            && self.committed_entries.is_empty()
            && self.messages.is_empty()
            && self.snapshot.is_none()
    }
}

/// Raft configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub id: NodeId,
    pub election_timeout_min: Duration,
    pub election_timeout_max: Duration,
    pub heartbeat_interval: Duration,
    pub max_msg_size: usize,
    pub max_inflight: usize,
    pub pre_vote: bool,
    pub check_quorum: bool,
    pub batch_apply: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id: 0,
            election_timeout_min: Duration::from_millis(150),
            election_timeout_max: Duration::from_millis(300),
            heartbeat_interval: Duration::from_millis(50),
            max_msg_size: 1024 * 1024,
            max_inflight: 256,
            pre_vote: true,
            check_quorum: true,
            batch_apply: true,
        }
    }
}

/// Soft state - can be lost on restart
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftState {
    pub leader_id: NodeId,
    pub raft_state: RaftState,
}

/// Read state for read index
#[derive(Debug)]
pub struct ReadState {
    pub index: LogIndex,
    pub request_ctx: Vec<u8>,
}

/// Persistent storage trait for Raft log
pub trait Storage: Send + Sync {
    fn initial_state(&self) -> crate::Result<PersistentState>;
    fn entries(&self, low: LogIndex, high: LogIndex, max_size: usize) -> crate::Result<Vec<LogEntry>>;
    fn term(&self, idx: LogIndex) -> crate::Result<Term>;
    fn first_index(&self) -> crate::Result<LogIndex>;
    fn last_index(&self) -> crate::Result<LogIndex>;
    fn snapshot(&self) -> crate::Result<Snapshot>;
}

/// Raft metrics for monitoring
#[derive(Debug, Default, Clone)]
pub struct RaftMetrics {
    pub state: RaftState,
    pub term: Term,
    pub leader_id: NodeId,
    pub commit_index: LogIndex,
    pub applied_index: LogIndex,
    pub last_index: LogIndex,
    pub pending_proposals: usize,
}

/// Error types for Raft operations
#[derive(Debug, Clone, PartialEq)]
pub enum RaftError {
    /// Node is not the leader, optional hint to the actual leader
    NotLeader { 
        /// Optional hint to the actual leader node ID
        hint: Option<NodeId> 
    },
    /// Proposal was dropped before being committed
    ProposalDropped,
    /// Storage layer error
    Storage(String),
    /// Configuration error
    Configuration(String),
    /// Communication channel was closed
    ChannelClosed,
    /// Invalid state transition
    InvalidState { 
        /// Expected state
        expected: String, 
        /// Actual state
        actual: String 
    },
}

impl fmt::Display for RaftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RaftError::NotLeader { hint } => {
                write!(f, "Not leader")?;
                if let Some(h) = hint {
                    write!(f, " (leader: {:?})", h)?;
                }
                Ok(())
            }
            RaftError::ProposalDropped => write!(f, "Proposal dropped"),
            RaftError::Storage(s) => write!(f, "Storage error: {}", s),
            RaftError::Configuration(s) => write!(f, "Configuration error: {}", s),
            RaftError::ChannelClosed => write!(f, "Channel closed"),
            RaftError::InvalidState { expected, actual } => {
                write!(f, "Invalid state: expected {}, actual {}", expected, actual)
            }
        }
    }
}

impl std::error::Error for RaftError {}

/// Check if votes form a quorum
pub fn is_quorum(voters: usize, votes: usize) -> bool {
    votes > voters / 2
}

/// Generate randomized election timeout
pub fn random_election_timeout(min: Duration, max: Duration) -> Duration {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let range = max.as_millis() - min.as_millis();
    let jitter = rng.gen_range(0..=range);
    min + Duration::from_millis(jitter as u64)
}
