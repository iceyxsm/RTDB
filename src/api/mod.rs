//! API layer for RTDB
//! 
//! Provides compatibility with:
//! - Qdrant (REST + gRPC)
//! - Milvus (gRPC)
//! - Weaviate (GraphQL + REST)

pub mod rest;
#[cfg(grpc_enabled)]
pub mod grpc;

use crate::Result;

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// HTTP port
    pub http_port: u16,
    /// gRPC port  
    pub grpc_port: u16,
    /// Enable CORS
    pub enable_cors: bool,
    /// API key for authentication
    pub api_key: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            http_port: 6333,
            grpc_port: 6334,
            enable_cors: true,
            api_key: None,
        }
    }
}

/// Start API servers
pub async fn start(_config: ApiConfig) -> Result<()> {
    // TODO: Start REST and gRPC servers
    Ok(())
}
