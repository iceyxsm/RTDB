//! Authentication and Authorization Module
//!
//! Implements API Key authentication and Role-Based Access Control (RBAC).
//!
//! Features:
//! - API Key authentication via X-API-Key header or Authorization: Bearer
//! - Predefined roles: Admin, Writer, Reader
//! - Resource-level permissions
//! - Constant-time key comparison to prevent timing attacks
//! - Public endpoint exclusion (health, metrics)

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

pub mod middleware;
pub mod rbac;

pub use middleware::auth_middleware;
pub use rbac::{Role, Permission, AccessControl};

/// Authentication error
#[derive(Debug, Clone)]
pub struct AuthError {
    pub message: String,
    pub status: StatusCode,
}

impl AuthError {
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            status: StatusCode::UNAUTHORIZED,
        }
    }
    
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            status: StatusCode::FORBIDDEN,
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": self.message,
            "status": self.status.as_u16(),
        });
        (self.status, Json(body)).into_response()
    }
}

/// API Key with associated metadata
#[derive(Debug, Clone)]
pub struct ApiKey {
    /// The key value (hashed in production)
    pub key: String,
    /// Key name/identifier
    pub name: String,
    /// Role assigned to this key
    pub role: Role,
    /// Optional expiration
    pub expires_at: Option<Instant>,
    /// Rate limit: requests per minute
    pub rate_limit: Option<u32>,
    /// When the key was created
    pub created_at: Instant,
    /// Last used timestamp
    pub last_used: Option<Instant>,
    /// Whether the key is active
    pub active: bool,
}

impl ApiKey {
    /// Create a new API key
    pub fn new(key: impl Into<String>, name: impl Into<String>, role: Role) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            role,
            expires_at: None,
            rate_limit: None,
            created_at: Instant::now(),
            last_used: None,
            active: true,
        }
    }
    
    /// Set expiration
    pub fn with_expiration(mut self, duration: Duration) -> Self {
        self.expires_at = Some(Instant::now() + duration);
        self
    }
    
    /// Set rate limit (requests per minute)
    pub fn with_rate_limit(mut self, limit: u32) -> Self {
        self.rate_limit = Some(limit);
        self
    }
    
    /// Check if key is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Instant::now() > exp)
            .unwrap_or(false)
    }
    
    /// Mark key as used
    pub fn mark_used(&mut self) {
        self.last_used = Some(Instant::now());
    }
}

/// Authenticated user context
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// API key identifier
    pub key_name: String,
    /// Assigned role
    pub role: Role,
    /// Authentication timestamp
    pub authenticated_at: Instant,
}

impl AuthContext {
    /// Check if user has required permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.role.has_permission(permission)
    }
    
    /// Check if user can access collection
    pub fn can_access_collection(&self, collection: &str) -> bool {
        // Admin can access all
        if self.role == Role::Admin {
            return true;
        }
        // TODO: Add collection-level permissions
        true
    }
}

/// API Key storage and validation
pub struct ApiKeyStore {
    keys: HashMap<String, ApiKey>,
}

impl ApiKeyStore {
    /// Create a new key store
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }
    
    /// Create with default keys from environment
    pub fn from_env() -> Self {
        let mut store = Self::new();
        
        // Add admin key from environment if set
        if let Ok(admin_key) = std::env::var("RTDB_ADMIN_API_KEY") {
            store.add_key(ApiKey::new(admin_key, "admin", Role::Admin));
        }
        
        // Add read-only key from environment if set
        if let Ok(read_key) = std::env::var("RTDB_READ_API_KEY") {
            store.add_key(ApiKey::new(read_key, "read-only", Role::Reader));
        }
        
        store
    }
    
    /// Add an API key
    pub fn add_key(&mut self, key: ApiKey) {
        self.keys.insert(key.key.clone(), key);
    }
    
    /// Validate an API key
    pub fn validate(&self, key_value: &str) -> Option<AuthContext> {
        // Constant-time comparison to prevent timing attacks
        for (stored_key, api_key) in &self.keys {
            if constant_time_eq::constant_time_eq(stored_key.as_bytes(), key_value.as_bytes()) {
                if !api_key.active {
                    warn!(key_name = %api_key.name, "API key is inactive");
                    return None;
                }
                
                if api_key.is_expired() {
                    warn!(key_name = %api_key.name, "API key is expired");
                    return None;
                }
                
                return Some(AuthContext {
                    key_name: api_key.name.clone(),
                    role: api_key.role.clone(),
                    authenticated_at: Instant::now(),
                });
            }
        }
        None
    }
    
    /// Revoke an API key
    pub fn revoke(&mut self, key_name: &str) -> bool {
        for (_, key) in self.keys.iter_mut() {
            if key.name == key_name {
                key.active = false;
                return true;
            }
        }
        false
    }
    
    /// List all keys (without the actual key values)
    pub fn list_keys(&self) -> Vec<(String, Role, bool)> {
        self.keys
            .values()
            .map(|k| (k.name.clone(), k.role.clone(), k.active))
            .collect()
    }
    
    /// Check if store has any keys configured
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl Default for ApiKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Paths that don't require authentication
    pub public_paths: Vec<String>,
    /// Header name for API key (default: X-API-Key)
    pub api_key_header: String,
    /// Also accept Authorization: Bearer header
    pub accept_bearer: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            public_paths: vec![
                "/health".to_string(),
                "/health/live".to_string(),
                "/health/ready".to_string(),
                "/health/startup".to_string(),
                "/metrics".to_string(),
                "/".to_string(), // Root health check
            ],
            api_key_header: "X-API-Key".to_string(),
            accept_bearer: true,
        }
    }
}

