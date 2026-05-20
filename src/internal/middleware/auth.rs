//! Auth middleware — pluggable authentication for any upstream API.
//!
//! ## Built-in providers
//!
//! | Provider | Mechanism | Config |
//! |----------|-----------|--------|
//! | `oidc` | OpenID Connect (Phase 1) | issuer_url, client_id, client_secret |
//! | `oauth2` | OAuth2 authorization code / client credentials | provider, client_id, client_secret |
//! | `api_key` | Static API key in header/query | header_name, api_key |
//! | `basic` | HTTP Basic Auth | username, password |
//! | `bearer` | Static Bearer token | token |
//! | `mtls` | Mutual TLS client certificate | cert_path, key_path |
//! | `header_passthrough` | Forward original auth header | header_name |
//! | `custom` | User-provided implementation | (via trait impl) |
//!
//! ## Custom auth
//!
//! Orgs implement `AuthMiddleware` trait and register via config:
//!
//! ```yaml
//! auth:
//!   provider: custom
//!   custom:
//!     module: my_org_auth        # Rust module or WASM plugin name
//!     config:
//!       token_exchange_url: https://internal-auth.corp/validate
//!       header_map:
//!         X-Corp-Token: "{token}"
//!         X-Corp-User: "{user_id}"
//! ```
//!
//! The trait:
//! ```rust,ignore
//! #[async_trait]
//! pub trait AuthMiddleware: Send + Sync {
//!     /// Authenticate a request, return context with headers/tokens to forward
//!     async fn authenticate(&self, request: &AuthRequest) -> Result<AuthContext>;
//!     /// Refresh credentials if needed (called when upstream returns 401)
//!     async fn refresh(&self, context: &mut AuthContext) -> Result<()>;
//!     /// Validate an existing context is still valid
//!     async fn validate(&self, context: &AuthContext) -> Result<bool>;
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// ──────────────────────────────────────────────
//  Core types
// ──────────────────────────────────────────────

/// Information about an incoming MCP request that needs authentication.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    /// The MCP tool being called (e.g. "listUsers")
    pub tool_name: String,

    /// The upstream API path (e.g. "/api/v1/users")
    pub api_path: String,

    /// HTTP method (GET, POST, etc.)
    pub method: String,

    /// Original request headers from the MCP client
    pub incoming_headers: HashMap<String, String>,

    /// MCP session ID (if any)
    pub session_id: Option<String>,

    /// Raw request metadata
    pub metadata: HashMap<String, String>,
}

/// Result of successful authentication — what to forward to the upstream API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Headers to inject into the upstream request
    pub headers: HashMap<String, String>,

    /// Query parameters to inject
    pub query_params: HashMap<String, String>,

    /// Authenticated principal identifier
    pub principal: Option<String>,

    /// Opaque session token for this auth context
    pub session_token: Option<String>,

    /// When this context expires (epoch seconds)
    pub expires_at: Option<u64>,

    /// Provider that authenticated this request
    pub provider: String,

    /// Arbitrary provider-specific state
    pub extra: HashMap<String, String>,
}

impl AuthContext {
    /// Create an empty (unauthenticated) context
    pub fn anonymous() -> Self {
        Self {
            headers: HashMap::new(),
            query_params: HashMap::new(),
            principal: None,
            session_token: None,
            expires_at: None,
            provider: "none".to_string(),
            extra: HashMap::new(),
        }
    }

    /// Check if context is expired
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now >= exp
        } else {
            false
        }
    }
}

/// Enum of supported auth provider types (for config-driven selection)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthProviderType {
    Oidc,
    Oauth2,
    ApiKey,
    Basic,
    Bearer,
    Mtls,
    HeaderPassthrough,
    Custom,
    None,
}

impl fmt::Display for AuthProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthProviderType::Oidc => write!(f, "oidc"),
            AuthProviderType::Oauth2 => write!(f, "oauth2"),
            AuthProviderType::ApiKey => write!(f, "api_key"),
            AuthProviderType::Basic => write!(f, "basic"),
            AuthProviderType::Bearer => write!(f, "bearer"),
            AuthProviderType::Mtls => write!(f, "mtls"),
            AuthProviderType::HeaderPassthrough => write!(f, "header_passthrough"),
            AuthProviderType::Custom => write!(f, "custom"),
            AuthProviderType::None => write!(f, "none"),
        }
    }
}

// ──────────────────────────────────────────────
//  Auth Middleware trait
// ──────────────────────────────────────────────

