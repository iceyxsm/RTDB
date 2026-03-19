//! RTDB - Production-Grade Smart Vector Database
//! 
//! A high-performance vector database with zero-AI intelligence,
//! drop-in compatibility with Qdrant/Milvus/Weaviate, and
//! production-grade reliability.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod storage;
pub mod index;
pub mod collection;
pub mod api;
pub mod smart;
pub mod query;
pub mod cluster;
pub mod auth;
pub mod telemetry;
pub mod cli;
pub mod config;
pub mod observability;
pub mod distance;
pub mod quantization;
pub mod filter;
pub mod migration;

pub mod simdx;
pub mod gpu;
pub mod replication;
pub mod wasm;
pub mod multimodal;
pub mod client;
pub mod cross_region;
pub mod streaming;
// pub mod k8s;
// pub mod sdk;
// pub mod testing;

// Re-export key types for easier access
pub use simdx::{SIMDXEngine, SIMDXError};
pub use quantization::advanced::{AdvancedQuantizer, QuantizationConfig as AdvancedQuantizationConfig, QuantizationMethod};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Convert error to storage error
pub fn into_storage_error<E: std::fmt::Display>(e: E) -> RTDBError {
    RTDBError::Storage(e.to_string())
}

/// Vector ID type
pub type VectorId = u64;

/// Collection name type
pub type CollectionName = String;

/// Score type for similarity
pub type Score = f32;

/// Core error types for RTDB
#[derive(Error, Debug, Clone)]
pub enum RTDBError {
    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(String),
    
    /// Index-related errors
    #[error("Index error: {0}")]
    Index(String),
    
    /// Query-related errors
    #[error("Query error: {0}")]
    Query(String),
    
    /// Collection not found
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),
    
    /// Vector not found
    #[error("Vector not found: {0}")]
    VectorNotFound(VectorId),
    
    /// Invalid vector dimension
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { 
        /// Expected dimension
        expected: usize, 
        /// Actual dimension received
        actual: usize 
    },
    
    /// IO errors
    #[error("IO error: {0}")]
    Io(String),
    
    /// Consensus errors
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Authentication errors
    #[error("Authentication error: {0}")]
    Auth(String),
    
    /// Authorization errors
    #[error("Authorization error: {0}")]
    Authorization(String),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(String),
    
    /// Network errors
    #[error("Network error: {0}")]
    Network(String),
    
    /// Migration errors
    #[error("Migration error: {0}")]
    Migration(String),
    
    /// Computation errors
    #[error("Computation error: {0}")]
    ComputationError(String),
    
    /// Invalid input errors
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    /// Internal errors
    #[error("Internal error: {0}")]
    Internal(String),
    
    /// Invalid configuration errors
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    
    /// Connection errors
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// API errors
    #[error("API error: {0}")]
    ApiError(String),
    
    /// HDF5 errors
    #[cfg(feature = "hdf5")]
    #[error("HDF5 error: {0}")]
    Hdf5(String),
}

