//! Authentication and authorization layer



/// RBAC manager
pub struct RBAC;

impl RBAC {
    /// Create new RBAC
    pub fn new() -> Self {
        Self
    }
}

impl Default for RBAC {
    fn default() -> Self {
        Self::new()
    }
}

/// Authentication provider
pub struct AuthProvider;

impl AuthProvider {
    /// Create new provider
    pub fn new() -> Self {
        Self
    }
}

impl Default for AuthProvider {
    fn default() -> Self {
        Self::new()
    }
}
