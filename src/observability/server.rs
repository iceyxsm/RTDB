//! Observability HTTP Server
//!
//! Production-grade HTTP endpoints for metrics and health checks.
//! Follows Kubernetes and Prometheus best practices.
//!
//! Endpoints:
//! - GET /metrics - Prometheus metrics in text format
//! - GET /health - Overall health status
//! - GET /health/live - Liveness probe (Kubernetes)
//! - GET /health/ready - Readiness probe (Kubernetes)
//! - GET /health/startup - Startup probe (Kubernetes)

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use super::{
    health::{HealthChecker, HealthStatus},
    metrics::MetricsCollector,
};

/// Shared state for observability handlers
#[derive(Clone)]
pub struct ObservabilityState {
    pub metrics: Arc<MetricsCollector>,
    pub health: Arc<HealthChecker>,
    pub start_time: Instant,
}

impl ObservabilityState {
    pub fn new(
        metrics: Arc<MetricsCollector>,
        health: Arc<HealthChecker>,
    ) -> Self {
        Self {
            metrics,
            health,
            start_time: Instant::now(),
        }
    }
}

/// Create the observability router
pub fn observability_router(state: ObservabilityState) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .route("/health/live", get(liveness_handler))
        .route("/health/ready", get(readiness_handler))
        .route("/health/startup", get(startup_handler))
        .with_state(state)
}

/// Prometheus metrics endpoint handler
/// 
/// Returns metrics in Prometheus text format (version 0.0.4)
/// Content-Type: text/plain; version=0.0.4; charset=utf-8
async fn metrics_handler(State(state): State<ObservabilityState>) -> Response {
    debug!("Serving Prometheus metrics request");
    
    match state.metrics.export_prometheus() {
        Ok(metrics_text) => {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
                metrics_text,
            )
                .into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to encode metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain")],
                format!("Failed to encode metrics: {}", e),
            )
                .into_response()
        }
    }
}

/// Overall health check endpoint
///
/// Returns aggregated health status from all registered checks.
/// Use this for general health monitoring.
async fn health_handler(State(state): State<ObservabilityState>) -> Response {
    debug!("Serving overall health check");
    
    let overall = state.health.check_all().await;
    let uptime_secs = state.start_time.elapsed().as_secs();
    
    let body = json!({
        "status": overall.status.to_string(),
        "checks": overall.checks.iter().map(|(name, c)| {
            json!({
                "name": name,
                "status": c.status.to_string(),
                "message": c.message,
                "response_time_ms": c.duration.as_millis() as u64,
            })
        }).collect::<Vec<_>>(),
        "uptime_seconds": uptime_secs,
    });
    
    let status_code = match overall.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK, // Still serving traffic
        HealthStatus::Unhealthy | HealthStatus::Unknown => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    (status_code, Json(body)).into_response()
}

