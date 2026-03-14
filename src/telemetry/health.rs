//! Health check endpoints

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Health status information for system monitoring and diagnostics.
/// 
/// Provides comprehensive health information including operational status,
/// version details, uptime, and system component health indicators.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    /// Current system status (healthy, degraded, unhealthy)
    status: String,
    /// Application version information
    version: String,
    uptime_seconds: u64,
    collections_count: usize,
    vectors_count: u64,
}

/// Health checker for monitoring system health and availability.
/// 
/// Tracks system health status, uptime, and provides health check endpoints
/// for load balancers and monitoring systems.
pub struct HealthChecker {
    /// Current health status flag
    healthy: AtomicBool,
    /// System start time for uptime calculation
    start_time: std::time::Instant,
}

impl HealthChecker {
    /// Create new health checker
    pub fn new() -> Self {
        Self {
            healthy: AtomicBool::new(true),
            start_time: std::time::Instant::now(),
        }
    }

    /// Mark as healthy
    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }

    /// Check if healthy
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    /// Get health status
    pub fn status(&self, collections: usize, vectors: u64) -> HealthStatus {
        HealthStatus {
            status: if self.is_healthy() {
                "healthy".to_string()
            } else {
                "unhealthy".to_string()
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            collections_count: collections,
            vectors_count: vectors,
        }
    }

    /// Get uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Liveness check - is the process running
pub fn liveness_check() -> bool {
    true
}

/// Readiness check - is the service ready to accept traffic
pub fn readiness_check(healthy: bool) -> bool {
    healthy
}
