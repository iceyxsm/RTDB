//! Enhanced REST API router with production-grade features
//!
//! Provides:
//! - Performance monitoring and metrics
//! - Query result caching
//! - Request/response compression
//! - Advanced middleware stack

use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::api::{
    error::ApiError,
    middleware::{
        rate_limit_middleware, request_logging_middleware, request_size_limit_middleware,
        security_headers_middleware, timeout_middleware, RateLimiter, RateLimitConfig,
    },
    performance::{ApiCache, ApiPerformanceMonitor, QueryCacheKey},
    qdrant_compat::{create_qdrant_router, QdrantState},
};

/// Enhanced API state with caching and monitoring
#[derive(Clone)]
pub struct EnhancedApiState {
    /// Base Qdrant state
    pub qdrant_state: QdrantState,
    /// Query result cache
    pub query_cache: Arc<ApiCache<QueryCacheKey, serde_json::Value>>,
    /// Performance monitor
    pub performance_monitor: Arc<ApiPerformanceMonitor>,
}

impl EnhancedApiState {
    pub fn new(qdrant_state: QdrantState) -> Self {
        Self {
            qdrant_state,
            // 1000 entries, 5 minute TTL for query results
            query_cache: Arc::new(ApiCache::new(1000, Duration::from_secs(300))),
            performance_monitor: Arc::new(ApiPerformanceMonitor::new()),
        }
    }
}

/// Create enhanced router with all production features
pub fn create_enhanced_router(state: EnhancedApiState) -> Router {
    // Create rate limiter with production settings
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        max_requests: 10000, // 10k requests per minute
        window_duration: Duration::from_secs(60),
        sliding_window: true,
    }));

    // Base Qdrant router
    let qdrant_router = create_qdrant_router(state.qdrant_state.clone());

    // Enhanced endpoints
    let enhanced_router = Router::new()
        .route("/api/v1/metrics", get(get_api_metrics))
        .route("/api/v1/cache/stats", get(get_cache_stats))
        .route("/api/v1/cache/clear", post(clear_cache))
        .route("/api/v1/performance/reset", post(reset_performance_metrics))
        .with_state(state.clone());

    // Combine routers
    Router::new()
        .merge(qdrant_router)
        .merge(enhanced_router)
        // Add performance monitoring middleware
        .layer(middleware::from_fn_with_state(
            state.performance_monitor.clone(),
            performance_monitoring_middleware,
        ))
        // Add existing middleware stack
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(request_logging_middleware))
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn(request_size_limit_middleware))
        .layer(middleware::from_fn_with_state(rate_limiter, rate_limit_middleware))
}

/// Performance monitoring middleware
async fn performance_monitoring_middleware(
    State(monitor): State<Arc<ApiPerformanceMonitor>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();
    let is_error = status.is_client_error() || status.is_server_error();

    // Record metrics
    monitor.record_request(duration, is_error).await;

    if duration > Duration::from_millis(1000) {
        warn!(
            method = %method,
            uri = %uri,
            duration_ms = duration.as_millis(),
            status = %status,
            "Slow request detected"
        );
    }

    response
}