impl AuthConfig {
    /// Check if path is public (no auth required)
    pub fn is_public_path(&self, path: &str) -> bool {
        self.public_paths.iter().any(|p| {
            // Exact match or path is a subpath (starts with "/path/")
            path == p || path.starts_with(&format!("{}/", p))
        })
    }
    
    /// Enable authentication
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }
}

/// Extract API key from request headers
pub fn extract_api_key(
    headers: &header::HeaderMap,
    config: &AuthConfig,
) -> Option<String> {
    // Try X-API-Key header first
    if let Some(key) = headers
        .get(&config.api_key_header)
        .and_then(|v| v.to_str().ok())
    {
        debug!("Found API key in {} header", config.api_key_header);
        return Some(key.to_string());
    }
    
    // Try Authorization: Bearer header
    if config.accept_bearer {
        if let Some(auth) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if auth.starts_with("Bearer ") {
                debug!("Found API key in Authorization Bearer header");
                return Some(auth[7..].to_string());
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_api_key_creation() {
        let key = ApiKey::new("test-key", "test", Role::Admin);
        assert_eq!(key.name, "test");
        assert_eq!(key.role, Role::Admin);
        assert!(key.active);
        assert!(!key.is_expired());
    }
    
    #[test]
    fn test_api_key_expiration() {
        let key = ApiKey::new("test", "test", Role::Reader)
            .with_expiration(Duration::from_millis(1));
        
        std::thread::sleep(Duration::from_millis(10));
        assert!(key.is_expired());
    }
    
    #[test]
    fn test_key_store_validation() {
        let mut store = ApiKeyStore::new();
        store.add_key(ApiKey::new("valid-key", "test-key", Role::Writer));
        
        let ctx = store.validate("valid-key");
        assert!(ctx.is_some());
        assert_eq!(ctx.unwrap().role, Role::Writer);
        
        let invalid = store.validate("invalid-key");
        assert!(invalid.is_none());
    }
    
    #[test]
    fn test_key_store_revoke() {
        let mut store = ApiKeyStore::new();
        store.add_key(ApiKey::new("key-to-revoke", "revokable", Role::Reader));
        
        assert!(store.validate("key-to-revoke").is_some());
        
        store.revoke("revokable");
        
        assert!(store.validate("key-to-revoke").is_none());
    }
    
    #[test]
    fn test_auth_config_public_paths() {
        let config = AuthConfig::default();
        
        assert!(config.is_public_path("/health"));
        assert!(config.is_public_path("/health/live"));
        assert!(config.is_public_path("/metrics"));
        assert!(!config.is_public_path("/collections"));
    }
    
    #[test]
    fn test_extract_api_key_x_api_key() {
        let config = AuthConfig::default();
        let mut headers = header::HeaderMap::new();
        headers.insert("X-API-Key", header::HeaderValue::from_static("my-key"));
        
        let key = extract_api_key(&headers, &config);
        assert_eq!(key, Some("my-key".to_string()));
    }
    
    #[test]
    fn test_extract_api_key_bearer() {
        let config = AuthConfig::default();
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Bearer my-bearer-key")
        );
        
        let key = extract_api_key(&headers, &config);
        assert_eq!(key, Some("my-bearer-key".to_string()));
    }
    
    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.has_permission(Permission::CreateCollection));
        assert!(Role::Admin.has_permission(Permission::DeleteCollection));
        assert!(Role::Admin.has_permission(Permission::Search));
        
        assert!(Role::Writer.has_permission(Permission::CreateCollection));
        assert!(Role::Writer.has_permission(Permission::Search));
        assert!(!Role::Writer.has_permission(Permission::DeleteCollection));
        
        assert!(Role::Reader.has_permission(Permission::Search));
        assert!(!Role::Reader.has_permission(Permission::CreateCollection));
        assert!(!Role::Reader.has_permission(Permission::DeleteCollection));
    }
}
