//! Comprehensive Error Handling for MielinMesh Core
//!
//! Provides:
//! - Network failure detection and recovery
//! - Retry mechanisms with backoff strategies
//! - Circuit breaker patterns
//! - Detailed error categorization

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::NodeId;

/// Comprehensive mesh error types
#[derive(Debug, Error)]
pub enum MeshNetworkError {
    #[error("Network connection failed to {addr}: {source}")]
    ConnectionFailed {
        addr: SocketAddr,
        #[source]
        source: io::Error,
    },

    #[error("Network timeout after {timeout:?} to {addr}")]
    Timeout { addr: SocketAddr, timeout: Duration },

    #[error("Peer {node_id} unreachable at {addr}")]
    PeerUnreachable { node_id: NodeId, addr: SocketAddr },

    #[error("Network partition detected: {details}")]
    NetworkPartition { details: String },

    #[error("DNS resolution failed for {hostname}: {source}")]
    DnsResolutionFailed {
        hostname: String,
        #[source]
        source: io::Error,
    },

    #[error("Too many connection attempts: {count} attempts to {addr}")]
    TooManyRetries { count: usize, addr: SocketAddr },

    #[error("Circuit breaker open{}", .addr.map(|a| format!(" for {a}")).unwrap_or_default())]
    CircuitBreakerOpen { addr: Option<SocketAddr> },

    #[error("Protocol error: {message}")]
    ProtocolError { message: String },

    #[error("Bandwidth limit exceeded: {current_bps} bps (limit: {limit_bps} bps)")]
    BandwidthLimitExceeded { current_bps: u64, limit_bps: u64 },

    #[error("Message too large: {size} bytes (max: {max_size} bytes)")]
    MessageTooLarge { size: usize, max_size: usize },

    #[error("TLS/Security error: {details}")]
    SecurityError { details: String },

    #[error("Serialization failed: {source}")]
    SerializationError {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("IO error: {source}")]
    IoError {
        #[source]
        source: io::Error,
    },

    #[error("Operation cancelled: {reason}")]
    Cancelled { reason: String },

    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
}

impl MeshNetworkError {
    /// Check if error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            MeshNetworkError::Timeout { .. }
                | MeshNetworkError::ConnectionFailed { .. }
                | MeshNetworkError::PeerUnreachable { .. }
                | MeshNetworkError::DnsResolutionFailed { .. }
        )
    }

    /// Check if error is fatal and should stop retries
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            MeshNetworkError::TooManyRetries { .. }
                | MeshNetworkError::CircuitBreakerOpen { .. }
                | MeshNetworkError::SecurityError { .. }
                | MeshNetworkError::Cancelled { .. }
        )
    }

    /// Get retry delay suggestion based on error type
    pub fn suggested_retry_delay(&self) -> Option<Duration> {
        match self {
            MeshNetworkError::Timeout { .. } => Some(Duration::from_millis(500)),
            MeshNetworkError::ConnectionFailed { .. } => Some(Duration::from_secs(1)),
            MeshNetworkError::PeerUnreachable { .. } => Some(Duration::from_secs(2)),
            MeshNetworkError::DnsResolutionFailed { .. } => Some(Duration::from_secs(5)),
            MeshNetworkError::NetworkPartition { .. } => Some(Duration::from_secs(10)),
            _ => None,
        }
    }
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed(Duration),
    /// Linear backoff: delay * attempt
    Linear {
        initial_delay: Duration,
        max_delay: Duration,
    },
    /// Exponential backoff: delay * 2^attempt
    Exponential {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: u32,
    },
}

impl BackoffStrategy {
    /// Calculate delay for given attempt number
    pub fn calculate_delay(&self, attempt: usize) -> Duration {
        match self {
            BackoffStrategy::Fixed(delay) => *delay,
            BackoffStrategy::Linear {
                initial_delay,
                max_delay,
            } => {
                let delay = *initial_delay * attempt as u32;
                delay.min(*max_delay)
            }
            BackoffStrategy::Exponential {
                initial_delay,
                max_delay,
                multiplier,
            } => {
                let factor = multiplier.saturating_pow(attempt as u32);
                let delay = *initial_delay * factor;
                delay.min(*max_delay)
            }
        }
    }
}

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub strategy: BackoffStrategy,
    pub retry_on: fn(&MeshNetworkError) -> bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: BackoffStrategy::Exponential {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(10),
                multiplier: 2,
            },
            retry_on: |err| err.is_transient(),
        }
    }
}