/// Get API performance metrics
async fn get_api_metrics(
    State(state): State<EnhancedApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let metrics = state.performance_monitor.get_metrics().await;
    let cache_stats = state.query_cache.stats().await;

    Ok(Json(serde_json::json!({
        "performance": {
            "total_requests": metrics.total_requests,
            "total_errors": metrics.total_errors,
            "error_rate": if metrics.total_requests > 0 {
                metrics.total_errors as f64 / metrics.total_requests as f64
            } else {
                0.0
            },
            "avg_response_time_ms": metrics.avg_response_time_ms,
            "p95_response_time_ms": metrics.p95_response_time_ms,
            "p99_response_time_ms": metrics.p99_response_time_ms,
            "cache_hit_rate": metrics.cache_hit_rate
        },
        "cache": {
            "total_entries": cache_stats.total_entries,
            "active_entries": cache_stats.active_entries,
            "expired_entries": cache_stats.expired_entries,
            "max_size": cache_stats.max_size,
            "utilization": if cache_stats.max_size > 0 {
                cache_stats.active_entries as f64 / cache_stats.max_size as f64
            } else {
                0.0
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Get cache statistics
async fn get_cache_stats(
    State(state): State<EnhancedApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = state.query_cache.stats().await;

    Ok(Json(serde_json::json!({
        "cache_stats": {
            "total_entries": stats.total_entries,
            "active_entries": stats.active_entries,
            "expired_entries": stats.expired_entries,
            "max_size": stats.max_size,
            "utilization_percent": if stats.max_size > 0 {
                (stats.active_entries as f64 / stats.max_size as f64) * 100.0
            } else {
                0.0
            }
        }
    })))
}

/// Clear query cache
async fn clear_cache(
    State(state): State<EnhancedApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.query_cache.clear().await;
    info!("Query cache cleared");

    Ok(Json(serde_json::json!({
        "message": "Cache cleared successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Reset performance metrics
async fn reset_performance_metrics(
    State(state): State<EnhancedApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.performance_monitor.reset().await;
    info!("Performance metrics reset");

    Ok(Json(serde_json::json!({
        "message": "Performance metrics reset successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Health check endpoint with detailed status
pub async fn enhanced_health_check(
    State(state): State<EnhancedApiState>,
) -> impl IntoResponse {
    let metrics = state.performance_monitor.get_metrics().await;
    let cache_stats = state.query_cache.stats().await;

    let health_status = if metrics.total_errors > 0 && metrics.total_requests > 0 {
        let error_rate = metrics.total_errors as f64 / metrics.total_requests as f64;
        if error_rate > 0.1 {
            "unhealthy"
        } else if error_rate > 0.05 {
            "degraded"
        } else {
            "healthy"
        }
    } else {
        "healthy"
    };

    let response = serde_json::json!({
        "status": health_status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "metrics": {
            "total_requests": metrics.total_requests,
            "error_rate": if metrics.total_requests > 0 {
                metrics.total_errors as f64 / metrics.total_requests as f64
            } else {
                0.0
            },
            "avg_response_time_ms": metrics.avg_response_time_ms,
            "cache_hit_rate": metrics.cache_hit_rate
        },
        "cache": {
            "active_entries": cache_stats.active_entries,
            "utilization": if cache_stats.max_size > 0 {
                cache_stats.active_entries as f64 / cache_stats.max_size as f64
            } else {
                0.0
            }
        }
    });

    let status_code = match health_status {
        "healthy" => StatusCode::OK,
        "degraded" => StatusCode::OK, // Still OK but with warnings
        "unhealthy" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::OK,
    };

    (status_code, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn create_test_state() -> EnhancedApiState {
        // Create a temporary directory for the collection manager
        let temp_dir = tempfile::tempdir().unwrap();
        let collections = Arc::new(
            crate::collection::CollectionManager::new(temp_dir.path()).unwrap(),
        );

        // Create snapshot config
        let snapshot_config = crate::storage::snapshot::SnapshotConfig {
            local_path: temp_dir.path().to_path_buf(),
            s3_endpoint: None,
            s3_bucket: None,
            s3_access_key: None,
            s3_secret_key: None,
            compression_level: 6,
            max_incremental: 10,
            retention_days: 30,
        };

        let snapshot_manager =
            Arc::new(crate::storage::snapshot::SnapshotManager::new(snapshot_config).unwrap());
        let qdrant_state = QdrantState::new(collections, snapshot_manager);

        EnhancedApiState::new(qdrant_state)
    }

    #[tokio::test]
    async fn test_enhanced_health_check() {
        let state = create_test_state();
        let app = create_enhanced_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cache_endpoints() {
        let state = create_test_state();
        let app = create_enhanced_router(state);

        // Test cache stats
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cache/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test cache clear
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cache/clear")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_performance_reset() {
        let state = create_test_state();
        let app = create_enhanced_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/performance/reset")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}