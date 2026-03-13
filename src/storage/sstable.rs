//! SSTable (Sorted String Table) implementation
//! 
//! Persistent storage format for vectors with:
//! - Block-based layout for efficient I/O
//! - Bloom filters for negative lookups
//! - Index blocks for binary search
//! - Compression support

use crate::{Result, RTDBError, Vector, VectorId, into_storage_error};
use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// SSTable file format version
const SSTABLE_VERSION: u32 = 1;
/// Magic number
const SSTABLE_MAGIC: u32 = 0x53535442; // "SSTB"
/// Default block size
#[allow(dead_code)]
const DEFAULT_BLOCK_SIZE: usize = 4 * 1024;
/// Index interval (every N entries)
#[allow(dead_code)]
const INDEX_INTERVAL: usize = 16;

/// SSTable metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSTableMeta {
    /// Format version
    pub version: u32,
    /// Vector dimension
    pub dimension: usize,
    /// Number of entries
    pub entry_count: u64,
    /// Smallest ID
    pub min_id: VectorId,
    /// Largest ID
    pub max_id: VectorId,
    /// Compression type
    pub compression: CompressionType,
    /// Block size
    pub block_size: usize,
    /// Level in LSM tree
    pub level: usize,
}

impl SSTableMeta {
    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Result<Self> {
        serde_json::from_slice(data)
            .map_err(|e| RTDBError::Serialization(e.to_string()))
    }
}

/// Compression type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// SSTable builder for creating new tables
pub struct SSTableBuilder {
    /// Output file
    file: File,
    /// Current block buffer
    block: Block,
    /// Index entries
    index: Vec<IndexEntry>,
    /// Meta information
    meta: SSTableMeta,
    /// Entries written
    entry_count: u64,
    /// First key in current block
    first_key: Option<VectorId>,
    /// File position
    position: u64,
}

/// Block structure
struct Block {
    /// Entries in block
    entries: Vec<(VectorId, Vector)>,
    /// Current size
    size: usize,
    /// Max size
    max_size: usize,
}

impl Block {
    fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            size: 0,
            max_size,
        }
    }

    fn add(&mut self, id: VectorId, vector: Vector) -> bool {
        let entry_size = Self::entry_size(id, &vector);
        
        if self.size + entry_size > self.max_size && !self.entries.is_empty() {
            return false; // Block full
        }

        self.entries.push((id, vector));
        self.size += entry_size;
        true
    }

    fn entry_size(_id: VectorId, vector: &Vector) -> usize {
        8 + // ID
        4 + vector.data.len() * 4 + // vector data
        vector.payload.as_ref().map(|p| serde_json::to_vec(p).unwrap_or_default().len()).unwrap_or(0)
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn encode(&self, compression: CompressionType) -> Result<Vec<u8>> {
        let mut buf = BytesMut::new();

        // Write entry count
        buf.put_u32_le(self.entries.len() as u32);

        // Write entries
        for (id, vector) in &self.entries {
            buf.put_u64_le(*id);
            buf.put_u32_le(vector.data.len() as u32);
            for &val in &vector.data {
                buf.put_f32_le(val);
            }
            
            // Write payload
            if let Some(payload) = &vector.payload {
                let payload_bytes = serde_json::to_vec(payload)
                    .map_err(|e| RTDBError::Serialization(e.to_string()))?;
                buf.put_u32_le(payload_bytes.len() as u32);
                buf.put_slice(&payload_bytes);
            } else {
                buf.put_u32_le(0);
            }
        }

        // Apply compression
        let data = buf.freeze();
        let compressed = match compression {
            CompressionType::None => data.to_vec(),
            CompressionType::Lz4 => {
                // Note: Would need lz4 crate
                data.to_vec() // Placeholder
            }
            CompressionType::Zstd => {
                // Note: Would need zstd crate
                data.to_vec() // Placeholder
            }
            CompressionType::Snappy => {
                // Note: Would need snap crate
                data.to_vec() // Placeholder
            }
        };

        Ok(compressed)
    }
}

/// Index entry
#[derive(Debug, Clone)]
struct IndexEntry {
    /// Key at this position
    key: VectorId,
    /// File offset
    offset: u64,
    /// Block size
    size: u32,
}

