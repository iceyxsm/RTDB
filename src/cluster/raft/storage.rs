//! In-memory storage implementation for Raft log
//!
//! Provides a reference implementation of the Storage trait.
//! Production deployments should use a persistent storage backend.

#![allow(missing_docs)]

use super::types::*;
use crate::{RTDBError, Result};
use parking_lot::RwLock;
use std::sync::Arc;

/// In-memory storage for Raft log entries and state
#[derive(Debug)]
pub struct MemStorage {
    inner: Arc<RwLock<MemStorageCore>>,
}

#[derive(Debug)]
struct MemStorageCore {
    /// Hard state
    hard_state: HardState,
    /// Configuration state
    conf_state: ConfState,
    /// Log entries
    entries: Vec<LogEntry>,
    /// Snapshot
    snapshot: Snapshot,
}

impl MemStorage {
    /// Create new memory storage with initial state
    pub fn new() -> Self {
        let core = MemStorageCore {
            hard_state: HardState::default(),
            conf_state: ConfState::new(Vec::new()),
            entries: Vec::new(),
            snapshot: Snapshot {
                metadata: SnapshotMetadata {
                    index: 0,
                    term: 0,
                    conf_state: ConfState::new(Vec::new()),
                },
                data: Vec::new(),
            },
        };

        Self {
            inner: Arc::new(RwLock::new(core)),
        }
    }

    /// Create with initial configuration
    pub fn with_conf_state(conf_state: ConfState) -> Self {
        let storage = Self::new();
        storage.inner.write().conf_state = conf_state;
        storage
    }

    /// Initialize with first index
    pub fn initialize_with_entries(&self, entries: Vec<LogEntry>) {
        let mut core = self.inner.write();
        core.entries = entries;
    }

    /// Set hard state
    pub fn set_hard_state(&self, hard_state: HardState) {
        self.inner.write().hard_state = hard_state;
    }

    /// Append entries to log
    pub fn append(&self, entries: &[LogEntry]) {
        let mut core = self.inner.write();
        
        if entries.is_empty() {
            return;
        }

        // Find insertion point
        let first = entries[0].index;
        if first <= core.snapshot.metadata.index {
            // Already compacted
            return;
        }

        // Truncate if needed
        let offset = core.entries.first().map(|e| e.index).unwrap_or(1);
        if first < offset {
            panic!("entry {} is out of range, offset {}", first, offset);
        }

        // Calculate position
        let pos = (first - offset) as usize;
        
        // Truncate and append
        if pos < core.entries.len() {
            core.entries.truncate(pos);
        }
        
        core.entries.extend_from_slice(entries);
    }

    /// Compact log up to compact_index
    pub fn compact(&self, compact_index: LogIndex) -> Result<()> {
        let mut core = self.inner.write();

        // Only compact if we have entries to remove (compact_index > snapshot index)
        // If compact_index == snapshot_index, we still want to remove old entries
        if compact_index < core.snapshot.metadata.index {
            return Ok(());
        }

        if compact_index > core.last_index() {
            return Err(RTDBError::Storage(format!(
                "compact index {} exceeds last index {}",
                compact_index,
                core.last_index()
            )));
        }

        let offset = core.entries[0].index;
        // Keep entries after compact_index (compact_index + 1)
        let pos = (compact_index + 1 - offset) as usize;

        // Create new compacted entries
        let new_entries = core.entries[pos..].to_vec();
        core.entries = new_entries;

        Ok(())
    }

    /// Create snapshot
    pub fn create_snapshot(
        &self,
        index: LogIndex,
        conf_state: ConfState,
        data: Vec<u8>,
    ) -> Result<()> {
        let mut core = self.inner.write();

        if index <= core.snapshot.metadata.index {
            return Err(RTDBError::Storage(format!(
                "snapshot index {} is not greater than existing {}",
                index,
                core.snapshot.metadata.index
            )));
        }

        let term = core.term(index)?;

        core.snapshot = Snapshot {
            metadata: SnapshotMetadata {
                index,
                term,
                conf_state,
            },
            data,
        };

        Ok(())
    }

