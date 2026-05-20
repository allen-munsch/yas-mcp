//! Provider Router — multi-tenant OIDC routing.
//!
//! Maps MCP tool paths to OIDC authentication providers using glob patterns.
//! Allows different API routes to be protected by different identity providers,
//! all within a single yas-mcp instance.

use super::oidc_discovery::OidcProviderConfig;
use anyhow::{anyhow, Result};
use regex::Regex;
use std::sync::Arc;
use tracing::debug;

/// A route-scoped OIDC provider binding
#[derive(Debug, Clone)]
pub struct RouteProvider {
    /// Provider name (human-readable)
    pub name: String,

    /// The provider configuration
    pub provider: Arc<OidcProviderConfig>,

    /// Compiled glob pattern for matching tool paths
    route_pattern: Regex,
}

impl RouteProvider {
    /// Check if this provider should handle a given tool path
    pub fn matches(&self, tool_path: &str) -> bool {
        self.route_pattern.is_match(tool_path)
    }
}

/// Multi-tenant provider router.
///
/// Routes tool call authentication requests to the appropriate OIDC provider
/// based on glob pattern matching against the tool's upstream API path.
#[derive(Debug, Clone)]
pub struct ProviderRouter {
    /// Ordered list of providers with their route patterns
    providers: Vec<RouteProvider>,

    /// Default provider name (used when no route pattern matches)
    default_provider: Option<String>,

    /// Name of the no-auth provider (for public/unprotected routes)
    no_auth_provider: Option<String>,
}

impl ProviderRouter {
    /// Create an empty router
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            default_provider: None,
            no_auth_provider: None,
        }
    }

    /// Add a provider with a route filter pattern.
    ///
    /// Patterns use glob-style syntax converted to regex:
    /// - `/api/corp/**` → matches `/api/corp/` and everything under it
    /// - `/api/public/*` → matches `/api/public/<single-segment>`
    /// - `/**` → matches everything
    pub fn add_provider(&mut self, config: OidcProviderConfig) -> Result<()> {
        let pattern = config
            .route_filter
            .clone()
            .unwrap_or_else(|| "/**".to_string());

        let regex = glob_to_regex(&pattern)?;

        debug!(
            provider = %config.name,
            pattern = %pattern,
            regex = %regex,
            "Adding route provider"
        );

        self.providers.push(RouteProvider {
            name: config.name.clone(),
            provider: Arc::new(config),
            route_pattern: regex,
        });

        Ok(())
    }

    /// Set the default provider (used when no route pattern matches)
    pub fn set_default_provider(&mut self, provider_name: &str) {
        self.default_provider = Some(provider_name.to_string());
    }

    /// Set a no-auth provider name (routes matching this provider skip auth)
    pub fn set_no_auth_provider(&mut self, provider_name: Option<String>) {
        self.no_auth_provider = provider_name;
    }

    /// Resolve which provider should handle a given tool path.
    ///
    /// Returns the matching `OidcProviderConfig`, or `None` if no provider matches
    /// (which means the route is unprotected).
    pub fn resolve(&self, tool_path: &str) -> Option<&OidcProviderConfig> {
        // Check each provider in order of registration (first match wins)
        for route_provider in &self.providers {
            if route_provider.matches(tool_path) {
                // If this is the no-auth provider, return None (unprotected)
                if let Some(ref no_auth) = self.no_auth_provider {
                    if route_provider.name == *no_auth {
                        debug!(path = %tool_path, "Route is public (no auth)");
                        return None;
                    }
                }

                debug!(
                    path = %tool_path,
                    provider = %route_provider.name,
                    "Route matched to provider"
                );
                return Some(&route_provider.provider);
            }
        }

        // No match — try default provider
        if let Some(ref default_name) = self.default_provider {
            for route_provider in &self.providers {
                if route_provider.name == *default_name {
                    debug!(
                        path = %tool_path,
                        provider = %default_name,
                        "Using default provider"
                    );
                    return Some(&route_provider.provider);
                }
            }
        }

        debug!(path = %tool_path, "No provider matched — route is public");
        None
    }

    /// Get all registered provider names
    pub fn provider_names(&self) -> Vec<String> {
        self.providers
            .iter()
            .map(|rp| rp.name.clone())
            .collect()
    }

    /// Get a provider by name
    pub fn get_provider(&self, name: &str) -> Option<&OidcProviderConfig> {
        self.providers
            .iter()
            .find(|rp| rp.name == name)
            .map(|rp| rp.provider.as_ref())
    }

    /// Count of registered providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a glob-style pattern to a regex.
