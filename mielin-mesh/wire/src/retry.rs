//! Network Error Handling and Retry Logic
//!
//! Provides exponential backoff, circuit breaker pattern, and retry policies
//! for reliable network communication in unreliable environments.

use crate::WireError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Default base delay for exponential backoff (100ms)
const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(100);

/// Default maximum delay for exponential backoff (30s)
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(30);

/// Default maximum retry attempts
const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// Default circuit breaker failure threshold
const DEFAULT_FAILURE_THRESHOLD: u32 = 5;

/// Default circuit breaker timeout (60s)
const DEFAULT_CIRCUIT_TIMEOUT: Duration = Duration::from_secs(60);

/// Retry strategy configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed,
    /// Exponential backoff (2^n * base_delay)
    #[default]
    Exponential,
    /// Fibonacci sequence delay
    Fibonacci,
    /// Linear backoff (n * base_delay)
    Linear,
}

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Retry strategy to use
    pub strategy: RetryStrategy,
    /// Base delay for backoff calculations
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Whether to add jitter to delays (reduces thundering herd)
    pub use_jitter: bool,
    /// Jitter factor (0.0 - 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            strategy: RetryStrategy::Exponential,
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            use_jitter: true,
            jitter_factor: 0.3,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the retry strategy
    pub fn with_strategy(mut self, strategy: RetryStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the base delay
    pub fn with_base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = delay;
        self
    }

    /// Set the maximum delay
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the maximum attempts
    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Enable or disable jitter
    pub fn with_jitter(mut self, use_jitter: bool) -> Self {
        self.use_jitter = use_jitter;
        self
    }

    /// Preset for aggressive retries (fast, many attempts)
    pub fn aggressive() -> Self {
        Self {
            strategy: RetryStrategy::Exponential,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(5),
            max_attempts: 10,
            use_jitter: true,
            jitter_factor: 0.3,
        }
    }

    /// Preset for conservative retries (slow, few attempts)
    pub fn conservative() -> Self {
        Self {
            strategy: RetryStrategy::Linear,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_attempts: 3,
            use_jitter: true,
            jitter_factor: 0.2,
        }
    }

    /// Preset for production use (balanced)
    pub fn production() -> Self {
        Self::default()
    }

    /// Calculate delay for a given attempt number
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_millis = self.base_delay.as_millis() as u64;

        let delay_millis = match self.strategy {
            RetryStrategy::Fixed => base_millis,
            RetryStrategy::Exponential => {
                // 2^attempt * base_delay, with overflow protection
                let multiplier = 2u64.saturating_pow(attempt);
                base_millis.saturating_mul(multiplier)
            }
            RetryStrategy::Fibonacci => {
                // Fibonacci sequence: 1, 1, 2, 3, 5, 8, 13, ...
                let fib = Self::fibonacci(attempt as u64);
                base_millis.saturating_mul(fib)
            }
            RetryStrategy::Linear => {
                // n * base_delay
                base_millis.saturating_mul(attempt as u64 + 1)
            }
        };

        let mut delay = Duration::from_millis(delay_millis.min(self.max_delay.as_millis() as u64));

        // Apply jitter if enabled
        if self.use_jitter {
            delay = self.apply_jitter(delay);
        }

        delay
    }

    /// Apply jitter to a delay
    fn apply_jitter(&self, delay: Duration) -> Duration {
        use rand::Rng;
        let mut rng = rand::rng();

        let delay_millis = delay.as_millis() as f64;
        let jitter_range = delay_millis * self.jitter_factor;
        let jitter = rng.random_range(-jitter_range..jitter_range);

        let final_millis = (delay_millis + jitter).max(0.0) as u64;
        Duration::from_millis(final_millis)
    }

    /// Calculate Fibonacci number (iterative, fast)
    fn fibonacci(n: u64) -> u64 {
        if n <= 1 {
            return 1;
        }

        let mut a = 1u64;
        let mut b = 1u64;

        for _ in 2..=n {
            let next = a.saturating_add(b);
            a = b;
            b = next;
        }

        b
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Number of successes needed to close circuit from half-open
    pub success_threshold: u32,
    /// How long to wait before testing in half-open state
    pub timeout: Duration,
    /// How long to keep metrics (sliding window)
    pub metrics_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            success_threshold: 2,
            timeout: DEFAULT_CIRCUIT_TIMEOUT,
            metrics_window: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
    total_requests: u64,
    total_failures: u64,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            opened_at: None,
            total_requests: 0,
            total_failures: 0,
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::default())),
            config,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Check if a request can proceed
    pub async fn can_proceed(&self) -> Result<(), WireError> {
        let mut state = self.state.write().await;
        state.total_requests += 1;

        match state.state {
            CircuitState::Closed => Ok(()),
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.config.timeout {
                        info!("Circuit breaker transitioning to half-open");
                        state.state = CircuitState::HalfOpen;
                        state.success_count = 0;
                        Ok(())
                    } else {
                        Err(WireError::TransportError(
                            "Circuit breaker is open".to_string(),
                        ))
                    }
                } else {
                    Err(WireError::TransportError(
                        "Circuit breaker is open".to_string(),
                    ))
                }
            }
            CircuitState::HalfOpen => Ok(()),
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match state.state {
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
                state.last_failure_time = None;
            }
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    info!("Circuit breaker closing after successful recovery");
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.opened_at = None;
                }
            }
            CircuitState::Open => {
                // Should not happen, but handle gracefully
                debug!("Received success while circuit is open");
            }
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        state.total_failures += 1;
        state.last_failure_time = Some(Instant::now());

        match state.state {
            CircuitState::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.config.failure_threshold {
                    warn!(
                        "Circuit breaker opening after {} failures",
                        state.failure_count
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    state.failure_count = 0;
                }
            }
            CircuitState::HalfOpen => {
                warn!("Circuit breaker re-opening after failed test request");
                state.state = CircuitState::Open;
                state.opened_at = Some(Instant::now());
                state.success_count = 0;
            }
            CircuitState::Open => {
                // Already open, just track the failure
                debug!("Additional failure while circuit is open");
            }
        }
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        self.state.read().await.state
    }

    /// Get statistics
    pub async fn stats(&self) -> CircuitBreakerStats {
        let state = self.state.read().await;
        CircuitBreakerStats {
            state: state.state,
            failure_count: state.failure_count,
            success_count: state.success_count,
            total_requests: state.total_requests,
            total_failures: state.total_failures,
            opened_at: state.opened_at,
        }
    }

    /// Reset the circuit breaker to closed state
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        state.state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.opened_at = None;
        info!("Circuit breaker manually reset");
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub total_requests: u64,
    pub total_failures: u64,
    #[serde(skip)]
    pub opened_at: Option<Instant>,
}

