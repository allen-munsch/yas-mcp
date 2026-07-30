//! Secret Resolver Trait
//!
//! Defines the interface for resolving secret references to their values.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// A reference to a secret, parsed from a `scheme://path` string.
///
/// # Examples
///
/// ```text
/// env://MY_SECRET           → scheme="env", path="MY_SECRET", key=None
/// file:///run/secrets/token → scheme="file", path="/run/secrets/token", key=None
/// vault://secret/data/app#api_key → scheme="vault", path="secret/data/app", key=Some("api_key")
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    /// The scheme (e.g., "env", "file", "vault", "aws-secretsmanager")
    pub scheme: String,
    /// The path portion (e.g., variable name, file path, vault path)
    pub path: String,
    /// Optional key within the resolved value (for JSON/object backends like Vault)
    pub key: Option<String>,
}

impl SecretRef {
    /// Parse a secret reference string like `env://MY_SECRET` or `file:///run/secrets/token`
    pub fn parse(raw: &str) -> Option<Self> {
        // Must contain ://
        let (scheme, rest) = raw.split_once("://")?;

        // Split key from path (after #)
        let (path, key) = match rest.split_once('#') {
            Some((p, k)) => (p.to_string(), Some(k.to_string())),
            None => (rest.to_string(), None),
        };

        Some(Self {
            scheme: scheme.to_string(),
            path,
            key,
        })
    }

    /// Returns true if this string looks like a secret reference
    pub fn is_secret_ref(s: &str) -> bool {
        s.contains("://") && !s.starts_with("http://") && !s.starts_with("https://")
    }
}

/// Trait for resolving secret references to their string values.
///
/// Implement this trait to add a new secret backend.
/// Each implementation handles one scheme (e.g., "env", "file", "vault").
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// The scheme this resolver handles (e.g., "env", "file", "vault")
    fn scheme(&self) -> &str;

    /// Resolve a secret reference to its value
    async fn resolve(&self, secret_ref: &SecretRef) -> Result<String>;

    /// Human-readable name for logging
    fn name(&self) -> &str {
        self.scheme()
    }
}

// ── Built-in: Environment Variable Resolver ────────────────────────────────

/// Resolves `env://VARIABLE_NAME` references from environment variables.
///
/// Optionally holds a static override map for testing, so callers can inject
/// fake environment values without mutating the real process environment.
pub struct EnvResolver {
    overrides: Option<HashMap<String, String>>,
}

impl EnvResolver {
    /// Create an `EnvResolver` that reads from the real process environment.
    pub fn new() -> Self {
        Self { overrides: None }
    }

    /// Create an `EnvResolver` with a static override map.
    /// The map is checked first; if a variable isn't found there,
    /// falls back to the real process environment.
    pub fn with_map(map: HashMap<String, String>) -> Self {
        Self {
            overrides: Some(map),
        }
    }
}

impl Default for EnvResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretResolver for EnvResolver {
    fn scheme(&self) -> &str {
        "env"
    }

    async fn resolve(&self, secret_ref: &SecretRef) -> Result<String> {
        // Check override map first
        if let Some(ref overrides) = self.overrides {
            if let Some(value) = overrides.get(&secret_ref.path) {
                return Ok(value.clone());
            }
        }
        // Fall back to real environment
        std::env::var(&secret_ref.path).map_err(|_| {
            anyhow::anyhow!(
                "Environment variable '{}' not set (referenced as env://{})",
                secret_ref.path,
                secret_ref.path
            )
        })
    }

    fn name(&self) -> &str {
        "environment"
    }
}

// ── Built-in: File Resolver ─────────────────────────────────────────────────

/// Resolves `file:///path/to/secret` references by reading a file.
///
/// Used for:
/// - Docker secrets: `file:///run/secrets/oauth_token`
/// - Kubernetes secret mounts: `file:///etc/secrets/db-password`
/// - Any local secret file
pub struct FileResolver;

#[async_trait]
impl SecretResolver for FileResolver {
    fn scheme(&self) -> &str {
        "file"
    }