    /// Apply snapshot
    pub fn apply_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        let mut core = self.inner.write();

        let meta = &snapshot.metadata;

        if meta.index < core.snapshot.metadata.index {
            return Err(RTDBError::Storage(format!(
                "snapshot index {} is older than existing {}",
                meta.index,
                core.snapshot.metadata.index
            )));
        }

        core.snapshot = snapshot;
        core.hard_state.term = core.snapshot.metadata.term;
        core.hard_state.commit_index = core.snapshot.metadata.index;
        core.conf_state = core.snapshot.metadata.conf_state.clone();
        core.entries.clear();

        Ok(())
    }
}

impl Default for MemStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for MemStorage {
    fn initial_state(&self) -> Result<PersistentState> {
        let core = self.inner.read();
        Ok(PersistentState {
            hard_state: core.hard_state,
            conf_state: core.conf_state.clone(),
        })
    }

    fn entries(&self, low: LogIndex, high: LogIndex, max_size: usize) -> Result<Vec<LogEntry>> {
        let core = self.inner.read();
        core.entries(low, high, max_size)
    }

    fn term(&self, idx: LogIndex) -> Result<Term> {
        let core = self.inner.read();
        core.term(idx)
    }

    fn first_index(&self) -> Result<LogIndex> {
        let core = self.inner.read();
        Ok(core.first_index())
    }

    fn last_index(&self) -> Result<LogIndex> {
        let core = self.inner.read();
        Ok(core.last_index())
    }

    fn snapshot(&self) -> Result<Snapshot> {
        let core = self.inner.read();
        Ok(core.snapshot.clone())
    }
}

impl MemStorageCore {
    fn first_index(&self) -> LogIndex {
        self.entries.first().map(|e| e.index).unwrap_or(self.snapshot.metadata.index + 1)
    }

    fn last_index(&self) -> LogIndex {
        self.entries.last().map(|e| e.index).unwrap_or(self.snapshot.metadata.index)
    }

    fn term(&self, idx: LogIndex) -> crate::Result<Term> {
        if idx == 0 {
            return Ok(0);
        }

        if idx == self.snapshot.metadata.index {
            return Ok(self.snapshot.metadata.term);
        }

        if idx < self.first_index() || idx > self.last_index() {
            return Err(RTDBError::Storage(format!(
                "index {} out of range [{}, {}]",
                idx,
                self.first_index(),
                self.last_index()
            )));
        }

        let offset = self.entries[0].index;
        Ok(self.entries[(idx - offset) as usize].term)
    }

    fn entries(&self, low: LogIndex, high: LogIndex, max_size: usize) -> crate::Result<Vec<LogEntry>> {
        if low > high {
            return Err(RTDBError::Storage(format!(
                "low {} > high {}",
                low, high
            )));
        }

        if low < self.first_index() {
            return Err(RTDBError::Storage(format!(
                "low {} is less than first index {}",
                low,
                self.first_index()
            )));
        }

        if high > self.last_index() + 1 {
            return Err(RTDBError::Storage(format!(
                "high {} is greater than last index + 1 {}",
                high,
                self.last_index() + 1
            )));
        }

        let offset = self.entries[0].index;
        let lo = (low - offset) as usize;
        let hi = (high - offset) as usize;

        let mut entries = Vec::with_capacity(hi - lo);
        let mut size = 0;

        for entry in &self.entries[lo..hi] {
            size += entry.data.len() + 32;
            if size > max_size && !entries.is_empty() {
                break;
            }
            entries.push(entry.clone());
            if size > max_size {
                break;
            }
        }

        Ok(entries)
    }
}

/// Persistent storage using file-based WAL (write-ahead log)
/// 
/// This is a simplified version - production should use proper WAL
/// like RocksDB or dedicated WAL implementation.
#[derive(Debug)]
pub struct FileStorage {
    #[allow(dead_code)]
    path: std::path::PathBuf,
    inner: RwLock<FileStorageCore>,
}

#[derive(Debug)]
struct FileStorageCore {
    hard_state: HardState,
    conf_state: ConfState,
    entries: Vec<LogEntry>,
    snapshot: Snapshot,
}

