//! Raft consensus implementation for distributed mode

use crate::{Result, RTDBError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


/// Raft node state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// Follower state - receives log entries from leader
    Follower,
    /// Candidate state - running for election
    Candidate,
    /// Leader state - handles all client requests
    Leader,
}

/// Raft consensus node
pub struct RaftNode {
    /// Node ID
    id: String,
    /// Current state
    state: NodeState,
    /// Current term
    term: u64,
    /// Voted for in current term
    voted_for: Option<String>,
    /// Log entries
    log: Vec<LogEntry>,
    /// Commit index
    #[allow(dead_code)]
    commit_index: u64,
    /// Last applied index
    #[allow(dead_code)]
    last_applied: u64,
    /// Next index for each peer (leader only)
    #[allow(dead_code)]
    next_index: HashMap<String, u64>,
    /// Match index for each peer (leader only)
    #[allow(dead_code)]
    match_index: HashMap<String, u64>,
    /// Cluster members
    #[allow(dead_code)]
    peers: Vec<String>,
}

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Term when entry was received
    pub term: u64,
    /// Index in the log
    pub index: u64,
    /// Command to apply
    pub command: Command,
}

/// Command types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Upsert vectors
    Upsert {
        /// Collection name
        collection: String,
        /// Vector data (id, vector pairs)
        vectors: Vec<(u64, Vec<f32>)>,
    },
    /// Delete vectors
    Delete {
        /// Collection name
        collection: String,
        /// Vector IDs to delete
        ids: Vec<u64>,
    },
    /// Create collection
    CreateCollection {
        /// Collection name
        name: String,
        /// Vector dimension
        dimension: usize,
    },
    /// Delete collection
    DeleteCollection {
        /// Collection name
        name: String,
    },
}

/// Raft RPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    /// Leader's term
    pub term: u64,
    /// Leader ID
    pub leader_id: String,
    /// Index of log entry immediately preceding new ones
    pub prev_log_index: u64,
    /// Term of prev_log_index entry
    pub prev_log_term: u64,
    /// Log entries to store
    pub entries: Vec<LogEntry>,
    /// Leader's commit index
    pub leader_commit: u64,
}

/// Raft RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    /// Current term
    pub term: u64,
    /// True if follower contained entry matching prev_log_index and prev_log_term
    pub success: bool,
}

/// Vote request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteRequest {
    /// Candidate's term
    pub term: u64,
    /// Candidate requesting vote
    pub candidate_id: String,
    /// Index of candidate's last log entry
    pub last_log_index: u64,
    /// Term of candidate's last log entry
    pub last_log_term: u64,
}

/// Vote response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteResponse {
    /// Current term
    pub term: u64,
    /// True means candidate received vote
    pub vote_granted: bool,
}

impl RaftNode {
    /// Create new Raft node
    pub fn new(id: String, peers: Vec<String>) -> Self {
        Self {
            id,
            state: NodeState::Follower,
            term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            peers,
        }
    }

    /// Get node ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current state
    pub fn state(&self) -> NodeState {
        self.state
    }

    /// Get current term
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Check if leader
    pub fn is_leader(&self) -> bool {
        self.state == NodeState::Leader
    }

    /// Handle AppendEntries RPC
    pub fn handle_append_entries(
        &mut self,
        req: AppendEntriesRequest,
    ) -> AppendEntriesResponse {
        // Reply false if term < current_term
        if req.term < self.term {
            return AppendEntriesResponse {
                term: self.term,
                success: false,
            };
        }

        // Update term and convert to follower if needed
        if req.term > self.term {
            self.term = req.term;
            self.state = NodeState::Follower;
            self.voted_for = None;
        }

        // TODO: Implement log consistency check
        // TODO: Append new entries
        // TODO: Update commit index

        AppendEntriesResponse {
            term: self.term,
            success: true,
        }
    }

    /// Handle RequestVote RPC
    pub fn handle_request_vote(&mut self, req: RequestVoteRequest) -> RequestVoteResponse {
        // Reply false if term < current_term
        if req.term < self.term {
            return RequestVoteResponse {
                term: self.term,
                vote_granted: false,
            };
        }

        // Update term if needed
        if req.term > self.term {
            self.term = req.term;
            self.voted_for = None;
        }

        // Check if we can vote for this candidate
        let can_vote = self.voted_for.is_none() || self.voted_for.as_ref() == Some(&req.candidate_id);

        // Check if candidate's log is at least as up-to-date
        let last_log_index = self.log.len() as u64;
        let last_log_term = self.log.last().map(|e| e.term).unwrap_or(0);
        
        let log_ok = req.last_log_term > last_log_term
            || (req.last_log_term == last_log_term && req.last_log_index >= last_log_index);

        if can_vote && log_ok {
            self.voted_for = Some(req.candidate_id);
            return RequestVoteResponse {
                term: self.term,
                vote_granted: true,
            };
        }

        RequestVoteResponse {
            term: self.term,
            vote_granted: false,
        }
    }

    /// Propose a new entry (leader only)
    pub fn propose(&mut self, command: Command) -> Result<u64> {
        if self.state != NodeState::Leader {
            return Err(RTDBError::Storage(
                "Not the leader".to_string()
            ));
        }

        let entry = LogEntry {
            term: self.term,
            index: self.log.len() as u64 + 1,
            command,
        };

        let index = entry.index;
        self.log.push(entry);

        Ok(index)
    }

    /// Start election timer (called periodically)
    pub fn tick(&mut self) {
        // TODO: Implement election timeout
        // If follower hasn't heard from leader, become candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_node_creation() {
        let node = RaftNode::new("node1".to_string(), vec!["node2".to_string()]);
        
        assert_eq!(node.id(), "node1");
        assert_eq!(node.state(), NodeState::Follower);
        assert_eq!(node.term(), 0);
        assert!(!node.is_leader());
    }

    #[test]
    fn test_request_vote() {
        let mut node = RaftNode::new("node1".to_string(), vec![]);
        
        let req = RequestVoteRequest {
            term: 1,
            candidate_id: "node2".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };
        
        let resp = node.handle_request_vote(req);
        assert!(resp.vote_granted);
        assert_eq!(resp.term, 1);
    }

    #[test]
    fn test_append_entries() {
        let mut node = RaftNode::new("node1".to_string(), vec![]);
        node.term = 1;
        
        let req = AppendEntriesRequest {
            term: 1,
            leader_id: "node2".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        };
        
        let resp = node.handle_append_entries(req);
        assert!(resp.success);
    }
}
