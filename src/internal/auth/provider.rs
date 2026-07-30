//! Auth Provider Trait
//!
//! Defines the interface for authentication providers.
//! New auth methods are added by implementing this trait — no changes
//! to the request pipeline needed.

use anyhow::Result;
use std::collections::HashMap;

/// The result of a successful authentication
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    /// Provider-unique user identifier
    pub subject: String,
    /// Human-readable name (if available)
    pub name: Option<String>,
    /// Email or principal name
    pub email: Option<String>,
    /// Provider name (e.g., "oidc", "bearer", "api_key")
    pub provider: String,
    /// Additional provider-specific claims
    pub claims: HashMap<String, String>,
}

/// Trait for authentication providers.
///
/// Implement this trait to add a new auth method.
/// Providers are stateless — configuration is injected at construction time.
pub trait AuthProvider: Send + Sync {
    /// Return the provider type name
    fn provider_type(&self) -> &str;

    /// Authenticate a request given its headers.
    ///
    /// Returns `Ok(Some(identity))` on success,
    /// `Ok(None)` if no auth credentials were present (let other middleware decide),
    /// `Err(...)` if credentials were present but invalid.
    fn authenticate(&self, headers: &HashMap<String, String>) -> Result<Option<AuthIdentity>>;

    /// Does this provider match the given route?
    ///
    /// Used by ProviderRouter to determine which provider handles a request.
    fn matches_route(&self, path: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test provider that authenticates with a static token
    struct TestProvider {
        token: String,
        route_pattern: String,
    }

    impl AuthProvider for TestProvider {
        fn provider_type(&self) -> &str {
            "test"
        }

        fn authenticate(&self, headers: &HashMap<String, String>) -> Result<Option<AuthIdentity>> {
            let auth_header = headers.get("authorization");
            match auth_header {
                Some(h) if h == &format!("Bearer {}", self.token) => Ok(Some(AuthIdentity {
                    subject: "test-user".into(),
                    name: Some("Test User".into()),
                    email: Some("test@example.com".into()),
                    provider: "test".into(),
                    claims: HashMap::new(),
                })),
                Some(_) => Ok(None), // Invalid token
                None => Ok(None),    // No auth header
            }
        }

        fn matches_route(&self, path: &str) -> bool {
            path.starts_with(&self.route_pattern)
        }
    }

    #[test]
    fn test_provider_authenticate_success() {
        let provider = TestProvider {
            token: "secret123".into(),
            route_pattern: "/api".into(),
        };

        let mut headers = HashMap::new();
        headers.insert("authorization".into(), "Bearer secret123".into());

        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_some());
        let identity = result.unwrap();
        assert_eq!(identity.subject, "test-user");
        assert_eq!(identity.provider, "test");
    }

    #[test]
    fn test_provider_authenticate_no_header() {
        let provider = TestProvider {
            token: "secret123".into(),
            route_pattern: "/api".into(),
        };

        let headers = HashMap::new();
        let result = provider.authenticate(&headers).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_provider_matches_route() {
        let provider = TestProvider {
            token: "secret123".into(),
            route_pattern: "/api/protected".into(),
        };

        assert!(provider.matches_route("/api/protected/users"));
        assert!(provider.matches_route("/api/protected"));
        assert!(!provider.matches_route("/api/public"));
        assert!(!provider.matches_route("/other"));
    }
}