/// The core trait orgs implement for custom authentication.
///
/// # Example (hypothetical org)
///
/// ```rust,ignore
/// struct CorpSsoMiddleware {
///     validation_url: String,
///     client: reqwest::Client,
/// }
///
/// #[async_trait]
/// impl AuthMiddleware for CorpSsoMiddleware {
///     async fn authenticate(&self, req: &AuthRequest) -> Result<AuthContext> {
///         let token = req.incoming_headers.get("X-Corp-Token")
///             .ok_or(anyhow!("Missing X-Corp-Token"))?;
///
///         // Validate with internal auth service
///         let resp = self.client
///             .post(&self.validation_url)
///             .json(&serde_json::json!({"token": token}))
///             .send().await?;
///
///         let claims: CorpClaims = resp.json().await?;
///
///         Ok(AuthContext {
///             headers: hashmap! {
///                 "Authorization" => format!("Bearer {}", claims.internal_token),
///                 "X-Corp-User" => claims.user_id,
///             },
///             principal: Some(claims.user_id),
///             expires_at: Some(claims.exp),
///             provider: "corp-sso".into(),
///             ..Default::default()
///         })
///     }
///
///     async fn refresh(&self, ctx: &mut AuthContext) -> Result<()> { ... }
///     async fn validate(&self, ctx: &AuthContext) -> Result<bool> { ... }
/// }
/// ```
#[async_trait]
pub trait AuthMiddleware: Send + Sync {
    /// Authenticate an incoming request.
    ///
    /// Returns an `AuthContext` with headers/tokens to forward to the upstream API.
    /// If authentication fails, return an error with a descriptive message.
    async fn authenticate(&self, request: &AuthRequest) -> Result<AuthContext, AuthError>;

    /// Refresh authentication context.
    ///
    /// Called when the upstream API returns 401 or the context is expired.
    /// Should obtain fresh credentials and update the context in place.
    async fn refresh(&self, context: &mut AuthContext) -> Result<(), AuthError>;

    /// Validate that an existing auth context is still valid.
    ///
    /// Used for pre-flight checks before making upstream calls.
    /// Returns `true` if the context is valid and can be used.
    async fn validate(&self, context: &AuthContext) -> Result<bool, AuthError>;

    /// Optional: called once during server startup for initialization.
    async fn initialize(&self) -> Result<(), AuthError> {
        Ok(())
    }

    /// Optional: called during graceful shutdown.
    async fn shutdown(&self) -> Result<(), AuthError> {
        Ok(())
    }
}

// ──────────────────────────────────────────────
//  Auth Error
// ──────────────────────────────────────────────

/// Structured auth error with metadata for diagnostics and client response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthError {
    /// Machine-readable error code
    pub code: AuthErrorCode,

    /// Human-readable message
    pub message: String,

    /// Whether this error is retryable
    pub retryable: bool,

    /// Suggested HTTP status code for the MCP response
    pub http_status: u16,

    /// Additional context (e.g. which provider failed, missing scopes)
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthErrorCode {
    Unauthorized,
    Forbidden,
    Expired,
    InvalidToken,
    ProviderUnavailable,
    ConfigurationError,
    RateLimited,
    Unknown,
}

impl fmt::Display for AuthErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthErrorCode::Unauthorized => write!(f, "unauthorized"),
            AuthErrorCode::Forbidden => write!(f, "forbidden"),
            AuthErrorCode::Expired => write!(f, "expired"),
            AuthErrorCode::InvalidToken => write!(f, "invalid_token"),
            AuthErrorCode::ProviderUnavailable => write!(f, "provider_unavailable"),
            AuthErrorCode::ConfigurationError => write!(f, "configuration_error"),
            AuthErrorCode::RateLimited => write!(f, "rate_limited"),
            AuthErrorCode::Unknown => write!(f, "unknown"),
        }
    }
}

impl AuthError {
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            code: AuthErrorCode::Unauthorized,
            message: msg.into(),
            retryable: false,
            http_status: 401,
            details: HashMap::new(),
        }
    }

    pub fn expired(msg: impl Into<String>) -> Self {
        Self {
            code: AuthErrorCode::Expired,
            message: msg.into(),
            retryable: true,
            http_status: 401,
            details: HashMap::new(),
        }
    }

    pub fn provider_unavailable(msg: impl Into<String>) -> Self {
        Self {
            code: AuthErrorCode::ProviderUnavailable,
            message: msg.into(),
            retryable: true,
            http_status: 502,
            details: HashMap::new(),
        }
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (retryable={}, http={})",
            self.code, self.message, self.retryable, self.http_status
        )
    }
}

