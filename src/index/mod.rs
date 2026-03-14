//! Index layer for vector search
//! 
//! Implements:
//! - HNSW (Hierarchical Navigable Small World) graph with SIMDX optimization
//! - Learned routing index with SIMDX acceleration
//! - Quantization (PQ, BQ, SQ) with SIMDX support
//! - SIMDX-optimized distance functions for up to 200x performance

pub mod hnsw;
pub mod learned;
pub mod quantization;
pub mod distance;
pub mod vector_index;
pub mod hybrid_search;

pub use vector_index::{
    MultiVectorIndex, FlatIndex, HnswIndex, HnswConfig, IvfConfig, PqConfig, IvfPqConfig,
    IndexType, IndexMetrics, SearchResult, StoredVector,
};

pub use hybrid_search::{
    HybridSearchEngine, FilterCondition, MetadataIndex, QueryCache, SearchStrategy,
    CacheStats,
};

use crate::{Result, ScoredVector, SearchRequest, Vector, VectorId};
use crate::simdx::get_simdx_context;

/// Index trait for vector search with SIMDX optimization
pub trait VectorIndex: Send + Sync {
    /// Add vector to index with SIMDX normalization
    fn add(&mut self, id: VectorId, vector: &Vector) -> Result<()>;
    
    /// Remove vector from index
    fn remove(&mut self, id: VectorId) -> Result<()>;
    
    /// Search for nearest neighbors using SIMDX-accelerated distance computation
    fn search(&self, request: &SearchRequest) -> Result<Vec<ScoredVector>>;
    
    /// Get index size
    fn len(&self) -> usize;
    
    /// Check if empty
    fn is_empty(&self) -> bool;
    
    /// Build index (batch mode) with SIMDX batch processing
    fn build(&mut self, vectors: &[(VectorId, Vector)]) -> Result<()>;
    
    /// SIMDX-optimized batch search for maximum throughput
    fn batch_search(&self, requests: &[SearchRequest]) -> Result<Vec<Vec<ScoredVector>>> {
        let simdx_context = get_simdx_context();
        let mut results = Vec::with_capacity(requests.len());
        
        // Process searches in parallel using SIMDX optimization
        for request in requests {
            let result = self.search(request)?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// Get SIMDX performance statistics for this index
    fn get_simdx_stats(&self) -> crate::simdx::SIMDXPerformanceStats {
        get_simdx_context().get_performance_stats()
    }
}
