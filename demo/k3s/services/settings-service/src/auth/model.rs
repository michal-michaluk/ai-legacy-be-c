//! The authenticated principal and entity-ownership value objects.
//!
//! `Authority` / `Ownership` belong to the **auth (identity)** context. The
//! `settings` slice receives `Authority` as a method argument (middleware
//! injected) and applies `Ownership` access checks. All synchronous value
//! objects — no JWT claims, no framework.

use std::fmt;

/// The tenant a principal / entity belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tenant(String);

impl Tenant {
    pub fn new(s: &str) -> Self {
        Self(s.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tenant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A user within a tenant.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(String);

impl UserId {
    pub fn new(s: &str) -> Self {
        Self(s.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The role carried by the principal. Only two roles; `Admin` is tenant-scoped.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Admin => "Admin",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The authenticated principal (domain abstraction). Built by the auth adapter
/// from a validated token; the domain never parses a raw JWT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authority {
    tenant: Tenant,
    user: UserId,
    role: Role,
}

impl Authority {
    pub fn new(tenant: Tenant, user: UserId, role: Role) -> Self {
        Self { tenant, user, role }
    }

    pub fn tenant(&self) -> &Tenant {
        &self.tenant
    }

    pub fn user(&self) -> &UserId {
        &self.user
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// Access check (D1): tenant must match; `User` matches only their own
    /// entries; `Admin` sees their own entries **and** tenant-scoped entries
    /// (never another user's).
    pub fn can_read(&self, ownership: &Ownership) -> bool {
        if self.tenant != ownership.tenant {
            return false;
        }
        match self.role {
            Role::Admin => ownership.user.is_none() || ownership.user.as_ref() == Some(self.user()),
            Role::User => ownership.user.as_ref() == Some(self.user()),
        }
    }

    /// Writes use the same scope as reads in the settings domain.
    pub fn can_write(&self, ownership: &Ownership) -> bool {
        self.can_read(ownership)
    }

    /// The ownership an authority operates within: `Admin` -> tenant-scoped
    /// (user `None`), `User` -> their own user-scoped.
    pub fn scope_ownership(&self) -> Ownership {
        match self.role {
            Role::Admin => Ownership::for_tenant(self.tenant.clone()),
            Role::User => Ownership::for_user(self.tenant.clone(), self.user.clone()),
        }
    }
}

/// Entity ownership. `user = None` means **tenant-scoped** (Admin-only).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ownership {
    tenant: Tenant,
    user: Option<UserId>,
}

impl Ownership {
    pub fn for_tenant(tenant: Tenant) -> Self {
        Self { tenant, user: None }
    }

    pub fn for_user(tenant: Tenant, user: UserId) -> Self {
        Self {
            tenant,
            user: Some(user),
        }
    }

    pub fn tenant(&self) -> &Tenant {
        &self.tenant
    }

    pub fn user(&self) -> Option<&UserId> {
        self.user.as_ref()
    }

    /// True when this is a tenant-scoped (user == `None`) entry.
    pub fn is_tenant_scoped(&self) -> bool {
        self.user.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> Tenant {
        Tenant::new("acme")
    }
    fn user(id: &str) -> UserId {
        UserId::new(id)
    }

    #[test]
    fn admin_can_read_tenant_scoped() {
        let a = Authority::new(tenant(), user("admin"), Role::Admin);
        assert!(a.can_read(&Ownership::for_tenant(tenant())));
    }

    #[test]
    fn user_cannot_read_tenant_scoped() {
        let a = Authority::new(tenant(), user("bob"), Role::User);
        assert!(!a.can_read(&Ownership::for_tenant(tenant())));
    }

    #[test]
    fn user_can_read_own_scoped() {
        let a = Authority::new(tenant(), user("bob"), Role::User);
        assert!(a.can_read(&Ownership::for_user(tenant(), user("bob"))));
    }

    #[test]
    fn user_cannot_read_other_users() {
        let a = Authority::new(tenant(), user("bob"), Role::User);
        assert!(!a.can_read(&Ownership::for_user(tenant(), user("alice"))));
    }

    #[test]
    fn tenant_must_match() {
        let a = Authority::new(Tenant::new("acme"), user("bob"), Role::Admin);
        assert!(!a.can_read(&Ownership::for_tenant(Tenant::new("other"))));
    }

    #[test]
    fn display_impls_render_stable_forms() {
        assert_eq!(Tenant::new("acme").to_string(), "acme");
        assert_eq!(UserId::new("bob").to_string(), "bob");
        assert_eq!(Role::User.to_string(), "User");
        assert_eq!(Role::Admin.as_str(), "Admin");
    }

    #[test]
    fn admin_can_read_own_scoped_entry() {
        let a = Authority::new(tenant(), user("admin"), Role::Admin);
        assert!(a.can_read(&Ownership::for_user(tenant(), user("admin"))));
    }

    #[test]
    fn admin_cannot_read_another_users_entry() {
        let a = Authority::new(tenant(), user("admin"), Role::Admin);
        assert!(!a.can_read(&Ownership::for_user(tenant(), user("alice"))));
    }

    #[test]
    fn can_write_matches_can_read_scope() {
        let a = Authority::new(tenant(), user("bob"), Role::User);
        assert!(a.can_write(&Ownership::for_user(tenant(), user("bob"))));
        assert!(!a.can_write(&Ownership::for_tenant(tenant())));
    }

    #[test]
    fn scope_ownership_derives_from_role() {
        let admin = Authority::new(tenant(), user("admin"), Role::Admin);
        assert!(admin.scope_ownership().is_tenant_scoped());
        let bob = Authority::new(tenant(), user("bob"), Role::User);
        assert_eq!(bob.scope_ownership().user(), Some(&user("bob")));
    }
}
