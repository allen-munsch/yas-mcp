//! WIMSE workload identity validation for yas-mcp token exchange.
//!
//! Implements the validation half of draft-ietf-wimse-arch:
//! YAS-MCP is the token exchange point — it validates platform JWTs
//! issued by the Weft control plane and exchanges them for OAuth2
//! access tokens to external resources.
//!
//! This module is a self-contained port of weft-identity's validator,
//! without the weft-vault dependency (which is only needed for issuance).
//!
//! # Flow
//!
//! ```text
//! Sandbox → Platform JWT → YAS-MCP (/api/auth/exchange)
//!   │                           │
//!   │  1. POST {jwt, audience}  │
//!   │──────────────────────────▶│
//!   │                           │ 2. IdentityValidator::validate(jwt)
//!   │                           │ 3. Check audience matches
//!   │                           │ 4. Look up OAuth2 token
//!   │  5. {access_token, ...}   │
//!   │◀──────────────────────────│
//!   ▼
//! External Resource (Gmail, Slack, GitHub...)
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};
use uuid::Uuid;

// ── Core Types ──────────────────────────────────────────────────────

/// A target audience for a platform identity JWT.
///
/// Each audience represents a specific purpose. A token audienced for
/// the YAS-MCP token exchange MUST NOT be used to directly access
/// platform resources.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    /// Exchange at YAS-MCP for an OAuth2 access token (federation step)
    TokenExchange,

    /// Direct access to a specific connector (e.g., "gmail", "slack")
    Connector(String),

    /// Platform-internal access (e.g., MosaicDB, FlowEngine status)
    PlatformInternal,

    /// Custom audience string
    Custom(String),
}

impl Audience {
    /// Parse an audience from its string representation (JWT `aud` claim).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "weft:token-exchange" => Audience::TokenExchange,
            "weft:platform-internal" => Audience::PlatformInternal,
            other if other.starts_with("weft:connector:") => {
                let connector = other.strip_prefix("weft:connector:").unwrap_or("unknown");
                Audience::Connector(connector.to_string())
            }
            other => Audience::Custom(other.to_string()),
        }
    }

    /// Return the audience as a string for JWT `aud` claim.
    pub fn as_str(&self) -> String {
        match self {
            Audience::TokenExchange => "weft:token-exchange".into(),
            Audience::Connector(name) => format!("weft:connector:{name}"),
            Audience::PlatformInternal => "weft:platform-internal".into(),
            Audience::Custom(s) => s.clone(),
        }
    }

    /// Check if this audience allows federation (token exchange).
    pub fn allows_federation(&self) -> bool {
        matches!(self, Audience::TokenExchange | Audience::Connector(_))
    }

    /// Extract the connector name if this is a connector audience.
    pub fn connector_name(&self) -> Option<&str> {
        match self {
            Audience::Connector(name) => Some(name.as_str()),
            _ => None,
        }
    }
}

impl std::fmt::Display for Audience {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Permission scope for an audience — what the token bearer is allowed to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    EmailRead,
    EmailSend,
    EmailDelete,
    CalendarRead,
    CalendarWrite,
    SlackRead,
    SlackWrite,
    JiraRead,
    JiraCreate,
    JiraDelete,
    GithubRead,
    GithubWrite,
    FilesRead,
    FilesWrite,
    BrowserNavigate,
    BrowserExtract,
    SandboxExec,
    All,
}

impl Scope {
    pub fn as_str(&self) -> &str {
        match self {
            Scope::EmailRead => "email:read",
            Scope::EmailSend => "email:send",
            Scope::EmailDelete => "email:delete",
            Scope::CalendarRead => "calendar:read",
            Scope::CalendarWrite => "calendar:write",
            Scope::SlackRead => "slack:read",
            Scope::SlackWrite => "slack:write",
            Scope::JiraRead => "jira:read",
            Scope::JiraCreate => "jira:create",
            Scope::JiraDelete => "jira:delete",
            Scope::GithubRead => "github:read",
            Scope::GithubWrite => "github:write",
            Scope::FilesRead => "files:read",
            Scope::FilesWrite => "files:write",
            Scope::BrowserNavigate => "browser:navigate",
            Scope::BrowserExtract => "browser:extract",
            Scope::SandboxExec => "sandbox:exec",
            Scope::All => "*",
        }
    }
}

