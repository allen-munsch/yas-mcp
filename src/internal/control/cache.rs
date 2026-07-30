//! Response Cache
//!
//! Simple TTL-based in-memory cache for tool call responses.
//! Reduces upstream API load for repeated identical calls.
//!
//! # Usage
//!
//! ```yaml
//! cache:
//!   enabled: true
//!   default_ttl_secs: 60
//!   max_entries: 1000
//!   per_route:
//!     - path: /users
//!       methods: [GET]
//!       ttl_secs: 300
//! ```

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::debug;

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Whether caching is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Default TTL in seconds for cached entries
    #[serde(default = "default_ttl")]
    pub default_ttl_secs: u64,
    /// Maximum number of cached entries
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Per-route cache overrides
    #[serde(default)]
    pub per_route: Vec<RouteCacheConfig>,
}

fn default_ttl() -> u64 {
    60
}
fn default_max_entries() -> usize {
    1000
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_ttl_secs: 60,
            max_entries: 1000,
            per_route: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCacheConfig {
    pub path: String,
    pub methods: Vec<String>,
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
}

/// A cached response entry
#[derive(Debug, Clone)]
struct CacheEntry {
    body: Vec<u8>,
    status_code: u16,
    cached_at: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

/// Thread-safe in-memory response cache
pub struct ResponseCache {
    entries: DashMap<String, CacheEntry>,
    config: CacheConfig,
}

impl ResponseCache {
    /// Create a new cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: DashMap::with_capacity(config.max_entries),
            config,
        }
    }

    /// Build a cache key from tool name and parameters
    pub fn cache_key(tool_name: &str, params_json: &str) -> String {
        format!("{tool_name}:{params_json}")
    }

    /// Get a cached response, if available and not expired
    pub fn get(&self, key: &str) -> Option<(u16, Vec<u8>)> {
        if !self.config.enabled {
            return None;
        }

        let entry = self.entries.get(key)?;
        if entry.is_expired() {
            drop(entry);
            self.entries.remove(key);
            debug!("Cache entry expired: {}", key);
            return None;
        }

        debug!("Cache hit: {}", key);
        Some((entry.status_code, entry.body.clone()))
    }

    /// Store a response in the cache
    pub fn set(&self, key: &str, status_code: u16, body: &[u8], ttl_override: Option<u64>) {
        if !self.config.enabled {
            return;
        }

        // Enforce capacity
        if self.entries.len() >= self.config.max_entries {
            self.evict_one();
        }

        let ttl = Duration::from_secs(ttl_override.unwrap_or(self.config.default_ttl_secs));

        self.entries.insert(
            key.to_string(),
            CacheEntry {
                body: body.to_vec(),
                status_code,
                cached_at: Instant::now(),
                ttl,
            },
        );

        debug!("Cached response for: {} (TTL: {}s)", key, ttl.as_secs());
    }

    /// Invalidate a specific cache entry
    pub fn invalidate(&self, key: &str) {
        self.entries.remove(key);
    }

    /// Invalidate all entries matching a tool name prefix
    pub fn invalidate_tool(&self, tool_name: &str) {
        let prefix = format!("{tool_name}:");
        self.entries.retain(|k, _| !k.starts_with(&prefix));
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Number of cached entries
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Prune expired entries
    pub fn prune_expired(&self) -> usize {
        let mut removed = 0;
        self.entries.retain(|_, entry| {
            let keep = !entry.is_expired();
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }

    /// Evict the oldest entry
    fn evict_one(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_time = Instant::now();

        for entry in &self.entries {
            if entry.cached_at < oldest_time {
                oldest_time = entry.cached_at;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.entries.remove(&key);
        }
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_enabled() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 60,
            max_entries: 100,
            ..Default::default()
        });

        let key = ResponseCache::cache_key("get_users", "{}");
        cache.set(&key, 200, b"hello", None);

        let result = cache.get(&key);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), (200, b"hello".to_vec()));
    }

    #[test]
    fn test_cache_disabled() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: false,
            ..Default::default()
        });

        let key = ResponseCache::cache_key("get_users", "{}");
        cache.set(&key, 200, b"hello", None);

        let result = cache.get(&key);
        assert!(result.is_none(), "Cache disabled should never return hits");
    }

    #[test]
    fn test_cache_key_uniqueness() {
        let key1 = ResponseCache::cache_key("get_users", "{}");
        let key2 = ResponseCache::cache_key("get_users", r#"{"page":1}"#);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 60,
            max_entries: 100,
            ..Default::default()
        });

        let key = ResponseCache::cache_key("tool", "{}");
        cache.set(&key, 200, b"data", None);
        assert!(cache.get(&key).is_some());

        cache.invalidate(&key);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_invalidate_tool() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 60,
            max_entries: 100,
            ..Default::default()
        });

        cache.set(
            &ResponseCache::cache_key("get_users", "{}"),
            200,
            b"users",
            None,
        );
        cache.set(
            &ResponseCache::cache_key("get_users", r#"{"page":1}"#),
            200,
            b"users_p1",
            None,
        );
        cache.set(
            &ResponseCache::cache_key("get_projects", "{}"),
            200,
            b"projects",
            None,
        );

        assert_eq!(cache.count(), 3);

        cache.invalidate_tool("get_users");
        assert_eq!(cache.count(), 1);
        assert!(
            cache
                .get(&ResponseCache::cache_key("get_projects", "{}"))
                .is_some()
        );
    }

    #[test]
    fn test_cache_clear() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 60,
            max_entries: 100,
            ..Default::default()
        });

        cache.set(&ResponseCache::cache_key("a", "{}"), 200, b"a", None);
        cache.set(&ResponseCache::cache_key("b", "{}"), 200, b"b", None);
        assert_eq!(cache.count(), 2);

        cache.clear();
        assert_eq!(cache.count(), 0);
    }

    #[test]
    fn test_cache_expiry() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 0, // Immediate expiry
            max_entries: 100,
            ..Default::default()
        });

        let key = ResponseCache::cache_key("test", "{}");
        cache.set(&key, 200, b"data", None);

        // Should be expired immediately (TTL=0)
        let result = cache.get(&key);
        assert!(result.is_none());
    }

    #[test]
    fn test_prune_expired() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 0,
            max_entries: 100,
            ..Default::default()
        });

        cache.set(&ResponseCache::cache_key("x", "{}"), 200, b"x", None);
        cache.set(&ResponseCache::cache_key("y", "{}"), 200, b"y", None);

        let removed = cache.prune_expired();
        assert_eq!(removed, 2);
        assert_eq!(cache.count(), 0);
    }

    #[test]
    fn test_ttl_override() {
        let cache = ResponseCache::new(CacheConfig {
            enabled: true,
            default_ttl_secs: 0,
            max_entries: 100,
            ..Default::default()
        });

        let key = ResponseCache::cache_key("test", "{}");
        // Override TTL to 3600 seconds
        cache.set(&key, 200, b"data", Some(3600));

        let result = cache.get(&key);
        assert!(result.is_some(), "Should not be expired with TTL override");
    }
}