impl RetryPolicy {
    /// Create an aggressive retry policy (more attempts, shorter delays)
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            strategy: BackoffStrategy::Exponential {
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(5),
                multiplier: 2,
            },
            retry_on: |err| err.is_transient(),
        }
    }

    /// Create a conservative retry policy (fewer attempts, longer delays)
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            strategy: BackoffStrategy::Exponential {
                initial_delay: Duration::from_millis(500),
                max_delay: Duration::from_secs(30),
                multiplier: 3,
            },
            retry_on: |err| err.is_transient() && !err.is_fatal(),
        }
    }

    /// Check if should retry given error and attempt count
    pub fn should_retry(&self, error: &MeshNetworkError, attempt: usize) -> bool {
        attempt < self.max_attempts && (self.retry_on)(error)
    }

    /// Get delay for next attempt
    pub fn get_delay(&self, attempt: usize) -> Duration {
        self.strategy.calculate_delay(attempt)
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for fault tolerance
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug)]
struct CircuitBreakerState {
    current_state: CircuitState,
    failure_count: usize,
    success_count: usize,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failures needed to open circuit
    pub failure_threshold: usize,
    /// Successes needed to close circuit from half-open
    pub success_threshold: usize,
    /// Time to wait before trying half-open
    pub timeout: Duration,
    /// Maximum time circuit can stay open
    pub max_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
            max_timeout: Duration::from_secs(300),
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState {
                current_state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                last_state_change: Instant::now(),
            })),
            config,
        }
    }

    /// Check if circuit allows request
    pub async fn allow_request(&self) -> bool {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout elapsed to try half-open
                if state.last_state_change.elapsed() >= self.config.timeout {
                    state.current_state = CircuitState::HalfOpen;
                    state.success_count = 0;
                    state.last_state_change = Instant::now();
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record successful operation
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    state.current_state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.last_state_change = Instant::now();
                }
            }
            CircuitState::Closed => {
                state.failure_count = 0;
            }
            _ => {}
        }
    }

    /// Record failed operation
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;

        state.failure_count += 1;
        state.last_failure_time = Some(Instant::now());

        match state.current_state {
            CircuitState::Closed => {
                if state.failure_count >= self.config.failure_threshold {
                    state.current_state = CircuitState::Open;
                    state.last_state_change = Instant::now();
                }
            }
            CircuitState::HalfOpen => {
                state.current_state = CircuitState::Open;
                state.success_count = 0;
                state.last_state_change = Instant::now();
            }
            _ => {}
        }
    }

    /// Get current circuit state
    pub async fn state(&self) -> CircuitState {
        let state = self.state.read().await;
        state.current_state
    }

    /// Reset circuit breaker to closed state
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        state.current_state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.last_state_change = Instant::now();
    }
}

