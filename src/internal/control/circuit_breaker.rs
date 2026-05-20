//! Circuit Breaker
//!
//! Per-upstream-API circuit breaker pattern.
//! Prevents cascading failures when an upstream API is degraded.
//!
//! # States
//!
//! ```text
//!           failure threshold reached
//!   CLOSED ─────────────────────────► OPEN
//!     ▲                                │
//!     │       cooldown elapsed         │
//!     └─ HALF_OPEN ◄───────────────────┘
//!          │                │
//!          │ success        │ failure
//!          ▼                ▼
//!        CLOSED           OPEN (reset timer)
//! ```

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

/// Configuration for a circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures to trip the breaker
    pub failure_threshold: u32,
    /// How long to stay open before trying half-open
    pub cooldown_secs: u64,
    /// Maximum number of requests allowed in half-open state
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            cooldown_secs: 30,
            half_open_max_requests: 1,
        }
    }
}

/// A circuit breaker for a single upstream API
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: AtomicU8,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_time: Mutex<Option<Instant>>,
    opened_at: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(name: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_time: Mutex::new(None),
            opened_at: Mutex::new(None),
        }
    }

    /// Check if a request should be allowed through.
    /// Returns `Ok(())` if the circuit is closed or half-open and accepting.
    /// Returns `Err(state)` if the circuit is open.
    pub fn check(&self) -> Result<(), CircuitState> {
        let state = self.current_state();

        match state {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                // In half-open, allow a limited number of probe requests
                let failures = self.failure_count.load(Ordering::Relaxed);
                if failures < self.config.half_open_max_requests as u64 {
                    Ok(())
                } else {
                    Err(CircuitState::HalfOpen)
                }
            }
            CircuitState::Open => {
                // Check if cooldown has elapsed → transition to half-open
                let should_half_open = self
                    .opened_at
                    .lock().unwrap()
                    .is_some_and(|opened| {
                        opened.elapsed() >= Duration::from_secs(self.config.cooldown_secs)
                    });

                if should_half_open {
                    self.transition_to(CircuitState::HalfOpen);
                    Ok(())
                } else {
                    Err(CircuitState::Open)
                }
            }
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
        let current = self.current_state();
        if current == CircuitState::HalfOpen {
            // Success in half-open → close the circuit
            self.transition_to(CircuitState::Closed);
            self.failure_count.store(0, Ordering::Relaxed);
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        *self.last_failure_time.lock().unwrap() = Some(Instant::now());

        let failures = self.failure_count.load(Ordering::Relaxed);
        let current = self.current_state();

        match current {
            CircuitState::Closed => {
                if failures >= self.config.failure_threshold as u64 {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Failure in half-open → go back to open
                self.transition_to(CircuitState::Open);
            }
            CircuitState::Open => {
                // Already open, reset the timer on additional failures
                *self.opened_at.lock().unwrap() = Some(Instant::now());
            }
        }
    }

    /// Get the current circuit state
    pub fn current_state(&self) -> CircuitState {
        let raw = self.state.load(Ordering::Relaxed);
        match raw {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }

    /// Total failures recorded (lifetime)
    pub fn total_failures(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Total successes recorded (lifetime)
    pub fn total_successes(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }

    /// Circuit breaker name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Reset the circuit breaker to closed state
    pub fn reset(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.transition_to(CircuitState::Closed);
    }

    fn transition_to(&self, new_state: CircuitState) {
        let old = self.state.swap(new_state as u8, Ordering::Relaxed);
        if old != new_state as u8 {
            if new_state == CircuitState::Open {
                *self.opened_at.lock().unwrap() = Some(Instant::now());
            }
            tracing::info!(
                circuit = %self.name,
                old_state = ?CircuitState::from_u8(old),
                new_state = ?new_state,
                "Circuit breaker state transition"
            );
        }
    }
}

impl CircuitState {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_initial_state_is_closed() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check().is_ok());
    }

    #[test]
    fn test_opens_after_failures() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown_secs: 3600, // Long cooldown to prevent auto half-open
            half_open_max_requests: 1,
        });

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check().is_ok());

        // Third failure trips the breaker
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(cb.check().is_err());
    }

    #[test]
    fn test_half_open_after_cooldown() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown_secs: 0, // Immediate cooldown for testing
            half_open_max_requests: 1,
        });

        // Trip immediately
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);

        // After zero cooldown, should allow half-open probe
        assert!(cb.check().is_ok());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_success_in_half_open_closes_circuit() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown_secs: 0,
            half_open_max_requests: 1,
        });

        cb.record_failure();
        assert!(cb.check().is_ok()); // Half-open

        cb.record_success();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check().is_ok());
    }

    #[test]
    fn test_failure_in_half_open_reopens() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown_secs: 3600, // Long cooldown so it stays open after re-trip
            half_open_max_requests: 1,
        });

        // Trip → Open
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Force half-open by overriding opened_at to distant past
        *cb.opened_at.lock().unwrap() = Some(Instant::now() - Duration::from_secs(3601));

        // Now check should go to half-open
        assert!(cb.check().is_ok());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        // Failure in half-open reopens
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(cb.check().is_err());
    }

    #[test]
    fn test_reset() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown_secs: 3600,
            half_open_max_requests: 1,
        });

        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.total_failures(), 0);
        assert!(cb.check().is_ok());
    }

    #[test]
    fn test_total_counts() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());

        cb.record_success();
        cb.record_success();
        cb.record_failure();

        assert_eq!(cb.total_successes(), 2);
        assert_eq!(cb.total_failures(), 1);
    }
}
