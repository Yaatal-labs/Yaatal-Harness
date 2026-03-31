use std::time::{Duration, Instant};

/// Current state of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CircuitBreakerState {
    /// Normal operation — requests flow through.
    #[default]
    Closed,
    /// Failure threshold breached — all requests rejected.
    Open,
    /// After timeout, allows one probe request.
    HalfOpen,
}

/// Configuration for a [`CircuitBreaker`].
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before tripping to Open.
    pub failure_threshold: usize,
    /// Number of consecutive successes in HalfOpen before resetting to Closed.
    pub half_open_success_threshold: usize,
    /// How long to stay Open before transitioning to HalfOpen.
    pub reset_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            half_open_success_threshold: 1,
            reset_timeout: Duration::from_secs(30),
        }
    }
}

/// Standard circuit breaker (closed → open → half-open → closed).
///
/// Used around external API calls during ingestion to prevent
/// cascading failures when a source is down.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitBreakerState,
    consecutive_failures: usize,
    consecutive_successes: usize,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitBreakerState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            opened_at: None,
        }
    }

    /// Returns the current state of the breaker.
    pub fn state(&self) -> CircuitBreakerState {
        self.state
    }

    /// Returns `true` if the request should be allowed through.
    pub fn should_allow(&mut self) -> bool {
        self.should_allow_at(Instant::now())
    }

    pub fn should_allow_at(&mut self, now: Instant) -> bool {
        match self.state {
            CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => true,
            CircuitBreakerState::Open => {
                if self
                    .opened_at
                    .map(|opened_at| now.duration_since(opened_at) >= self.config.reset_timeout)
                    .unwrap_or(false)
                {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.consecutive_successes = 0;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Records a successful operation, potentially resetting the breaker.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.consecutive_failures = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.consecutive_successes += 1;
                if self.consecutive_successes >= self.config.half_open_success_threshold {
                    self.state = CircuitBreakerState::Closed;
                    self.consecutive_failures = 0;
                    self.consecutive_successes = 0;
                    self.opened_at = None;
                }
            }
            CircuitBreakerState::Open => {}
        }
    }

    /// Records a failed operation, potentially tripping the breaker.
    pub fn record_failure(&mut self) {
        self.record_failure_at(Instant::now());
    }

    pub fn record_failure_at(&mut self, now: Instant) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.trip(now);
                }
            }
            CircuitBreakerState::HalfOpen => self.trip(now),
            CircuitBreakerState::Open => {
                self.opened_at = Some(now);
            }
        }
    }

    fn trip(&mut self, now: Instant) {
        self.state = CircuitBreakerState::Open;
        self.consecutive_failures = 0;
        self.consecutive_successes = 0;
        self.opened_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trip() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        });

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitBreakerState::Closed);

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitBreakerState::Open);
        assert!(!breaker.should_allow());
    }

    #[test]
    fn reset() {
        let now = Instant::now();
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            half_open_success_threshold: 1,
            reset_timeout: Duration::from_millis(10),
        });

        breaker.record_failure_at(now);
        assert_eq!(breaker.state(), CircuitBreakerState::Open);
        assert!(breaker.should_allow_at(now + Duration::from_millis(10)));

        breaker.record_success();
        assert_eq!(breaker.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn half_open_probe() {
        let now = Instant::now();
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            half_open_success_threshold: 2,
            reset_timeout: Duration::from_millis(10),
        });

        breaker.record_failure_at(now);
        assert!(breaker.should_allow_at(now + Duration::from_millis(10)));
        assert_eq!(breaker.state(), CircuitBreakerState::HalfOpen);

        breaker.record_success();
        assert_eq!(breaker.state(), CircuitBreakerState::HalfOpen);

        breaker.record_success();
        assert_eq!(breaker.state(), CircuitBreakerState::Closed);
    }
}
