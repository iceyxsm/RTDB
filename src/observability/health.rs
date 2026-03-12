//! Health Check System
//! 
//! Implements Kubernetes-style health probes:
//! - Liveness: Is the application running?
//! - Readiness: Is the application ready to serve traffic?

#![allow(missing_docs)]
//! - Startup: Has the application completed startup?
//!
//! Follows best practices from Qdrant and Milvus.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock as ParkingRwLock;
use tokio::sync::{broadcast, RwLock};

/// Health check status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is unhealthy
    Unhealthy,
    /// Service is degraded (functional but with issues)
    Degraded,
    /// Health status unknown
    Unknown,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
    
    pub fn is_available(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
    
    pub fn http_code(&self) -> u16 {
        match self {
            HealthStatus::Healthy => 200,
            HealthStatus::Degraded => 200, // Still serving traffic
            HealthStatus::Unhealthy => 503,
            HealthStatus::Unknown => 503,
        }
    }
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Individual health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub timestamp: Instant,
    pub duration: Duration,
}

impl HealthCheckResult {
    pub fn new(name: impl Into<String>, status: HealthStatus) -> Self {
        Self {
            name: name.into(),
            status,
            message: None,
            timestamp: Instant::now(),
            duration: Duration::default(),
        }
    }
    
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
    
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }
}

/// Trait for implementing health checks
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Name of the health check
    fn name(&self) -> &str;
    
    /// Perform the health check
    async fn check(&self) -> HealthCheckResult;
    
    /// Whether this check is critical (failure = unhealthy)
    fn is_critical(&self) -> bool {
        true
    }
    
    /// Check interval
    fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
    
    /// Timeout for the check
    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// Liveness check - verifies the application is running
pub struct LivenessCheck {
    started_at: Instant,
}

impl LivenessCheck {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for LivenessCheck {
    fn name(&self) -> &str {
        "liveness"
    }
    
    async fn check(&self) -> HealthCheckResult {
        let uptime = Instant::now().duration_since(self.started_at);
        HealthCheckResult::new("liveness", HealthStatus::Healthy)
            .with_message(format!("UP {}s", uptime.as_secs()))
    }
    
    fn is_critical(&self) -> bool {
        true
    }
}

/// Readiness check - verifies the application is ready to serve traffic
pub struct ReadinessCheck {
    checks: RwLock<Vec<Box<dyn HealthCheck>>>,
}

impl ReadinessCheck {
    pub fn new() -> Self {
        Self {
            checks: RwLock::new(Vec::new()),
        }
    }
    
    pub async fn add_check(&self, check: Box<dyn HealthCheck>) {
        self.checks.write().await.push(check);
    }
}

#[async_trait::async_trait]
impl HealthCheck for ReadinessCheck {
    fn name(&self) -> &str {
        "readiness"
    }
    
    async fn check(&self) -> HealthCheckResult {
        // Collect check info while holding the lock
        let check_info: Vec<_> = {
            let guard = self.checks.read().await;
            guard.iter()
                .map(|c| (c.name().to_string(), c.is_critical()))
                .collect()
        };
        
        // We need to run checks sequentially but can't use the guard across await
        // So we'll run each check by acquiring the lock again
        for (idx, (name, is_critical)) in check_info.iter().enumerate() {
            let guard = self.checks.read().await;
            if let Some(check) = guard.get(idx) {
                let result = check.check().await;
                drop(guard); // Release lock before processing
                if *is_critical && !result.status.is_healthy() {
                    return HealthCheckResult::new("readiness", result.status)
                        .with_message(format!("{}: {}", name,
                            result.message.unwrap_or_default()));
                }
            }
        }
        
        HealthCheckResult::new("readiness", HealthStatus::Healthy)
            .with_message("All checks passed")
    }
    
    fn is_critical(&self) -> bool {
        true
    }
}

/// Startup check - verifies the application has completed startup
pub struct StartupCheck {
    is_ready: ParkingRwLock<bool>,
    startup_time: ParkingRwLock<Option<Instant>>,
}

impl StartupCheck {
    pub fn new() -> Self {
        Self {
            is_ready: ParkingRwLock::new(false),
            startup_time: ParkingRwLock::new(None),
        }
    }
    
