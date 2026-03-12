//! Role-Based Access Control (RBAC)
//!
//! Implements permission-based access control with predefined roles.

#![allow(missing_docs)]

/// Permissions for vector database operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // Collection management
    /// Create a new collection
    CreateCollection,
    /// Delete an existing collection
    DeleteCollection,
    /// List all collections
    ListCollections,
    /// Get collection information
    GetCollectionInfo,
    
    // Vector operations
    /// Insert vectors into a collection
    InsertVectors,
    /// Delete vectors from a collection
    DeleteVectors,
    /// Search for similar vectors
    Search,
    /// Retrieve vectors by ID
    Retrieve,
    
    // Index operations
    /// Create a new index
    CreateIndex,
    /// Delete an existing index
    DeleteIndex,
    /// Optimize index performance
    OptimizeIndex,
    
    // Cluster operations
    /// Administer cluster operations
    ClusterAdmin,
    /// View cluster status
    ViewClusterStatus,
    
    // System operations
    /// System administration
    SystemAdmin,
    /// View system metrics
    ViewMetrics,
    /// View system logs
    ViewLogs,
}

/// Predefined roles with associated permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Full access to all operations
    Admin,
    /// Can create collections, insert/search vectors
    Writer,
    /// Read-only access (search, retrieve)
    Reader,
}

impl Role {
    /// Check if this role has the specified permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        match self {
            Role::Admin => true, // Admin has all permissions
            Role::Writer => Self::writer_permissions().contains(&permission),
            Role::Reader => Self::reader_permissions().contains(&permission),
        }
    }
    
    /// Get all permissions for the Writer role
    fn writer_permissions() -> &'static [Permission] {
        use Permission::*;
        &[
            CreateCollection,
            ListCollections,
            GetCollectionInfo,
            InsertVectors,
            DeleteVectors,
            Search,
            Retrieve,
            ViewMetrics,
        ]
    }
    
    /// Get all permissions for the Reader role
    fn reader_permissions() -> &'static [Permission] {
        use Permission::*;
        &[
            ListCollections,
            GetCollectionInfo,
            Search,
            Retrieve,
            ViewMetrics,
        ]
    }
    
    /// Get human-readable role name
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Writer => "writer",
            Role::Reader => "reader",
        }
    }
    
    /// Parse role from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "writer" => Some(Role::Writer),
            "reader" => Some(Role::Reader),
            _ => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Access control for resource-level permissions
#[derive(Debug, Clone)]
pub struct AccessControl {
    /// Default role for unauthenticated users (if any)
    pub default_role: Option<Role>,
    /// Collection-level access rules
    pub collection_rules: std::collections::HashMap<String, Vec<Role>>,
}

impl AccessControl {
    /// Create new access control
    pub fn new() -> Self {
        Self {
            default_role: None,
            collection_rules: std::collections::HashMap::new(),
        }
    }
    
    /// Set default role for unauthenticated users
    pub fn with_default_role(mut self, role: Role) -> Self {
        self.default_role = Some(role);
        self
    }
    
    /// Grant access to a collection for specific roles
    pub fn grant_collection_access(&mut self, collection: impl Into<String>, roles: Vec<Role>) {
        self.collection_rules.insert(collection.into(), roles);
    }
    
    /// Check if role can access collection
    pub fn can_access_collection(&self, collection: &str, role: Role) -> bool {
        // Admin always has access
        if role == Role::Admin {
            return true;
        }
        
        // Check collection-specific rules
        match self.collection_rules.get(collection) {
            Some(allowed_roles) => allowed_roles.contains(&role),
            None => true, // No restrictions = allow all
        }
    }
}

impl Default for AccessControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Permission checker for specific operations
pub struct PermissionChecker;

