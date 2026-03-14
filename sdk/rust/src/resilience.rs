use crate::{RTDBError, RTDBResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker implementation
pub struct CircuitBreakerClient {
    config: CircuitBreakerConfig,
    state: Arc<std::sync::Mutex<CircuitBreakerState>>,
    failure_count: AtomicU64,
    last_failure_time: Arc<std::sync::Mutex<Option<Instant>>>,
    half_open_calls: AtomicU64,
}

impl CircuitBreakerClient {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(std::sync::Mutex::new(CircuitBreakerState::Closed)),
            failure_count: AtomicU64::new(0),
            last_failure_time: Arc::new(std::sync::Mutex::new(None)),
            half_open_calls: AtomicU64::new(0),
        }
    }

    pub async fn call<F, Fut, T>(&self, f: F) -> RTDBResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, reqwest::Error>>,
    {
        // Check circuit breaker state
        self.update_state();
        
        let current_state = {
            let state = self.state.lock().unwrap();
            state.clone()
        };

        match current_state {
            CircuitBreakerState::Open => {
                return Err(RTDBError::CircuitBreakerOpen);
            }
            CircuitBreakerState::HalfOpen => {
                let calls = self.half_open_calls.fetch_add(1, Ordering::SeqCst);
                if calls >= self.config.half_open_max_calls {
                    return Err(RTDBError::CircuitBreakerOpen);
                }
            }
            CircuitBreakerState::Closed => {}
        }

        // Execute the function
        match f().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(e) => {
                self.on_failure();
                Err(RTDBError::NetworkError(e))
            }
        }
    }

    fn update_state(&self) {
        let mut state = self.state.lock().unwrap();
        
        match *state {
            CircuitBreakerState::Open => {
                if let Some(last_failure) = *self.last_failure_time.lock().unwrap() {
                    if last_failure.elapsed() >= self.config.recovery_timeout {
                        *state = CircuitBreakerState::HalfOpen;
                        self.half_open_calls.store(0, Ordering::SeqCst);
                    }
                }
            }
            _ => {}
        }
    }

    fn on_success(&self) {
        let mut state = self.state.lock().unwrap();
        
        match *state {
            CircuitBreakerState::HalfOpen => {
                *state = CircuitBreakerState::Closed;
                self.failure_count.store(0, Ordering::SeqCst);
                self.half_open_calls.store(0, Ordering::SeqCst);
            }
            CircuitBreakerState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        if failures >= self.config.failure_threshold {
            let mut state = self.state.lock().unwrap();
            *state = CircuitBreakerState::Open;
            
            let mut last_failure = self.last_failure_time.lock().unwrap();
            *last_failure = Some(Instant::now());
        }
    }
}