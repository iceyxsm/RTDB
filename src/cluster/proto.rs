//! Protocol Buffer definitions for cluster RPC
//!
//! Generated code from rpc.proto

#[cfg(grpc)]
pub mod cluster {
    #![allow(missing_docs)]
    tonic::include_proto!("cluster");
}

#[cfg(grpc)]
pub use cluster::*;

// Stub types for when gRPC is disabled
#[cfg(not(grpc))]
pub mod cluster_service_client {
    //! Stub client when gRPC is disabled
}

#[cfg(not(grpc))]
pub mod cluster_service_server {
    //! Stub server when gRPC is disabled
}