impl From<std::io::Error> for RTDBError {
    fn from(err: std::io::Error) -> Self {
        RTDBError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for RTDBError {
    fn from(err: serde_json::Error) -> Self {
        RTDBError::Serialization(err.to_string())
    }
}

impl From<parquet::errors::ParquetError> for RTDBError {
    fn from(err: parquet::errors::ParquetError) -> Self {
        RTDBError::Migration(format!("Parquet error: {}", err))
    }
}

impl From<arrow_schema::ArrowError> for RTDBError {
    fn from(err: arrow_schema::ArrowError) -> Self {
        RTDBError::Migration(format!("Arrow error: {}", err))
    }
}

#[cfg(feature = "hdf5")]
impl From<hdf5::Error> for RTDBError {
    fn from(err: hdf5::Error) -> Self {
        RTDBError::Hdf5(err.to_string())
    }
}

#[cfg(feature = "hdf5")]
impl From<ndarray::ShapeError> for RTDBError {
    fn from(err: ndarray::ShapeError) -> Self {
        RTDBError::Migration(format!("Array shape error: {}", err))
    }
}

/// Result type for RTDB operations
pub type Result<T> = std::result::Result<T, RTDBError>;

/// A vector with optional payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Vector {
    /// Vector data
    pub data: Vec<f32>,
    /// Optional metadata payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

impl Vector {
    /// Create a new vector
    pub fn new(data: Vec<f32>) -> Self {
        Self { data, payload: None }
    }
    
    /// Create a new vector with payload
    pub fn with_payload(data: Vec<f32>, payload: Payload) -> Self {
        Self { 
            data, 
            payload: Some(payload) 
        }
    }
    
    /// Get vector dimension
    pub fn dim(&self) -> usize {
        self.data.len()
    }
    
    /// Calculate L2 norm
    pub fn l2_norm(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }
    
    /// Normalize vector to unit length using SIMDX optimization
    pub fn normalize(&mut self) {
        let norm = self.l2_norm();
        if norm > 0.0 {
            self.data.iter_mut().for_each(|x| *x /= norm);
        }
    }
}

/// Payload is a dynamic JSON-like structure for metadata
pub type Payload = serde_json::Map<String, serde_json::Value>;

/// A scored vector result from search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredVector {
    /// Vector ID
    pub id: VectorId,
    /// Similarity score (higher is better)
    pub score: Score,
    /// Vector data (optional, depending on query)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec<f32>>,
    /// Payload (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

/// Distance metric for vector comparison
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Distance {
    /// Euclidean distance (L2)
    Euclidean,
    /// Cosine similarity
    Cosine,
    /// Dot product
    Dot,
    /// Manhattan distance (L1)
    Manhattan,
}

impl Distance {
    /// Calculate distance between two vectors using SIMDX optimization
    pub fn calculate(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(RTDBError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }
        
        // Use SIMDX for optimal performance when available
        let simdx_engine = crate::simdx::SIMDXEngine::new(None);
        
        let score = match self {
            Distance::Euclidean => {
                // Calculate Euclidean distance: sqrt(sum((a[i] - b[i])^2))
                let mut sum = 0.0f32;
                for i in 0..a.len() {
                    let diff = a[i] - b[i];
                    sum += diff * diff;
                }
                sum.sqrt()
            }
            Distance::Cosine => {
                simdx_engine.cosine_distance(a, b)
                    .map_err(|e| RTDBError::ComputationError(e.to_string()))?
            }
            Distance::Dot => {
                // Calculate dot product
                a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
            }
            Distance::Manhattan => {
                // Manhattan distance not yet SIMDX optimized, use scalar
                a.iter().zip(b.iter())
                    .map(|(x, y)| (x - y).abs())
                    .sum()
            }
        };
        
        Ok(score)
    }
}

/// Collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    /// Vector dimension
    pub dimension: usize,
    /// Distance metric
    pub distance: Distance,
    /// HNSW configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hnsw_config: Option<HnswConfig>,
    /// Quantization configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization_config: Option<QuantizationConfig>,
    /// Optimizer configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_config: Option<OptimizerConfig>,
}

impl CollectionConfig {
    /// Create default config for dimension
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            distance: Distance::Cosine,
            hnsw_config: Some(HnswConfig::default()),
            quantization_config: None,
            optimizer_config: Some(OptimizerConfig::default()),
        }
    }
}

/// HNSW index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Number of edges per node (M parameter)
    pub m: usize,
    /// Size of dynamic candidate list (ef_construct)
    pub ef_construct: usize,
    /// Search time candidate list size (ef)
    pub ef: usize,
    /// Number of layers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_layers: Option<usize>,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construct: 100,
            ef: 10,
            num_layers: None,
        }
    }
}

/// Quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantizationConfig {
    /// Scalar quantization
    Scalar(ScalarQuantization),
    /// Product quantization
    Product(ProductQuantization),
    /// Binary quantization
    Binary(BinaryQuantization),
}

/// Scalar quantization config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarQuantization {
    /// Quantization type
    pub quantile: Option<f32>,
    /// Always use RAM
    #[serde(default)]
    pub always_ram: bool,
}

/// Product quantization config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductQuantization {
    /// Compression ratio
    pub compression: PQCompressionRatio,
    /// Always use RAM
    #[serde(default)]
    pub always_ram: bool,
}

/// PQ compression ratio
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PQCompressionRatio {
    /// 4x compression
    X4,
    /// 8x compression
    X8,
    /// 16x compression
    X16,
    /// 32x compression
    X32,
    /// 64x compression
    X64,
}

