//! Timeout Management for Service Mesh
//!
//! Provides per-service timeout configuration and enforcement for mesh operations.
//!
//! Features:
//! - Per-service timeout configuration
//! - Operation-specific timeouts (connection, request, idle)
//! - Adaptive timeout adjustment based on historical latencies
//! - Timeout policy inheritance (service -> global defaults)
//! - Circuit breaker integration

use crate::error::{CircuitBreaker, CircuitBreakerConfig, MeshNetworkError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Timeout management errors
#[derive(Debug, Error)]
pub enum TimeoutError {
    #[error("Operation timeout after {timeout:?}")]
    OperationTimeout { timeout: Duration },

    #[error("Service timeout configuration not found: {service_name}")]
    ConfigNotFound { service_name: String },

    #[error("Invalid timeout configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Circuit breaker open for service: {service_name}")]
    CircuitBreakerOpen { service_name: String },
}

/// Timeout operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeoutOperation {
    /// Initial connection establishment
    Connect,
    /// Request/response round-trip
    Request,
    /// Stream or long-lived connection idle timeout
    Idle,
    /// DNS lookup timeout
    DnsLookup,
    /// TLS handshake timeout
    TlsHandshake,
    /// Custom operation timeout
    Custom,
}

/// Timeout configuration for a specific operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Timeout duration
    pub duration: Duration,
    /// Whether this timeout is adaptive (adjusts based on history)
    pub adaptive: bool,
    /// Percentile to use for adaptive timeout (e.g., 0.95 for P95)
    pub percentile: f64,
    /// Minimum timeout (for adaptive timeouts)
    pub min_duration: Duration,
    /// Maximum timeout (for adaptive timeouts)
    pub max_duration: Duration,
}

impl TimeoutConfig {
    /// Create a new timeout configuration
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            adaptive: false,
            percentile: 0.95,
            min_duration: Duration::from_millis(10),
            max_duration: Duration::from_secs(60),
        }
    }

    /// Enable adaptive timeout adjustment
    pub fn adaptive(mut self, percentile: f64) -> Self {
        self.adaptive = true;
        self.percentile = percentile.clamp(0.5, 0.99);
        self
    }

    /// Set minimum and maximum duration for adaptive timeouts
    pub fn with_bounds(mut self, min: Duration, max: Duration) -> Self {
        self.min_duration = min;
        self.max_duration = max;
        self
    }

    /// Calculate timeout based on historical latencies
    pub fn calculate_timeout(&self, latencies: &[Duration]) -> Duration {
        if !self.adaptive || latencies.is_empty() {
            return self.duration;
        }

        let mut sorted = latencies.to_vec();
        sorted.sort();

        let index = ((sorted.len() - 1) as f64 * self.percentile) as usize;
        let percentile_latency = sorted[index];

        // Add 20% buffer on top of percentile latency
        let calculated = percentile_latency.mul_f64(1.2);
        calculated.clamp(self.min_duration, self.max_duration)
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

/// Per-service timeout policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTimeoutPolicy {
    /// Service name this policy applies to
    pub service_name: String,
    /// Operation-specific timeouts
    pub timeouts: HashMap<TimeoutOperation, TimeoutConfig>,
    /// Global timeout (applies to all operations if specific timeout not set)
    pub global_timeout: Duration,
    /// Circuit breaker configuration
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

impl ServiceTimeoutPolicy {
    /// Create a new service timeout policy
    pub fn new(service_name: impl Into<String>) -> Self {
        let mut timeouts = HashMap::new();
        timeouts.insert(
            TimeoutOperation::Connect,
            TimeoutConfig::new(Duration::from_secs(5)),
        );
        timeouts.insert(
            TimeoutOperation::Request,
            TimeoutConfig::new(Duration::from_secs(30)),
        );
        timeouts.insert(
            TimeoutOperation::Idle,
            TimeoutConfig::new(Duration::from_secs(300)),
        );
        timeouts.insert(
            TimeoutOperation::DnsLookup,
            TimeoutConfig::new(Duration::from_secs(3)),
        );
        timeouts.insert(
            TimeoutOperation::TlsHandshake,
            TimeoutConfig::new(Duration::from_secs(10)),
        );

        Self {
            service_name: service_name.into(),
            timeouts,
            global_timeout: Duration::from_secs(60),
            circuit_breaker: Some(CircuitBreakerConfig::default()),
        }
    }

    /// Set timeout for specific operation
    pub fn set_timeout(mut self, operation: TimeoutOperation, config: TimeoutConfig) -> Self {
        self.timeouts.insert(operation, config);
        self
    }

    /// Set global timeout
    pub fn with_global_timeout(mut self, timeout: Duration) -> Self {
        self.global_timeout = timeout;
        self
    }

    /// Enable circuit breaker
    pub fn with_circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(config);
        self
    }

    /// Disable circuit breaker
    pub fn without_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = None;
        self
    }

    /// Get timeout for specific operation
    pub fn get_timeout(&self, operation: TimeoutOperation) -> Duration {
        self.timeouts
            .get(&operation)
            .map(|c| c.duration)
            .unwrap_or(self.global_timeout)
    }
}

