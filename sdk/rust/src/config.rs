use std::time::Duration;
use crate::resilience::CircuitBreakerConfig;

/// Configuration for RTDB client
#[derive(Debug, Clone)]
pub struct RTDBConfig {
    pub endpoint: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub max_idle_connections: usize,
    pub idle_timeout: Duration,
    pub circuit_breaker_config: CircuitBreakerConfig,
    pub batch_size: usize,
}

impl RTDBConfig {
    /// Create a new configuration with the given endpoint
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_idle_connections: 10,
            idle_timeout: Duration::from_secs(90),
            circuit_breaker_config: CircuitBreakerConfig::default(),
            batch_size: 100,
        }
    }

    /// Set the request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the batch size for bulk operations
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }
}