//! Rate Limiter — Token Bucket
//!
//! Per-client rate limiting using the token bucket algorithm.
//! Thread-safe, configurable limits, with `X-RateLimit-*` headers.
//!
//! # How It Works
//!
//! Each client (identified by IP or session ID) gets a bucket of tokens.
//! Tokens refill at a configurable rate. Each request consumes one token.
//! If the bucket is empty, the request gets `429 Too Many Requests`.

use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Configuration for a rate limiter bucket
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum tokens in the bucket (burst capacity)
    pub capacity: u32,
    /// Tokens refilled per second (sustained rate)
    pub refill_rate: f64,
    /// How the client is identified
    pub client_id_mode: ClientIdMode,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            capacity: 100,
            refill_rate: 10.0,
            client_id_mode: ClientIdMode::Ip,
        }
    }
}

/// How to identify a client for rate limiting
#[derive(Debug, Clone)]
pub enum ClientIdMode {
    /// Use the client's IP address
    Ip,
    /// Use a session/token identifier from headers
    SessionHeader(String),
}

/// A single token bucket
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            tokens: capacity as f64,
            capacity: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume a token. Returns true if allowed, false if rate limited.
    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }

    /// Estimated seconds until next token is available (for Retry-After header)
    fn retry_after_secs(&self) -> f64 {
        if self.tokens >= 1.0 {
            0.0
        } else {
            ((1.0 - self.tokens) / self.refill_rate).ceil()
        }
    }
}

/// Thread-safe rate limiter for multiple clients
pub struct RateLimiter {
    buckets: DashMap<String, TokenBucket>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter with the given config
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: DashMap::new(),
            config,
        }
    }

    /// Check if a request from the given client is allowed.
    /// Returns `Ok(())` if allowed, or `Err(retry_after_secs)` if rate limited.
    pub fn check(&self, client_id: &str) -> Result<(), f64> {
        let mut bucket = self
            .buckets
            .entry(client_id.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.capacity, self.config.refill_rate));

        if bucket.try_consume() {
            Ok(())
        } else {
            let retry_after = bucket.retry_after_secs();
            Err(retry_after.max(1.0))
        }
    }

    /// Get the current token count for a client (for debugging)
    pub fn tokens_remaining(&self, client_id: &str) -> f64 {
        self.buckets
            .get(client_id)
            .map(|b| {
                let mut bucket = TokenBucket::new(self.config.capacity, self.config.refill_rate);
                bucket.tokens = b.tokens;
                bucket.refill();
                bucket.tokens
            })
            .unwrap_or(self.config.capacity as f64)
    }

    /// Clean up stale buckets (clients that haven't been seen in a while)
    pub fn prune_stale(&self, max_age: Duration) -> usize {
        let now = Instant::now();
        let mut removed = 0;
        self.buckets.retain(|_, bucket| {
            let keep = now.duration_since(bucket.last_refill) < max_age;
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }

    /// Number of active client buckets
    pub fn active_clients(&self) -> usize {
        self.buckets.len()
    }

    /// Get the rate limit configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_allows_up_to_capacity() {
        let mut bucket = TokenBucket::new(5, 10.0);
        for _ in 0..5 {
            assert!(bucket.try_consume(), "Should allow up to capacity");
        }
        assert!(
            !bucket.try_consume(),
            "Should deny after capacity exhausted"
        );
    }

    #[test]
    fn test_token_bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(5, 100.0); // Very fast refill
        // Exhaust tokens
        for _ in 0..5 {
            bucket.try_consume();
        }
        assert!(!bucket.try_consume());

        // Simulate time passing by manipulating last_refill
        bucket.last_refill = Instant::now() - Duration::from_secs(1);

        // Should now have tokens from refill
        assert!(bucket.try_consume(), "Should refill after time passes");
    }

    #[test]
    fn test_rate_limiter_per_client_isolation() {
        let limiter = RateLimiter::new(RateLimitConfig {
            capacity: 3,
            refill_rate: 10.0,
            ..Default::default()
        });

        // Client A uses all tokens
        for _ in 0..3 {
            assert!(limiter.check("client-a").is_ok());
        }
        assert!(limiter.check("client-a").is_err());

        // Client B should still have tokens
        assert!(limiter.check("client-b").is_ok());
        assert!(limiter.check("client-b").is_ok());
    }

    #[test]
    fn test_rate_limiter_returns_retry_after() {
        let limiter = RateLimiter::new(RateLimitConfig {
            capacity: 1,
            refill_rate: 1.0, // 1 token per second
            ..Default::default()
        });

        assert!(limiter.check("client").is_ok());
        let err = limiter.check("client").unwrap_err();
        assert!(err >= 1.0, "Retry-After should be at least 1 second");
    }

    #[test]
    fn test_tokens_remaining() {
        let limiter = RateLimiter::new(RateLimitConfig {
            capacity: 10,
            refill_rate: 100.0,
            ..Default::default()
        });

        assert!(limiter.tokens_remaining("new-client") > 0.0);

        limiter.check("new-client").unwrap();
        let remaining = limiter.tokens_remaining("new-client");
        assert!(remaining < 10.0, "Tokens should decrease after consumption");
    }

    #[test]
    fn test_prune_stale() {
        let limiter = RateLimiter::new(RateLimitConfig::default());

        limiter.check("active").unwrap();
        assert_eq!(limiter.active_clients(), 1);

        // No staleness for recent clients
        let removed = limiter.prune_stale(Duration::from_secs(3600));
        assert_eq!(removed, 0, "Active client should not be pruned");

        // Zero-age pruning should remove all
        let removed = limiter.prune_stale(Duration::ZERO);
        assert_eq!(removed, 1, "Zero-age should prune everything");
        assert_eq!(limiter.active_clients(), 0);
    }

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.capacity, 100);
        assert_eq!(config.refill_rate, 10.0);
    }

    #[test]
    fn test_multiple_clients_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(RateLimiter::new(RateLimitConfig {
            capacity: 1000,
            refill_rate: 1000.0,
            ..Default::default()
        }));

        let mut handles = vec![];
        for client_id in 0..10 {
            let limiter = limiter.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    assert!(limiter.check(&format!("client-{client_id}")).is_ok());
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(limiter.active_clients(), 10);
    }
}