/// Latency history tracker for adaptive timeouts
#[derive(Debug)]
struct LatencyTracker {
    service_name: String,
    operation: TimeoutOperation,
    latencies: Vec<Duration>,
    max_samples: usize,
    last_update: Instant,
}

impl LatencyTracker {
    fn new(service_name: String, operation: TimeoutOperation) -> Self {
        Self {
            service_name,
            operation,
            latencies: Vec::with_capacity(100),
            max_samples: 100,
            last_update: Instant::now(),
        }
    }

    fn record(&mut self, latency: Duration) {
        self.latencies.push(latency);
        if self.latencies.len() > self.max_samples {
            self.latencies.remove(0);
        }
        self.last_update = Instant::now();
    }

    fn get_latencies(&self) -> &[Duration] {
        &self.latencies
    }

    fn clear_if_stale(&mut self, max_age: Duration) {
        if self.last_update.elapsed() > max_age {
            self.latencies.clear();
            debug!(
                "Cleared stale latency history for {}.{:?}",
                self.service_name, self.operation
            );
        }
    }
}

/// Timeout manager for the service mesh
pub struct TimeoutManager {
    /// Per-service timeout policies
    policies: Arc<RwLock<HashMap<String, ServiceTimeoutPolicy>>>,
    /// Default policy for services without explicit configuration
    default_policy: ServiceTimeoutPolicy,
    /// Circuit breakers per service
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    /// Latency history for adaptive timeouts
    latency_trackers: Arc<RwLock<HashMap<(String, TimeoutOperation), LatencyTracker>>>,
}

impl TimeoutManager {
    /// Create a new timeout manager
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
            default_policy: ServiceTimeoutPolicy::new("default"),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            latency_trackers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set default policy for all services
    pub async fn set_default_policy(&mut self, policy: ServiceTimeoutPolicy) {
        self.default_policy = policy;
    }

    /// Register a service-specific timeout policy
    pub async fn register_policy(&self, policy: ServiceTimeoutPolicy) {
        let service_name = policy.service_name.clone();
        let circuit_breaker_config = policy.circuit_breaker.clone();

        // Register policy
        self.policies
            .write()
            .await
            .insert(service_name.clone(), policy);

        // Create circuit breaker if configured
        if let Some(config) = circuit_breaker_config {
            let cb = CircuitBreaker::new(config);
            self.circuit_breakers
                .write()
                .await
                .insert(service_name.clone(), cb);
        }

        debug!("Registered timeout policy for service: {}", service_name);
    }

    /// Remove a service policy
    pub async fn unregister_policy(&self, service_name: &str) {
        self.policies.write().await.remove(service_name);
        self.circuit_breakers.write().await.remove(service_name);
        debug!("Unregistered timeout policy for service: {}", service_name);
    }

    /// Get timeout for a service operation
    pub async fn get_timeout(&self, service_name: &str, operation: TimeoutOperation) -> Duration {
        let policies = self.policies.read().await;
        let policy = policies.get(service_name).unwrap_or(&self.default_policy);

        let config = policy.timeouts.get(&operation);

        match config {
            Some(cfg) if cfg.adaptive => {
                // Get latency history
                let trackers = self.latency_trackers.read().await;
                let key = (service_name.to_string(), operation);

                if let Some(tracker) = trackers.get(&key) {
                    cfg.calculate_timeout(tracker.get_latencies())
                } else {
                    cfg.duration
                }
            }
            Some(cfg) => cfg.duration,
            None => policy.global_timeout,
        }
    }

    /// Record operation latency for adaptive timeout calculation
    pub async fn record_latency(
        &self,
        service_name: &str,
        operation: TimeoutOperation,
        latency: Duration,
    ) {
        let key = (service_name.to_string(), operation);
        let mut trackers = self.latency_trackers.write().await;

        trackers
            .entry(key.clone())
            .or_insert_with(|| LatencyTracker::new(service_name.to_string(), operation))
            .record(latency);
    }

