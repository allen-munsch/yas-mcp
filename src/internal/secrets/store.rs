//! Secret Store — Registry of Secret Resolvers
//!
//! The `SecretStore` holds a map of scheme → resolver and resolves
//! secret references by dispatching to the appropriate resolver.
//!
//! # Usage
//!
//! ```rust,ignore
//! let store = SecretStore::default();  // comes with env + file resolvers
//! let value = store.resolve_value("env://DATABASE_URL").await?;
//! ```

use crate::internal::secrets::resolver::{
    EnvResolver, FileResolver, LiteralResolver, SecretRef, SecretResolver,
};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Registry of secret resolvers, keyed by scheme.
pub struct SecretStore {
    resolvers: HashMap<String, Arc<dyn SecretResolver>>,
}

impl Default for SecretStore {
    /// Creates a store with the built-in resolvers: `env`, `file`, `literal`
    fn default() -> Self {
        let mut store = Self {
            resolvers: HashMap::new(),
        };
        store.register(Arc::new(EnvResolver));
        store.register(Arc::new(FileResolver));
        store.register(Arc::new(LiteralResolver));
        store
    }
}

impl SecretStore {
    /// Create an empty store (no resolvers registered)
    pub fn empty() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    /// Register a resolver for its scheme
    pub fn register(&mut self, resolver: Arc<dyn SecretResolver>) {
        let scheme = resolver.scheme().to_string();
        debug!(
            "Registered secret resolver '{}' for scheme '{}'",
            resolver.name(),
            scheme
        );
        self.resolvers.insert(scheme, resolver);
    }

    /// Check if a scheme is registered
    pub fn has_scheme(&self, scheme: &str) -> bool {
        self.resolvers.contains_key(scheme)
    }

    /// List all registered schemes
    pub fn schemes(&self) -> Vec<&str> {
        self.resolvers.keys().map(|k| k.as_str()).collect()
    }

    /// Resolve a single secret reference string (e.g., "env://MY_VAR")
    pub async fn resolve_ref(&self, raw: &str) -> Result<String> {
        let secret_ref = SecretRef::parse(raw).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid secret reference '{}'. Expected format: scheme://path or scheme://path#key",
                raw
            )
        })?;

        let resolver = self.resolvers.get(&secret_ref.scheme).ok_or_else(|| {
            anyhow::anyhow!(
                "No secret resolver registered for scheme '{}'. Available schemes: {:?}",
                secret_ref.scheme,
                self.schemes()
            )
        })?;

        let mut value = resolver.resolve(&secret_ref).await?;

        // If there's a key, try to extract it from JSON
        if let Some(key) = &secret_ref.key {
            value = extract_json_key(&value, key)?;
        }

        debug!(
            "Resolved secret '{}' via {} resolver",
            raw,
            resolver.name()
        );

        Ok(value)
    }

    /// Resolve a config value that might be a secret reference.
    ///
    /// If the value looks like a secret reference (`scheme://...`), it's resolved.
    /// Otherwise, it's returned as-is (plain text value).
    pub async fn resolve_value(&self, value: &str) -> Result<String> {
        if SecretRef::is_secret_ref(value) {
            self.resolve_ref(value).await
        } else {
            Ok(value.to_string())
        }
    }

    /// Resolve all secret references in a HashMap of config values.
    /// Plain values are left untouched.
    pub async fn resolve_map(
        &self,
        map: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        let mut resolved = HashMap::with_capacity(map.len());
        for (key, value) in map {
            resolved.insert(key.clone(), self.resolve_value(value).await?);
        }
        Ok(resolved)
    }
}

/// Extract a named key from a JSON value string.
///
/// For backends like Vault that return `{"data": {"api_key": "sk-..."}}`,
/// you can reference `vault://secret/data/app#data.api_key`.
fn extract_json_key(json_str: &str, key: &str) -> Result<String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| {
            anyhow::anyhow!("Secret backend returned non-JSON but a key was requested: {}", e)
        })?;

    // Support nested keys like "data.api_key"
    let mut current = &value;
    for part in key.split('.') {
        current = current.get(part).ok_or_else(|| {
            anyhow::anyhow!(
                "Key '{}' not found in secret JSON. Available keys at this level: {:?}",
                key,
                current.as_object().map(|o| o.keys().collect::<Vec<_>>())
            )
        })?;
    }

    match current {
        serde_json::Value::String(s) => Ok(s.clone()),
        other => Ok(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_env_value() {
        unsafe { std::env::set_var("TEST_STORE_VAR", "from-env-456"); }
        let store = SecretStore::default();

        let value = store.resolve_value("env://TEST_STORE_VAR").await.unwrap();
        assert_eq!(value, "from-env-456");

        unsafe { std::env::remove_var("TEST_STORE_VAR"); }
    }

    #[tokio::test]
    async fn test_resolve_plain_value_passthrough() {
        let store = SecretStore::default();
        let value = store.resolve_value("just-a-plain-string").await.unwrap();
        assert_eq!(value, "just-a-plain-string");
    }

    #[tokio::test]
    async fn test_resolve_invalid_ref_error() {
        let store = SecretStore::default();
        let result = store.resolve_ref("unknown://something").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_map() {
        unsafe { std::env::set_var("TEST_MAP_PASS", "s3cr3t"); }
        let store = SecretStore::default();

        let mut map = HashMap::new();
        map.insert("username".into(), "admin".into()); // plain
        map.insert("password".into(), "env://TEST_MAP_PASS".into()); // secret ref

        let resolved = store.resolve_map(&map).await.unwrap();
        assert_eq!(resolved["username"], "admin");
        assert_eq!(resolved["password"], "s3cr3t");

        unsafe { std::env::remove_var("TEST_MAP_PASS"); }
    }

    #[test]
    fn test_extract_json_key_simple() {
        let json = r#"{"api_key": "sk-abc123", "other": 42}"#;
        let value = extract_json_key(json, "api_key").unwrap();
        assert_eq!(value, "sk-abc123");
    }

    #[test]
    fn test_extract_json_key_nested() {
        let json = r#"{"data": {"api_key": "sk-nested-789"}}"#;
        let value = extract_json_key(json, "data.api_key").unwrap();
        assert_eq!(value, "sk-nested-789");
    }

    #[test]
    fn test_extract_json_key_missing() {
        let json = r#"{"other": true}"#;
        let result = extract_json_key(json, "api_key");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_store_has_builtins() {
        let store = SecretStore::default();
        assert!(store.has_scheme("env"));
        assert!(store.has_scheme("file"));
        assert!(store.has_scheme("literal"));
        assert!(!store.has_scheme("vault"));
    }

    #[test]
    fn test_empty_store() {
        let store = SecretStore::empty();
        assert!(store.schemes().is_empty());
    }

    #[test]
    fn test_register_custom() {
        let mut store = SecretStore::empty();
        store.register(Arc::new(EnvResolver));
        store.register(Arc::new(FileResolver));
        assert_eq!(store.schemes().len(), 2);
        assert!(store.has_scheme("env"));
        assert!(store.has_scheme("file"));
    }
}
