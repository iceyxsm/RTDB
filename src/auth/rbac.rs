//! Role-Based Access Control (RBAC)

use crate::{Result, RTDBError};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// RBAC manager
pub struct RBAC {
    /// Roles defined in the system
    roles: DashMap<String, Role>,
    /// User to role mappings
    user_roles: DashMap<String, Vec<String>>,
    /// Resource permissions
    #[allow(dead_code)]
    resource_permissions: DashMap<String, Vec<Permission>>,
}

/// Role definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Role name
    pub name: String,
    /// Role description
    pub description: String,
    /// Permissions granted by this role
    pub permissions: Vec<Permission>,
}

/// Permission for an action on a resource
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permission {
    /// Resource type (collection, namespace, etc.)
    pub resource: String,
    /// Action (create, read, update, delete, search)
    pub action: Action,
    /// Optional resource name (None = all resources of this type)
    pub resource_name: Option<String>,
}

/// Available actions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    /// Create new resources
    Create,
    /// Read existing resources
    Read,
    /// Update existing resources
    Update,
    /// Delete resources
    Delete,
    /// Search/query resources
    Search,
    /// Admin operations
    Admin,
}

impl RBAC {
    /// Create new RBAC system with default roles
    pub fn new() -> Self {
        let rbac = Self {
            roles: DashMap::new(),
            user_roles: DashMap::new(),
            resource_permissions: DashMap::new(),
        };

        // Create default roles
        rbac.create_default_roles();
        
        rbac
    }

    /// Create default roles
    fn create_default_roles(&self) {
        // Admin role - full access
        let admin_role = Role {
            name: "admin".to_string(),
            description: "Full access to all resources".to_string(),
            permissions: vec![
                Permission {
                    resource: "*".to_string(),
                    action: Action::Admin,
                    resource_name: None,
                },
            ],
        };
        self.roles.insert("admin".to_string(), admin_role);

        // Writer role - can read, write, search
        let writer_role = Role {
            name: "writer".to_string(),
            description: "Can create, read, update, and search".to_string(),
            permissions: vec![
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Create,
                    resource_name: None,
                },
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Read,
                    resource_name: None,
                },
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Update,
                    resource_name: None,
                },
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Search,
                    resource_name: None,
                },
            ],
        };
        self.roles.insert("writer".to_string(), writer_role);

        // Reader role - read and search only
        let reader_role = Role {
            name: "reader".to_string(),
            description: "Can read and search only".to_string(),
            permissions: vec![
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Read,
                    resource_name: None,
                },
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Search,
                    resource_name: None,
                },
            ],
        };
        self.roles.insert("reader".to_string(), reader_role);
    }

    /// Create custom role
    pub fn create_role(&self, role: Role) -> Result<()> {
        if self.roles.contains_key(&role.name) {
            return Err(RTDBError::Authorization(
                format!("Role '{}' already exists", role.name)
            ));
        }
        
        self.roles.insert(role.name.clone(), role);
        Ok(())
    }

    /// Assign role to user
    pub fn assign_role(&self, user: &str, role: &str) -> Result<()> {
        if !self.roles.contains_key(role) {
            return Err(RTDBError::Authorization(
                format!("Role '{}' does not exist", role)
            ));
        }

        self.user_roles
            .entry(user.to_string())
            .and_modify(|roles| roles.push(role.to_string()))
            .or_insert_with(|| vec![role.to_string()]);
        
        Ok(())
    }

    /// Check if user has permission
    pub fn check_permission(
        &self,
        user: &str,
        resource: &str,
        action: Action,
        resource_name: Option<&str>,
    ) -> bool {
        // Get user's roles
        let user_roles = match self.user_roles.get(user) {
            Some(roles) => roles.clone(),
            None => return false,
        };

        // Check each role
        for role_name in &user_roles {
            if let Some(role) = self.roles.get(role_name) {
                for permission in &role.permissions {
                    // Check if permission matches
                    if self.permission_matches(permission, resource, action, resource_name) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if permission matches request
    fn permission_matches(
        &self,
        permission: &Permission,
        resource: &str,
        action: Action,
        resource_name: Option<&str>,
    ) -> bool {
        // Check resource type (wildcards allowed)
        if permission.resource != "*" && permission.resource != resource {
            return false;
        }

        // Check action
        if permission.action != action && permission.action != Action::Admin {
            return false;
        }

        // Check resource name if specified
        if let Some(perm_name) = &permission.resource_name {
            if let Some(req_name) = resource_name {
                if perm_name != req_name {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Get user roles
    pub fn get_user_roles(&self, user: &str) -> Vec<String> {
        self.user_roles
            .get(user)
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

impl Default for RBAC {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_defaults() {
        let rbac = RBAC::new();
        
        // Assign admin role
        assert!(rbac.assign_role("alice", "admin").is_ok());
        
        // Check admin has all permissions
        assert!(rbac.check_permission("alice", "collection", Action::Create, None));
        assert!(rbac.check_permission("alice", "collection", Action::Delete, None));
    }

    #[test]
    fn test_reader_permissions() {
        let rbac = RBAC::new();
        
        rbac.assign_role("bob", "reader").unwrap();
        
        // Reader can read and search
        assert!(rbac.check_permission("bob", "collection", Action::Read, None));
        assert!(rbac.check_permission("bob", "collection", Action::Search, None));
        
        // Reader cannot create or delete
        assert!(!rbac.check_permission("bob", "collection", Action::Create, None));
        assert!(!rbac.check_permission("bob", "collection", Action::Delete, None));
    }

    #[test]
    fn test_custom_role() {
        let rbac = RBAC::new();
        
        let custom_role = Role {
            name: "custom".to_string(),
            description: "Custom role".to_string(),
            permissions: vec![
                Permission {
                    resource: "collection".to_string(),
                    action: Action::Read,
                    resource_name: Some("specific_collection".to_string()),
                },
            ],
        };
        
        rbac.create_role(custom_role).unwrap();
        rbac.assign_role("charlie", "custom").unwrap();
        
        // Can read specific collection
        assert!(rbac.check_permission(
            "charlie",
            "collection",
            Action::Read,
            Some("specific_collection")
        ));
        
        // Cannot read other collections
        assert!(!rbac.check_permission(
            "charlie",
            "collection",
            Action::Read,
            Some("other_collection")
        ));
    }
}