/// Binary quantization config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryQuantization {
    /// Always use RAM
    #[serde(default)]
    pub always_ram: bool,
}

/// Optimizer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    /// Minimum segment size in bytes
    pub indexing_threshold: usize,
    /// Number of vectors between flushes
    pub memmap_threshold: Option<usize>,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            indexing_threshold: 20_000,
            memmap_threshold: None,
        }
    }
}

/// Search request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// Query vector
    pub vector: Vec<f32>,
    /// Number of results to return
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
    /// Minimum score threshold
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f32>,
    /// Payload selector
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_payload: Option<WithPayload>,
    /// Include vector in results
    #[serde(default)]
    pub with_vector: bool,
    /// Filter conditions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
    /// Search params (ef, etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<SearchParams>,
}

impl SearchRequest {
    /// Create new search request
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: false,
            filter: None,
            params: None,
        }
    }
}

/// Payload inclusion selector
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WithPayload {
    /// Boolean flag
    Bool(bool),
    /// Include specific fields
    Include(Vec<String>),
    /// Exclude specific fields
    Exclude(Vec<String>),
}

/// Search parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    /// HNSW ef parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hnsw_ef: Option<usize>,
    /// Exact search flag
    #[serde(default)]
    pub exact: bool,
    /// Quantization search params
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<QuantizationSearchParams>,
}

/// Quantization search params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationSearchParams {
    /// Ignore quantized vectors
    #[serde(default)]
    pub ignore: bool,
    /// Rescore with original vectors
    #[serde(default = "default_rescore")]
    pub rescore: bool,
    /// Oversampling factor
    #[serde(default = "default_oversampling")]
    pub oversampling: f64,
}

fn default_rescore() -> bool {
    true
}

fn default_oversampling() -> f64 {
    1.0
}

/// Filter conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Must match all conditions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub must: Option<Vec<Condition>>,
    /// Should match any condition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should: Option<Vec<Condition>>,
    /// Must not match any condition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub must_not: Option<Vec<Condition>>,
}

/// Filter condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Field condition
    Field(FieldCondition),
    /// Nested filter
    Filter(Filter),
    /// Has ID condition
    HasId(HasIdCondition),
}

/// Field condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldCondition {
    /// Field key
    pub key: String,
    /// Match condition
    #[serde(flatten)]
    pub r#match: Match,
}

/// Match types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Match {
    /// Exact value match
    Value(MatchValue),
    /// Integer match
    Integer(MatchInteger),
    /// Text match
    Text(MatchText),
    /// Range match
    Range(Range),
}

/// Match by value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MatchValue {
    /// String value
    Keyword(String),
    /// Integer value
    Integer(i64),
}

/// Match by integer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInteger {
    /// Integer value
    pub integer: i64,
}

/// Match by text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchText {
    /// Text query
    pub text: String,
}

/// Range condition
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Range {
    /// Greater than
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<f64>,
    /// Greater than or equal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<f64>,
    /// Less than
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<f64>,
    /// Less than or equal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<f64>,
}

/// Has ID condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HasIdCondition {
    /// Vector IDs
    pub has_id: Vec<VectorId>,
}

/// Upsert request for vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertRequest {
    /// Vectors to upsert
    pub vectors: Vec<(VectorId, Vector)>,
}

/// Delete request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    /// IDs to delete
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<VectorId>>,
    /// Filter to delete by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_dim() {
        let v = Vector::new(vec![1.0, 2.0, 3.0]);
        assert_eq!(v.dim(), 3);
    }

    #[test]
    fn test_distance_euclidean() {
        let a = [0.0, 0.0];
        let b = [3.0, 4.0];
        let dist = Distance::Euclidean.calculate(&a, &b).unwrap();
        assert!((dist - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_distance_cosine() {
        let a = [1.0, 0.0];
        let b = [0.0, 1.0];
        let dist = Distance::Cosine.calculate(&a, &b).unwrap();
        // Distance::Cosine returns cosine distance (1.0 - similarity)
        // For orthogonal vectors, cosine similarity = 0, so cosine distance = 1.0
        assert!((dist - 1.0).abs() < 1e-6);
    }
}
