//! Authentication and authorization for the API.
//!
//! Supports multiple authentication modes:
//! - None: No authentication required (trusted LAN)
//! - PIN: Simple numeric PIN for basic access control
//! - Token: API token in Authorization header
//! - Full: Role-based access control with JWT

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use base64::Engine;
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::Error;

/// Authentication mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityMode {
    /// No authentication required.
    #[default]
    None,
    /// Simple PIN-based authentication.
    Pin,
    /// API token authentication.
    Token,
    /// Full role-based access control.
    Full,
}

/// Permission types for API operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Read access (GET operations).
    Read,
    /// Create connections.
    Connect,
    /// Remove connections.
    Disconnect,
    /// Modify existing connections (volume, mute).
    Modify,
    /// Administrative operations.
    Admin,
}

impl Permission {
    /// Returns all available permissions.
    #[must_use]
    pub fn all() -> HashSet<Permission> {
        [
            Permission::Read,
            Permission::Connect,
            Permission::Disconnect,
            Permission::Modify,
            Permission::Admin,
        ]
        .into_iter()
        .collect()
    }

    /// Returns read-only permissions.
    #[must_use]
    pub fn read_only() -> HashSet<Permission> {
        [Permission::Read].into_iter().collect()
    }

    /// Returns standard user permissions (no admin).
    #[must_use]
    pub fn standard() -> HashSet<Permission> {
        [
            Permission::Read,
            Permission::Connect,
            Permission::Disconnect,
            Permission::Modify,
        ]
        .into_iter()
        .collect()
    }
}

/// Security configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Authentication mode.
    pub mode: SecurityMode,
    /// PIN for PIN mode (4-6 digits).
    #[serde(default)]
    pub pin: Option<String>,
    /// API token for token mode.
    #[serde(default)]
    pub api_token: Option<String>,
    /// Allow read operations without authentication.
    #[serde(default = "default_true")]
    pub read_only_without_auth: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            mode: SecurityMode::None,
            pin: None,
            api_token: None,
            read_only_without_auth: true,
        }
    }
}

impl SecurityConfig {
    /// Creates a new security config with no authentication.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Creates a PIN-based security config.
    #[must_use]
    pub fn with_pin(pin: impl Into<String>) -> Self {
        Self {
            mode: SecurityMode::Pin,
            pin: Some(pin.into()),
            api_token: None,
            read_only_without_auth: true,
        }
    }

    /// Creates a token-based security config.
    #[must_use]
    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            mode: SecurityMode::Token,
            pin: None,
            api_token: Some(token.into()),
            read_only_without_auth: true,
        }
    }

    /// Generates a new random API token.
    #[must_use]
    pub fn generate_token() -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }
}

/// Authenticated user context.
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    /// Whether the user is authenticated.
    pub authenticated: bool,
    /// User's permissions.
    pub permissions: HashSet<Permission>,
    /// Optional user identifier (for logging).
    pub user_id: Option<String>,
}

impl AuthContext {
    /// Creates an unauthenticated context.
    #[must_use]
    pub fn unauthenticated() -> Self {
        Self::default()
    }

    /// Creates a fully authenticated admin context.
    #[must_use]
    pub fn admin() -> Self {
        Self {
            authenticated: true,
            permissions: Permission::all(),
            user_id: Some("admin".to_string()),
        }
    }

    /// Creates an authenticated context with standard permissions.
    #[must_use]
    pub fn standard_user() -> Self {
        Self {
            authenticated: true,
            permissions: Permission::standard(),
            user_id: None,
        }
    }

    /// Creates a read-only context.
    #[must_use]
    pub fn read_only() -> Self {
        Self {
            authenticated: false,
            permissions: Permission::read_only(),
            user_id: None,
        }
    }

    /// Checks if the context has a specific permission.
    #[must_use]
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Checks if the context has all specified permissions.
    #[must_use]
    pub fn has_all_permissions(&self, permissions: &[Permission]) -> bool {
        permissions.iter().all(|p| self.permissions.contains(p))
    }
}

/// Authentication manager.
#[derive(Clone)]
pub struct AuthManager {
    inner: Arc<AuthManagerInner>,
}

struct AuthManagerInner {
    config: RwLock<SecurityConfig>,
}

impl AuthManager {
    /// Creates a new authentication manager.
    #[must_use]
    pub fn new(config: SecurityConfig) -> Self {
        Self {
            inner: Arc::new(AuthManagerInner {
                config: RwLock::new(config),
            }),
        }
    }

    /// Creates an authentication manager with no security.
    #[must_use]
    pub fn no_auth() -> Self {
        Self::new(SecurityConfig::none())
    }