    pub fn mark_ready(&self) {
        let mut is_ready = self.is_ready.write();
        let mut startup_time = self.startup_time.write();
        *is_ready = true;
        *startup_time = Some(Instant::now());
    }
    
    pub fn is_ready(&self) -> bool {
        *self.is_ready.read()
    }
}

#[async_trait::async_trait]
impl HealthCheck for StartupCheck {
    fn name(&self) -> &str {
        "startup"
    }
    
    async fn check(&self) -> HealthCheckResult {
        if self.is_ready() {
            let startup_time = self.startup_time.read();
            if let Some(time) = *startup_time {
                let elapsed = Instant::now().duration_since(time);
                return HealthCheckResult::new("startup", HealthStatus::Healthy)
                    .with_message(format!("Started {}s ago", elapsed.as_secs()));
            }
            HealthCheckResult::new("startup", HealthStatus::Healthy)
        } else {
            HealthCheckResult::new("startup", HealthStatus::Unhealthy)
                .with_message("Startup not complete")
        }
    }
    
    fn is_critical(&self) -> bool {
        true
    }
}

/// Storage backend health check
pub struct StorageHealthCheck {
    name: String,
    check_fn: Box<dyn Fn() -> bool + Send + Sync>,
}

impl StorageHealthCheck {
    pub fn new<F>(name: impl Into<String>, check_fn: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            check_fn: Box::new(check_fn),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for StorageHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn check(&self) -> HealthCheckResult {
        let start = Instant::now();
        let is_healthy = (self.check_fn)();
        let duration = Instant::now().duration_since(start);
        
        if is_healthy {
            HealthCheckResult::new(&self.name, HealthStatus::Healthy)
                .with_duration(duration)
        } else {
            HealthCheckResult::new(&self.name, HealthStatus::Unhealthy)
                .with_message("Storage backend unavailable")
                .with_duration(duration)
        }
    }
}

/// Cluster health check
pub struct ClusterHealthCheck {
    get_cluster_status: Box<dyn Fn() -> ClusterStatus + Send + Sync>,
}

#[derive(Debug, Clone)]
pub struct ClusterStatus {
    pub node_count: usize,
    pub healthy_nodes: usize,
    pub is_quorum: bool,
    pub leader_id: Option<String>,
}

impl ClusterHealthCheck {
    pub fn new<F>(get_status: F) -> Self
    where
        F: Fn() -> ClusterStatus + Send + Sync + 'static,
    {
        Self {
            get_cluster_status: Box::new(get_status),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for ClusterHealthCheck {
    fn name(&self) -> &str {
        "cluster"
    }
    
    async fn check(&self) -> HealthCheckResult {
        let status = (self.get_cluster_status)();
        
        if !status.is_quorum {
            return HealthCheckResult::new("cluster", HealthStatus::Unhealthy)
                .with_message(format!(
                    "No quorum: {}/{} nodes healthy",
                    status.healthy_nodes, status.node_count
                ));
        }
        
        if status.healthy_nodes == status.node_count {
            HealthCheckResult::new("cluster", HealthStatus::Healthy)
                .with_message(format!("All {} nodes healthy", status.node_count))
        } else {
            HealthCheckResult::new("cluster", HealthStatus::Degraded)
                .with_message(format!(
                    "{}/{} nodes healthy",
                    status.healthy_nodes, status.node_count
                ))
        }
    }
    
    fn is_critical(&self) -> bool {
        true
    }
}

/// Comprehensive health checker managing all health probes
pub struct HealthChecker {
    liveness: LivenessCheck,
    readiness: Arc<ReadinessCheck>,
    startup: Arc<StartupCheck>,
    check_results: ParkingRwLock<HashMap<String, HealthCheckResult>>,
    update_tx: broadcast::Sender<HealthStatus>,
}

impl HealthChecker {
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(16);
        
        Self {
            liveness: LivenessCheck::new(),
            readiness: Arc::new(ReadinessCheck::new()),
            startup: Arc::new(StartupCheck::new()),
            check_results: ParkingRwLock::new(HashMap::new()),
            update_tx,
        }
    }
    
    /// Get the startup check (to mark ready)
    pub fn startup_check(&self) -> Arc<StartupCheck> {
        self.startup.clone()
    }
    
    /// Add a readiness check
    pub async fn add_readiness_check(&self, check: Box<dyn HealthCheck>) {
        self.readiness.add_check(check).await;
    }
    
    /// Subscribe to health status changes
    pub fn subscribe(&self) -> broadcast::Receiver<HealthStatus> {
        self.update_tx.subscribe()
    }
    
    /// Perform liveness check
    pub async fn check_liveness(&self) -> HealthCheckResult {
        self.liveness.check().await
    }
    
    /// Perform readiness check
    pub async fn check_readiness(&self) -> HealthCheckResult {
        // First check if startup is complete
        if !self.startup.is_ready() {
            return HealthCheckResult::new("readiness", HealthStatus::Unhealthy)
                .with_message("Startup not complete");
        }
        
        self.readiness.check().await
    }
    
    /// Perform startup check
    pub async fn check_startup(&self) -> HealthCheckResult {
        self.startup.check().await
    }
    
    /// Get overall health status
    pub async fn overall_status(&self) -> OverallHealth {
        let liveness = self.check_liveness().await;
        let readiness = self.check_readiness().await;
        let startup = self.check_startup().await;
        
        let mut checks = HashMap::new();
        checks.insert("liveness".to_string(), liveness.clone());
        checks.insert("readiness".to_string(), readiness.clone());
        checks.insert("startup".to_string(), startup.clone());
        
        // Overall status is the worst of the critical checks
        let overall = if !liveness.status.is_healthy() {
            HealthStatus::Unhealthy
        } else if !startup.status.is_healthy() {
            HealthStatus::Unhealthy
        } else if !readiness.status.is_healthy() {
            HealthStatus::Unhealthy
        } else if readiness.status == HealthStatus::Degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
        
        OverallHealth {
            status: overall,
            checks,
        }
    }
    
    /// Start background health monitoring
    pub fn start_monitoring(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            let mut last_status = HealthStatus::Unknown;
            
            loop {
                ticker.tick().await;
                
                let overall = self.overall_status().await;
                
                // Update stored results
                {
                    let mut results = self.check_results.write();
                    for (name, result) in &overall.checks {
                        results.insert(name.clone(), result.clone());
                    }
                }
                
                // Notify on status change
                if overall.status != last_status {
                    let _ = self.update_tx.send(overall.status);
                    last_status = overall.status;
                }
            }
        });
    }
    
