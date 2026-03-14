//! Production-grade Rust client for RTDB vector database
//! 
//! This client provides high-performance, production-ready access to RTDB
//! with built-in resilience, observability, and optimization features.
//! 
//! # Features
//! 
//! - **High Performance**: Optimized for low latency and high throughput
//! - **Resilience**: Circuit breaker, retry logic, connection pooling
//! - **Observability**: Comprehensive metrics and tracing
//! - **Type Safety**: Full Rust type safety with serde integration
//! - **Async/Await**: Native async support with tokio
//! 
//! # Quick Start
//! 
//! ```rust
//! use rtdb_client::{RTDBClient, RTDBConfig, Vector, SearchRequest};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = RTDBConfig::new("http://localhost:8080");
//!     let client = RTDBClient::new(config).await?;
//!     
//!     // Create a collection
//!     client.create_collection("my_vectors", 768).await?;
//!     
//!     // Insert vectors
//!     let vectors = vec![
//!         Vector::new("doc1", vec![0.1, 0.2, 0.3]),
//!         Vector::new("doc2", vec![0.4, 0.5, 0.6]),
//!     ];
//!     client.insert_vectors("my_vectors", vectors).await?;
//!     
//!     // Search
//!     let query = vec![0.1, 0.2, 0.3];
//!     let results = client.search("my_vectors", query, 10).await?;
//!     
//!     println!("Found {} results", results.len());
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod types;
pub mod resilience;
pub mod metrics;

pub use client::RTDBClient;
pub use config::RTDBConfig;
pub use error::{RTDBError, RTDBResult};
pub use types::{Vector, SearchRequest, SearchResponse, Collection, CollectionInfo};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{RTDBClient, RTDBConfig, RTDBError, RTDBResult};
    pub use crate::{Vector, SearchRequest, SearchResponse, Collection, CollectionInfo};
}