    /// Execute an operation with timeout
    pub async fn execute_with_timeout<F, T>(
        &self,
        service_name: &str,
        operation: TimeoutOperation,
        future: F,
    ) -> Result<T, TimeoutError>
    where
        F: std::future::Future<Output = T>,
    {
        // Check circuit breaker first
        if let Some(cb) = self.circuit_breakers.read().await.get(service_name) {
            if !cb.allow_request().await {
                warn!(
                    "Circuit breaker open for service: {}, operation: {:?}",
                    service_name, operation
                );
                return Err(TimeoutError::CircuitBreakerOpen {
                    service_name: service_name.to_string(),
                });
            }
        }

        let timeout_duration = self.get_timeout(service_name, operation).await;
        let start = Instant::now();

        match timeout(timeout_duration, future).await {
            Ok(result) => {
                // Record success
                let latency = start.elapsed();
                self.record_latency(service_name, operation, latency).await;

                if let Some(cb) = self.circuit_breakers.read().await.get(service_name) {
                    cb.record_success().await;
                }

                Ok(result)
            }
            Err(_) => {
                // Timeout occurred
                warn!(
                    "Operation timeout for service: {}, operation: {:?}, timeout: {:?}",
                    service_name, operation, timeout_duration
                );

                if let Some(cb) = self.circuit_breakers.read().await.get(service_name) {
                    cb.record_failure().await;
                }

                Err(TimeoutError::OperationTimeout {
                    timeout: timeout_duration,
                })
            }
        }
    }

    /// Execute an operation with timeout and convert timeout error to MeshNetworkError
    pub async fn execute_with_timeout_mesh<F, T>(
        &self,
        service_name: &str,
        operation: TimeoutOperation,
        addr: std::net::SocketAddr,
        future: F,
    ) -> Result<T, MeshNetworkError>
    where
        F: std::future::Future<Output = Result<T, MeshNetworkError>>,
    {
        match self
            .execute_with_timeout(service_name, operation, future)
            .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e),
            Err(TimeoutError::OperationTimeout { timeout }) => {
                Err(MeshNetworkError::Timeout { addr, timeout })
            }
            Err(TimeoutError::CircuitBreakerOpen { .. }) => {
                Err(MeshNetworkError::CircuitBreakerOpen { addr })
            }
            Err(e) => Err(MeshNetworkError::ProtocolError {
                message: e.to_string(),
            }),
        }
    }

    /// Clean up stale latency history
    pub async fn cleanup_stale_history(&self, max_age: Duration) {
        let mut trackers = self.latency_trackers.write().await;
        for tracker in trackers.values_mut() {
            tracker.clear_if_stale(max_age);
        }
    }

    /// Get statistics for a service
    pub async fn get_stats(&self, service_name: &str) -> Option<ServiceTimeoutStats> {
        let policies = self.policies.read().await;
        let policy = policies.get(service_name)?;

        let circuit_breaker_state =
            if let Some(cb) = self.circuit_breakers.read().await.get(service_name) {
                Some(cb.state().await)
            } else {
                None
            };

        let trackers = self.latency_trackers.read().await;
        let mut operation_stats = HashMap::new();

        for (op, config) in &policy.timeouts {
            let key = (service_name.to_string(), *op);
            let latencies = trackers
                .get(&key)
                .map(|t| t.get_latencies().to_vec())
                .unwrap_or_default();

            operation_stats.insert(
                *op,
                OperationStats {
                    current_timeout: config.duration,
                    adaptive_timeout: if config.adaptive && !latencies.is_empty() {
                        Some(config.calculate_timeout(&latencies))
                    } else {
                        None
                    },
                    sample_count: latencies.len(),
                },
            );
        }

        Some(ServiceTimeoutStats {
            service_name: service_name.to_string(),
            circuit_breaker_state,
            operation_stats,
        })
    }
}

