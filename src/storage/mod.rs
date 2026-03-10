//! Storage layer for RTDB
//! 
//! Implements LSM-tree based vector storage with:
//! - Write-Ahead Log (WAL) for durability
//! - MemTable for in-memory buffering
//! - SSTables for persistent storage
//! - Columnar format for vectors

pub mod wal;
pub mod memtable;
pub mod sstable;
pub mod engine;

pub use engine::StorageEngine;
pub use wal::{WAL, WALEntry};
pub use memtable::MemTable;

use crate::{Result, RTDBError, Vector, VectorId};
use serde::{Deserialize, Serialize};

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Storage directory path
    pub path: String,
    /// WAL segment size in bytes
    pub wal_segment_size: usize,
    /// MemTable size threshold in bytes
    pub memtable_size_threshold: usize,
    /// Block size for SSTables
    pub block_size: usize,
    /// Compression type
    pub compression: CompressionType,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "./rtdb_storage".to_string(),
            wal_segment_size: 64 * 1024 * 1024, // 64MB
            memtable_size_threshold: 64 * 1024 * 1024, // 64MB
            block_size: 4 * 1024, // 4KB
            compression: CompressionType::Zstd,
        }
    }
}

/// Compression types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    /// No compression
    None,
    /// LZ4 compression
    Lz4,
    /// Zstd compression
    Zstd,
    /// Snappy compression
    Snappy,
}

/// Record types for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Record {
    /// Insert or update a vector
    Put {
        id: VectorId,
        vector: Vector,
        timestamp: u64,
    },
    /// Delete a vector
    Delete {
        id: VectorId,
        timestamp: u64,
    },
}

/// Storage statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Total vectors stored
    pub vector_count: u64,
    /// Total bytes used
    pub storage_size: u64,
    /// WAL size
    pub wal_size: u64,
    /// Number of SSTables
    pub sstable_count: usize,
    /// MemTable size in bytes
    pub memtable_size: usize,
}

/// Helper trait for storage operations
pub trait Storage: Send + Sync {
    /// Get a vector by ID
    fn get(&self, id: VectorId) -> Result<Option<Vector>>;
    
    /// Put a vector
    fn put(&self, id: VectorId, vector: Vector) -> Result<()>;
    
    /// Delete a vector
    fn delete(&self, id: VectorId) -> Result<()>;
    
    /// Scan vectors in range
    fn scan(&self, start: Option<VectorId>, end: Option<VectorId>) -> Result<Vec<(VectorId, Vector)>>;
    
    /// Flush memtable to disk
    fn flush(&self) -> Result<()>;
    
    /// Get storage statistics
    fn stats(&self) -> StorageStats;
}

/// Convert storage error
pub fn into_storage_error<E: std::fmt::Display>(e: E) -> RTDBError {
    RTDBError::Storage(e.to_string())
}
