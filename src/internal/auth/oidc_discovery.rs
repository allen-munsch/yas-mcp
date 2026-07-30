//! OIDC Discovery
//!
//! Auto-discovers OIDC provider endpoints from `/.well-known/openid-configuration`.
//! No more manual configuration of auth_url, token_url, jwks_uri, etc.
//!
//! # Usage
//!
//! ```yaml
//! oauth:
//!   enabled: true
//!   provider: oidc
//!   issuer_url: "https://accounts.google.com"   # ← just this
//!   client_id: "${OIDC_CLIENT_ID}"
//!   client_secret: "${OIDC_CLIENT_SECRET}"
//! ```
//!
//! The discovery document is fetched at startup and cached.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// A cached OIDC discovery document with expiry
#[derive(Debug, Clone)]
pub struct CachedDiscovery {
    pub document: OidcDiscovery,
    pub fetched_at: Instant,
    pub ttl: Duration,
}

impl CachedDiscovery {
    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

/// OIDC Discovery document from `/.well-known/openid-configuration`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,

    #[serde(rename = "authorization_endpoint")]
    pub authorization_endpoint: String,

    #[serde(rename = "token_endpoint")]
    pub token_endpoint: String,

    #[serde(rename = "userinfo_endpoint")]
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,

    #[serde(rename = "jwks_uri")]
    pub jwks_uri: String,

    #[serde(rename = "registration_endpoint")]
    #[serde(default)]
    pub registration_endpoint: Option<String>,

    #[serde(rename = "scopes_supported")]
    #[serde(default)]
    pub scopes_supported: Vec<String>,

    #[serde(rename = "response_types_supported")]
    #[serde(default)]
    pub response_types_supported: Vec<String>,

    #[serde(rename = "grant_types_supported")]
    #[serde(default)]
    pub grant_types_supported: Vec<String>,

    #[serde(rename = "subject_types_supported")]
    #[serde(default)]
    pub subject_types_supported: Vec<String>,

    #[serde(rename = "id_token_signing_alg_values_supported")]
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,

    #[serde(rename = "token_endpoint_auth_methods_supported")]
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,

    #[serde(rename = "claims_supported")]
    #[serde(default)]
    pub claims_supported: Vec<String>,

    /// Catch-all for provider-specific extensions
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl OidcDiscovery {
    /// Fetch the OIDC discovery document from an issuer URL.
    ///
    /// The standard path is `{issuer}/.well-known/openid-configuration`.
    /// Some providers (like Dex) put the path at `{issuer}/.well-known/openid-configuration`
    /// directly on the issuer URL.
    pub async fn fetch(issuer_url: &str) -> Result<Self> {
        let well_known_url = build_well_known_url(issuer_url);
        debug!("Fetching OIDC discovery from: {}", well_known_url);

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("yas-mcp-oidc-discovery/1.0")
            .build()
            .context("Failed to build HTTP client for OIDC discovery")?;

        let response = client
            .get(&well_known_url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch OIDC discovery from {well_known_url}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "OIDC discovery at {} returned HTTP {}",
                well_known_url,
                status
            ));
        }

        let doc: OidcDiscovery = response.json().await.with_context(|| {
            format!("Failed to parse OIDC discovery JSON from {well_known_url}")
        })?;

        // Validate required fields
        if doc.issuer.is_empty() {
            return Err(anyhow::anyhow!(
                "OIDC discovery document missing 'issuer' field"
            ));
        }
        if doc.authorization_endpoint.is_empty() {
            return Err(anyhow::anyhow!(
                "OIDC discovery document missing 'authorization_endpoint' field"
            ));
        }
        if doc.token_endpoint.is_empty() {
            return Err(anyhow::anyhow!(
                "OIDC discovery document missing 'token_endpoint' field"
            ));
        }
        if doc.jwks_uri.is_empty() {
            return Err(anyhow::anyhow!(
                "OIDC discovery document missing 'jwks_uri' field"
            ));
        }

