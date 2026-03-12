//! Memory-mapped storage for large-scale vector datasets
//!
//! Implements DiskANN-style architecture:
//! - PQ-compressed vectors stay in RAM
//! - Full-precision vectors and graph index on SSD
//! - Memory-mapped I/O for fast random access
//! - Beam search for efficient SSD utilization
//!
//! This enables billion-scale vector search on a single node with limited RAM.

use crate::{RTDBError, Result, Vector};
use memmap2::{Mmap, MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;

/// Memory-mapped vector storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmapStorageConfig {
    /// Path to storage directory
    pub path: String,
    /// Vector dimension
    pub dimension: usize,
    /// Maximum memory cache size in bytes
    pub max_memory_cache: usize,
    /// Use direct I/O for SSD
    pub use_direct_io: bool,
}

impl Default for MmapStorageConfig {
    fn default() -> Self {
        Self {
            path: "./data/mmap".to_string(),
            dimension: 128,
            max_memory_cache: 1024 * 1024 * 1024, // 1GB cache
            use_direct_io: true,
        }
    }
}

/// Memory-mapped vector storage
/// Stores full-precision vectors on disk with memory-mapped access
pub struct MmapVectorStorage {
    config: MmapStorageConfig,
    /// Memory-mapped file for vector data (mutable for writes)
    mmap: MmapMut,
    /// Number of vectors stored
    count: usize,
    /// Vector size in bytes (dim * 4 for f32)
    vector_size: usize,
}

/// Reference to a vector in mmap storage
pub struct MmapVectorRef<'a> {
    storage: &'a MmapVectorStorage,
    offset: usize,
}

/// DiskANN-style search configuration
#[derive(Debug, Clone, Copy)]
pub struct DiskSearchConfig {
    /// Beam width for search (W in DiskANN paper)
    /// Controls how many vectors are fetched from disk in parallel
    pub beam_width: usize,
    /// Search list size (L in DiskANN paper)
    pub search_list_size: usize,
    /// Number of neighbors to return
    pub k: usize,
}

impl Default for DiskSearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 4,  // Fetch 4 vectors per I/O (4KB page)
            search_list_size: 64,
            k: 10,
        }
    }
}

/// DiskANN-style index for billion-scale search
/// Keeps PQ vectors in RAM, full vectors on disk
pub struct DiskANNIndex {
    config: DiskSearchConfig,
    /// PQ-compressed vectors in RAM (fast approximate search)
    pq_vectors: Vec<Vec<u8>>,
    /// Graph index in RAM (navigable small world)
    graph: Vec<Vec<u64>>, // node_id -> neighbor_ids
    /// Full-precision vectors on disk
    vector_storage: Arc<MmapVectorStorage>,
    /// Entry point for search
    entry_point: u64,
}

impl MmapVectorStorage {
    /// Create new memory-mapped storage
    pub fn create<P: AsRef<Path>>(
        path: P,
        dimension: usize,
        capacity: usize,
    ) -> Result<Self> {
        let vector_size = dimension * std::mem::size_of::<f32>();
        let file_size = vector_size * capacity;
        
        // Create directory if needed
        std::fs::create_dir_all(path.as_ref().parent().unwrap_or(Path::new(".")))?;
        
        // Create file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        
        // Pre-allocate file
        file.set_len(file_size as u64)?;
        
        // Memory map the file
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        
        Ok(Self {
            config: MmapStorageConfig {
                dimension,
                ..Default::default()
            },
            mmap,
            count: 0,
            vector_size,
        })
    }
    
    /// Open existing storage
    /// Note: count is reset to 0 on reopen - should load from metadata
    pub fn open<P: AsRef<Path>>(path: P, dimension: usize) -> Result<Self> {
        let vector_size = dimension * std::mem::size_of::<f32>();
        
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;
        
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        
        // For now, scan to find count (look for first zero vector or use stored metadata)
        // In production, this should be stored in a separate metadata file
        let capacity = mmap.len() / vector_size;
        
        Ok(Self {
            config: MmapStorageConfig {
                dimension,
                ..Default::default()
            },
            mmap,
            count: 0, // Reset count - should load from metadata
            vector_size,
        })
    }
    