/// Retry executor with circuit breaker support
pub struct RetryExecutor {
    policy: RetryPolicy,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl RetryExecutor {
    /// Create executor with retry policy
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            circuit_breaker: None,
        }
    }

    /// Create executor with circuit breaker
    pub fn with_circuit_breaker(policy: RetryPolicy, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            policy,
            circuit_breaker: Some(circuit_breaker),
        }
    }

    /// Execute operation with retries
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> Result<T, MeshNetworkError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, MeshNetworkError>>,
    {
        let mut attempt = 0;

        loop {
            // Check circuit breaker
            if let Some(cb) = &self.circuit_breaker {
                if !cb.allow_request().await {
                    return Err(MeshNetworkError::CircuitBreakerOpen { addr: None });
                }
            }

            // Execute operation
            match operation().await {
                Ok(result) => {
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_success().await;
                    }
                    return Ok(result);
                }
                Err(err) => {
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_failure().await;
                    }

                    attempt += 1;

                    // Check if should retry
                    if !self.policy.should_retry(&err, attempt) {
                        return Err(err);
                    }

                    // Wait before retry
                    let delay = self.policy.get_delay(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_transient_classification() {
        let timeout_err = MeshNetworkError::Timeout {
            addr: "127.0.0.1:8080".parse().unwrap(),
            timeout: Duration::from_secs(5),
        };
        assert!(timeout_err.is_transient());
        assert!(!timeout_err.is_fatal());

        let security_err = MeshNetworkError::SecurityError {
            details: "Invalid certificate".to_string(),
        };
        assert!(!security_err.is_transient());
        assert!(security_err.is_fatal());
    }

    #[test]
    fn test_backoff_strategies() {
        let fixed = BackoffStrategy::Fixed(Duration::from_millis(100));
        assert_eq!(fixed.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(fixed.calculate_delay(5), Duration::from_millis(100));

        let linear = BackoffStrategy::Linear {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        };
        assert_eq!(linear.calculate_delay(1), Duration::from_millis(100));
        assert_eq!(linear.calculate_delay(2), Duration::from_millis(200));
        assert_eq!(linear.calculate_delay(100), Duration::from_secs(5));

        let exponential = BackoffStrategy::Exponential {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2,
        };
        assert_eq!(exponential.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(exponential.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(exponential.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(exponential.calculate_delay(10), Duration::from_secs(10));
    }

    #[test]
    fn test_retry_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);

        let timeout_err = MeshNetworkError::Timeout {
            addr: "127.0.0.1:8080".parse().unwrap(),
            timeout: Duration::from_secs(5),
        };
        assert!(policy.should_retry(&timeout_err, 0));
        assert!(policy.should_retry(&timeout_err, 1));
        assert!(policy.should_retry(&timeout_err, 2));
        assert!(!policy.should_retry(&timeout_err, 3));

        let security_err = MeshNetworkError::SecurityError {
            details: "Invalid".to_string(),
        };
        assert!(!policy.should_retry(&security_err, 0));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            max_timeout: Duration::from_secs(10),
        };
        let cb = CircuitBreaker::new(config);

        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);

        // Record failures
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_circuit_breaker_open_to_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            max_timeout: Duration::from_secs(10),
        };
        let cb = CircuitBreaker::new(config);

        // Trigger open state
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            max_timeout: Duration::from_secs(10),
        };
        let cb = CircuitBreaker::new(config);

        // Get to half-open state
        cb.record_failure().await;
        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.allow_request().await;

        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record successes
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_retry_executor_success() {
        let policy = RetryPolicy::default();
        let executor = RetryExecutor::new(policy);

        let result = executor
            .execute(|| async { Ok::<i32, MeshNetworkError>(42) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_retry_executor_eventual_success() {
        let policy = RetryPolicy::default();
        let executor = RetryExecutor::new(policy);

        let attempts = Arc::new(tokio::sync::Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = executor
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    let mut count = attempts.lock().await;
                    *count += 1;
                    let attempt_num = *count;
                    drop(count);

                    if attempt_num < 2 {
                        Err(MeshNetworkError::Timeout {
                            addr: "127.0.0.1:8080".parse().unwrap(),
                            timeout: Duration::from_secs(1),
                        })
                    } else {
                        Ok::<i32, MeshNetworkError>(42)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*attempts.lock().await, 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_retry_executor_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 3,
            strategy: BackoffStrategy::Fixed(Duration::from_millis(10)),
            retry_on: |err| err.is_transient(),
        };
        let executor = RetryExecutor::new(policy);

        let attempts = Arc::new(tokio::sync::Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = executor
            .execute(|| {
                let attempts = attempts_clone.clone();
                async move {
                    let mut count = attempts.lock().await;
                    *count += 1;
                    drop(count);

                    Err::<i32, MeshNetworkError>(MeshNetworkError::Timeout {
                        addr: "127.0.0.1:8080".parse().unwrap(),
                        timeout: Duration::from_secs(1),
                    })
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(*attempts.lock().await, 3);
    }

    #[test]
    fn test_circuit_breaker_open_display() {
        let with_addr = MeshNetworkError::CircuitBreakerOpen {
            addr: Some("127.0.0.1:9000".parse().unwrap()),
        };
        assert_eq!(
            with_addr.to_string(),
            "Circuit breaker open for 127.0.0.1:9000"
        );

        let without_addr = MeshNetworkError::CircuitBreakerOpen { addr: None };
        assert_eq!(without_addr.to_string(), "Circuit breaker open");
    }
}