impl std::error::Error for AuthError {}

// Convenience conversion from our error types
impl From<anyhow::Error> for AuthError {
    fn from(e: anyhow::Error) -> Self {
        AuthError {
            code: AuthErrorCode::Unknown,
            message: e.to_string(),
            retryable: false,
            http_status: 500,
            details: HashMap::new(),
        }
    }
}

// ──────────────────────────────────────────────
//  Built-in auth middleware implementations
// ──────────────────────────────────────────────

/// API Key auth — injects a static key into a header or query parameter.
pub struct ApiKeyMiddleware {
    header_name: String,
    api_key: String,
    query_param: Option<String>,
}

impl ApiKeyMiddleware {
    pub fn new(header_name: String, api_key: String, query_param: Option<String>) -> Self {
        Self {
            header_name,
            api_key,
            query_param,
        }
    }
}

#[async_trait]
impl AuthMiddleware for ApiKeyMiddleware {
    async fn authenticate(&self, _request: &AuthRequest) -> Result<AuthContext, AuthError> {
        let mut headers = HashMap::new();
        headers.insert(self.header_name.clone(), self.api_key.clone());

        let mut query_params = HashMap::new();
        if let Some(ref param) = self.query_param {
            query_params.insert(param.clone(), self.api_key.clone());
        }

        Ok(AuthContext {
            headers,
            query_params,
            principal: Some(format!("apikey:{}", &self.api_key[..8.min(self.api_key.len())])),
            session_token: None,
            expires_at: None,
            provider: "api_key".to_string(),
            extra: HashMap::new(),
        })
    }

    async fn refresh(&self, _context: &mut AuthContext) -> Result<(), AuthError> {
        // API keys don't refresh — rotate manually
        Ok(())
    }

    async fn validate(&self, _context: &AuthContext) -> Result<bool, AuthError> {
        Ok(true) // Static key is always "valid"
    }
}

/// Basic Auth middleware — injects Authorization: Basic header.
pub struct BasicAuthMiddleware {
    username: String,
    password: String,
}

impl BasicAuthMiddleware {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

#[async_trait]
impl AuthMiddleware for BasicAuthMiddleware {
    async fn authenticate(&self, _request: &AuthRequest) -> Result<AuthContext, AuthError> {
        let encoded = base64_encode(&format!("{}:{}", self.username, self.password));
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), format!("Basic {}", encoded));

        Ok(AuthContext {
            headers,
            query_params: HashMap::new(),
            principal: Some(self.username.clone()),
            session_token: None,
            expires_at: None,
            provider: "basic".to_string(),
            extra: HashMap::new(),
        })
    }

    async fn refresh(&self, _context: &mut AuthContext) -> Result<(), AuthError> {
        Ok(())
    }

    async fn validate(&self, _context: &AuthContext) -> Result<bool, AuthError> {
        Ok(true)
    }
}

/// Bearer token auth — injects a static Bearer token.
pub struct BearerTokenMiddleware {
    token: String,
}

impl BearerTokenMiddleware {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

#[async_trait]
impl AuthMiddleware for BearerTokenMiddleware {
    async fn authenticate(&self, _request: &AuthRequest) -> Result<AuthContext, AuthError> {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", self.token),
        );

        Ok(AuthContext {
            headers,
            query_params: HashMap::new(),
            principal: None,
            session_token: None,
            expires_at: None,
            provider: "bearer".to_string(),
            extra: HashMap::new(),
        })
    }

    async fn refresh(&self, _context: &mut AuthContext) -> Result<(), AuthError> {
        Ok(())
    }

    async fn validate(&self, _context: &AuthContext) -> Result<bool, AuthError> {
        Ok(true)
    }
}

/// Header passthrough — forward an auth header from the MCP request to the upstream.
pub struct HeaderPassthroughMiddleware {
    header_name: String,
    prefix: Option<String>,
}

impl HeaderPassthroughMiddleware {
    pub fn new(header_name: String, prefix: Option<String>) -> Self {
        Self {
            header_name,
            prefix,
        }
    }
}

