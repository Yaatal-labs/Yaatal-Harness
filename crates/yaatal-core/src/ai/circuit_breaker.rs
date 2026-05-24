//! Circuit breaker for the AI router.
//!
//! Tracks consecutive failures per tier and opens for a cool-off period after
//! the failure threshold is reached. Transitions:
//!
//! ```text
//! Closed  ──[threshold failures]──▶  Open(until)
//! Open    ──[cool-off elapsed]───▶  HalfOpen
//! HalfOpen──[on_failure]──────────▶  Open(now + cool_off)
//! HalfOpen──[on_success]──────────▶  Closed
//! Closed  ──[on_success]──────────▶  Closed  (counter reset)
//! ```

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Externally-observable state of a [`CircuitBreaker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation — requests are forwarded.
    Closed,
    /// Tripped — requests are rejected until `until`.
    Open { until: Instant },
    /// Tentative — one request is forwarded to probe recovery.
    HalfOpen,
}

struct BreakerInner {
    state: BreakerState,
    consecutive_failures: u32,
}

/// Thread-safe circuit breaker.
pub struct CircuitBreaker {
    threshold: u32,
    cool_off: Duration,
    state: Mutex<BreakerInner>,
}

impl CircuitBreaker {
    /// Create a new breaker.
    ///
    /// * `threshold` — number of consecutive failures before opening.
    /// * `cool_off`  — how long to stay open before probing (half-open).
    pub fn new(threshold: u32, cool_off: Duration) -> Self {
        Self {
            threshold,
            cool_off,
            state: Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                consecutive_failures: 0,
            }),
        }
    }

    /// Returns `true` if the caller should forward the request.
    ///
    /// * `Closed`  → `true`.
    /// * `HalfOpen` → `true`.
    /// * `Open` + cool-off elapsed → transitions to `HalfOpen`, returns `true`.
    /// * `Open` + cool-off not elapsed → `false`.
    pub fn allow_request(&self) -> bool {
        let mut inner = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match inner.state {
            BreakerState::Closed | BreakerState::HalfOpen => true,
            BreakerState::Open { until } => {
                if Instant::now() >= until {
                    inner.state = BreakerState::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful call.
    ///
    /// Resets the failure counter.  `HalfOpen` → `Closed`.
    pub fn on_success(&self) {
        let mut inner = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        inner.consecutive_failures = 0;
        inner.state = BreakerState::Closed;
    }

    /// Record a failed call.
    ///
    /// * `Closed` + reaches threshold → `Open(now + cool_off)`.
    /// * `HalfOpen` → `Open(now + cool_off)` immediately.
    /// * `Open` → stays open, no-op.
    pub fn on_failure(&self) {
        let mut inner = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match inner.state {
            BreakerState::Open { .. } => {
                // Already open — nothing to do.
            }
            BreakerState::HalfOpen => {
                inner.state = BreakerState::Open {
                    until: Instant::now() + self.cool_off,
                };
            }
            BreakerState::Closed => {
                inner.consecutive_failures += 1;
                if inner.consecutive_failures >= self.threshold {
                    inner.state = BreakerState::Open {
                        until: Instant::now() + self.cool_off,
                    };
                }
            }
        }
    }

    /// Return a snapshot of the current breaker state.
    pub fn state(&self) -> BreakerState {
        let inner = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        inner.state
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use std::time::Duration;

    fn breaker_default() -> CircuitBreaker {
        CircuitBreaker::new(5, Duration::from_secs(300))
    }

    #[test]
    fn starts_closed_and_allows_requests() {
        let cb = breaker_default();
        assert_eq!(cb.state(), BreakerState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn five_consecutive_failures_opens_breaker() {
        let cb = breaker_default();
        for _ in 0..5 {
            cb.on_failure();
        }
        assert!(matches!(cb.state(), BreakerState::Open { .. }));
        assert!(!cb.allow_request());
    }

    #[test]
    fn success_resets_counter_so_four_plus_success_plus_failure_does_not_trip() {
        let cb = breaker_default();
        // 4 failures — not yet tripped
        for _ in 0..4 {
            cb.on_failure();
        }
        assert_eq!(cb.state(), BreakerState::Closed);
        // success resets to 0
        cb.on_success();
        assert_eq!(cb.state(), BreakerState::Closed);
        // one more failure — counter is 1, well below threshold
        cb.on_failure();
        assert_eq!(cb.state(), BreakerState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn open_transitions_to_half_open_after_cool_off() {
        let cb = CircuitBreaker::new(5, Duration::from_millis(50));
        for _ in 0..5 {
            cb.on_failure();
        }
        assert!(!cb.allow_request(), "should be denied while open");
        std::thread::sleep(Duration::from_millis(80));
        assert!(cb.allow_request(), "should be allowed after cool-off");
        assert_eq!(cb.state(), BreakerState::HalfOpen);
    }

    #[test]
    fn half_open_on_failure_opens_immediately() {
        let cb = CircuitBreaker::new(5, Duration::from_millis(50));
        for _ in 0..5 {
            cb.on_failure();
        }
        std::thread::sleep(Duration::from_millis(80));
        // transitions to HalfOpen
        assert!(cb.allow_request());
        assert_eq!(cb.state(), BreakerState::HalfOpen);
        // probe fails
        cb.on_failure();
        assert!(matches!(cb.state(), BreakerState::Open { .. }));
        assert!(!cb.allow_request());
    }

    #[test]
    fn half_open_on_success_closes_breaker() {
        let cb = CircuitBreaker::new(5, Duration::from_millis(50));
        for _ in 0..5 {
            cb.on_failure();
        }
        std::thread::sleep(Duration::from_millis(80));
        assert!(cb.allow_request());
        assert_eq!(cb.state(), BreakerState::HalfOpen);
        cb.on_success();
        assert_eq!(cb.state(), BreakerState::Closed);
        assert!(cb.allow_request());
    }
}
