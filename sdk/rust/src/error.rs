use thiserror::Error;

/// Result type for RTDB operations
pub type RTDBResult<T> = Result<T, RTDBError>;

/// RTDB client errors
#[derive(Error, Debug)]
pub enum RTDBError {
    #[error("Client initialization failed: {0}")]
    ClientInitialization(String),

    #[error("Server error {0}: {1}")]
    ServerError(u16, String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,

    #[error("Timeout")]
    Timeout,

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}