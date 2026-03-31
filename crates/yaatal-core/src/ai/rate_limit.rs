//! Token-bucket rate limiter for AI provider endpoints.
//!
//! Each provider gets its own bucket. Tokens refill continuously at
//! `refill_rate` tokens per second.  No external dependencies — uses
//! only [`std::time::Instant`].

use std::collections::HashMap;
use std::time::Instant;

/// A single token bucket for one provider.
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum tokens the bucket can hold.
    capacity: f64,
    /// Current token count (can be fractional between refills).
    tokens: f64,
    /// Tokens added per second.
    refill_rate: f64,
    /// Last time we refilled.
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new limiter.
    ///
    /// * `capacity` — max burst size (and initial fill).
    /// * `refill_rate` — tokens restored per second.
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume one token.  Returns `true` if the request is
    /// allowed, `false` if the bucket is empty (caller should skip
    /// this provider).
    pub fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Estimated milliseconds until the next token becomes available.
    pub fn retry_after_ms(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let deficit = 1.0 - self.tokens;
        let secs = deficit / self.refill_rate;
        (secs * 1000.0).ceil() as u64
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }
}

/// A pool of per-provider rate limiters.
#[derive(Debug, Default)]
pub struct RateLimiterPool {
    limiters: HashMap<String, RateLimiter>,
}

impl RateLimiterPool {
    /// Register a provider with the given requests-per-minute limit.
    ///
    /// If the provider already exists it is **not** replaced — the
    /// existing bucket (and its remaining tokens) is preserved.
    pub fn register(&mut self, provider: &str, rpm: u32) {
        self.limiters.entry(provider.to_owned()).or_insert_with(|| {
            let capacity = rpm as f64;
            let refill_rate = capacity / 60.0; // tokens per second
            RateLimiter::new(capacity, refill_rate)
        });
    }

    /// Try to acquire a token for `provider`.
    ///
    /// Returns `true` if allowed, `false` if rate-limited.
    /// If the provider was never registered, it is assumed unlimited.
    pub fn try_acquire(&mut self, provider: &str) -> bool {
        match self.limiters.get_mut(provider) {
            Some(limiter) => limiter.try_acquire(),
            None => true, // unregistered = unlimited
        }
    }

    /// Estimated retry-after for a provider (0 if not rate-limited).
    pub fn retry_after_ms(&self, provider: &str) -> u64 {
        match self.limiters.get(provider) {
            Some(limiter) => limiter.retry_after_ms(),
            None => 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn limiter_allows_up_to_capacity() {
        let mut limiter = RateLimiter::new(3.0, 1.0);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn retry_after_is_nonzero_when_empty() {
        let mut limiter = RateLimiter::new(1.0, 1.0);
        limiter.try_acquire(); // drain
        assert!(limiter.retry_after_ms() > 0);
    }

    #[test]
    fn pool_unregistered_is_unlimited() {
        let mut pool = RateLimiterPool::default();
        assert!(pool.try_acquire("unknown-provider"));
    }

    #[test]
    fn pool_registered_respects_limit() {
        let mut pool = RateLimiterPool::default();
        pool.register("test-provider", 2);
        assert!(pool.try_acquire("test-provider"));
        assert!(pool.try_acquire("test-provider"));
        assert!(!pool.try_acquire("test-provider"));
    }

    #[test]
    fn pool_register_does_not_replace() {
        let mut pool = RateLimiterPool::default();
        pool.register("p", 2);
        pool.try_acquire("p");
        pool.register("p", 100); // should NOT reset
        assert!(pool.try_acquire("p")); // 1 token left from original capacity=2
        assert!(!pool.try_acquire("p")); // now empty
    }
}