/// Retry executor that combines retry policy and circuit breaker
pub struct RetryExecutor {
    policy: RetryPolicy,
    circuit_breaker: Option<CircuitBreaker>,
}

impl RetryExecutor {
    /// Create a new retry executor
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            circuit_breaker: None,
        }
    }

    /// Add circuit breaker protection
    pub fn with_circuit_breaker(mut self, breaker: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Execute an operation with retries
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> Result<T, WireError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, WireError>>,
    {
        for attempt in 0..self.policy.max_attempts {
            // Check circuit breaker
            if let Some(ref breaker) = self.circuit_breaker {
                breaker.can_proceed().await?;
            }

            // Execute operation
            match operation().await {
                Ok(result) => {
                    if let Some(ref breaker) = self.circuit_breaker {
                        breaker.record_success().await;
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if let Some(ref breaker) = self.circuit_breaker {
                        breaker.record_failure().await;
                    }

                    if attempt + 1 >= self.policy.max_attempts {
                        warn!(
                            "Operation failed after {} attempts: {}",
                            self.policy.max_attempts, e
                        );
                        return Err(e);
                    }

                    let delay = self.policy.calculate_delay(attempt);
                    debug!(
                        "Attempt {} failed, retrying in {:?}: {}",
                        attempt + 1,
                        delay,
                        e
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(WireError::TransportError(
            "Max retry attempts exceeded".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_creation() {
        let policy = RetryPolicy::new();
        assert_eq!(policy.strategy, RetryStrategy::Exponential);
        assert_eq!(policy.max_attempts, DEFAULT_MAX_ATTEMPTS);
        assert!(policy.use_jitter);
    }

    #[test]
    fn test_retry_policy_presets() {
        let aggressive = RetryPolicy::aggressive();
        assert_eq!(aggressive.max_attempts, 10);
        assert!(aggressive.base_delay < DEFAULT_BASE_DELAY);

        let conservative = RetryPolicy::conservative();
        assert_eq!(conservative.max_attempts, 3);
        assert_eq!(conservative.strategy, RetryStrategy::Linear);

        let production = RetryPolicy::production();
        assert_eq!(production.max_attempts, DEFAULT_MAX_ATTEMPTS);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let policy = RetryPolicy::new()
            .with_strategy(RetryStrategy::Exponential)
            .with_base_delay(Duration::from_millis(100))
            .with_jitter(false);

        let delay0 = policy.calculate_delay(0);
        let delay1 = policy.calculate_delay(1);
        let delay2 = policy.calculate_delay(2);

        assert_eq!(delay0, Duration::from_millis(100)); // 2^0 * 100
        assert_eq!(delay1, Duration::from_millis(200)); // 2^1 * 100
        assert_eq!(delay2, Duration::from_millis(400)); // 2^2 * 100
    }

    #[test]
    fn test_linear_backoff_calculation() {
        let policy = RetryPolicy::new()
            .with_strategy(RetryStrategy::Linear)
            .with_base_delay(Duration::from_millis(100))
            .with_jitter(false);

        let delay0 = policy.calculate_delay(0);
        let delay1 = policy.calculate_delay(1);
        let delay2 = policy.calculate_delay(2);

        assert_eq!(delay0, Duration::from_millis(100)); // 1 * 100
        assert_eq!(delay1, Duration::from_millis(200)); // 2 * 100
        assert_eq!(delay2, Duration::from_millis(300)); // 3 * 100
    }

    #[test]
    fn test_fibonacci_backoff_calculation() {
        let policy = RetryPolicy::new()
            .with_strategy(RetryStrategy::Fibonacci)
            .with_base_delay(Duration::from_millis(100))
            .with_jitter(false);

        let delay0 = policy.calculate_delay(0);
        let delay1 = policy.calculate_delay(1);
        let delay2 = policy.calculate_delay(2);
        let delay3 = policy.calculate_delay(3);

        assert_eq!(delay0, Duration::from_millis(100)); // fib(0)=1 * 100
        assert_eq!(delay1, Duration::from_millis(100)); // fib(1)=1 * 100
        assert_eq!(delay2, Duration::from_millis(200)); // fib(2)=2 * 100
        assert_eq!(delay3, Duration::from_millis(300)); // fib(3)=3 * 100
    }

    #[test]
    fn test_max_delay_cap() {
        let policy = RetryPolicy::new()
            .with_strategy(RetryStrategy::Exponential)
            .with_base_delay(Duration::from_millis(100))
            .with_max_delay(Duration::from_secs(1))
            .with_jitter(false);

        let delay10 = policy.calculate_delay(10); // Would be 102.4s without cap
        assert!(delay10 <= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let breaker = CircuitBreaker::default_config();
        assert_eq!(breaker.state().await, CircuitState::Closed);
        assert!(breaker.can_proceed().await.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // Record failures
        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_rejects_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);
        assert!(breaker.can_proceed().await.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(breaker.can_proceed().await.is_ok());
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        breaker.can_proceed().await.ok();

        // Record successes
        breaker.record_success().await;
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);

        breaker.record_success().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let breaker = CircuitBreaker::default_config();

        breaker.can_proceed().await.ok();
        breaker.record_success().await;
        breaker.can_proceed().await.ok();
        breaker.record_failure().await;

        let stats = breaker.stats().await;
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_failures, 1);
    }

    #[tokio::test]
    async fn test_retry_executor_success() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let policy = RetryPolicy::new().with_max_attempts(3);
        let executor = RetryExecutor::new(policy);

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = executor
            .execute(|| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, WireError>(42)
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_executor_eventual_success() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let policy = RetryPolicy::new()
            .with_max_attempts(5)
            .with_base_delay(Duration::from_millis(10))
            .with_jitter(false);
        let executor = RetryExecutor::new(policy);

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = executor
            .execute(|| {
                let count = call_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, Ordering::SeqCst) + 1;
                    if current < 3 {
                        Err(WireError::TransportError("Temporary failure".to_string()))
                    } else {
                        Ok::<_, WireError>(42)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_executor_max_attempts() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let policy = RetryPolicy::new()
            .with_max_attempts(3)
            .with_base_delay(Duration::from_millis(10))
            .with_jitter(false);
        let executor = RetryExecutor::new(policy);

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = executor
            .execute(|| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(WireError::TransportError("Always fails".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_executor_with_circuit_breaker() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let policy = RetryPolicy::new()
            .with_max_attempts(10)
            .with_base_delay(Duration::from_millis(10));

        let breaker_config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(breaker_config);
        let executor = RetryExecutor::new(policy).with_circuit_breaker(breaker.clone());

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = executor
            .execute(|| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(WireError::TransportError("Always fails".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        // Should stop after circuit opens (3 failures)
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[test]
    fn test_fibonacci_sequence() {
        assert_eq!(RetryPolicy::fibonacci(0), 1);
        assert_eq!(RetryPolicy::fibonacci(1), 1);
        assert_eq!(RetryPolicy::fibonacci(2), 2);
        assert_eq!(RetryPolicy::fibonacci(3), 3);
        assert_eq!(RetryPolicy::fibonacci(4), 5);
        assert_eq!(RetryPolicy::fibonacci(5), 8);
        assert_eq!(RetryPolicy::fibonacci(6), 13);
    }

    #[test]
    fn test_retry_strategy_equality() {
        assert_eq!(RetryStrategy::Exponential, RetryStrategy::Exponential);
        assert_ne!(RetryStrategy::Exponential, RetryStrategy::Linear);
    }
}