impl Default for TimeoutManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a service's timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTimeoutStats {
    pub service_name: String,
    pub circuit_breaker_state: Option<crate::error::CircuitState>,
    pub operation_stats: HashMap<TimeoutOperation, OperationStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStats {
    pub current_timeout: Duration,
    pub adaptive_timeout: Option<Duration>,
    pub sample_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_creation() {
        let config = TimeoutConfig::new(Duration::from_secs(10));
        assert_eq!(config.duration, Duration::from_secs(10));
        assert!(!config.adaptive);
    }

    #[test]
    fn test_adaptive_timeout_calculation() {
        let config = TimeoutConfig::new(Duration::from_secs(5))
            .adaptive(0.95)
            .with_bounds(Duration::from_millis(100), Duration::from_secs(30));

        let latencies = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(150),
            Duration::from_millis(180),
            Duration::from_millis(250),
        ];

        let timeout = config.calculate_timeout(&latencies);
        assert!(timeout >= Duration::from_millis(100));
        assert!(timeout <= Duration::from_secs(30));
    }

    #[test]
    fn test_service_timeout_policy() {
        let policy = ServiceTimeoutPolicy::new("test-service")
            .with_global_timeout(Duration::from_secs(60))
            .set_timeout(
                TimeoutOperation::Connect,
                TimeoutConfig::new(Duration::from_secs(3)),
            );

        assert_eq!(
            policy.get_timeout(TimeoutOperation::Connect),
            Duration::from_secs(3)
        );
        assert_eq!(
            policy.get_timeout(TimeoutOperation::Custom),
            Duration::from_secs(60)
        );
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_timeout_manager_policy_registration() {
        let manager = TimeoutManager::new();
        let policy = ServiceTimeoutPolicy::new("test-service");

        manager.register_policy(policy).await;

        let timeout = manager
            .get_timeout("test-service", TimeoutOperation::Connect)
            .await;
        assert_eq!(timeout, Duration::from_secs(5));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_timeout_manager_execution() {
        let manager = TimeoutManager::new();
        let policy = ServiceTimeoutPolicy::new("test-service").set_timeout(
            TimeoutOperation::Request,
            TimeoutConfig::new(Duration::from_millis(100)),
        );

        manager.register_policy(policy).await;

        // Test successful execution
        let result = manager
            .execute_with_timeout("test-service", TimeoutOperation::Request, async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                42
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_timeout_manager_timeout_error() {
        let manager = TimeoutManager::new();
        let policy = ServiceTimeoutPolicy::new("test-service").set_timeout(
            TimeoutOperation::Request,
            TimeoutConfig::new(Duration::from_millis(10)),
        );

        manager.register_policy(policy).await;

        // Test timeout
        let result = manager
            .execute_with_timeout("test-service", TimeoutOperation::Request, async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                42
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TimeoutError::OperationTimeout { .. }
        ));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_latency_tracking() {
        let manager = TimeoutManager::new();

        manager
            .record_latency(
                "test-service",
                TimeoutOperation::Request,
                Duration::from_millis(100),
            )
            .await;

        manager
            .record_latency(
                "test-service",
                TimeoutOperation::Request,
                Duration::from_millis(150),
            )
            .await;

        let trackers = manager.latency_trackers.read().await;
        let key = ("test-service".to_string(), TimeoutOperation::Request);
        let tracker = trackers.get(&key).unwrap();

        assert_eq!(tracker.get_latencies().len(), 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_adaptive_timeout_with_history() {
        let manager = TimeoutManager::new();
        let policy = ServiceTimeoutPolicy::new("test-service").set_timeout(
            TimeoutOperation::Request,
            TimeoutConfig::new(Duration::from_secs(5))
                .adaptive(0.95)
                .with_bounds(Duration::from_millis(50), Duration::from_secs(10)),
        );

        manager.register_policy(policy).await;

        // Record some latencies
        for latency_ms in &[100, 120, 110, 130, 150] {
            manager
                .record_latency(
                    "test-service",
                    TimeoutOperation::Request,
                    Duration::from_millis(*latency_ms),
                )
                .await;
        }

        let timeout = manager
            .get_timeout("test-service", TimeoutOperation::Request)
            .await;

        // Adaptive timeout should be calculated from latency history
        assert!(timeout >= Duration::from_millis(50));
        assert!(timeout <= Duration::from_secs(10));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_get_stats() {
        let manager = TimeoutManager::new();
        let policy = ServiceTimeoutPolicy::new("test-service");
        manager.register_policy(policy).await;

        manager
            .record_latency(
                "test-service",
                TimeoutOperation::Connect,
                Duration::from_millis(100),
            )
            .await;

        let stats = manager.get_stats("test-service").await;
        assert!(stats.is_some());

        let stats = stats.unwrap();
        assert_eq!(stats.service_name, "test-service");
        assert!(stats
            .operation_stats
            .contains_key(&TimeoutOperation::Connect));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_cleanup_stale_history() {
        let manager = TimeoutManager::new();

        manager
            .record_latency(
                "test-service",
                TimeoutOperation::Request,
                Duration::from_millis(100),
            )
            .await;

        // Wait for history to become stale
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cleanup with very short max_age
        manager
            .cleanup_stale_history(Duration::from_millis(50))
            .await;

        let trackers = manager.latency_trackers.read().await;
        let key = ("test-service".to_string(), TimeoutOperation::Request);
        let tracker = trackers.get(&key).unwrap();

        assert_eq!(tracker.get_latencies().len(), 0);
    }
}