impl PermissionChecker {
    /// Check if user can perform collection operation
    pub fn check_collection_op(role: Role, op: CollectionOperation) -> bool {
        let permission = match op {
            CollectionOperation::Create => Permission::CreateCollection,
            CollectionOperation::Delete => Permission::DeleteCollection,
            CollectionOperation::List => Permission::ListCollections,
            CollectionOperation::GetInfo => Permission::GetCollectionInfo,
        };
        role.has_permission(permission)
    }
    
    /// Check if user can perform vector operation
    pub fn check_vector_op(role: Role, op: VectorOperation) -> bool {
        let permission = match op {
            VectorOperation::Insert => Permission::InsertVectors,
            VectorOperation::Delete => Permission::DeleteVectors,
            VectorOperation::Search => Permission::Search,
            VectorOperation::Retrieve => Permission::Retrieve,
        };
        role.has_permission(permission)
    }
}

/// Collection operations
#[derive(Debug, Clone, Copy)]
pub enum CollectionOperation {
    /// Create a collection
    Create,
    /// Delete a collection
    Delete,
    /// List collections
    List,
    /// Get collection info
    GetInfo,
}

/// Vector operations
#[derive(Debug, Clone, Copy)]
pub enum VectorOperation {
    /// Insert vectors
    Insert,
    /// Delete vectors
    Delete,
    /// Search vectors
    Search,
    /// Retrieve vectors
    Retrieve,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_admin_has_all_permissions() {
        let admin = Role::Admin;
        assert!(admin.has_permission(Permission::CreateCollection));
        assert!(admin.has_permission(Permission::DeleteCollection));
        assert!(admin.has_permission(Permission::SystemAdmin));
        assert!(admin.has_permission(Permission::ClusterAdmin));
    }
    
    #[test]
    fn test_writer_permissions() {
        let writer = Role::Writer;
        assert!(writer.has_permission(Permission::CreateCollection));
        assert!(writer.has_permission(Permission::InsertVectors));
        assert!(writer.has_permission(Permission::Search));
        assert!(!writer.has_permission(Permission::DeleteCollection));
        assert!(!writer.has_permission(Permission::SystemAdmin));
    }
    
    #[test]
    fn test_reader_permissions() {
        let reader = Role::Reader;
        assert!(reader.has_permission(Permission::Search));
        assert!(reader.has_permission(Permission::Retrieve));
        assert!(reader.has_permission(Permission::ListCollections));
        assert!(!reader.has_permission(Permission::CreateCollection));
        assert!(!reader.has_permission(Permission::InsertVectors));
    }
    
    #[test]
    fn test_access_control_collection() {
        let mut ac = AccessControl::new();
        ac.grant_collection_access("public", vec![Role::Reader, Role::Writer, Role::Admin]);
        ac.grant_collection_access("private", vec![Role::Admin]);
        
        assert!(ac.can_access_collection("public", Role::Reader));
        assert!(ac.can_access_collection("public", Role::Writer));
        assert!(ac.can_access_collection("public", Role::Admin));
        
        assert!(!ac.can_access_collection("private", Role::Reader));
        assert!(!ac.can_access_collection("private", Role::Writer));
        assert!(ac.can_access_collection("private", Role::Admin));
        
        // No rules = allow all
        assert!(ac.can_access_collection("unrestricted", Role::Reader));
    }
    
    #[test]
    fn test_role_parse() {
        assert_eq!(Role::from_str("admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("Admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("ADMIN"), Some(Role::Admin));
        assert_eq!(Role::from_str("writer"), Some(Role::Writer));
        assert_eq!(Role::from_str("reader"), Some(Role::Reader));
        assert_eq!(Role::from_str("invalid"), None);
    }
    
    #[test]
    fn test_permission_checker() {
        assert!(PermissionChecker::check_collection_op(Role::Writer, CollectionOperation::Create));
        assert!(!PermissionChecker::check_collection_op(Role::Reader, CollectionOperation::Create));
        
        assert!(PermissionChecker::check_vector_op(Role::Reader, VectorOperation::Search));
        assert!(!PermissionChecker::check_vector_op(Role::Reader, VectorOperation::Insert));
    }
}
