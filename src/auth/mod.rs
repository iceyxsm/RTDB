//! Authentication and authorization layer

pub mod rbac;

/// Authentication provider
pub struct AuthProvider {
    /// RBAC system
    pub rbac: rbac::RBAC,
}

impl AuthProvider {
    /// Create new auth provider
    pub fn new() -> Self {
        Self {
            rbac: rbac::RBAC::new(),
        }
    }

    /// Authenticate user (simple API key check for now)
    pub fn authenticate(&self, api_key: &str) -> Option<String> {
        // In production, validate against stored API keys
        // For now, simple check
        if api_key.is_empty() {
            None
        } else {
            Some(format!("user_{}", &api_key[..8.min(api_key.len())]))
        }
    }
}

impl Default for AuthProvider {
    fn default() -> Self {
        Self::new()
    }
}