/// Whether an action was delegated by a user or initiated autonomously by an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum AutonomyLevel {
    #[default]
    Delegated,
    Autonomous,
}

/// One hop in a delegation chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationHop {
    pub agent_id: String,
    pub scope_in: Scope,
    pub scope_out: Scope,
    pub autonomy: AutonomyLevel,
}

/// Ordered chain of delegation hops from the original principal to the current agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationChain {
    pub hops: Vec<DelegationHop>,
}

impl DelegationChain {
    pub fn root_principal(&self) -> Option<&str> {
        self.hops.first().map(|h| h.agent_id.as_str())
    }

    pub fn current_agent(&self) -> Option<&str> {
        self.hops.last().map(|h| h.agent_id.as_str())
    }

    pub fn depth(&self) -> usize {
        self.hops.len()
    }

    pub fn has_autonomous_action(&self) -> bool {
        self.hops
            .iter()
            .any(|h| h.autonomy == AutonomyLevel::Autonomous)
    }

    pub fn effective_scope(&self) -> Scope {
        self.hops
            .last()
            .map(|h| h.scope_out.clone())
            .unwrap_or(Scope::All)
    }
}

/// A decoded platform identity JWT — proves a workload's identity within the Weft trust domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityToken {
    /// Unique token identifier (jti claim)
    pub jti: Uuid,

    /// Issuer — the Weft control plane (iss claim)
    pub iss: String,

    /// Subject — the agent this token represents (sub claim)
    pub sub: String,

    /// Intended audience for this token (aud claim)
    pub aud: Audience,

    /// When this token was issued (iat claim)
    pub iat: DateTime<Utc>,

    /// When this token expires (exp claim)
    pub exp: DateTime<Utc>,

    /// The sandbox this token is bound to
    pub sandbox_id: String,

    /// Trust domain identifier
    pub trust_domain: String,

    /// Delegation provenance chain — who authorized this agent
    pub delegation_chain: DelegationChain,
}

impl IdentityToken {
    /// Check if this token is expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.exp
    }
}

// ── Validator ───────────────────────────────────────────────────────

/// Validates platform identity JWTs for a specific trust domain.
///
/// Trust boundaries (YAS-MCP, connectors) use this to verify that
/// an incoming platform JWT is authentic, unexpired, and correctly
/// scoped to the requested audience.
pub struct IdentityValidator {
    /// Trust domain this validator belongs to
    trust_domain: String,

    /// Expected issuer URL
    expected_issuer: String,

    /// HMAC signing key bytes
    signing_key: Vec<u8>,
}

impl IdentityValidator {
    /// Create a new validator with the given trust domain and signing key.
    pub fn new(trust_domain: &str, signing_key: &[u8]) -> Self {
        Self {
            trust_domain: trust_domain.to_string(),
            expected_issuer: format!("https://{trust_domain}"),
            signing_key: signing_key.to_vec(),
        }
    }