    /// Returns the current security mode.
    #[must_use]
    pub fn mode(&self) -> SecurityMode {
        self.inner.config.read().mode
    }

    /// Validates credentials and returns an auth context.
    #[must_use]
    pub fn authenticate(&self, credentials: Option<&str>) -> AuthContext {
        let config = self.inner.config.read();

        match config.mode {
            SecurityMode::None => {
                // No auth required - grant full access
                AuthContext::admin()
            },
            SecurityMode::Pin => {
                if let Some(cred) = credentials {
                    if config.pin.as_deref() == Some(cred) {
                        return AuthContext::standard_user();
                    }
                }
                if config.read_only_without_auth {
                    AuthContext::read_only()
                } else {
                    AuthContext::unauthenticated()
                }
            },
            SecurityMode::Token => {
                if let Some(cred) = credentials {
                    // Strip "Bearer " prefix if present
                    let token = cred.strip_prefix("Bearer ").unwrap_or(cred);
                    if config.api_token.as_deref() == Some(token) {
                        return AuthContext::admin();
                    }
                }
                if config.read_only_without_auth {
                    AuthContext::read_only()
                } else {
                    AuthContext::unauthenticated()
                }
            },
            SecurityMode::Full => {
                // Full mode would validate JWT and extract roles
                // For now, treat like token mode
                if let Some(cred) = credentials {
                    let token = cred.strip_prefix("Bearer ").unwrap_or(cred);
                    if config.api_token.as_deref() == Some(token) {
                        return AuthContext::admin();
                    }
                }
                if config.read_only_without_auth {
                    AuthContext::read_only()
                } else {
                    AuthContext::unauthenticated()
                }
            },
        }
    }

    /// Updates the security configuration.
    pub fn set_config(&self, config: SecurityConfig) {
        *self.inner.config.write() = config;
    }

    /// Gets a clone of the current configuration.
    #[must_use]
    pub fn config(&self) -> SecurityConfig {
        self.inner.config.read().clone()
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::no_auth()
    }
}

/// Extractor for authentication context.
///
/// Use in handlers to check permissions:
/// ```ignore
/// async fn protected_handler(auth: Auth) -> Result<(), Error> {
///     auth.require_permission(Permission::Modify)?;
///     // ... handler logic
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Auth(pub AuthContext);

impl Auth {
    /// Requires a specific permission.
    ///
    /// # Errors
    ///
    /// Returns `Error::Unauthorized` if not authenticated.
    /// Returns `Error::Forbidden` if permission is denied.
    pub fn require_permission(&self, permission: Permission) -> crate::Result<()> {
        if !self.0.authenticated && permission != Permission::Read {
            return Err(Error::Unauthorized("Authentication required".into()));
        }
        if !self.0.has_permission(permission) {
            return Err(Error::Forbidden(format!(
                "Permission denied: {permission:?}"
            )));
        }
        Ok(())
    }

    /// Requires authentication (any valid auth).
    ///
    /// # Errors
    ///
    /// Returns `Error::Unauthorized` if not authenticated.
    pub fn require_auth(&self) -> crate::Result<()> {
        if !self.0.authenticated {
            return Err(Error::Unauthorized("Authentication required".into()));
        }
        Ok(())
    }
}

impl<S> FromRequestParts<S> for Auth
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        _state: &'life1 S,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Self, Self::Rejection>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // Extract Authorization header
            let auth_header = parts
                .headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(String::from);

            // Get AuthManager from extensions (set by middleware)
            let auth_manager = parts
                .extensions
                .get::<AuthManager>()
                .cloned()
                .unwrap_or_default();

            let context = auth_manager.authenticate(auth_header.as_deref());

            Ok(Auth(context))
        })
    }
}

