//! Storage Engine
//! 
//! Top-level storage coordinator that manages:
//! - WAL for durability
//! - MemTable for in-memory buffering
//! - SSTables for persistent storage
//! - Compaction

use super::{MemTable, Record, SSTable, SSTableBuilder, Storage, StorageConfig, StorageStats, sstable::CompressionType as SSTCompression};
use crate::into_storage_error;
use crate::{Result, RTDBError, Vector, VectorId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Storage Engine
pub struct StorageEngine {
    /// Configuration
    config: StorageConfig,
    /// Write-ahead log
    wal: parking_lot::Mutex<super::WAL>,
    /// Active memtable
    memtable: Arc<RwLock<Arc<MemTable>>>,
    /// Immutable memtables waiting to flush
    immutable: parking_lot::Mutex<Vec<Arc<MemTable>>>,
    /// SSTables by level
    levels: RwLock<Vec<Vec<SSTable>>>,
    /// Vector count
    vector_count: AtomicU64,
    /// Background flush handle
    flush_handle: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl StorageEngine {
    /// Create or open storage engine
    pub fn open(config: StorageConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.path).map_err(into_storage_error)?;

        // Open WAL
        let wal_path = Path::new(&config.path).join("wal");
        let mut wal = super::WAL::open(&wal_path, config.wal_segment_size as u64)?;

        // Create memtable
        let memtable = MemTable::new(config.memtable_size_threshold);

        // Recover from WAL
        Self::recover(&mut wal, &memtable)?;

        // Load existing SSTables
        let levels = Self::load_sstables(&config.path)?;

        // Count vectors
        let mut vector_count = 0u64;
        for level in &levels {
            for table in level {
                vector_count += table.meta.entry_count;
            }
        }

        Ok(StorageEngine {
            config,
            wal: parking_lot::Mutex::new(wal),
            memtable: Arc::new(RwLock::new(Arc::new(memtable))),
            immutable: parking_lot::Mutex::new(Vec::new()),
            levels: RwLock::new(levels),
            vector_count: AtomicU64::new(vector_count),
            flush_handle: parking_lot::Mutex::new(None),
        })
    }

    /// Recover from WAL
    fn recover(wal: &mut super::WAL, memtable: &MemTable) -> Result<()> {
        let entries = wal.read_entries()?;
        
        for entry in entries {
            let record: Record = serde_json::from_slice(&entry.data)
                .map_err(|e| RTDBError::Serialization(e.to_string()))?;

            match record {
                Record::Put { id, vector, .. } => {
                    memtable.put(id, vector)?;
                }
                Record::Delete { id, .. } => {
                    memtable.delete(id)?;
                }
            }
        }

        Ok(())
    }

    /// Load existing SSTables from disk
    fn load_sstables(path: &str) -> Result<Vec<Vec<SSTable>>> {
        let mut levels: Vec<Vec<SSTable>> = Vec::new();
        let path = Path::new(path);

        // Read level directories
        for level_idx in 0..=6 {
            let level_path = path.join(format!("level-{}", level_idx));
            if !level_path.exists() {
                std::fs::create_dir_all(&level_path).map_err(into_storage_error)?;
                levels.push(Vec::new());
                continue;
            }

            let mut level_tables = Vec::new();
            for entry in std::fs::read_dir(&level_path).map_err(into_storage_error)? {
                let entry = entry.map_err(into_storage_error)?;
                let path = entry.path();
                
                if path.extension().map(|e| e == "sst").unwrap_or(false) {
                    match SSTable::open(&path) {
                        Ok(table) => level_tables.push(table),
                        Err(e) => {
                            eprintln!("Warning: Failed to open SSTable {:?}: {}", path, e);
                        }
                    }
                }
            }

            // Sort by creation time (filename)
            level_tables.sort_by(|a: &SSTable, b: &SSTable| a.path().cmp(b.path()));
            levels.push(level_tables);
        }

        Ok(levels)
    }

    /// Maybe flush memtable
    fn maybe_flush(&self) -> Result<()> {
        let memtable = self.memtable.read().clone();
        
        if memtable.should_flush() {
            // Create new memtable
            let new_memtable = Arc::new(MemTable::new(self.config.memtable_size_threshold));
            
            // Swap
            {
                let mut guard = self.memtable.write();
                *guard = new_memtable;
            }

            // Add to immutable queue
            self.immutable.lock().push(memtable);

            // Trigger background flush
            self.trigger_flush()?;
        }

        Ok(())
    }

    /// Trigger background flush
    fn trigger_flush(&self) -> Result<()> {
        let mut handle = self.flush_handle.lock();
        
        if handle.is_none() {
            let immutable = std::mem::take(&mut *self.immutable.lock());
            let config = self.config.clone();
            
            let task = tokio::spawn(async move {
                for memtable in immutable {
                    if let Err(e) = Self::flush_memtable(&config, &memtable).await {
                        eprintln!("Flush error: {}", e);
                    }
                }
            });

            *handle = Some(task);
        }

        Ok(())
    }

    /// Flush memtable to SSTable
    async fn flush_memtable(config: &StorageConfig, memtable: &MemTable) -> Result<()> {
        let level_path = Path::new(&config.path).join("level-0");
        std::fs::create_dir_all(&level_path).map_err(into_storage_error)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros();

        let path = level_path.join(format!("{:020}.sst", timestamp));

        let mut builder = SSTableBuilder::create(
            &path,
            0, // Will detect from data
            SSTCompression::None,
            config.block_size,
            0,
        )?;

        for (id, entry) in memtable.iter() {
            if let super::MemTableEntry::Put(vector) = entry {
                builder.add(id, vector)?;
            }
        }

        builder.finish()?;

        Ok(())
    }

    /// Get vector from memtables
    fn get_from_memtables(&self, id: &VectorId) -> Option<Vector> {
        // Check active memtable
        if let Some(entry) = self.memtable.read().get(id) {
            return match entry {
                super::MemTableEntry::Put(v) => Some(v),
                super::MemTableEntry::Delete => None,
            };
        }

        // Check immutable memtables
        for memtable in self.immutable.lock().iter() {
            if let Some(entry) = memtable.get(id) {
                return match entry {
                    super::MemTableEntry::Put(v) => Some(v),
                    super::MemTableEntry::Delete => None,
                };
            }
        }

        None
    }

    /// Get vector from SSTables
    fn get_from_sstables(&self, id: VectorId) -> Result<Option<Vector>> {
        let levels = self.levels.read();

        // Search from newest to oldest
        for level in levels.iter() {
            for table in level.iter().rev() {
                if !table.may_contain(id) {
                    continue;
                }

                // Clone table data to avoid borrow issues
                let path = table.path().to_path_buf();
                
                let table_result = SSTable::open(&path)?.get(id);
                if let Some(vector) = table_result? {
                    return Ok(Some(vector));
                }

                return Ok(None);
            }
        }

        Ok(None)
    }

    /// Compact level
    pub fn compact_level(&self, level: usize) -> Result<()> {
        if level >= 6 {
            return Ok(()); // Max level
        }

        let mut levels = self.levels.write();
        if level >= levels.len() || levels[level].len() < 4 {
            return Ok(()); // Not enough files to compact
        }

        // Pick files to compact (oldest)
        let num_to_compact = levels[level].len().min(4);
        let _to_compact: Vec<SSTable> = levels[level].drain(0..num_to_compact).collect();

        // Merge and write to next level
        let next_level_path = Path::new(&self.config.path).join(format!("level-{}", level + 1));
        std::fs::create_dir_all(&next_level_path).map_err(into_storage_error)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros();

        let _output_path = next_level_path.join(format!("{:020}.sst", timestamp));

        // TODO: Implement actual merge
        // For now, just move files
        drop(levels);

        // Rebuild levels
        let levels = Self::load_sstables(&self.config.path)?;
        *self.levels.write() = levels;

        Ok(())
    }
}

impl Storage for StorageEngine {
    fn get(&self, id: VectorId) -> Result<Option<Vector>> {
        // 1. Check memtables
        if let Some(vector) = self.get_from_memtables(&id) {
            return Ok(Some(vector));
        }

        // 2. Check SSTables
        self.get_from_sstables(id)
    }

    fn put(&self, id: VectorId, vector: Vector) -> Result<()> {
        // 1. Write to WAL
        let record = Record::Put {
            id,
            vector: vector.clone(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
        };

        let data = serde_json::to_vec(&record)
            .map_err(|e| RTDBError::Serialization(e.to_string()))?;

        self.wal.lock().append(&data)?;

        // 2. Write to memtable
        self.memtable.read().put(id, vector)?;

        // 3. Maybe flush
        self.maybe_flush()?;

        // 4. Update count
        self.vector_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    fn delete(&self, id: VectorId) -> Result<()> {
        // 1. Write to WAL
        let record = Record::Delete {
            id,
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
        };

        let data = serde_json::to_vec(&record)
            .map_err(|e| RTDBError::Serialization(e.to_string()))?;

        self.wal.lock().append(&data)?;

        // 2. Write tombstone to memtable
        self.memtable.read().delete(id)?;

        // 3. Maybe flush
        self.maybe_flush()?;

        Ok(())
    }

    fn scan(&self, start: Option<VectorId>, end: Option<VectorId>) -> Result<Vec<(VectorId, Vector)>> {
        let mut results: HashMap<VectorId, Vector> = HashMap::new();
        let mut deleted: std::collections::HashSet<VectorId> = std::collections::HashSet::new();

        // 1. Scan memtables
        for (id, entry) in self.memtable.read().iter() {
            match entry {
                super::MemTableEntry::Put(v) => {
                    if Self::in_range(id, start, end) {
                        results.insert(id, v);
                    }
                }
                super::MemTableEntry::Delete => {
                    deleted.insert(id);
                }
            }
        }

        // 2. Scan SSTables
        let levels = self.levels.read();
        for level in levels.iter() {
            for table in level.iter().rev() {
                let path = table.path().to_path_buf();
                let table = SSTable::open(&path)?;
                let entries = table.scan(start, end)?;

                for (id, vector) in entries {
                    if !deleted.contains(&id) && !results.contains_key(&id) {
                        results.insert(id, vector);
                    }
                }
            }
        }

        // Convert to vec
        let mut vec: Vec<_> = results.into_iter().collect();
        vec.sort_by_key(|(id, _)| *id);

        Ok(vec)
    }

    fn flush(&self) -> Result<()> {
        // Force flush all memtables
        let mut handle = self.flush_handle.lock();
        
        if let Some(task) = handle.take() {
            // Wait for current flush
            drop(task);
        }

        // Trigger new flush
        self.trigger_flush()?;

        // Wait for completion
        loop {
            let has_immutable = !self.immutable.lock().is_empty();
            if !has_immutable {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        Ok(())
    }

    fn stats(&self) -> StorageStats {
        let levels = self.levels.read();
        let mut sstable_count = 0;
        let mut storage_size = 0u64;

        for level in levels.iter() {
            sstable_count += level.len();
            for table in level {
                if let Ok(meta) = std::fs::metadata(table.path()) {
                    storage_size += meta.len();
                }
            }
        }

        StorageStats {
            vector_count: self.vector_count.load(Ordering::Relaxed),
            storage_size,
            wal_size: self.wal.lock().size(),
            sstable_count,
            memtable_size: self.memtable.read().size(),
        }
    }
}

impl StorageEngine {
    /// Check if ID is in range
    fn in_range(id: VectorId, start: Option<VectorId>, end: Option<VectorId>) -> bool {
        match (start, end) {
            (Some(s), Some(e)) => id >= s && id <= e,
            (Some(s), None) => id >= s,
            (None, Some(e)) => id <= e,
            (None, None) => true,
        }
    }

    /// Get configuration
    pub fn config(&self) -> &StorageConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::CompressionType;
    use tempfile::TempDir;

    #[test]
    fn test_storage_engine_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            path: temp_dir.path().to_str().unwrap().to_string(),
            wal_segment_size: 1024 * 1024,
            memtable_size_threshold: 1024 * 1024,
            block_size: 4 * 1024,
            compression: CompressionType::None,
        };

        let engine = StorageEngine::open(config).unwrap();

        // Put
        let v1 = Vector::new(vec![1.0, 2.0, 3.0]);
        engine.put(1, v1.clone()).unwrap();

        // Get
        let result = engine.get(1).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().data, v1.data);

        // Delete
        engine.delete(1).unwrap();
        let result = engine.get(1).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_storage_engine_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap().to_string();

        // Write
        {
            let config = StorageConfig {
                path: path.clone(),
                wal_segment_size: 1024 * 1024,
                memtable_size_threshold: 1024 * 1024,
                block_size: 4 * 1024,
                compression: CompressionType::None,
            };

            let engine = StorageEngine::open(config).unwrap();
            
            for i in 1..=100 {
                let v = Vector::new(vec![i as f32; 3]);
                engine.put(i, v).unwrap();
            }
        }

        // Reopen and read
        {
            let config = StorageConfig {
                path: path.clone(),
                wal_segment_size: 1024 * 1024,
                memtable_size_threshold: 1024 * 1024,
                block_size: 4 * 1024,
                compression: CompressionType::None,
            };

            let engine = StorageEngine::open(config).unwrap();
            
            for i in 1..=100 {
                let result = engine.get(i).unwrap();
                assert!(result.is_some(), "Failed to get {}", i);
                assert_eq!(result.unwrap().data, vec![i as f32; 3]);
            }
        }
    }

    #[test]
    fn test_storage_engine_scan() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            path: temp_dir.path().to_str().unwrap().to_string(),
            wal_segment_size: 1024 * 1024,
            memtable_size_threshold: 1024 * 1024,
            block_size: 4 * 1024,
            compression: CompressionType::None,
        };

        let engine = StorageEngine::open(config).unwrap();

        for i in 1..=100 {
            let v = Vector::new(vec![i as f32; 3]);
            engine.put(i, v).unwrap();
        }

        let results = engine.scan(Some(10), Some(20)).unwrap();
        assert_eq!(results.len(), 11);
    }
}
