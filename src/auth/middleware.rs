//! Authentication Middleware
//!
//! Axum middleware for API key authentication and RBAC enforcement.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::sync::Arc;
use tracing::warn;

use super::{extract_api_key, ApiKeyStore, AuthConfig, AuthContext, AuthError};

/// Authentication middleware for Axum
///
/// This middleware:
/// 1. Skips authentication for public paths
/// 2. Extracts API key from X-API-Key or Authorization header
/// 3. Validates the key against the store
/// 4. Injects AuthContext into request extensions
/// 5. Returns 401 if authentication fails
pub async fn auth_middleware(
    State(state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let config = &state.config;
    let key_store = &state.key_store;
    let path = request.uri().path();
    
    // Skip auth for public paths
    if config.is_public_path(path) {
        debug!(path, "Skipping auth for public path");
        return Ok(next.run(request).await);
    }
    
    // Skip auth if no keys are configured (open mode)
    if key_store.is_empty() {
        debug!("No API keys configured, allowing request");
        return Ok(next.run(request).await);
    }
    
    // Extract API key from headers
    let api_key = extract_api_key(request.headers(), &config)
        .ok_or_else(|| {
            warn!(path, "No API key provided");
            AuthError::unauthorized("API key required. Provide X-API-Key header or Authorization: Bearer token")
        })?;
    
    // Validate API key
    let auth_context = key_store.validate(&api_key)
        .ok_or_else(|| {
            warn!(path, "Invalid API key provided");
            AuthError::unauthorized("Invalid API key")
        })?;
    
    info!(
        path,
        key_name = %auth_context.key_name,
        role = %auth_context.role,
        "Authenticated request"
    );
    
    // Inject auth context into request extensions
    request.extensions_mut().insert(auth_context);
    
    Ok(next.run(request).await)
}

/// Require specific permission middleware factory
///
/// Usage:
/// ```rust
/// Router::new()
///     .route("/collections", post(create_collection))
///     .route_layer(middleware::from_fn(require_permission(Permission::CreateCollection)))
/// ```
pub fn require_permission(
    permission: super::rbac::Permission,
) -> impl Fn(Extension<AuthContext>, Request, Next) -> Response + Clone {
    move |Extension(auth): Extension<AuthContext>, request: Request, next: Next| {
        if !auth.has_permission(permission) {
            return AuthError::forbidden(format!(
                "Permission denied: {:?} required",
                permission
            )).into_response();
        }
        
        // Continue to next middleware/handler
        // Note: This is a simplified version; real implementation would use async
        // and return a future
        tokio::runtime::Handle::current().block_on(async {
            next.run(request).await
        })
    }
}

/// Handler for authenticated routes to get current user info
pub async fn me_handler(Extension(auth): Extension<AuthContext>) -> impl IntoResponse {
    let body = serde_json::json!({
        "key_name": auth.key_name,
        "role": auth.role.as_str(),
        "authenticated_at": auth.authenticated_at.elapsed().as_secs(),
        "permissions": get_permissions_for_role(auth.role),
    });
    
    (StatusCode::OK, Json(body))
}

fn get_permissions_for_role(role: super::rbac::Role) -> Vec<String> {
    use super::rbac::Permission;
    
    let all_permissions = vec![
        Permission::CreateCollection,
        Permission::DeleteCollection,
        Permission::ListCollections,
        Permission::GetCollectionInfo,
        Permission::InsertVectors,
        Permission::DeleteVectors,
        Permission::Search,
        Permission::Retrieve,
        Permission::CreateIndex,
        Permission::DeleteIndex,
        Permission::OptimizeIndex,
        Permission::ClusterAdmin,
        Permission::ViewClusterStatus,
        Permission::SystemAdmin,
        Permission::ViewMetrics,
        Permission::ViewLogs,
    ];
    
    all_permissions
        .into_iter()
        .filter(|p| role.has_permission(*p))
        .map(|p| format!("{:?}", p))
        .collect()
}

/// Auth state bundle for middleware
#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
    pub key_store: Arc<ApiKeyStore>,
}

impl AuthState {
    pub fn new(config: AuthConfig, key_store: Arc<ApiKeyStore>) -> Self {
        Self { config, key_store }
    }
}

/// Create auth state for middleware
pub fn create_auth_state(key_store: Arc<ApiKeyStore>) -> AuthState {
    let config = AuthConfig::default();
    AuthState::new(config, key_store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;
    
    fn create_test_request(path: &str, api_key: Option<&str>) -> Request {
        let mut builder = Request::builder()
            .uri(path)
            .method("GET");
        
        if let Some(key) = api_key {
            builder = builder.header("X-API-Key", key);
        }
        
        builder.body(axum::body::Body::empty()).unwrap()
    }
    
    #[test]
    fn test_public_path_skips_auth() {
        let config = AuthConfig::default();
        assert!(config.is_public_path("/health"));
        assert!(config.is_public_path("/metrics"));
        assert!(!config.is_public_path("/collections"));
    }
    
    #[test]
    fn test_extract_api_key_from_header() {
        let config = AuthConfig::default();
        let mut headers = header::HeaderMap::new();
        headers.insert("X-API-Key", header::HeaderValue::from_static("test-key"));
        
        let key = extract_api_key(&headers, &config);
        assert_eq!(key, Some("test-key".to_string()));
    }
    
    #[test]
    fn test_extract_api_key_from_bearer() {
        let config = AuthConfig::default();
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Bearer bearer-key")
        );
        
        let key = extract_api_key(&headers, &config);
        assert_eq!(key, Some("bearer-key".to_string()));
    }
}