/// Validates a WebSocket authentication query parameter.
#[must_use]
pub fn validate_ws_auth(auth_manager: &AuthManager, auth_param: Option<&str>) -> AuthContext {
    auth_manager.authenticate(auth_param)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_mode_default_is_none() {
        assert_eq!(SecurityMode::default(), SecurityMode::None);
    }

    #[test]
    fn security_config_default() {
        let config = SecurityConfig::default();
        assert_eq!(config.mode, SecurityMode::None);
        assert!(config.pin.is_none());
        assert!(config.api_token.is_none());
        assert!(config.read_only_without_auth);
    }

    #[test]
    fn security_config_with_pin() {
        let config = SecurityConfig::with_pin("1234");
        assert_eq!(config.mode, SecurityMode::Pin);
        assert_eq!(config.pin, Some("1234".to_string()));
    }

    #[test]
    fn security_config_with_token() {
        let config = SecurityConfig::with_token("my-token");
        assert_eq!(config.mode, SecurityMode::Token);
        assert_eq!(config.api_token, Some("my-token".to_string()));
    }

    #[test]
    fn generate_token_is_unique() {
        let t1 = SecurityConfig::generate_token();
        let t2 = SecurityConfig::generate_token();
        assert_ne!(t1, t2);
        assert!(!t1.is_empty());
    }

    #[test]
    fn auth_context_unauthenticated() {
        let ctx = AuthContext::unauthenticated();
        assert!(!ctx.authenticated);
        assert!(ctx.permissions.is_empty());
    }

    #[test]
    fn auth_context_admin() {
        let ctx = AuthContext::admin();
        assert!(ctx.authenticated);
        assert!(ctx.has_permission(Permission::Read));
        assert!(ctx.has_permission(Permission::Admin));
        assert!(ctx.has_permission(Permission::Modify));
    }

    #[test]
    fn auth_context_standard_user() {
        let ctx = AuthContext::standard_user();
        assert!(ctx.authenticated);
        assert!(ctx.has_permission(Permission::Read));
        assert!(ctx.has_permission(Permission::Modify));
        assert!(!ctx.has_permission(Permission::Admin));
    }

    #[test]
    fn auth_context_read_only() {
        let ctx = AuthContext::read_only();
        assert!(!ctx.authenticated);
        assert!(ctx.has_permission(Permission::Read));
        assert!(!ctx.has_permission(Permission::Modify));
    }

    #[test]
    fn auth_manager_no_auth_mode() {
        let manager = AuthManager::no_auth();
        let ctx = manager.authenticate(None);
        assert!(ctx.authenticated);
        assert!(ctx.has_permission(Permission::Admin));
    }

    #[test]
    fn auth_manager_pin_mode_valid() {
        let manager = AuthManager::new(SecurityConfig::with_pin("1234"));
        let ctx = manager.authenticate(Some("1234"));
        assert!(ctx.authenticated);
        assert!(ctx.has_permission(Permission::Modify));
        assert!(!ctx.has_permission(Permission::Admin));
    }

    #[test]
    fn auth_manager_pin_mode_invalid() {
        let manager = AuthManager::new(SecurityConfig::with_pin("1234"));
        let ctx = manager.authenticate(Some("wrong"));
        assert!(!ctx.authenticated);
        // Read-only should still work
        assert!(ctx.has_permission(Permission::Read));
    }

    #[test]
    fn auth_manager_token_mode_valid() {
        let manager = AuthManager::new(SecurityConfig::with_token("secret"));
        let ctx = manager.authenticate(Some("Bearer secret"));
        assert!(ctx.authenticated);
        assert!(ctx.has_permission(Permission::Admin));
    }

    #[test]
    fn auth_manager_token_mode_without_bearer() {
        let manager = AuthManager::new(SecurityConfig::with_token("secret"));
        let ctx = manager.authenticate(Some("secret"));
        assert!(ctx.authenticated);
    }

    #[test]
    fn auth_manager_token_mode_invalid() {
        let manager = AuthManager::new(SecurityConfig::with_token("secret"));
        let ctx = manager.authenticate(Some("Bearer wrong"));
        assert!(!ctx.authenticated);
    }

    #[test]
    fn permission_all_contains_every_permission() {
        let all = Permission::all();
        assert!(all.contains(&Permission::Read));
        assert!(all.contains(&Permission::Connect));
        assert!(all.contains(&Permission::Disconnect));
        assert!(all.contains(&Permission::Modify));
        assert!(all.contains(&Permission::Admin));
    }

    #[test]
    fn permission_standard_no_admin() {
        let standard = Permission::standard();
        assert!(standard.contains(&Permission::Read));
        assert!(standard.contains(&Permission::Modify));
        assert!(!standard.contains(&Permission::Admin));
    }

    #[test]
    fn has_all_permissions() {
        let ctx = AuthContext::admin();
        assert!(ctx.has_all_permissions(&[Permission::Read, Permission::Modify]));

        let readonly = AuthContext::read_only();
        assert!(!readonly.has_all_permissions(&[Permission::Read, Permission::Modify]));
    }

    #[test]
    fn ws_auth_validation() {
        let manager = AuthManager::new(SecurityConfig::with_token("secret"));

        let valid = validate_ws_auth(&manager, Some("secret"));
        assert!(valid.authenticated);

        let invalid = validate_ws_auth(&manager, Some("wrong"));
        assert!(!invalid.authenticated);
    }
}