        info!(
            "OIDC discovery successful — issuer: {}, auth_endpoint: {}, token_endpoint: {}, jwks_uri: {}",
            doc.issuer, doc.authorization_endpoint, doc.token_endpoint, doc.jwks_uri
        );

        // Log supported features
        if !doc.scopes_supported.is_empty() {
            debug!("Supported scopes: {:?}", doc.scopes_supported);
        }
        if !doc.grant_types_supported.is_empty() {
            debug!("Supported grant types: {:?}", doc.grant_types_supported);
        }
        debug!(
            "Signing algorithms: {:?}",
            doc.id_token_signing_alg_values_supported
        );

        Ok(doc)
    }

    /// Fetch and cache the discovery document
    pub async fn fetch_cached(issuer_url: &str, cache_ttl: Duration) -> Result<CachedDiscovery> {
        let document = Self::fetch(issuer_url).await?;
        Ok(CachedDiscovery {
            document,
            fetched_at: Instant::now(),
            ttl: cache_ttl,
        })
    }

    /// Check if a scope is supported by this provider
    pub fn supports_scope(&self, scope: &str) -> bool {
        self.scopes_supported.iter().any(|s| s == scope)
    }

    /// Check if a grant type is supported
    pub fn supports_grant_type(&self, grant_type: &str) -> bool {
        self.grant_types_supported.iter().any(|g| g == grant_type)
    }

    /// Check if a signing algorithm is supported (for JWKS/JWT validation)
    pub fn supports_signing_alg(&self, alg: &str) -> bool {
        self.id_token_signing_alg_values_supported
            .iter()
            .any(|a| a == alg)
    }
}

/// Build the well-known URL from an issuer URL.
///
/// Handles various issuer URL formats:
/// - `https://accounts.google.com` → `https://accounts.google.com/.well-known/openid-configuration`
/// - `https://accounts.google.com/` → `https://accounts.google.com/.well-known/openid-configuration`
/// - `http://dex:5556/dex` → `http://dex:5556/dex/.well-known/openid-configuration`
fn build_well_known_url(issuer: &str) -> String {
    let base = issuer.trim_end_matches('/');
    format!("{base}/.well-known/openid-configuration")
}

/// Configuration for the OIDC discovery client
#[derive(Debug, Clone)]
pub struct OidcDiscoveryConfig {
    /// The issuer URL (auto-discovered)
    pub issuer_url: String,
    /// How long to cache the discovery document (default: 1 hour)
    pub cache_ttl: Duration,
}

