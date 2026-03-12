//! API layer for RTDB
//! 
//! Provides compatibility with:
//! - Qdrant (REST + gRPC)
//! - Milvus (gRPC)
//! - Weaviate (GraphQL + REST)

#![allow(missing_docs)]

pub mod rest;
pub mod qdrant_compat;
#[cfg(feature = "grpc")]
pub mod grpc;

use crate::{
    collection::CollectionManager,
    observability::{MetricsCollector, HealthChecker},
    storage::snapshot::SnapshotManager,
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
    use crate::observability::server::ObservabilityServer;
    
    // Start observability server in background on dedicated port
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
    
    // Start REST server with Qdrant-compatible API
    let _rest_handle = tokio::spawn({
        let collections = collections.clone();
        let port = config.http_port;
        async move {
            // Create snapshot manager
            let snapshot_config = crate::storage::snapshot::SnapshotConfig::default();
            let snapshot_manager = match SnapshotManager::new(snapshot_config) {
                Ok(manager) => Arc::new(manager),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to create snapshot manager");
                    return;
                }
            };
            
            let state = qdrant_compat::QdrantState::new(collections, snapshot_manager);
            let app = qdrant_compat::create_qdrant_router(state);
            
            let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to bind REST server");
                    return;
                }
            };
            
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "REST server error");
            }
        }
    });
    
    // Start gRPC server with production-grade configuration
    #[cfg(feature = "grpc")]
    let _grpc_handle = tokio::spawn({
        let collections = collections.clone();
        let port = config.grpc_port;
        async move {
            use tonic::transport::Server;
            use std::time::Duration;
            
            let addr = format!("0.0.0.0:{}", port).parse().unwrap();
            
            let collections_service = grpc::CollectionsService::new(collections.clone());
            let points_service = grpc::PointsService::new(collections.clone());
            
            let collections_server = grpc::CollectionsServer::new(collections_service);
            let points_server = grpc::PointsServer::new(points_service);
            
            tracing::info!("Starting gRPC server on {}", addr);
            
            if let Err(e) = Server::builder()
                .tcp_keepalive(Some(Duration::from_secs(60)))
                .tcp_nodelay(true)
                .concurrency_limit_per_connection(256)
                .timeout(Duration::from_secs(30))
                .http2_keepalive_interval(Some(Duration::from_secs(30)))
                .http2_keepalive_timeout(Some(Duration::from_secs(10)))
                .http2_adaptive_window(Some(true))
                .add_service(collections_server)
                .add_service(points_server)
                .serve(addr)
                .await
            {
                tracing::error!(error = %e, "gRPC server error");
            }
        }
    });
    
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
