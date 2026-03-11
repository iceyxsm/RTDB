//! Protocol Buffer definitions for cluster RPC
//!
//! This module provides protobuf types and gRPC client/server implementations.
//! 
//! # Pre-generated Code
//! 
//! When the `grpc` feature is enabled, the protobuf code is loaded from
//! `src/cluster/generated/`. This means **you don't need protoc installed**
//! to build or use the gRPC features.
//! 
//! ## Regenerating Code
//! 
//! If you modify `src/cluster/rpc.proto`, regenerate the Rust code:
//! 
//! ```bash
//! # Install protoc first (one-time)
//! # macOS: brew install protobuf
//! # Ubuntu: sudo apt-get install protobuf-compiler
//! 
//! # Regenerate
//! cargo build --features regenerate-proto
//! 
//! # Check in the updated files
//! git add src/cluster/generated/
//! git commit -m "Update generated protobuf code"
//! ```

#[cfg(feature = "grpc")]
// Use pre-generated code (no protoc required)
pub use super::generated::*;

#[cfg(not(feature = "grpc"))]
/// Stub types when gRPC is disabled
pub mod cluster {
    //! Stub module when gRPC feature is not enabled
}

#[cfg(not(feature = "grpc"))]
/// Stub client when gRPC is disabled
pub mod cluster_service_client {
    //! Stub client when gRPC feature is not enabled
}

#[cfg(not(feature = "grpc"))]
/// Stub server when gRPC is disabled
pub mod cluster_service_server {
    //! Stub server when gRPC feature is not enabled
}
