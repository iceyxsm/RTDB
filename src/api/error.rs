//! Production-grade error handling for REST API
//!
//! Based on industry best practices for error handling in Rust web services.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Standard API error response format for consistent error handling.
/// 
/// Provides a structured error response format that includes status,
/// error details, and timing information for API clients.
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    /// Response status (typically "error")
    pub status: String,
    /// Detailed error information
    pub error: ErrorDetails,
    /// Request processing time in seconds
    pub time: f64,
}

/// Detailed error information included in API error responses.
/// 
/// Contains error code, human-readable message, and optional additional
/// context for debugging and error handling.
#[derive(Debug, Serialize)]
pub struct ErrorDetails {
    /// Error code identifier
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional error context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<ValidationError>,
}

/// Validation error for specific field validation failures.
/// 
/// Represents a single field validation error with field name,
/// error message, and error code for client-side error handling.
#[derive(Debug, Serialize, Clone)]
pub struct ValidationError {
    /// Name of the field that failed validation
    pub field: String,
    /// Human-readable validation error message
    pub message: String,
    /// Validation error code identifier
    pub code: String,
}

/// Application error types for API operations and request handling.
/// 
/// Comprehensive error enumeration covering validation, authentication,
/// rate limiting, and internal server errors with appropriate HTTP status codes.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Collection not found: {name}")]
    CollectionNotFound { name: String },
    
    #[error("Collection already exists: {name}")]
    CollectionAlreadyExists { name: String },
    
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidVectorDimension { expected: usize, actual: usize },
    
    #[error("Point not found: {id}")]
    PointNotFound { id: String },
    
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },
    
    #[error("Validation failed")]
    ValidationFailed { errors: Vec<ValidationError> },
    
    #[error("Rate limit exceeded: {limit} requests per {window}")]
    RateLimitExceeded { limit: u32, window: String },
    
    #[error("Internal server error: {message}")]
    InternalError { message: String },
    
    #[error("Service unavailable: {reason}")]
    ServiceUnavailable { reason: String },
    
    #[error("Timeout: operation took longer than {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    
    #[error("Storage error: {message}")]
    StorageError { message: String },
    
    #[error("Index error: {message}")]
    IndexError { message: String },
}

impl ApiError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::CollectionNotFound { .. } => StatusCode::NOT_FOUND,
            ApiError::PointNotFound { .. } => StatusCode::NOT_FOUND,
            ApiError::CollectionAlreadyExists { .. } => StatusCode::CONFLICT,
            ApiError::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::InvalidVectorDimension { .. } => StatusCode::BAD_REQUEST,
            ApiError::ValidationFailed { .. } => StatusCode::BAD_REQUEST,
            ApiError::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Timeout { .. } => StatusCode::REQUEST_TIMEOUT,
            ApiError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::StorageError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::IndexError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    
    /// Get the error code for this error
    pub fn error_code(&self) -> &'static str {
        match self {
            ApiError::CollectionNotFound { .. } => "COLLECTION_NOT_FOUND",
            ApiError::PointNotFound { .. } => "POINT_NOT_FOUND",
            ApiError::CollectionAlreadyExists { .. } => "COLLECTION_ALREADY_EXISTS",
            ApiError::InvalidRequest { .. } => "INVALID_REQUEST",
            ApiError::InvalidVectorDimension { .. } => "INVALID_VECTOR_DIMENSION",
            ApiError::ValidationFailed { .. } => "VALIDATION_FAILED",
            ApiError::RateLimitExceeded { .. } => "RATE_LIMIT_EXCEEDED",
            ApiError::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            ApiError::Timeout { .. } => "TIMEOUT",
            ApiError::InternalError { .. } => "INTERNAL_ERROR",
            ApiError::StorageError { .. } => "STORAGE_ERROR",
            ApiError::IndexError { .. } => "INDEX_ERROR",
        }
    }
    
    /// Get additional details for this error
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            ApiError::InvalidVectorDimension { expected, actual } => {
                Some(serde_json::json!({
                    "expected_dimension": expected,
                    "actual_dimension": actual
                }))
            }
            ApiError::RateLimitExceeded { limit, window } => {
                Some(serde_json::json!({
                    "limit": limit,
                    "window": window,
                    "retry_after": "60s"
                }))
            }
            _ => None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let start = std::time::Instant::now();
        
        let validation_errors = match &self {
            ApiError::ValidationFailed { errors } => errors.clone(),
            _ => Vec::new(),
        };
        
        let error_response = ApiErrorResponse {
            status: "error".to_string(),
            error: ErrorDetails {
                code: self.error_code().to_string(),
                message: self.to_string(),
                details: self.details(),
                validation_errors,
            },
            time: start.elapsed().as_secs_f64(),
        };
        
        (self.status_code(), Json(error_response)).into_response()
    }
}

/// Convert from validation errors to API errors
impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        ApiError::ValidationFailed { errors: vec![err] }
    }
}
impl From<crate::RTDBError> for ApiError {
    fn from(err: crate::RTDBError) -> Self {
        match err {
            crate::RTDBError::CollectionNotFound(name) => {
                ApiError::CollectionNotFound { name }
            }
            crate::RTDBError::InvalidDimension { expected, actual } => {
                ApiError::InvalidVectorDimension { expected, actual }
            }
            crate::RTDBError::Storage(msg) => {
                ApiError::StorageError { message: msg }
            }
            crate::RTDBError::Index(msg) => {
                ApiError::IndexError { message: msg }
            }
            crate::RTDBError::Serialization(msg) => {
                ApiError::InternalError { message: format!("Serialization error: {}", msg) }
            }
            crate::RTDBError::Io(err) => {
                ApiError::InternalError { message: format!("IO error: {}", err) }
            }
            _ => ApiError::InternalError { 
                message: "Unknown error occurred".to_string() 
            }
        }
    }
}

/// Validation helper functions
pub fn validate_collection_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError {
            field: "name".to_string(),
            message: "Collection name cannot be empty".to_string(),
            code: "REQUIRED".to_string(),
        });
    }
    
    if name.len() > 255 {
        return Err(ValidationError {
            field: "name".to_string(),
            message: "Collection name cannot exceed 255 characters".to_string(),
            code: "MAX_LENGTH".to_string(),
        });
    }
    
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(ValidationError {
            field: "name".to_string(),
            message: "Collection name can only contain alphanumeric characters, underscores, and hyphens".to_string(),
            code: "INVALID_FORMAT".to_string(),
        });
    }
    
    Ok(())
}

pub fn validate_vector_dimension(dimension: usize) -> Result<(), ValidationError> {
    if dimension == 0 {
        return Err(ValidationError {
            field: "dimension".to_string(),
            message: "Vector dimension must be greater than 0".to_string(),
            code: "MIN_VALUE".to_string(),
        });
    }
    
    if dimension > 65536 {
        return Err(ValidationError {
            field: "dimension".to_string(),
            message: "Vector dimension cannot exceed 65536".to_string(),
            code: "MAX_VALUE".to_string(),
        });
    }
    
    Ok(())
}

pub fn validate_limit(limit: usize) -> Result<(), ValidationError> {
    if limit == 0 {
        return Err(ValidationError {
            field: "limit".to_string(),
            message: "Limit must be greater than 0".to_string(),
            code: "MIN_VALUE".to_string(),
        });
    }
    
    if limit > 10000 {
        return Err(ValidationError {
            field: "limit".to_string(),
            message: "Limit cannot exceed 10000".to_string(),
            code: "MAX_VALUE".to_string(),
        });
    }
    
    Ok(())
}

/// Result type alias for API operations
pub type ApiResult<T> = Result<T, ApiError>;