    /// Validate a raw JWT string and return the decoded IdentityToken.
    /// Returns an error if the token is invalid, expired, or has a wrong issuer.
    pub fn validate(&self, jwt: &str) -> Result<IdentityToken> {
        let decoding_key = DecodingKey::from_secret(&self.signing_key);

        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_issuer(&[&self.expected_issuer]);
        validation.set_required_spec_claims(&["iss", "sub", "exp", "jti"]);
        validation.validate_aud = false; // We validate audience manually below
        validation.leeway = 0; // Strict expiry — no clock skew tolerance

        let token_data = decode::<Value>(jwt, &decoding_key, &validation)
            .context("failed to decode or validate JWT")?;

        let claims = token_data.claims;

        // Extract standard claims
        let sub = claims["sub"]
            .as_str()
            .context("missing sub claim")?
            .to_string();
        let jti = claims["jti"]
            .as_str()
            .context("missing jti claim")?
            .to_string();
        let iss = claims["iss"]
            .as_str()
            .context("missing iss claim")?
            .to_string();
        let aud_raw = claims["aud"].as_str().unwrap_or("unknown");
        let iat = claims["iat"].as_i64().context("missing iat claim")?;
        let exp = claims["exp"].as_i64().context("missing exp claim")?;

        // Extract Weft-specific claims
        let weft = claims
            .get("weft")
            .context("missing weft claim — not a Weft identity token")?;

        let sandbox_id = weft["sandbox_id"]
            .as_str()
            .context("missing weft.sandbox_id")?
            .to_string();

        let trust_domain = weft["trust_domain"]
            .as_str()
            .context("missing weft.trust_domain")?
            .to_string();

        let delegation_chain: DelegationChain =
            serde_json::from_value(weft["delegation_chain"].clone())
                .context("failed to parse delegation_chain")?;

        // Parse audience
        let aud = Audience::from_str(aud_raw);

        let token = IdentityToken {
            jti: Uuid::parse_str(&jti).context("invalid jti UUID")?,
            iss,
            sub,
            aud,
            iat: DateTime::from_timestamp(iat, 0).context("invalid iat timestamp")?,
            exp: DateTime::from_timestamp(exp, 0).context("invalid exp timestamp")?,
            sandbox_id,
            trust_domain,
            delegation_chain,
        };

        debug!(
            agent = %token.sub,
            sandbox = %token.sandbox_id,
            audience = %token.aud,
            "validated identity token"
        );

        Ok(token)
    }

    /// Validate and also check that the token is for the expected audience.
    pub fn validate_for_audience(
        &self,
        jwt: &str,
        expected_audience: &Audience,
    ) -> Result<IdentityToken> {
        let token = self.validate(jwt)?;

        if token.aud != *expected_audience {
            warn!(
                actual = %token.aud,
                expected = %expected_audience,
                "audience mismatch"
            );
            anyhow::bail!(
                "audience mismatch: token is for {}, but {} was expected",
                token.aud,
                expected_audience
            );
        }

        Ok(token)
    }

    /// Get the trust domain for this validator.
    pub fn trust_domain(&self) -> &str {
        &self.trust_domain
    }
}

// ── Exchange Request/Response ──────────────────────────────────────

/// Request body for the token exchange endpoint.
#[derive(Debug, Deserialize)]
pub struct TokenExchangeRequest {
    /// The platform identity JWT to validate
    pub platform_jwt: String,

    /// The desired audience for the exchange (e.g., "weft:connector:gmail")
    pub audience: String,
}

/// Response body for the token exchange endpoint.
#[derive(Debug, Serialize)]
pub struct TokenExchangeResponse {
    /// The OAuth2 access token for the target connector
    pub access_token: String,

    /// Token type (always "Bearer")
    pub token_type: String,

    /// Seconds until the access token expires
    pub expires_in: i64,

    /// The connector this token is for
    pub connector: String,

    /// The agent that requested the exchange
    pub agent_id: String,