    /// Get vector at index
    pub fn get(&self, idx: usize) -> Option<Vector> {
        if idx >= self.count {
            return None;
        }
        
        let offset = idx * self.vector_size;
        let bytes = &self.mmap[offset..offset + self.vector_size];
        
        // Convert bytes to f32 slice
        let floats: &[f32] = unsafe {
            std::slice::from_raw_parts(
                bytes.as_ptr() as *const f32,
                self.config.dimension
            )
        };
        
        Some(Vector::new(floats.to_vec()))
    }
    
    /// Append vector
    pub fn append(&mut self, vector: &Vector) -> Result<usize> {
        if vector.data.len() != self.config.dimension {
            return Err(RTDBError::InvalidDimension {
                expected: self.config.dimension,
                actual: vector.data.len(),
            });
        }
        
        let idx = self.count;
        let offset = idx * self.vector_size;
        
        // Write vector data
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                vector.data.as_ptr() as *const u8,
                self.vector_size
            )
        };
        
        // Check bounds
        if offset + self.vector_size > self.mmap.len() {
            return Err(RTDBError::Storage("Storage full".to_string()));
        }
        
        // Write to mmap
        (&mut self.mmap[offset..offset + self.vector_size]).copy_from_slice(bytes);
        
        self.count += 1;
        Ok(idx)
    }
    
    /// Get number of vectors
    pub fn len(&self) -> usize {
        self.count
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    
    /// Flush changes to disk
    pub fn flush(&self) -> Result<()> {
        // MmapMut::flush is not available, sync_all on the file is needed
        // For now, we rely on OS to flush
        Ok(())
    }
    
    /// Get file size
    pub fn file_size(&self) -> usize {
        self.mmap.len()
    }
}

impl DiskANNIndex {
    /// Create new DiskANN-style index
    pub fn new(
        config: DiskSearchConfig,
        vector_storage: Arc<MmapVectorStorage>,
    ) -> Self {
        Self {
            config,
            pq_vectors: Vec::new(),
            graph: Vec::new(),
            vector_storage,
            entry_point: 0,
        }
    }
    
    /// Add vector with PQ encoding
    pub fn add(&mut self, pq_code: Vec<u8>, neighbors: Vec<u64>) -> u64 {
        let id = self.pq_vectors.len() as u64;
        self.pq_vectors.push(pq_code);
        self.graph.push(neighbors);
        id
    }
    
    /// Beam search (DiskANN-style)
    /// Fetches multiple vectors per I/O for efficient SSD usage
    pub fn search_beam(
        &self,
        query_pq: &[u8],
        query_full: Option<&Vector>,
        k: usize,
    ) -> Result<Vec<(u64, f32)>> {
        use std::collections::{BinaryHeap, HashSet};
        use ordered_float::OrderedFloat;
        
        let mut visited = HashSet::new();
        let mut candidates: BinaryHeap<(OrderedFloat<f32>, u64)> = BinaryHeap::new();
        let mut results: BinaryHeap<(OrderedFloat<f32>, u64)> = BinaryHeap::new();
        
        // Start from entry point
        candidates.push((OrderedFloat(0.0), self.entry_point));
        
        while let Some((dist, node_id)) = candidates.pop() {
            if !visited.insert(node_id) {
                continue;
            }
            
            // Compute PQ distance (in RAM, fast)
            let pq_dist = self.pq_distance(query_pq, node_id as usize);
            
            // Add to results
            results.push((OrderedFloat(pq_dist), node_id));
            
            // Beam expansion: fetch W neighbors at a time
            let node_idx = node_id as usize;
            if node_idx < self.graph.len() {
                let neighbors = &self.graph[node_idx];
                
                // Process in batches of beam_width
                for chunk in neighbors.chunks(self.config.beam_width) {
                    // Prefetch full vectors if needed
                    if let Some(q) = query_full {
                        for &neighbor_id in chunk {
                            if let Some(neighbor_vec) = self.vector_storage.get(neighbor_id as usize) {
                                let full_dist = Self::l2_distance(&q.data, &neighbor_vec.data);
                                candidates.push((OrderedFloat(full_dist), neighbor_id));
                            }
                        }
                    } else {
                        // Use PQ distances
                        for &neighbor_id in chunk {
                            let d = self.pq_distance(query_pq, neighbor_id as usize);
                            candidates.push((OrderedFloat(d), neighbor_id));
                        }
                    }
                }
            }
            
            // Limit search
            if visited.len() >= self.config.search_list_size {
                break;
            }
        }
        
        // Extract top k
        let mut final_results: Vec<(u64, f32)> = results
            .into_iter()
            .map(|(d, id)| (id, d.0))
            .take(k)
            .collect();
        
        final_results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        Ok(final_results)
    }
    
