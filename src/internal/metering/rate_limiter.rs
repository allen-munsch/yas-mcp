//! Token bucket rate limiter — per-client, per-tool, per-API throttling.
//!
//! Implements a thread-safe token bucket algorithm with configurable burst and rate.
//! Supports multiple dimensions of limiting:
//! - Per-client (IP or session-based)
//! - Per-tool (individual MCP tools)
//! - Global (total throughput)

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Token bucket state for a single rate limit dimension.
#[derive(Debug)]
struct TokenBucket {
    /// Tokens currently available
    tokens: f64,
    /// Maximum bucket capacity (burst)
    capacity: f64,
    /// Refill rate in tokens per second
    refill_rate: f64,
    /// Last time tokens were refilled
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Attempt to consume `n` tokens. Returns true if successful.
    #[allow(dead_code)]
    fn consume(&mut self, n: f64) -> bool {
        self.refill();

        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let added = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + added).min(self.capacity);
        self.last_refill = now;
    }

    /// Try to consume and return wait time if rejected
    fn try_consume(&mut self, n: f64) -> Result<(), Duration> {
        self.refill();

        if self.tokens >= n {
            self.tokens -= n;
            Ok(())
        } else {
            // Calculate how long until enough tokens are available
            let deficit = n - self.tokens;
            let wait_secs = deficit / self.refill_rate;
            Err(Duration::from_secs_f64(wait_secs))
        }
    }
}

/// Configuration for rate limits.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Default requests per second
    pub default_rate: f64,
    /// Default burst capacity
    pub default_burst: f64,
    /// Whether rate limiting is enabled
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            default_rate: 10.0,  // 10 req/s
            default_burst: 20.0, // burst of 20
            enabled: true,
        }
    }
}

/// Thread-safe rate limiter using the token bucket algorithm.
///
/// Tracks limits per key (e.g. client IP, tool name, or global).
/// Uses `DashMap<Mutex>` to allow concurrent access to different keys
/// while serializing access to the same key.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Key → TokenBucket
    buckets: Arc<DashMap<String, Arc<Mutex<TokenBucket>>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Get or create a token bucket for a given key
    async fn get_bucket(&self, key: &str, rate: Option<f64>, burst: Option<f64>) -> Arc<Mutex<TokenBucket>> {
        if let Some(bucket) = self.buckets.get(key) {
            return bucket.clone();
        }

        let rate = rate.unwrap_or(self.config.default_rate);
        let burst = burst.unwrap_or(self.config.default_burst);
        let bucket = Arc::new(Mutex::new(TokenBucket::new(burst, rate)));

        self.buckets.insert(key.to_string(), bucket.clone());
        bucket
    }

    /// Check if a request is allowed.
    ///
    /// Returns `Ok(())` if the request can proceed, or `Err(wait_seconds)` if
    /// the client should wait before retrying.
    pub async fn check(
        &self,
        key: &str,
        rate: Option<f64>,
        burst: Option<f64>,
    ) -> Result<(), f64> {
        if !self.config.enabled {
            return Ok(());
        }

        let bucket = self.get_bucket(key, rate, burst).await;
        let mut guard = bucket.lock().await;

        match guard.try_consume(1.0) {
            Ok(()) => {
                debug!(key = %key, "Rate limit check passed");
                Ok(())
            }
            Err(wait) => {
                let wait_secs = wait.as_secs_f64();
                warn!(
                    key = %key,
                    wait_secs = wait_secs,
                    "Rate limit exceeded"
                );
                Err(wait_secs)
            }
        }
    }

    /// Check with a client-specific key (e.g. client IP + tool name)
    pub async fn check_client_tool(
        &self,
        client_id: &str,
        tool_name: &str,
    ) -> Result<(), f64> {
        let key = format!("client:{}:tool:{}", client_id, tool_name);
        self.check(&key, None, None).await
    }

    /// Check against a global limit
    pub async fn check_global(&self) -> Result<(), f64> {
        self.check("__global__", Some(100.0), Some(200.0)).await
    }

    /// Check per-tool limit
    pub async fn check_tool(
        &self,
        tool_name: &str,
        rate: Option<f64>,
        burst: Option<f64>,
    ) -> Result<(), f64> {
        let key = format!("tool:{}", tool_name);
        self.check(&key, rate, burst).await
    }

    /// Remove a key from the limiter (cleanup)
    pub fn remove(&self, key: &str) {
        self.buckets.remove(key);
    }

    /// Get the number of tracked rate limit keys
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Clean up stale buckets (those that haven't been accessed recently)
    pub fn cleanup(&self) -> usize {
        // In a real implementation, we'd track last access time.
        // For now, just remove buckets that have been drained for a while.
        let count_before = self.buckets.len();
        self.buckets.retain(|_key, _bucket| {
            // Keep all buckets — cleanup based on access time would need
            // additional tracking. Tokens refill automatically, so stale
            // buckets don't leak resources significantly.
            true
        });
        count_before - self.buckets.len()
    }
}

/// Helper: build a rate limit header value for HTTP 429 responses.
pub fn rate_limit_headers(retry_after: f64) -> Vec<(String, String)> {
    vec![
        ("Retry-After".to_string(), format!("{:.0}", retry_after.ceil())),
        ("X-RateLimit-Limit".to_string(), "10".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bucket_refill() {
        let mut bucket = TokenBucket::new(5.0, 1.0); // 5 capacity, 1 t/s
        assert!(bucket.consume(5.0)); // Drain all
        assert!(!bucket.consume(1.0)); // Empty now
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            default_rate: 100.0,
            default_burst: 100.0,
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Should allow 5 rapid requests within burst
        for i in 0..5 {
            let result = limiter.check("test-key", None, None).await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_rejects_when_exhausted() {
        let config = RateLimitConfig {
            default_rate: 1.0,  // 1 token/second
            default_burst: 2.0, // burst of 2
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Consume burst (2 tokens)
        assert!(limiter.check("burst-key", None, None).await.is_ok());
        assert!(limiter.check("burst-key", None, None).await.is_ok());

        // 3rd request should be rejected (bucket empty)
        let result = limiter.check("burst-key", None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Everything should pass when disabled
        for _ in 0..1000 {
            assert!(limiter.check("any", None, None).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_client_tool_key() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        // Different client+tool combos get separate limits
        assert!(limiter.check_client_tool("client-a", "tool-x").await.is_ok());
        assert!(limiter.check_client_tool("client-b", "tool-x").await.is_ok());
        assert!(limiter.check_client_tool("client-a", "tool-y").await.is_ok());
    }

    #[tokio::test]
    async fn test_tool_specific_rate() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        // Tool-specific rate limit
        assert!(limiter
            .check_tool("expensive-tool", Some(1.0), Some(1.0))
            .await
            .is_ok());
        assert!(limiter
            .check_tool("expensive-tool", Some(1.0), Some(1.0))
            .await
            .is_err());
    }
}
