// Pre-generated protobuf code for cluster RPC
//
// This module contains the generated Rust code from rpc.proto.
// It is checked into version control so users don't need protoc installed.
//
// To regenerate (when rpc.proto changes):
// 1. Install protoc: https://grpc.io/docs/protoc-installation/
// 2. Run: cargo build --features regenerate-proto
// 3. Check in the updated files

// Message types
pub mod cluster;

// Client implementation
pub mod cluster_service_client;

// Server implementation
pub mod cluster_service_server;

// Re-export all types
pub use cluster::*;
pub use cluster_service_client::cluster_service_client::ClusterServiceClient;
pub use cluster_service_server::cluster_service_server::{ClusterService, ClusterServiceServer};