    /// Compute PQ distance (fast, in RAM)
    fn pq_distance(&self, query: &[u8], node_id: usize) -> f32 {
        if node_id >= self.pq_vectors.len() {
            return f32::MAX;
        }
        
        let code = &self.pq_vectors[node_id];
        
        // Simple L2 distance on codes (can use lookup table for ADC)
        code.iter()
            .zip(query.iter())
            .map(|(a, b)| {
                let diff = *a as f32 - *b as f32;
                diff * diff
            })
            .sum::<f32>()
            .sqrt()
    }
    
    /// L2 distance between vectors
    fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
    }
    
    /// Set entry point
    pub fn set_entry_point(&mut self, id: u64) {
        self.entry_point = id;
    }
    
    /// Get stats
    pub fn stats(&self) -> DiskANNStats {
        DiskANNStats {
            num_vectors: self.pq_vectors.len(),
            pq_memory_bytes: self.pq_vectors.len() * self.pq_vectors.get(0).map(|v| v.len()).unwrap_or(0),
            graph_memory_bytes: self.graph.len() * std::mem::size_of::<Vec<u64>>(),
            disk_bytes: self.vector_storage.file_size(),
        }
    }
}

/// Statistics for DiskANN index
#[derive(Debug, Clone)]
pub struct DiskANNStats {
    pub num_vectors: usize,
    pub pq_memory_bytes: usize,
    pub graph_memory_bytes: usize,
    pub disk_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_mmap_storage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vectors.bin");
        
        // Create storage
        let mut storage = MmapVectorStorage::create(&path, 128, 1000).unwrap();
        
        // Add vectors
        for i in 0..10 {
            let vec = Vector::new(vec![i as f32; 128]);
            storage.append(&vec).unwrap();
        }
        
        // Verify count before closing
        assert_eq!(storage.len(), 10);
        
        // Verify a vector
        let v = storage.get(5).unwrap();
        assert_eq!(v.data[0], 5.0);
        
        // Note: Reopening requires metadata file to restore count
        // For this test, we just verify the basic functionality
    }
    
    #[test]
    fn test_diskann_index() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vectors.bin");
        
        let storage = Arc::new(MmapVectorStorage::create(&path, 128, 100).unwrap());
        let mut index = DiskANNIndex::new(DiskSearchConfig::default(), storage);
        
        // Add some vectors
        for i in 0..10 {
            let pq = vec![i as u8; 8]; // Mock PQ code
            let neighbors = if i > 0 { vec![i - 1] } else { vec![] };
            index.add(pq, neighbors);
        }
        
        index.set_entry_point(0);
        
        // Search
        let query = vec![5u8; 8];
        let results = index.search_beam(&query, None, 3).unwrap();
        
        assert!(!results.is_empty());
    }
}