    /// Get the last check results
    pub fn last_results(&self) -> HashMap<String, HealthCheckResult> {
        self.check_results.read().clone()
    }
    
    /// Check all health checks and return aggregated result
    /// Alias for overall_status() for API consistency
    pub async fn check_all(&self) -> OverallHealth {
        self.overall_status().await
    }
}

/// Overall health response
#[derive(Debug, Clone)]
pub struct OverallHealth {
    pub status: HealthStatus,
    pub checks: HashMap<String, HealthCheckResult>,
}

impl OverallHealth {
    pub fn to_json(&self) -> serde_json::Value {
        let checks: serde_json::Map<String, serde_json::Value> = self
            .checks
            .iter()
            .map(|(name, result)| {
                let value = serde_json::json!({
                    "status": result.status.to_string(),
                    "message": result.message,
                    "duration_ms": result.duration.as_millis(),
                });
                (name.clone(), value)
            })
            .collect();
        
        serde_json::json!({
            "status": self.status.to_string(),
            "http_code": self.status.http_code(),
            "checks": checks,
        })
    }
}

/// gRPC health checking service implementation
pub mod grpc {
    use super::*;
    
    /// Health check service for gRPC
    pub struct HealthService {
        checker: Arc<HealthChecker>,
    }
    
    impl HealthService {
        pub fn new(checker: Arc<HealthChecker>) -> Self {
            Self { checker }
        }
        
