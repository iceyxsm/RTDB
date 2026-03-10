//! MemTable implementation
//! 
//! In-memory buffer for recent writes using lock-free skiplist
//! for concurrent access.

use crate::{Result, RTDBError, Vector, VectorId};
use crossbeam_skiplist::SkipMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// MemTable entry types
#[derive(Debug, Clone)]
pub enum MemTableEntry {
    /// Valid vector entry
    Put(Vector),
    /// Deleted entry (tombstone)
    Delete,
}

/// MemTable for in-memory buffering
pub struct MemTable {
    /// Skip list storing entries
    entries: SkipMap<VectorId, MemTableEntry>,
    /// Approximate size in bytes
    size: AtomicUsize,
    /// Size threshold for flushing
    size_threshold: usize,
}

impl MemTable {
    /// Create new MemTable
    pub fn new(size_threshold: usize) -> Self {
        Self {
            entries: SkipMap::new(),
            size: AtomicUsize::new(0),
            size_threshold,
        }
    }

    /// Insert or update a vector
    pub fn put(&self, id: VectorId, vector: Vector) -> Result<()> {
        let entry_size = Self::estimate_entry_size(id, &vector);
        
        // Remove old entry if exists
        if let Some(old) = self.entries.remove(&id) {
            let old_size = Self::estimate_entry_size(id, match old.value() {
                MemTableEntry::Put(v) => v,
                MemTableEntry::Delete => return Err(RTDBError::Storage(
                    "Unexpected delete entry".to_string()
                )),
            });
            self.size.fetch_sub(old_size, Ordering::Relaxed);
        }

        // Insert new entry
        self.entries.insert(id, MemTableEntry::Put(vector));
        self.size.fetch_add(entry_size, Ordering::Relaxed);

        Ok(())
    }

    /// Delete a vector (insert tombstone)
    pub fn delete(&self, id: VectorId) -> Result<()> {
        // Remove old entry if exists
        if let Some(old) = self.entries.remove(&id) {
            let old_size = match old.value() {
                MemTableEntry::Put(v) => Self::estimate_entry_size(id, v),
                MemTableEntry::Delete => 0,
            };
            self.size.fetch_sub(old_size, Ordering::Relaxed);
        }

        // Insert tombstone
        self.entries.insert(id, MemTableEntry::Delete);
        self.size.fetch_add(Self::estimate_tombstone_size(id), Ordering::Relaxed);

        Ok(())
    }

    /// Get a vector by ID
    pub fn get(&self, id: &VectorId) -> Option<MemTableEntry> {
        self.entries.get(id).map(|e| e.value().clone())
    }

    /// Check if should flush
    pub fn should_flush(&self) -> bool {
        self.size.load(Ordering::Relaxed) >= self.size_threshold
    }

    /// Get approximate size
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate all entries
    pub fn iter(&self) -> impl Iterator<Item = (VectorId, MemTableEntry)> + '_ {
        self.entries.iter().map(|e| (*e.key(), e.value().clone()))
    }

    /// Scan entries in range
    pub fn scan(
        &self,
        start: Option<&VectorId>,
        end: Option<&VectorId>,
    ) -> Vec<(VectorId, Vector)> {
        let mut results = Vec::new();

        let iter = match start {
            Some(start_id) => self.entries.lower_bound(std::ops::Bound::Included(start_id)),
            None => self.entries.front(),
        };

        for entry in iter {
            if let Some(end_id) = end {
                if entry.key() > end_id {
                    break;
                }
            }

            if let MemTableEntry::Put(vector) = entry.value() {
                results.push((*entry.key(), vector.clone()));
            }
        }

        results
    }

    /// Estimate entry size
    fn estimate_entry_size(_id: VectorId, vector: &Vector) -> usize {
        // Rough estimation: ID (8) + vector data (4 * dim) + payload overhead
        let vector_size = vector.data.len() * 4;
        let payload_size = vector.payload.as_ref().map(|p| {
            serde_json::to_vec(p).map(|v| v.len()).unwrap_or(0)
        }).unwrap_or(0);
        
        8 + vector_size + payload_size + 64 // overhead
    }

    /// Estimate tombstone size
    fn estimate_tombstone_size(_id: VectorId) -> usize {
        8 + 1 // ID + flag
    }
}