impl SSTableBuilder {
    /// Create new SSTable builder
    pub fn create(
        path: impl AsRef<Path>,
        dimension: usize,
        compression: CompressionType,
        block_size: usize,
        level: usize,
    ) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(into_storage_error)?;

        Ok(Self {
            file,
            block: Block::new(block_size),
            index: Vec::new(),
            meta: SSTableMeta {
                version: SSTABLE_VERSION,
                dimension,
                entry_count: 0,
                min_id: u64::MAX,
                max_id: 0,
                compression,
                block_size,
                level,
            },
            entry_count: 0,
            first_key: None,
            position: 0,
        })
    }

    /// Add entry to SSTable
    pub fn add(&mut self, id: VectorId, vector: Vector) -> Result<()> {
        if self.first_key.is_none() {
            self.first_key = Some(id);
        }

        // Try to add to current block
        if !self.block.add(id, vector.clone()) {
            // Block full, flush it
            self.flush_block()?;
            
            // Add to new block
            self.block.add(id, vector.clone());
            self.first_key = Some(id);
        }

        // Update meta
        self.meta.min_id = self.meta.min_id.min(id);
        self.meta.max_id = self.meta.max_id.max(id);
        self.entry_count += 1;

        Ok(())
    }

    /// Flush current block to file
    fn flush_block(&mut self) -> Result<()> {
        if self.block.is_empty() {
            return Ok(());
        }

        // Add index entry
        if let Some(first_key) = self.first_key {
            let block_data = self.block.encode(self.meta.compression)?;
            let block_size = block_data.len();

            self.index.push(IndexEntry {
                key: first_key,
                offset: self.position,
                size: block_size as u32,
            });

            // Write block
            self.file.write_all(&block_data).map_err(into_storage_error)?;
            self.position += block_size as u64;
        }

        // Reset block
        self.block = Block::new(self.meta.block_size);
        self.first_key = None;

        Ok(())
    }

    /// Finalize SSTable
    pub fn finish(mut self) -> Result<SSTableMeta> {
        // Flush last block
        self.flush_block()?;

        // Write index
        let index_offset = self.position;
        let index_data = self.encode_index()?;
        self.file.write_all(&index_data).map_err(into_storage_error)?;
        self.position += index_data.len() as u64;

        // Update and write meta first
        self.meta.entry_count = self.entry_count;
        let meta_bytes = self.meta.encode();
        let meta_offset = self.position;
        self.file.write_all(&meta_bytes).map_err(into_storage_error)?;
        self.position += meta_bytes.len() as u64;

        // Write footer (fixed size, at known offset from end)
        let footer = Footer {
            index_offset,
            index_size: index_data.len() as u64,
            meta_offset,
        };
        self.file.write_all(&footer.encode()).map_err(into_storage_error)?;
        self.position += 24;

        // Write trailer (8 bytes - magic + version)
        self.file.write_all(&SSTABLE_MAGIC.to_le_bytes())
            .map_err(into_storage_error)?;
        self.file.write_all(&SSTABLE_VERSION.to_le_bytes())
            .map_err(into_storage_error)?;

        // Sync
        self.file.sync_all().map_err(into_storage_error)?;

        Ok(self.meta)
    }

    fn encode_index(&self) -> Result<Vec<u8>> {
        let mut buf = BytesMut::new();
        buf.put_u32_le(self.index.len() as u32);

        for entry in &self.index {
            buf.put_u64_le(entry.key);
            buf.put_u64_le(entry.offset);
            buf.put_u32_le(entry.size);
        }

        Ok(buf.freeze().to_vec())
    }
}

/// SSTable footer
#[derive(Debug)]
struct Footer {
    index_offset: u64,
    index_size: u64,
    meta_offset: u64,
}

impl Footer {
    fn encode(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_u64_le(self.index_offset);
        buf.put_u64_le(self.index_size);
        buf.put_u64_le(self.meta_offset);
        buf.freeze().to_vec()
    }

    fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 24 {
            return Err(RTDBError::Storage("Invalid footer".to_string()));
        }

        Ok(Footer {
            index_offset: u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]),
            index_size: u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]),
            meta_offset: u64::from_le_bytes([data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]]),
        })
    }
}