#[async_trait]
impl AuthMiddleware for HeaderPassthroughMiddleware {
    async fn authenticate(&self, request: &AuthRequest) -> Result<AuthContext, AuthError> {
        let value = request
            .incoming_headers
            .get(&self.header_name)
            .cloned()
            .ok_or_else(|| {
                AuthError::unauthorized(format!(
                    "Missing required header: {}",
                    self.header_name
                ))
            })?;

        let mut headers = HashMap::new();

        if let Some(ref prefix) = self.prefix {
            // If a prefix is configured, strip it from the incoming value
            // and apply it ourselves (e.g. strip "Bearer " and re-add)
            let stripped = value.strip_prefix(prefix).unwrap_or(&value);
            headers.insert(
                self.header_name.clone(),
                format!("{}{}", prefix, stripped),
            );
        } else {
            headers.insert(self.header_name.clone(), value);
        }

        Ok(AuthContext {
            headers,
            query_params: HashMap::new(),
            principal: None,
            session_token: None,
            expires_at: None,
            provider: "header_passthrough".to_string(),
            extra: HashMap::new(),
        })
    }

    async fn refresh(&self, _context: &mut AuthContext) -> Result<(), AuthError> {
        Ok(())
    }

    async fn validate(&self, _context: &AuthContext) -> Result<bool, AuthError> {
        Ok(true)
    }
}

// ──────────────────────────────────────────────
//  Auth middleware factory
// ──────────────────────────────────────────────

/// Configuration for auth middleware (from config.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMiddlewareConfig {
    /// Provider type
    #[serde(rename = "type")]
    pub provider_type: AuthProviderType,

    /// Provider-specific config
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,

    /// Route filter — which tools this middleware applies to
    #[serde(default)]
    pub route_filter: Option<String>,
}

/// Create an auth middleware from configuration.
///
/// This is the factory that maps config → concrete middleware implementation.
pub fn create_auth_middleware(
    config: &AuthMiddlewareConfig,
) -> Result<Arc<dyn AuthMiddleware>, AuthError> {
    match config.provider_type {
        AuthProviderType::ApiKey => {
            let header_name = get_config_string(&config.config, "header_name", "X-API-Key");
            let api_key = get_config_string_required(&config.config, "api_key")?;
            let query_param = get_config_optional_string(&config.config, "query_param");
            Ok(Arc::new(ApiKeyMiddleware::new(
                header_name, api_key, query_param,
            )))
        }
        AuthProviderType::Basic => {
            let username = get_config_string_required(&config.config, "username")?;
            let password = get_config_string_required(&config.config, "password")?;
            Ok(Arc::new(BasicAuthMiddleware::new(username, password)))
        }
        AuthProviderType::Bearer => {
            let token = get_config_string_required(&config.config, "token")?;
            Ok(Arc::new(BearerTokenMiddleware::new(token)))
        }
        AuthProviderType::HeaderPassthrough => {
            let header_name =
                get_config_string(&config.config, "header_name", "Authorization");
            let prefix = get_config_optional_string(&config.config, "prefix");
            Ok(Arc::new(HeaderPassthroughMiddleware::new(
                header_name, prefix,
            )))
        }
        AuthProviderType::Oidc | AuthProviderType::Oauth2 => {
            // These are handled by the Phase 1 OIDC subsystem
            Err(AuthError {
                code: AuthErrorCode::ConfigurationError,
                message: format!(
                    "{} auth is handled by the OIDC subsystem, not middleware factory",
                    config.provider_type
                ),
                retryable: false,
                http_status: 500,
                details: HashMap::new(),
            })
        }
        AuthProviderType::Custom => {
            // Custom middleware — loaded by name from plugin registry
            let module_name =
                get_config_string_required(&config.config, "module")
                    .map_err(|_| AuthError {
                        code: AuthErrorCode::ConfigurationError,
                        message: "Custom auth requires 'module' field".into(),
                        retryable: false,
                        http_status: 500,
                        details: HashMap::new(),
                    })?;

            Err(AuthError {
                code: AuthErrorCode::ConfigurationError,
                message: format!(
                    "Custom auth module '{}' not yet loaded. Plugins are loaded at startup.",
                    module_name
                ),
                retryable: false,
                http_status: 500,
                details: HashMap::new(),
            })
        }
        AuthProviderType::Mtls => {
            Err(AuthError {
                code: AuthErrorCode::ConfigurationError,
                message: "mTLS auth requires certificate configuration".into(),
                retryable: false,
                http_status: 500,
                details: HashMap::new(),
            })
        }
        AuthProviderType::None => {
            // No-op middleware
            struct NoOpMiddleware;
            #[async_trait]
            impl AuthMiddleware for NoOpMiddleware {
                async fn authenticate(
                    &self,
                    _request: &AuthRequest,
                ) -> Result<AuthContext, AuthError> {
                    Ok(AuthContext::anonymous())
                }
                async fn refresh(
                    &self,
                    _context: &mut AuthContext,
                ) -> Result<(), AuthError> {
                    Ok(())
                }
                async fn validate(
                    &self,
                    _context: &AuthContext,
                ) -> Result<bool, AuthError> {
                    Ok(true)
                }
            }
            Ok(Arc::new(NoOpMiddleware))
        }
    }
}

