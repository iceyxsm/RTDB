//! Index layer for vector search
//! 
//! Implements:
//! - HNSW (Hierarchical Navigable Small World) graph
//! - Learned routing index
//! - Quantization (PQ, BQ, SQ)
//! - SIMD-optimized distance functions

pub mod hnsw;
pub mod learned;
pub mod quantization;
pub mod distance;

use crate::{Result, ScoredVector, SearchRequest, Vector, VectorId};

/// Index trait for vector search
pub trait VectorIndex: Send + Sync {
    /// Add vector to index
    fn add(&mut self, id: VectorId, vector: &Vector) -> Result<()>;
    
    /// Remove vector from index
    fn remove(&mut self, id: VectorId) -> Result<()>;
    
    /// Search for nearest neighbors
    fn search(&self, request: &SearchRequest) -> Result<Vec<ScoredVector>>;
    
    /// Get index size
    fn len(&self) -> usize;
    
    /// Check if empty
    fn is_empty(&self) -> bool;
    
    /// Build index (batch mode)
    fn build(&mut self, vectors: &[(VectorId, Vector)]) -> Result<()>;
}