    async fn resolve(&self, secret_ref: &SecretRef) -> Result<String> {
        let content = tokio::fs::read_to_string(&secret_ref.path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to read secret file '{}' (referenced as file://{}): {}",
                secret_ref.path,
                secret_ref.path,
                e
            )
        })?;

        // Trim trailing newline (common in Docker secrets)
        Ok(content.trim().to_string())
    }

    fn name(&self) -> &str {
        "file"
    }
}

// ── Built-in: Literal / No-op Resolver ─────────────────────────────────────

/// Resolves `literal://value` — returns the path as-is.
/// Useful for testing or when you want to be explicit that a value is not a secret.
pub struct LiteralResolver;

#[async_trait]
impl SecretResolver for LiteralResolver {
    fn scheme(&self) -> &str {
        "literal"
    }

    async fn resolve(&self, secret_ref: &SecretRef) -> Result<String> {
        Ok(secret_ref.path.clone())
    }

    fn name(&self) -> &str {
        "literal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_ref() {
        let r = SecretRef::parse("env://MY_SECRET").unwrap();
        assert_eq!(r.scheme, "env");
        assert_eq!(r.path, "MY_SECRET");
        assert_eq!(r.key, None);
    }

    #[test]
    fn test_parse_file_ref() {
        let r = SecretRef::parse("file:///run/secrets/token").unwrap();
        assert_eq!(r.scheme, "file");
        assert_eq!(r.path, "/run/secrets/token");
        assert_eq!(r.key, None);
    }

    #[test]
    fn test_parse_vault_ref_with_key() {
        let r = SecretRef::parse("vault://secret/data/app#api_key").unwrap();
        assert_eq!(r.scheme, "vault");
        assert_eq!(r.path, "secret/data/app");
        assert_eq!(r.key, Some("api_key".into()));
    }

    #[test]
    fn test_parse_aws_ref() {
        let r = SecretRef::parse("aws-secretsmanager://prod/yas-mcp/oauth").unwrap();
        assert_eq!(r.scheme, "aws-secretsmanager");
        assert_eq!(r.path, "prod/yas-mcp/oauth");
        assert_eq!(r.key, None);
    }

    #[test]
    fn test_is_secret_ref() {
        assert!(SecretRef::is_secret_ref("env://FOO"));
        assert!(SecretRef::is_secret_ref("file:///run/secrets/x"));
        assert!(SecretRef::is_secret_ref("vault://secret/x#key"));
        assert!(!SecretRef::is_secret_ref("plain-value"));
        assert!(!SecretRef::is_secret_ref("http://example.com"));
        assert!(!SecretRef::is_secret_ref("https://example.com"));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(SecretRef::parse("no-scheme").is_none());
        assert!(SecretRef::parse("http://example.com").is_some()); // but it would be filtered by is_secret_ref
    }

    #[tokio::test]
    async fn test_env_resolver() {
        let mut map = HashMap::new();
        map.insert("TEST_SECRET_ENV_VAR".into(), "secret-value-123".into());
        let resolver = EnvResolver::with_map(map);
        let secret_ref = SecretRef::parse("env://TEST_SECRET_ENV_VAR").unwrap();
        let value = resolver.resolve(&secret_ref).await.unwrap();
        assert_eq!(value, "secret-value-123");
    }

    #[tokio::test]
    async fn test_env_resolver_missing() {
        let resolver = EnvResolver::new();
        let secret_ref = SecretRef::parse("env://DEFINITELY_NOT_SET_12345").unwrap();
        let result = resolver.resolve(&secret_ref).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_resolver() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "file-secret-value\n").unwrap();

        let resolver = FileResolver;
        let secret_ref =
            SecretRef::parse(&format!("file://{}", tmp.path().display())).unwrap();
        let value = resolver.resolve(&secret_ref).await.unwrap();
        assert_eq!(value, "file-secret-value"); // newline trimmed
    }

    #[tokio::test]
    async fn test_literal_resolver() {
        let resolver = LiteralResolver;
        let secret_ref = SecretRef::parse("literal://hello-world").unwrap();
        let value = resolver.resolve(&secret_ref).await.unwrap();
        assert_eq!(value, "hello-world");
    }
}