impl FileStorage {
    /// Create a new file-based Raft storage
    pub fn new(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        // Create directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let core = FileStorageCore {
            hard_state: HardState::default(),
            conf_state: ConfState::new(Vec::new()),
            entries: Vec::new(),
            snapshot: Snapshot {
                metadata: SnapshotMetadata {
                    index: 0,
                    term: 0,
                    conf_state: ConfState::new(Vec::new()),
                },
                data: Vec::new(),
            },
        };

        Ok(Self {
            path,
            inner: RwLock::new(core),
        })
    }
}

impl Storage for FileStorage {
    fn initial_state(&self) -> crate::Result<PersistentState> {
        let core = self.inner.read();
        Ok(PersistentState {
            hard_state: core.hard_state,
            conf_state: core.conf_state.clone(),
        })
    }

    fn entries(&self, low: LogIndex, high: LogIndex, _max_size: usize) -> crate::Result<Vec<LogEntry>> {
        let core = self.inner.read();
        if core.entries.is_empty() {
            return Ok(Vec::new());
        }

        let offset = core.entries[0].index;
        let lo = ((low.saturating_sub(offset)).max(0)) as usize;
        let hi = ((high - offset).min(core.entries.len() as u64)) as usize;

        Ok(core.entries[lo..hi].to_vec())
    }

    fn term(&self, idx: LogIndex) -> crate::Result<Term> {
        let core = self.inner.read();
        
        if idx == 0 {
            return Ok(0);
        }

        if idx == core.snapshot.metadata.index {
            return Ok(core.snapshot.metadata.term);
        }

        if core.entries.is_empty() {
            return Err(RTDBError::Storage(format!("empty log, index {}", idx)));
        }

        let offset = core.entries[0].index;
        if idx < offset || idx > offset + core.entries.len() as u64 - 1 {
            return Err(RTDBError::Storage(format!("index {} out of range", idx)));
        }

        Ok(core.entries[(idx - offset) as usize].term)
    }

    fn first_index(&self) -> crate::Result<LogIndex> {
        let core = self.inner.read();
        Ok(core.entries.first().map(|e| e.index).unwrap_or(core.snapshot.metadata.index + 1))
    }

    fn last_index(&self) -> crate::Result<LogIndex> {
        let core = self.inner.read();
        Ok(core.entries.last().map(|e| e.index).unwrap_or(core.snapshot.metadata.index))
    }

    fn snapshot(&self) -> crate::Result<Snapshot> {
        let core = self.inner.read();
        Ok(core.snapshot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_storage_basic() {
        let storage = MemStorage::new();
        
        let state = storage.initial_state().unwrap();
        assert_eq!(state.hard_state.term, 0);
        
        // Append entries
        let entries = vec![
            LogEntry::new(1, 1, vec![1, 2, 3]),
            LogEntry::new(2, 1, vec![4, 5, 6]),
        ];
        storage.append(&entries);
        
        assert_eq!(storage.last_index().unwrap(), 2);
        assert_eq!(storage.first_index().unwrap(), 1);
        
        // Get entries
        let fetched = storage.entries(1, 3, usize::MAX).unwrap();
        assert_eq!(fetched.len(), 2);
    }

    #[test]
    fn test_mem_storage_snapshot() {
        let storage = MemStorage::new();
        
        // Add entries
        let entries = vec![
            LogEntry::new(1, 1, vec![1]),
            LogEntry::new(2, 1, vec![2]),
            LogEntry::new(3, 2, vec![3]),
        ];
        storage.append(&entries);
        
        // Create snapshot
        storage.create_snapshot(2, ConfState::new(vec![1, 2, 3]), vec![9, 8, 7]).unwrap();
        
        let snapshot = storage.snapshot().unwrap();
        assert_eq!(snapshot.metadata.index, 2);
        assert_eq!(snapshot.metadata.term, 1);
        
        // Compact
        storage.compact(2).unwrap();
        
        assert_eq!(storage.first_index().unwrap(), 3);
    }
}