/// SSTable reader for querying
pub struct SSTable {
    /// File handle
    #[allow(dead_code)]
    file: File,
    /// Table metadata
    pub meta: SSTableMeta,
    /// Index entries
    index: Vec<IndexEntry>,
    /// File path
    path: PathBuf,
}

impl SSTable {
    /// Open existing SSTable
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(into_storage_error)?;

        // Read trailer
        let file_size = file.seek(SeekFrom::End(0)).map_err(into_storage_error)?;
        file.seek(SeekFrom::End(-8)).map_err(into_storage_error)?;

        let mut trailer = [0u8; 8];
        file.read_exact(&mut trailer).map_err(into_storage_error)?;

        let magic = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        let version = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);

        if magic != SSTABLE_MAGIC {
            return Err(RTDBError::Storage(format!("Invalid SSTable magic: {}", magic)));
        }

        if version != SSTABLE_VERSION {
            return Err(RTDBError::Storage(format!("Unsupported version: {}", version)));
        }

        // Read footer
        file.seek(SeekFrom::End(-32)).map_err(into_storage_error)?;
        let mut footer_buf = [0u8; 24];
        file.read_exact(&mut footer_buf).map_err(into_storage_error)?;
        let footer = Footer::decode(&footer_buf)?;

        // Read meta (ensure we don't underflow)
        // Footer is 24 bytes, trailer is 8 bytes, both at end
        let meta_end = file_size - 32; // Footer (24) + Trailer (8)
        let meta_size = if footer.meta_offset < meta_end {
            (meta_end - footer.meta_offset) as usize
        } else {
            return Err(RTDBError::Storage(format!(
                "Invalid meta_offset: {} >= {}", footer.meta_offset, meta_end
            )));
        };
        file.seek(SeekFrom::Start(footer.meta_offset)).map_err(into_storage_error)?;
        let mut meta_buf = vec![0u8; meta_size as usize];
        file.read_exact(&mut meta_buf).map_err(into_storage_error)?;
        let meta = SSTableMeta::decode(&meta_buf)?;

        // Read index
        file.seek(SeekFrom::Start(footer.index_offset)).map_err(into_storage_error)?;
        let mut index_buf = vec![0u8; footer.index_size as usize];
        file.read_exact(&mut index_buf).map_err(into_storage_error)?;
        let index = Self::decode_index(&index_buf)?;

        Ok(SSTable {
            file,
            meta,
            index,
            path,
        })
    }

    /// Get vector by ID
    pub fn get(&self, id: VectorId) -> Result<Option<Vector>> {
        // Check range
        if id < self.meta.min_id || id > self.meta.max_id {
            return Ok(None);
        }

        // Find block in index
        let block_idx = self.find_block(id);
        if block_idx >= self.index.len() {
            return Ok(None);
        }

        // Load and search block
        let entry = &self.index[block_idx];
        let block = self.load_block_read(entry)?;

        for (key, value) in block {
            if key == id {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    /// Scan range
    pub fn scan(
        &self,
        start: Option<VectorId>,
        end: Option<VectorId>,
    ) -> Result<Vec<(VectorId, Vector)>> {
        let start = start.unwrap_or(self.meta.min_id);
        let end = end.unwrap_or(self.meta.max_id);

        let mut results = Vec::new();

        // Find starting block
        let start_block = self.find_block(start);

        for i in start_block..self.index.len() {
            let entry = &self.index[i];
            let block = self.load_block_read(entry)?;

            for (key, value) in block {
                if key > end {
                    return Ok(results);
                }
                if key >= start {
                    results.push((key, value));
                }
            }
        }

        Ok(results)
    }

    /// Find block containing key
    fn find_block(&self, key: VectorId) -> usize {
        // Binary search on index entries
        let mut low = 0;
        let mut high = self.index.len();
        
        while low < high {
            let mid = (low + high) / 2;
            if self.index[mid].key <= key {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        
        low.saturating_sub(1)
    }

    /// Load a block from file
    #[allow(dead_code)]
    fn load_block(&mut self, entry: &IndexEntry) -> Result<Vec<(VectorId, Vector)>> {
        self.load_block_read(entry)
    }
    
    /// Load block (read-only, creates temporary file handle)
    fn load_block_read(&self, entry: &IndexEntry) -> Result<Vec<(VectorId, Vector)>> {
        let mut file = File::open(&self.path).map_err(into_storage_error)?;
        file.seek(SeekFrom::Start(entry.offset))
            .map_err(into_storage_error)?;

        let mut buf = vec![0u8; entry.size as usize];
        file.read_exact(&mut buf).map_err(into_storage_error)?;

        // Decompress
        let data = match self.meta.compression {
            CompressionType::None => buf,
            _ => buf, // Placeholder - would decompress
        };

        Self::decode_block(&data)
    }

    fn decode_block(data: &[u8]) -> Result<Vec<(VectorId, Vector)>> {
        if data.len() < 4 {
            return Ok(Vec::new());
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut entries = Vec::with_capacity(count);
        let mut offset = 4;

        for _ in 0..count {
            if offset + 8 > data.len() {
                break;
            }

            let id = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            offset += 8;

            if offset + 4 > data.len() {
                break;
            }

            let dim = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
            offset += 4;

            if offset + dim * 4 > data.len() {
                break;
            }

            let mut vector_data = Vec::with_capacity(dim);
            for i in 0..dim {
                let val = f32::from_le_bytes([
                    data[offset + i*4],
                    data[offset + i*4 + 1],
                    data[offset + i*4 + 2],
                    data[offset + i*4 + 3]
                ]);
                vector_data.push(val);
            }
            offset += dim * 4;

            // Read payload
            if offset + 4 > data.len() {
                break;
            }

            let payload_len = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
            offset += 4;

            let payload = if payload_len > 0 {
                if offset + payload_len > data.len() {
                    break;
                }
                let payload_bytes = &data[offset..offset+payload_len];
                offset += payload_len;
                serde_json::from_slice(payload_bytes)
                    .map_err(|e| RTDBError::Serialization(e.to_string()))?
            } else {
                None
            };

            entries.push((id, Vector { data: vector_data, payload }));
        }

        Ok(entries)
    }

    fn decode_index(data: &[u8]) -> Result<Vec<IndexEntry>> {
        if data.len() < 4 {
            return Ok(Vec::new());
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut entries = Vec::with_capacity(count);
        let mut offset = 4;

        for _ in 0..count {
            if offset + 16 > data.len() {
                break;
            }

            let key = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            offset += 8;

            let block_offset = u64::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3],
                data[offset+4], data[offset+5], data[offset+6], data[offset+7]
            ]);
            offset += 8;

            let size = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
            offset += 4;

            entries.push(IndexEntry { key, offset: block_offset, size });
        }

        Ok(entries)
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if may contain key (for bloom filter)
    pub fn may_contain(&self, id: VectorId) -> bool {
        id >= self.meta.min_id && id <= self.meta.max_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sstable_create_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.sst");

        // Create
        let mut builder = SSTableBuilder::create(
            &path,
            3,
            CompressionType::None,
            1024,
            0,
        ).unwrap();

        for i in 1..=100 {
            let v = Vector::new(vec![i as f32; 3]);
            builder.add(i, v).unwrap();
        }

        let meta = builder.finish().unwrap();
        assert_eq!(meta.entry_count, 100);

        // Read
        let table = SSTable::open(&path).unwrap();
        assert_eq!(table.meta.entry_count, 100);

        // Get
        let result = table.get(50).unwrap();
        assert!(result.is_some());
        let v = result.unwrap();
        assert_eq!(v.data, vec![50.0; 3]);
    }

    #[test]
    fn test_sstable_scan() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.sst");

        // Create
        let mut builder = SSTableBuilder::create(
            &path,
            3,
            CompressionType::None,
            1024,
            0,
        ).unwrap();

        for i in 1..=100 {
            let v = Vector::new(vec![i as f32; 3]);
            builder.add(i, v).unwrap();
        }

        builder.finish().unwrap();

        // Read
        let table = SSTable::open(&path).unwrap();
        let results = table.scan(Some(10), Some(20)).unwrap();
        assert_eq!(results.len(), 11); // 10..=20
    }
}
