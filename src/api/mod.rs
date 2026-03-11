//! API layer for RTDB
//! 
//! Provides compatibility with:
//! - Qdrant (REST + gRPC)
//! - Milvus (gRPC)
//! - Weaviate (GraphQL + REST)

pub mod rest;
#[cfg(feature = "grpc")]
pub mod grpc;

use crate::{
    collection::CollectionManager,
    observability::{MetricsCollector, HealthChecker, server::ObservabilityServer},
    Result,
};
use std::sync::Arc;

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// HTTP port
    pub http_port: u16,
    /// gRPC port  
    pub grpc_port: u16,
    /// Metrics/Observability port
    pub metrics_bind: String,
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
            metrics_bind: "0.0.0.0:9090".to_string(),
            enable_cors: true,
            api_key: None,
        }
    }
}

/// Server handle for managing running servers
pub struct ServerHandle {
    pub rest_port: u16,
    pub grpc_port: u16,
    pub metrics_port: u16,
}

/// Start all API servers (REST, gRPC, and observability)
pub async fn start_all(
    config: ApiConfig,
    collections: Arc<CollectionManager>,
    metrics: Arc<MetricsCollector>,
    health: Arc<HealthChecker>,
) -> Result<ServerHandle> {
    // Start observability server in background
    let obs_server = ObservabilityServer::new(
        metrics.clone(),
        health.clone(),
        &config.metrics_bind,
    );
    let _obs_handle = obs_server.start_background()?;
    
    // Parse metrics port
    let metrics_port = config.metrics_bind
        .split(':')
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(9090);
    
    // Start REST server
    let rest_handle = tokio::spawn({
        let collections = collections.clone();
        let port = config.http_port;
        async move {
            if let Err(e) = rest::start_server(port, collections).await {
                tracing::error!(error = %e, "REST server error");
            }
        }
    });
    
    // TODO: Start gRPC server when implemented
    
    // Mark startup as complete
    health.startup_check().mark_ready();
    
    tracing::info!("All servers started successfully");
    
    Ok(ServerHandle {
        rest_port: config.http_port,
        grpc_port: config.grpc_port,
        metrics_port,
    })
}

/// Start API servers (legacy, for compatibility)
pub async fn start(_config: ApiConfig) -> Result<()> {
    // TODO: Start REST and gRPC servers
    Ok(())
}