///
/// Supported glob syntax:
/// - `**` → matches everything including `/` (any depth)
/// - `*` → matches single path segment (no `/`)
/// - `?` → matches single character
/// - Literal text → exact match
fn glob_to_regex(pattern: &str) -> Result<Regex> {
    let mut regex_str = String::from("^");

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // ** → match any sequence including /
            regex_str.push_str(".*");
            i += 2;
            // Skip trailing / after **
            if i < chars.len() && chars[i] == '/' {
                regex_str.push_str("/?");
                i += 1;
            }
        } else if chars[i] == '*' {
            // * → match any single segment (no /)
            regex_str.push_str("[^/]*");
            i += 1;
        } else if chars[i] == '?' {
            regex_str.push_str("[^/]");
            i += 1;
        } else {
            regex_str.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }

    regex_str.push('$');

    Regex::new(&regex_str).map_err(|e| anyhow!("Invalid glob pattern '{}': {}", pattern, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_provider(name: &str, route_filter: &str) -> OidcProviderConfig {
        OidcProviderConfig {
            name: name.to_string(),
            issuer_url: "https://auth.example.com".to_string(),
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            scopes: vec!["openid".to_string()],
            route_filter: Some(route_filter.to_string()),
            discovery: None,
            jwks: None,
            authorization_endpoint_override: None,
            token_endpoint_override: None,
            jwks_uri_override: None,
            userinfo_endpoint_override: None,
        }
    }

    #[test]
    fn test_glob_single_segment() {
        let router = {
            let mut r = ProviderRouter::new();
            r.add_provider(make_test_provider("corp", "/api/corp/*")).unwrap();
            r
        };

        assert!(router.resolve("/api/corp/users").is_some());
        assert!(router.resolve("/api/corp/orders").is_some());
        assert!(router.resolve("/api/corp/").is_some()); // * matches zero chars
        assert!(router.resolve("/api/partner/users").is_none());
    }

    #[test]
    fn test_glob_double_star() {
        let router = {
            let mut r = ProviderRouter::new();
            r.add_provider(make_test_provider("corp", "/api/corp/**")).unwrap();
            r
        };

        assert!(router.resolve("/api/corp/users").is_some());
        assert!(router.resolve("/api/corp/deep/nested/path").is_some());
        assert!(router.resolve("/api/corp/").is_some()); // /** matches everything under
        assert!(router.resolve("/api/partner/users").is_none());
    }

    #[test]
    fn test_glob_match_all() {
        let router = {
            let mut r = ProviderRouter::new();
            r.add_provider(make_test_provider("everywhere", "/**")).unwrap();
            r
        };

        assert!(router.resolve("/anything").is_some());
        assert!(router.resolve("/deeply/nested/path").is_some());
        assert!(router.resolve("/").is_some());
    }

    #[test]
    fn test_first_match_wins() {
        let mut router = ProviderRouter::new();
        router
            .add_provider(make_test_provider("specific", "/api/corp/specific/**"))
            .unwrap();
        router
            .add_provider(make_test_provider("general", "/api/corp/**"))
            .unwrap();

        // Both match, but "specific" was added first
        let resolved = router.resolve("/api/corp/specific/endpoint");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name, "specific");

        // Only "general" matches
        let resolved = router.resolve("/api/corp/other/endpoint");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name, "general");
    }

    #[test]
    fn test_default_provider() {
        let mut router = ProviderRouter::new();
        router
            .add_provider(make_test_provider("corp", "/api/corp/**"))
            .unwrap();
        router
            .add_provider(make_test_provider("default", "/**"))
            .unwrap();
        router.set_default_provider("default");

        // No match → default
        assert!(router.resolve("/api/other/stuff").is_some());
    }

    #[test]
    fn test_no_auth_provider() {
        let mut router = ProviderRouter::new();
        router
            .add_provider(make_test_provider("public", "/api/public/**"))
            .unwrap();
        router
            .add_provider(make_test_provider("corp", "/api/corp/**"))
            .unwrap();
        router.set_no_auth_provider(Some("public".to_string()));

        assert!(router.resolve("/api/public/data").is_none()); // No auth
        assert!(router.resolve("/api/corp/data").is_some()); // Needs auth
    }

    #[test]
    fn test_glob_question_mark() {
        let re = glob_to_regex("/api/v?").unwrap();
        assert!(re.is_match("/api/v1"));
        assert!(re.is_match("/api/v2"));
        assert!(!re.is_match("/api/v10")); // ? matches single char
        assert!(!re.is_match("/api/v1/users"));
    }
}