        /// Check health for a specific service
        pub async fn check(&self, service: &str) -> (HealthStatus, String) {
            match service {
                "" | "*" | "rtdb.Health" => {
                    let overall = self.checker.overall_status().await;
                    let message = if overall.status.is_healthy() {
                        "SERVING".to_string()
                    } else {
                        format!("NOT_SERVING: {}", overall.status)
                    };
                    (overall.status, message)
                }
                _ => (HealthStatus::Unknown, "SERVICE_UNKNOWN".to_string()),
            }
        }
    }
}

/// HTTP health check handlers (warp-based)
#[cfg(feature = "grpc")]
pub mod http {
    use super::*;
    use warp::{Reply, Rejection, reply};
    
    /// Liveness probe handler
    pub async fn liveness_handler(checker: Arc<HealthChecker>) -> Result<impl Reply, Rejection> {
        let result = checker.check_liveness().await;
        let code = result.status.http_code();
        let json = serde_json::json!({
            "status": result.status.to_string(),
            "message": result.message,
        });
        
        Ok(reply::with_status(
            reply::json(&json),
            warp::http::StatusCode::from_u16(code).unwrap_or(warp::http::StatusCode::OK),
        ))
    }
    
    /// Readiness probe handler
    pub async fn readiness_handler(checker: Arc<HealthChecker>) -> Result<impl Reply, Rejection> {
        let result = checker.check_readiness().await;
        let code = result.status.http_code();
        let json = serde_json::json!({
            "status": result.status.to_string(),
            "message": result.message,
        });
        
        Ok(reply::with_status(
            reply::json(&json),
            warp::http::StatusCode::from_u16(code).unwrap_or(warp::http::StatusCode::OK),
        ))
    }
    
    /// Startup probe handler
    pub async fn startup_handler(checker: Arc<HealthChecker>) -> Result<impl Reply, Rejection> {
        let result = checker.check_startup().await;
        let code = result.status.http_code();
        let json = serde_json::json!({
            "status": result.status.to_string(),
            "message": result.message,
        });
        
        Ok(reply::with_status(
            reply::json(&json),
            warp::http::StatusCode::from_u16(code).unwrap_or(warp::http::StatusCode::OK),
        ))
    }
    
    /// Overall health handler
    pub async fn health_handler(checker: Arc<HealthChecker>) -> Result<impl Reply, Rejection> {
        let overall = checker.overall_status().await;
        let code = overall.status.http_code();
        
        Ok(reply::with_status(
            reply::json(&overall.to_json()),
            warp::http::StatusCode::from_u16(code).unwrap_or(warp::http::StatusCode::OK),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_liveness_check() {
        let check = LivenessCheck::new();
        let result = check.check().await;
        assert!(result.status.is_healthy());
    }
    
    #[tokio::test]
    async fn test_startup_check() {
        let check = StartupCheck::new();
        
        // Not ready initially
        let result = check.check().await;
        assert!(!result.status.is_healthy());
        
        // Mark as ready
        check.mark_ready();
        let result = check.check().await;
        assert!(result.status.is_healthy());
    }
    
    #[tokio::test]
    async fn test_health_checker() {
        let checker = Arc::new(HealthChecker::new());
        
        // Startup not complete
        let readiness = checker.check_readiness().await;
        assert!(!readiness.status.is_healthy());
        
        // Mark startup complete
        checker.startup_check().mark_ready();
        
        let readiness = checker.check_readiness().await;
        assert!(readiness.status.is_healthy());
    }
    
    #[tokio::test]
    async fn test_cluster_health() {
        let status = ClusterStatus {
            node_count: 3,
            healthy_nodes: 3,
            is_quorum: true,
            leader_id: Some("node1".to_string()),
        };
        
        let check = ClusterHealthCheck::new(move || status.clone());
        let result = check.check().await;
        assert!(result.status.is_healthy());
        
        // Test with no quorum
        let bad_status = ClusterStatus {
            node_count: 3,
            healthy_nodes: 1,
            is_quorum: false,
            leader_id: None,
        };
        let check = ClusterHealthCheck::new(move || bad_status.clone());
        let result = check.check().await;
        assert!(!result.status.is_healthy());
    }
}