/// Immutable MemTable (after flush trigger)
pub struct ImmutableMemTable {
    /// Inner table
    inner: Arc<MemTable>,
}

impl ImmutableMemTable {
    /// Create from mutable memtable
    pub fn new(table: MemTable) -> Self {
        Self {
            inner: Arc::new(table),
        }
    }

    /// Get entries for flushing
    pub fn entries(&self) -> Vec<(VectorId, Vector)> {
        self.inner
            .iter()
            .filter_map(|(id, entry)| {
                if let MemTableEntry::Put(vector) = entry {
                    Some((id, vector))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get tombstones
    pub fn tombstones(&self) -> Vec<VectorId> {
        self.inner
            .iter()
            .filter_map(|(id, entry)| {
                if let MemTableEntry::Delete = entry {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Approximate size
    pub fn size(&self) -> usize {
        self.inner.size()
    }
}

/// MemTable manager handling rotation
pub struct MemTableManager {
    /// Active mutable memtable
    active: Arc<MemTable>,
    /// Immutable memtables waiting to flush
    immutable: parking_lot::Mutex<Vec<ImmutableMemTable>>,
    /// Size threshold
    size_threshold: usize,
}

impl MemTableManager {
    /// Create new manager
    pub fn new(size_threshold: usize) -> Self {
        Self {
            active: Arc::new(MemTable::new(size_threshold)),
            immutable: parking_lot::Mutex::new(Vec::new()),
            size_threshold,
        }
    }

    /// Get active memtable
    pub fn active(&self) -> &MemTable {
        &self.active
    }

    /// Rotate memtable (make active immutable, create new)
    pub fn rotate(&self) -> ImmutableMemTable {
        let new_active = Arc::new(MemTable::new(self.size_threshold));
        let old_active = std::mem::replace(
            unsafe { &mut *(Arc::as_ptr(&self.active) as *mut MemTable) },
            MemTable::new(self.size_threshold)
        );

        let immutable = ImmutableMemTable::new(old_active);
        self.immutable.lock().push(ImmutableMemTable {
            inner: immutable.inner.clone(),
        });

        immutable
    }

    /// Take immutable tables for flushing
    pub fn take_immutable(&self) -> Vec<ImmutableMemTable> {
        std::mem::take(&mut *self.immutable.lock())
    }

    /// Check if needs rotation
    pub fn needs_rotation(&self) -> bool {
        self.active.should_flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memtable_put_get() {
        let mt = MemTable::new(1024 * 1024);

        let v1 = Vector::new(vec![1.0, 2.0, 3.0]);
        mt.put(1, v1.clone()).unwrap();

        let result = mt.get(&1);
        assert!(matches!(result, Some(MemTableEntry::Put(_))));
        
        if let Some(MemTableEntry::Put(v)) = result {
            assert_eq!(v.data, v1.data);
        }
    }

    #[test]
    fn test_memtable_delete() {
        let mt = MemTable::new(1024 * 1024);

        let v1 = Vector::new(vec![1.0, 2.0, 3.0]);
        mt.put(1, v1).unwrap();
        mt.delete(1).unwrap();

        assert!(matches!(mt.get(&1), Some(MemTableEntry::Delete)));
    }

    #[test]
    fn test_memtable_scan() {
        let mt = MemTable::new(1024 * 1024);

        for i in 1..=10 {
            let v = Vector::new(vec![i as f32; 3]);
            mt.put(i, v).unwrap();
        }

        // Delete some
        mt.delete(3).unwrap();
        mt.delete(7).unwrap();

        // Scan range
        let results = mt.scan(Some(&2), Some(&8));
        assert_eq!(results.len(), 5); // 2, 4, 5, 6, 8
    }

    #[test]
    fn test_memtable_flush_trigger() {
        let mt = MemTable::new(100); // Small threshold

        for i in 0..100 {
            let v = Vector::new(vec![i as f32; 100]);
            mt.put(i, v).unwrap();
        }

        assert!(mt.should_flush());
    }
}