impl Default for OidcDiscoveryConfig {
    fn default() -> Self {
        Self {
            issuer_url: String::new(),
            cache_ttl: Duration::from_secs(3600),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_well_known_url_standard() {
        let url = build_well_known_url("https://accounts.google.com");
        assert_eq!(
            url,
            "https://accounts.google.com/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_build_well_known_url_trailing_slash() {
        let url = build_well_known_url("https://accounts.google.com/");
        assert_eq!(
            url,
            "https://accounts.google.com/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_build_well_known_url_with_path() {
        let url = build_well_known_url("http://dex:5556/dex");
        assert_eq!(url, "http://dex:5556/dex/.well-known/openid-configuration");
    }

    #[test]
    fn test_build_well_known_url_with_path_trailing_slash() {
        let url = build_well_known_url("https://keycloak:8080/realms/myrealm/");
        assert_eq!(
            url,
            "https://keycloak:8080/realms/myrealm/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_parse_discovery_document() {
        let json = serde_json::json!({
            "issuer": "https://auth.example.com",
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "userinfo_endpoint": "https://auth.example.com/userinfo",
            "jwks_uri": "https://auth.example.com/jwks",
            "scopes_supported": ["openid", "profile", "email"],
            "response_types_supported": ["code", "token", "id_token"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256", "ES256"]
        });

        let doc: OidcDiscovery = serde_json::from_value(json).unwrap();
        assert_eq!(doc.issuer, "https://auth.example.com");
        assert_eq!(
            doc.authorization_endpoint,
            "https://auth.example.com/authorize"
        );
        assert_eq!(doc.token_endpoint, "https://auth.example.com/token");
        assert_eq!(
            doc.userinfo_endpoint,
            Some("https://auth.example.com/userinfo".into())
        );
        assert_eq!(doc.jwks_uri, "https://auth.example.com/jwks");
        assert_eq!(doc.scopes_supported.len(), 3);
    }

    #[test]
    fn test_supports_scope() {
        let doc = OidcDiscovery {
            issuer: "https://example.com".into(),
            authorization_endpoint: "https://example.com/auth".into(),
            token_endpoint: "https://example.com/token".into(),
            jwks_uri: "https://example.com/jwks".into(),
            scopes_supported: vec!["openid".into(), "profile".into(), "email".into()],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            subject_types_supported: vec![],
            id_token_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            claims_supported: vec![],
            userinfo_endpoint: None,
            registration_endpoint: None,
            extra: Default::default(),
        };

        assert!(doc.supports_scope("openid"));
        assert!(doc.supports_scope("email"));
        assert!(!doc.supports_scope("offline_access"));
    }

    #[test]
    fn test_supports_grant_type() {
        let doc = OidcDiscovery {
            issuer: "https://example.com".into(),
            authorization_endpoint: "https://example.com/auth".into(),
            token_endpoint: "https://example.com/token".into(),
            jwks_uri: "https://example.com/jwks".into(),
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec!["authorization_code".into(), "refresh_token".into()],
            subject_types_supported: vec![],
            id_token_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            claims_supported: vec![],
            userinfo_endpoint: None,
            registration_endpoint: None,
            extra: Default::default(),
        };

        assert!(doc.supports_grant_type("authorization_code"));
        assert!(!doc.supports_grant_type("client_credentials"));
    }

    #[test]
    fn test_supports_signing_alg() {
        let doc = OidcDiscovery {
            issuer: "https://example.com".into(),
            authorization_endpoint: "https://example.com/auth".into(),
            token_endpoint: "https://example.com/token".into(),
            jwks_uri: "https://example.com/jwks".into(),
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            subject_types_supported: vec![],
            id_token_signing_alg_values_supported: vec!["RS256".into(), "ES256".into()],
            token_endpoint_auth_methods_supported: vec![],
            claims_supported: vec![],
            userinfo_endpoint: None,
            registration_endpoint: None,
            extra: Default::default(),
        };

        assert!(doc.supports_signing_alg("RS256"));
        assert!(!doc.supports_signing_alg("HS256"));
    }

    #[tokio::test]
    async fn test_fetch_nonexistent_issuer() {
        // This should fail gracefully
        let result = OidcDiscovery::fetch("http://localhost:19999/nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_expiry() {
        let doc = OidcDiscovery {
            issuer: "https://example.com".into(),
            authorization_endpoint: "https://example.com/auth".into(),
            token_endpoint: "https://example.com/token".into(),
            jwks_uri: "https://example.com/jwks".into(),
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            subject_types_supported: vec![],
            id_token_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            claims_supported: vec![],
            userinfo_endpoint: None,
            registration_endpoint: None,
            extra: Default::default(),
        };

        let cached = CachedDiscovery {
            document: doc,
            fetched_at: Instant::now() - Duration::from_secs(3601),
            ttl: Duration::from_secs(3600),
        };

        assert!(cached.is_expired());
    }

    #[test]
    fn test_cache_not_expired() {
        let doc = OidcDiscovery {
            issuer: "https://example.com".into(),
            authorization_endpoint: "https://example.com/auth".into(),
            token_endpoint: "https://example.com/token".into(),
            jwks_uri: "https://example.com/jwks".into(),
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            subject_types_supported: vec![],
            id_token_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            claims_supported: vec![],
            userinfo_endpoint: None,
            registration_endpoint: None,
            extra: Default::default(),
        };

        let cached = CachedDiscovery {
            document: doc,
            fetched_at: Instant::now(),
            ttl: Duration::from_secs(3600),
        };

        assert!(!cached.is_expired());
    }
}
