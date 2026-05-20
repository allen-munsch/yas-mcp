//! OIDC configuration types — separate from legacy AppConfig for clean separation.
//!
//! These types define the Phase 1 OIDC Plug-and-Play configuration schema.

use serde::{Deserialize, Serialize};

/// Top-level OIDC configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OidcConfig {
    /// Session management settings
    #[serde(default)]
    pub session: OidcSessionConfig,

    /// JWT validation settings
    #[serde(default)]
    pub validation: OidcValidationConfig,

    /// Default provider name (used when no route filter matches)
    pub default_provider: Option<String>,

    /// No-auth provider name (routes matching this provider skip authentication)
    #[serde(default)]
    pub no_auth_provider: Option<String>,

    /// Provider definitions
    #[serde(default)]
    pub providers: Vec<OidcProviderEntry>,
}

/// Session lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcSessionConfig {
    /// Session TTL in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_session_ttl")]
    pub ttl: u64,

    /// Refresh buffer in seconds — refresh token when it's within this many seconds of expiry
    #[serde(default = "default_refresh_buffer")]
    pub refresh_buffer: u64,

    /// Max sessions per user
    #[serde(default = "default_max_sessions")]
    pub max_sessions_per_user: u64,
}

impl Default for OidcSessionConfig {
    fn default() -> Self {
        Self {
            ttl: default_session_ttl(),
            refresh_buffer: default_refresh_buffer(),
            max_sessions_per_user: default_max_sessions(),
        }
    }
}

fn default_session_ttl() -> u64 {
    3600
}
fn default_refresh_buffer() -> u64 {
    300
}
fn default_max_sessions() -> u64 {
    10
}

/// JWT validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcValidationConfig {
    /// Verify the issuer (iss) claim
    #[serde(default = "default_true")]
    pub verify_iss: bool,

    /// Verify the audience (aud) claim
    #[serde(default = "default_true")]
    pub verify_aud: bool,

    /// Verify the expiration (exp) claim
    #[serde(default = "default_true")]
    pub verify_exp: bool,

    /// Verify the not-before (nbf) claim
    #[serde(default = "default_false")]
    pub verify_nbf: bool,

    /// Clock skew tolerance in seconds
    #[serde(default = "default_leeway")]
    pub leeway: u64,
}

impl Default for OidcValidationConfig {
    fn default() -> Self {
        Self {
            verify_iss: true,
            verify_aud: true,
            verify_exp: true,
            verify_nbf: false,
            leeway: 60,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_leeway() -> u64 {
    60
}

/// A single OIDC provider entry from the configuration file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcProviderEntry {
    /// Human-readable provider name
    pub name: String,

    /// Issuer URL (used for OIDC discovery via .well-known)
    pub issuer_url: String,

    /// OAuth2 client ID
    pub client_id: String,

    /// OAuth2 client secret (should come from env var or secrets manager)
    pub client_secret: String,

    /// Requested OIDC scopes
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Glob route filter — determines which MCP tools this provider protects
    #[serde(default)]
    pub route_filter: Option<String>,

    /// Optional overrides (for providers without .well-known support)
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    pub userinfo_endpoint: Option<String>,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}