// ──────────────────────────────────────────────
//  Config helpers
// ──────────────────────────────────────────────

fn get_config_string(
    config: &HashMap<String, serde_json::Value>,
    key: &str,
    default: &str,
) -> String {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn get_config_optional_string(
    config: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn get_config_string_required(
    config: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<String, AuthError> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AuthError {
            code: AuthErrorCode::ConfigurationError,
            message: format!("Missing required config key: '{}'", key),
            retryable: false,
            http_status: 500,
            details: HashMap::new(),
        })
}

fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity(((bytes.len() + 2) / 3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

// ──────────────────────────────────────────────
//  Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_key_middleware() {
        let mw = ApiKeyMiddleware::new("X-API-Key".into(), "secret123".into(), None);
        let req = AuthRequest {
            tool_name: "test".into(),
            api_path: "/api/test".into(),
            method: "GET".into(),
            incoming_headers: HashMap::new(),
            session_id: None,
            metadata: HashMap::new(),
        };

        let ctx = mw.authenticate(&req).await.unwrap();
        assert_eq!(ctx.headers.get("X-API-Key").unwrap(), "secret123");
        assert!(ctx.principal.unwrap().contains("apikey:"));
    }

    #[tokio::test]
    async fn test_basic_auth_middleware() {
        let mw = BasicAuthMiddleware::new("admin".into(), "pass123".into());
        let req = AuthRequest {
            tool_name: "test".into(),
            api_path: "/api/test".into(),
            method: "GET".into(),
            incoming_headers: HashMap::new(),
            session_id: None,
            metadata: HashMap::new(),
        };

        let ctx = mw.authenticate(&req).await.unwrap();
        let auth = ctx.headers.get("Authorization").unwrap();
        assert!(auth.starts_with("Basic "));
        assert_eq!(ctx.principal.unwrap(), "admin");
    }

    #[tokio::test]
    async fn test_bearer_token_middleware() {
        let mw = BearerTokenMiddleware::new("tok_deadbeef".into());
        let req = AuthRequest {
            tool_name: "test".into(),
            api_path: "/api/test".into(),
            method: "GET".into(),
            incoming_headers: HashMap::new(),
            session_id: None,
            metadata: HashMap::new(),
        };

        let ctx = mw.authenticate(&req).await.unwrap();
        assert_eq!(
            ctx.headers.get("Authorization").unwrap(),
            "Bearer tok_deadbeef"
        );
    }

    #[tokio::test]
    async fn test_header_passthrough() {
        let mw = HeaderPassthroughMiddleware::new("Authorization".into(), Some("Bearer ".into()));
        let mut incoming = HashMap::new();
        incoming.insert("Authorization".into(), "Bearer abc123".into());

        let req = AuthRequest {
            tool_name: "test".into(),
            api_path: "/api/test".into(),
            method: "GET".into(),
            incoming_headers: incoming,
            session_id: None,
            metadata: HashMap::new(),
        };

        let ctx = mw.authenticate(&req).await.unwrap();
        assert_eq!(
            ctx.headers.get("Authorization").unwrap(),
            "Bearer abc123"
        );
    }

    #[tokio::test]
    async fn test_header_passthrough_missing_header() {
        let mw = HeaderPassthroughMiddleware::new("X-Custom-Auth".into(), None);
        let req = AuthRequest {
            tool_name: "test".into(),
            api_path: "/api/test".into(),
            method: "GET".into(),
            incoming_headers: HashMap::new(),
            session_id: None,
            metadata: HashMap::new(),
        };

        let result = mw.authenticate(&req).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, AuthErrorCode::Unauthorized);
    }

    #[test]
    fn test_auth_context_expiry() {
        let ctx = AuthContext {
            expires_at: Some(0), // 1970 — definitely expired
            ..AuthContext::anonymous()
        };
        assert!(ctx.is_expired());

        let ctx = AuthContext {
            expires_at: Some(u64::MAX),
            ..AuthContext::anonymous()
        };
        assert!(!ctx.is_expired());
    }

    #[test]
    fn test_auth_error_formatting() {
        let err = AuthError::unauthorized("bad token")
            .with_detail("provider", "corp-sso")
            .with_detail("reason", "expired");
        assert!(err.to_string().contains("bad token"));
        assert!(err.to_string().contains("401"));
    }
}