    /// The sandbox the agent is running in
    pub sandbox_id: String,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    /// Helper: create a minimal valid JWT for testing
    fn create_test_jwt(
        signing_key: &[u8],
        issuer: &str,
        audience: &str,
        sandbox_id: &str,
        agent_id: &str,
        lifetime_minutes: i64,
    ) -> String {
        let now = Utc::now();
        let claims = serde_json::json!({
            "jti": Uuid::new_v4().to_string(),
            "iss": issuer,
            "sub": format!("agent:{agent_id}"),
            "aud": audience,
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::minutes(lifetime_minutes)).timestamp(),
            "weft": {
                "sandbox_id": sandbox_id,
                "trust_domain": "test.weft.local",
                "delegation_chain": {
                    "hops": [{
                        "agent_id": "user-test",
                        "scope_in": "All",
                        "scope_out": "All",
                        "autonomy": "delegated"
                    }]
                }
            }
        });

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        encode(&header, &claims, &EncodingKey::from_secret(signing_key))
            .expect("failed to create test JWT")
    }

    #[test]
    fn validates_valid_token() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let jwt = create_test_jwt(
            key,
            "https://test.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            5,
        );

        let validator = IdentityValidator::new("test.weft.local", key);
        let result = validator.validate(&jwt);
        assert!(
            result.is_ok(),
            "expected valid token, got: {:?}",
            result.err()
        );

        let token = result.unwrap();
        assert_eq!(token.sub, "agent:agent-1");
        assert_eq!(token.sandbox_id, "sandbox-abc");
        assert_eq!(token.aud, Audience::Connector("gmail".into()));
    }

    #[test]
    fn rejects_expired_token() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let jwt = create_test_jwt(
            key,
            "https://test.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            -1,
        );

        let validator = IdentityValidator::new("test.weft.local", key);
        let result = validator.validate(&jwt);
        assert!(result.is_err(), "expected expired token to be rejected");
    }

    #[test]
    fn rejects_wrong_audience() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let jwt = create_test_jwt(
            key,
            "https://test.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            5,
        );

        let validator = IdentityValidator::new("test.weft.local", key);
        let result = validator.validate_for_audience(&jwt, &Audience::Connector("slack".into()));
        assert!(result.is_err(), "expected wrong audience to be rejected");
    }

    #[test]
    fn rejects_wrong_signing_key() {
        let key_a = b"test-signing-key-a-32-bytes!!";
        let key_b = b"test-signing-key-b-32-bytes!!";
        let jwt = create_test_jwt(
            key_a,
            "https://test.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            5,
        );

        let validator = IdentityValidator::new("test.weft.local", key_b);
        let result = validator.validate(&jwt);
        assert!(result.is_err(), "expected wrong key to be rejected");
    }

    #[test]
    fn rejects_wrong_issuer() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let jwt = create_test_jwt(
            key,
            "https://evil.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            5,
        );

        let validator = IdentityValidator::new("test.weft.local", key);
        let result = validator.validate(&jwt);
        assert!(result.is_err(), "expected wrong issuer to be rejected");
    }

    #[test]
    fn rejects_missing_weft_claim() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let now = Utc::now();
        let claims = serde_json::json!({
            "jti": Uuid::new_v4().to_string(),
            "iss": "https://test.weft.local",
            "sub": "agent:agent-1",
            "aud": "weft:connector:gmail",
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::minutes(5)).timestamp(),
            // no "weft" claim
        });

        let jwt = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(key),
        )
        .expect("failed to create test JWT");

        let validator = IdentityValidator::new("test.weft.local", key);
        let result = validator.validate(&jwt);
        assert!(
            result.is_err(),
            "expected missing weft claim to be rejected"
        );
        assert!(
            result.unwrap_err().to_string().contains("weft"),
            "error should mention missing weft claim"
        );
    }

    #[test]
    fn audience_parsing() {
        assert_eq!(
            Audience::from_str("weft:token-exchange"),
            Audience::TokenExchange
        );
        assert_eq!(
            Audience::from_str("weft:connector:gmail"),
            Audience::Connector("gmail".into())
        );
        assert_eq!(
            Audience::from_str("weft:connector:slack"),
            Audience::Connector("slack".into())
        );
        assert_eq!(
            Audience::from_str("weft:platform-internal"),
            Audience::PlatformInternal
        );
        assert_eq!(
            Audience::from_str("custom-audience"),
            Audience::Custom("custom-audience".into())
        );
    }

    #[test]
    fn audience_connector_name() {
        let aud = Audience::Connector("gmail".into());
        assert_eq!(aud.connector_name(), Some("gmail"));

        let aud = Audience::TokenExchange;
        assert_eq!(aud.connector_name(), None);

        let aud = Audience::PlatformInternal;
        assert_eq!(aud.connector_name(), None);
    }

    #[test]
    fn token_exchange_flow() {
        let key = b"test-signing-key-32-bytes!!!!!!";
        let jwt = create_test_jwt(
            key,
            "https://test.weft.local",
            "weft:connector:gmail",
            "sandbox-abc",
            "agent-1",
            5,
        );

        // Step 1: validate the platform JWT
        let validator = IdentityValidator::new("test.weft.local", key);
        let token = validator
            .validate_for_audience(&jwt, &Audience::Connector("gmail".into()))
            .expect("validation should succeed");

        // Step 2: extract connector name
        let connector = token.aud.connector_name().expect("should have connector");

        // Step 3: verify agent identity
        assert_eq!(connector, "gmail");
        assert_eq!(token.sub, "agent:agent-1");
        assert_eq!(token.sandbox_id, "sandbox-abc");
        assert!(!token.is_expired());
        assert_eq!(token.delegation_chain.depth(), 1);
    }
}
