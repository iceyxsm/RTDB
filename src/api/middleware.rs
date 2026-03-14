//! Production-grade middleware for REST API
//!
//! Includes rate limiting, request validation, and security headers.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Configuration for API rate limiting middleware.
/// 
/// Defines rate limiting parameters including request limits, time windows,
/// and window type (sliding vs fixed) for protecting API endpoints.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests allowed per time window
    pub max_requests: u32,
    /// Duration of the rate limiting time window
    pub window_duration: Duration,
    /// Whether to use sliding window (true) or fixed window (false)
    pub sliding_window: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 1000,
            window_duration: Duration::from_secs(60),
            sliding_window: true,
        }
    }
}

/// Rate limiter implementation for tracking and enforcing API request limits.
/// 
/// Maintains per-client request counters and time windows to enforce
/// rate limiting policies and protect against API abuse.
#[derive(Debug)]
pub struct RateLimiter {
    /// Rate limiting configuration parameters
    config: RateLimitConfig,
    /// Client IP tracking: maps IP to (request_count, window_start)
    clients: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Check if request is allowed for the given client IP
    pub fn is_allowed(&self, client_ip: &str) -> bool {
        let mut clients = self.clients.lock().unwrap();
        let now = Instant::now();
        
        match clients.get_mut(client_ip) {
            Some((count, window_start)) => {
                // Check if window has expired
                if now.duration_since(*window_start) >= self.config.window_duration {
                    // Reset window
                    *count = 1;
                    *window_start = now;
                    true
                } else if *count >= self.config.max_requests {
                    // Rate limit exceeded
                    false
                } else {
                    // Increment count
                    *count += 1;
                    true
                }
            }
            None => {
                // New client
                clients.insert(client_ip.to_string(), (1, now));
                true
            }
        }
    }
    
    /// Get remaining requests for client
    pub fn remaining_requests(&self, client_ip: &str) -> u32 {
        let clients = self.clients.lock().unwrap();
        match clients.get(client_ip) {
            Some((count, window_start)) => {
                let now = Instant::now();
                if now.duration_since(*window_start) >= self.config.window_duration {
                    self.config.max_requests
                } else {
                    self.config.max_requests.saturating_sub(*count)
                }
            }
            None => self.config.max_requests,
        }
    }
    
    /// Clean up expired entries (should be called periodically)
    pub fn cleanup_expired(&self) {
        let mut clients = self.clients.lock().unwrap();
        let now = Instant::now();
        
        clients.retain(|_, (_, window_start)| {
            now.duration_since(*window_start) < self.config.window_duration * 2
        });
    }
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract client IP
    let client_ip = extract_client_ip(&request);
    
    // Check rate limit
    if !rate_limiter.is_allowed(&client_ip) {
        warn!(client_ip = %client_ip, "Rate limit exceeded");
        
        let remaining = rate_limiter.remaining_requests(&client_ip);
        let retry_after = rate_limiter.config.window_duration.as_secs();
        
        let error = crate::api::error::ApiError::RateLimitExceeded {
            limit: rate_limiter.config.max_requests,
            window: format!("{}s", rate_limiter.config.window_duration.as_secs()),
        };
        
        let mut response = error.into_response();
        
        // Add rate limit headers
        let headers = response.headers_mut();
        headers.insert("X-RateLimit-Limit", HeaderValue::from(rate_limiter.config.max_requests));
        headers.insert("X-RateLimit-Remaining", HeaderValue::from(remaining));
        headers.insert("X-RateLimit-Reset", HeaderValue::from(retry_after));
        headers.insert("Retry-After", HeaderValue::from(retry_after));
        
        return Ok(response);
    }
    
    // Process request
    let mut response = next.run(request).await;
    
    // Add rate limit headers to successful responses
    let remaining = rate_limiter.remaining_requests(&client_ip);
    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", HeaderValue::from(rate_limiter.config.max_requests));
    headers.insert("X-RateLimit-Remaining", HeaderValue::from(remaining));
    
    Ok(response)
}

/// Security headers middleware
pub async fn security_headers_middleware(
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    
    // Security headers
    headers.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert("X-XSS-Protection", HeaderValue::from_static("1; mode=block"));
    headers.insert("Referrer-Policy", HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers.insert("Content-Security-Policy", HeaderValue::from_static("default-src 'self'"));
    
    // CORS headers for API
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert("Access-Control-Allow-Methods", HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"));
    headers.insert("Access-Control-Allow-Headers", HeaderValue::from_static("Content-Type, Authorization, X-API-Key"));
    headers.insert("Access-Control-Max-Age", HeaderValue::from_static("86400"));
    
    response
}

/// Request logging middleware
pub async fn request_logging_middleware(
    request: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let client_ip = extract_client_ip(&request);
    
    debug!(
        method = %method,
        uri = %uri,
        client_ip = %client_ip,
        "Processing request"
    );
    
    let response = next.run(request).await;
    
    let duration = start.elapsed();
    let status = response.status();
    
    debug!(
        method = %method,
        uri = %uri,
        client_ip = %client_ip,
        status = %status,
        duration_ms = duration.as_millis(),
        "Request completed"
    );
    
    response
}

/// Request timeout middleware
pub async fn timeout_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let timeout_duration = Duration::from_secs(30); // 30 second timeout
    
    match tokio::time::timeout(timeout_duration, next.run(request)).await {
        Ok(response) => Ok(response),
        Err(_) => {
            warn!("Request timed out after {:?}", timeout_duration);
            
            let error = crate::api::error::ApiError::Timeout {
                timeout_ms: timeout_duration.as_millis() as u64,
            };
            
            Ok(error.into_response())
        }
    }
}

/// Extract client IP from request
fn extract_client_ip(request: &Request) -> String {
    // Check X-Forwarded-For header first (for load balancers/proxies)
    if let Some(forwarded) = request.headers().get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    
    // Check X-Real-IP header
    if let Some(real_ip) = request.headers().get("X-Real-IP") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }
    
    // Fallback to connection info (if available)
    // In production, this would come from the connection info
    "unknown".to_string()
}

/// Request size limit middleware
pub async fn request_size_limit_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    const MAX_REQUEST_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    
    // Check Content-Length header
    if let Some(content_length) = request.headers().get("content-length") {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<u64>() {
                if length > MAX_REQUEST_SIZE {
                    warn!(
                        content_length = length,
                        max_size = MAX_REQUEST_SIZE,
                        "Request size exceeds limit"
                    );
                    
                    let error = crate::api::error::ApiError::InvalidRequest {
                        message: format!(
                            "Request size {} exceeds maximum allowed size of {} bytes",
                            length, MAX_REQUEST_SIZE
                        ),
                    };
                    
                    return Ok(error.into_response());
                }
            }
        }
    }
    
    Ok(next.run(request).await)
}