/// Liveness probe endpoint (Kubernetes)
///
/// Checks if the application process is running.
/// Should be lightweight - only check if the process can respond.
/// Kubernetes restarts the container if this fails.
async fn liveness_handler(State(state): State<ObservabilityState>) -> Response {
    debug!("Serving liveness probe");
    
    // Liveness should be simple - just check if we can respond
    // Don't check dependencies here (DB, storage, etc.)
    let uptime_secs = state.start_time.elapsed().as_secs();
    
    // Consider the app alive if it's been running for at least 1 second
    // and the health checker itself is functional
    let is_alive = uptime_secs >= 1;
    
    if is_alive {
        let body = json!({
            "status": "healthy",
            "message": "Application is running",
            "uptime_seconds": uptime_secs,
        });
        (StatusCode::OK, Json(body)).into_response()
    } else {
        warn!(uptime_secs, "Liveness probe failed - app starting up");
        let body = json!({
            "status": "unhealthy",
            "message": "Application is starting up",
            "uptime_seconds": uptime_secs,
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
    }
}

/// Readiness probe endpoint (Kubernetes)
///
/// Checks if the application is ready to serve traffic.
/// Should check dependencies (DB connections, storage, etc.).
/// Kubernetes stops sending traffic if this fails.
async fn readiness_handler(State(state): State<ObservabilityState>) -> Response {
    debug!("Serving readiness probe");
    
    // Readiness should check dependencies
    // For now, we check the overall health status
    let overall = state.health.check_all().await;
    let uptime_secs = state.start_time.elapsed().as_secs();
    
    let body = json!({
        "status": overall.status.to_string(),
        "message": match overall.status {
            HealthStatus::Healthy => "Application is ready to serve traffic",
            HealthStatus::Degraded => "Application is degraded but serving traffic",
            _ => "Application is not ready to serve traffic",
        },
        "uptime_seconds": uptime_secs,
        "checks": overall.checks.len(),
    });
    
    let status_code = match overall.status {
        HealthStatus::Healthy | HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy | HealthStatus::Unknown => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    (status_code, Json(body)).into_response()
}

/// Startup probe endpoint (Kubernetes)
///
/// Checks if the application has completed startup.
/// Used for slow-starting containers to give them time before liveness checks.
/// Should have higher failureThreshold and longer period.
async fn startup_handler(State(state): State<ObservabilityState>) -> Response {
    debug!("Serving startup probe");
    
    let uptime_secs = state.start_time.elapsed().as_secs();
    
    // For now, consider startup complete after 5 seconds
    // In production, this should check if all initialization is done
    // (DB migrations, index loading, etc.)
    const STARTUP_GRACE_PERIOD_SECS: u64 = 5;
    
    let is_started = uptime_secs >= STARTUP_GRACE_PERIOD_SECS;
    
    if is_started {
        let body = json!({
            "status": "healthy",
            "message": "Application startup complete",
            "uptime_seconds": uptime_secs,
        });
        (StatusCode::OK, Json(body)).into_response()
    } else {
        debug!(uptime_secs, "Startup probe - still initializing");
        let body = json!({
            "status": "unknown",
            "message": "Application is starting up",
            "uptime_seconds": uptime_secs,
            "grace_period_seconds": STARTUP_GRACE_PERIOD_SECS,
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
    }
}

/// Observability HTTP server
pub struct ObservabilityServer {
    state: ObservabilityState,
    bind_addr: String,
}

impl ObservabilityServer {
    /// Create a new observability server
    pub fn new(
        metrics: Arc<MetricsCollector>,
        health: Arc<HealthChecker>,
        bind_addr: impl Into<String>,
    ) -> Self {
        let state = ObservabilityState::new(metrics, health);
        Self {
            state,
            bind_addr: bind_addr.into(),
        }
    }
    
    /// Start the observability HTTP server
    pub async fn start(&self) -> crate::Result<()> {
        let app = observability_router(self.state.clone());
        
        // Parse bind address
        let addr: std::net::SocketAddr = self.bind_addr
            .parse()
            .map_err(|e| crate::RTDBError::Config(format!("Invalid metrics bind address: {}", e)))?;
        
        info!(address = %self.bind_addr, "Starting observability HTTP server");
        info!("  - Metrics: http://{}/metrics", addr);
        info!("  - Health:  http://{}/health", addr);
        info!("  - Liveness: http://{}/health/live", addr);
        info!("  - Readiness: http://{}/health/ready", addr);
        
        let listener = tokio::net::TcpListener::bind(addr).await
            .map_err(|e| crate::RTDBError::Io(format!("Failed to bind observability server: {}", e)))?;
        
        axum::serve(listener, app).await
            .map_err(|e| crate::RTDBError::Io(format!("Observability server error: {}", e)))?;
        
        Ok(())
    }
    
    /// Start the server in the background (non-blocking)
    pub fn start_background(&self) -> crate::Result<tokio::task::JoinHandle<()>> {
        let app = observability_router(self.state.clone());
        
        let addr: std::net::SocketAddr = self.bind_addr
            .parse()
            .map_err(|e| crate::RTDBError::Config(format!("Invalid metrics bind address: {}", e)))?;
        
        info!(address = %self.bind_addr, "Starting observability HTTP server in background");
        info!("  - Metrics: http://{}/metrics", addr);
        info!("  - Health:  http://{}/health", addr);
        info!("  - Liveness: http://{}/health/live", addr);
        info!("  - Readiness: http://{}/health/ready", addr);
        
        let handle = tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!(error = %e, "Failed to bind observability server");
                    return;
                }
            };
            
            if let Err(e) = axum::serve(listener, app).await {
                error!(error = %e, "Observability server error");
            }
        });
        
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    
    fn create_test_state() -> ObservabilityState {
        let metrics = Arc::new(MetricsCollector::new("test".to_string(), "0.1.0".to_string()));
        let health = Arc::new(HealthChecker::new());
        // Mark startup as ready for tests
        health.startup_check().mark_ready();
        ObservabilityState::new(metrics, health)
    }
    
    #[tokio::test]
    async fn test_metrics_endpoint() {
        let state = create_test_state();
        let app = observability_router(state);
        
        let response = app
            .oneshot(Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
        
        // Check content type
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/plain"));
    }
    
    #[tokio::test]
    async fn test_health_endpoint() {
        let state = create_test_state();
        let app = observability_router(state);
        
        let response = app
            .oneshot(Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        // Should return OK since startup is marked ready and no failing checks
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_liveness_endpoint() {
        let state = create_test_state();
        // Wait a bit for uptime >= 1 second
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        
        let app = observability_router(state);
        
        let response = app
            .oneshot(Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_readiness_endpoint() {
        let state = create_test_state();
        let app = observability_router(state);
        
        let response = app
            .oneshot(Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        // Should be OK since no failing checks are registered
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_startup_endpoint_initially_fails() {
        let state = create_test_state();
        let app = observability_router(state);
        
        // Immediately after creation, startup should fail
        let response = app
            .oneshot(Request::builder()
                .uri("/health/startup")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        // Should be 503 immediately after startup
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
    
    #[tokio::test]
    async fn test_startup_endpoint_eventually_succeeds() {
        let state = create_test_state();
        
        // Wait for startup grace period
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        
        let app = observability_router(state);
        
        let response = app
            .oneshot(Request::builder()
                .uri("/health/startup")
                .body(Body::empty())
                .unwrap())
            .await
            .unwrap();
        
        // Should be OK after grace period
        assert_eq!(response.status(), StatusCode::OK);
    }
}
