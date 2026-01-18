//! Failure Recovery and Resilience
//!
//! Provides mechanisms for handling failures and recovery:
//! - Automatic peer reconnection with exponential backoff
//! - Migration retry with configurable policies
//! - State reconciliation after network partitions
//! - Graceful degradation under failures

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Recovery error types
#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("Maximum retry attempts exceeded: {0}")]
    MaxRetriesExceeded(u32),
    #[error("Reconnection failed: {0}")]
    ReconnectionFailed(String),
    #[error("State reconciliation failed: {0}")]
    ReconciliationFailed(String),
    #[error("Recovery timeout")]
    Timeout,
    #[error("Node unreachable: {0}")]
    NodeUnreachable(NodeId),
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed,
    /// Exponential backoff with optional jitter
    Exponential,
    /// Linear increase in delay
    Linear,
}

/// Configuration for retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier (for exponential backoff)
    pub multiplier: f64,
    /// Backoff strategy
    pub strategy: BackoffStrategy,
    /// Whether to add jitter to delays
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            strategy: BackoffStrategy::Exponential,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a config for aggressive retries (fast recovery)
    pub fn aggressive() -> Self {
        Self {
            max_retries: 10,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(5),
            multiplier: 1.5,
            strategy: BackoffStrategy::Exponential,
            jitter: true,
        }
    }

    /// Create a config for conservative retries (resource-friendly)
    pub fn conservative() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            multiplier: 3.0,
            strategy: BackoffStrategy::Exponential,
            jitter: true,
        }
    }

    /// Calculate delay for a given attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base_delay = match self.strategy {
            BackoffStrategy::Fixed => self.initial_delay,
            BackoffStrategy::Exponential => {
                let multiplied =
                    self.initial_delay.as_secs_f64() * self.multiplier.powi(attempt as i32);
                Duration::from_secs_f64(multiplied)
            }
            BackoffStrategy::Linear => {
                self.initial_delay
                    + Duration::from_secs_f64(self.initial_delay.as_secs_f64() * attempt as f64)
            }
        };

        let capped_delay = base_delay.min(self.max_delay);

        if self.jitter {
            // Add up to 25% jitter
            let jitter_range = capped_delay.as_millis() as u64 / 4;
            let jitter = if jitter_range > 0 {
                // Simple pseudo-random jitter based on attempt
                let hash = (attempt as u64).wrapping_mul(0x517cc1b727220a95);
                Duration::from_millis(hash % jitter_range)
            } else {
                Duration::ZERO
            };
            capped_delay + jitter
        } else {
            capped_delay
        }
    }
}

/// State of a connection recovery attempt
#[derive(Debug, Clone)]
pub struct ConnectionRecoveryState {
    /// Node being recovered
    pub node_id: NodeId,
    /// Current attempt number
    pub attempt: u32,
    /// When recovery started
    pub started_at: Instant,
    /// When the last attempt was made
    pub last_attempt: Option<Instant>,
    /// Whether recovery is in progress
    pub in_progress: bool,
    /// Last error encountered
    pub last_error: Option<String>,
}

impl ConnectionRecoveryState {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            attempt: 0,
            started_at: Instant::now(),
            last_attempt: None,
            in_progress: true,
            last_error: None,
        }
    }

    /// Record a failed attempt
    pub fn record_failure(&mut self, error: String) {
        self.attempt += 1;
        self.last_attempt = Some(Instant::now());
        self.last_error = Some(error);
    }

    /// Mark recovery as complete
    pub fn mark_complete(&mut self) {
        self.in_progress = false;
    }

    /// Get total recovery duration
    pub fn duration(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// Recovery event for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryEvent {
    /// Connection recovery started
    RecoveryStarted { node_id: NodeId },
    /// Retry attempt made
    RetryAttempt {
        node_id: NodeId,
        attempt: u32,
        delay: Duration,
    },
    /// Recovery succeeded
    RecoverySucceeded {
        node_id: NodeId,
        attempts: u32,
        duration: Duration,
    },
    /// Recovery failed permanently
    RecoveryFailed {
        node_id: NodeId,
        attempts: u32,
        error: String,
    },
    /// Entering degraded mode
    DegradedModeEntered { reason: String },
    /// Exiting degraded mode
    DegradedModeExited,
    /// State reconciliation started
    ReconciliationStarted { peer_count: usize },
    /// State reconciliation completed
    ReconciliationCompleted { duration: Duration },
}

/// Type alias for event handler
type EventHandler = Box<dyn Fn(RecoveryEvent) + Send + Sync>;

/// Type alias for event handlers collection
type EventHandlers = Arc<RwLock<Vec<EventHandler>>>;

/// Connection recovery manager
pub struct ConnectionRecovery {
    config: RetryConfig,
    recovery_states: Arc<RwLock<HashMap<NodeId, ConnectionRecoveryState>>>,
    event_handlers: EventHandlers,
}

impl ConnectionRecovery {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            recovery_states: Arc::new(RwLock::new(HashMap::new())),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start recovery for a disconnected peer
    pub async fn start_recovery(&self, node_id: NodeId) {
        let mut states = self.recovery_states.write().await;

        if states.contains_key(&node_id) {
            debug!("Recovery already in progress for node {}", node_id);
            return;
        }

        let state = ConnectionRecoveryState::new(node_id);
        states.insert(node_id, state);

        self.emit_event(RecoveryEvent::RecoveryStarted { node_id })
            .await;
        info!("Started connection recovery for node {}", node_id);
    }

    /// Record a failed reconnection attempt
    pub async fn record_failure(
        &self,
        node_id: NodeId,
        error: String,
    ) -> Result<Duration, RecoveryError> {
        let mut states = self.recovery_states.write().await;

        let state = states.get_mut(&node_id).ok_or_else(|| {
            RecoveryError::ReconnectionFailed("No recovery in progress".to_string())
        })?;

        state.record_failure(error.clone());

        if state.attempt >= self.config.max_retries {
            state.mark_complete();
            let attempts = state.attempt;

            self.emit_event(RecoveryEvent::RecoveryFailed {
                node_id,
                attempts,
                error,
            })
            .await;

            return Err(RecoveryError::MaxRetriesExceeded(state.attempt));
        }

        let delay = self.config.delay_for_attempt(state.attempt);

        self.emit_event(RecoveryEvent::RetryAttempt {
            node_id,
            attempt: state.attempt,
            delay,
        })
        .await;

        Ok(delay)
    }

    /// Record successful recovery
    pub async fn record_success(&self, node_id: NodeId) {
        let mut states = self.recovery_states.write().await;

        if let Some(state) = states.get_mut(&node_id) {
            let duration = state.duration();
            let attempts = state.attempt;
            state.mark_complete();

            self.emit_event(RecoveryEvent::RecoverySucceeded {
                node_id,
                attempts,
                duration,
            })
            .await;

            info!(
                "Connection recovery succeeded for node {} after {} attempts ({:?})",
                node_id, attempts, duration
            );
        }

        states.remove(&node_id);
    }

    /// Cancel recovery for a node
    pub async fn cancel_recovery(&self, node_id: NodeId) {
        let mut states = self.recovery_states.write().await;
        states.remove(&node_id);
        debug!("Cancelled recovery for node {}", node_id);
    }

    /// Get current recovery state for a node
    pub async fn get_state(&self, node_id: NodeId) -> Option<ConnectionRecoveryState> {
        let states = self.recovery_states.read().await;
        states.get(&node_id).cloned()
    }

    /// Get all nodes currently in recovery
    pub async fn recovering_nodes(&self) -> Vec<NodeId> {
        let states = self.recovery_states.read().await;
        states.keys().copied().collect()
    }

    /// Check if a node is currently recovering
    pub async fn is_recovering(&self, node_id: NodeId) -> bool {
        let states = self.recovery_states.read().await;
        states.contains_key(&node_id)
    }

    /// Register an event handler
    pub async fn on_event(&self, handler: impl Fn(RecoveryEvent) + Send + Sync + 'static) {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(Box::new(handler));
    }

    /// Emit an event to all handlers
    async fn emit_event(&self, event: RecoveryEvent) {
        let handlers = self.event_handlers.read().await;
        for handler in handlers.iter() {
            handler(event.clone());
        }
    }

    /// Get recovery statistics
    pub async fn stats(&self) -> RecoveryStats {
        let states = self.recovery_states.read().await;

        let mut total_attempts = 0u32;
        let mut in_progress = 0usize;

        for state in states.values() {
            total_attempts += state.attempt;
            if state.in_progress {
                in_progress += 1;
            }
        }

        RecoveryStats {
            nodes_recovering: in_progress,
            total_attempts,
        }
    }
}

/// Recovery statistics
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    pub nodes_recovering: usize,
    pub total_attempts: u32,
}

/// Graceful degradation manager
pub struct DegradationManager {
    /// Whether system is in degraded mode
    degraded: Arc<RwLock<bool>>,
    /// Reason for degradation
    reason: Arc<RwLock<Option<String>>>,
    /// Degradation thresholds
    thresholds: DegradationThresholds,
    /// Event handlers
    event_handlers: EventHandlers,
}

/// Thresholds for entering degraded mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationThresholds {
    /// Minimum visible nodes ratio before degradation
    pub min_visible_ratio: f64,
    /// Maximum failed operations before degradation
    pub max_failures: u32,
    /// Maximum latency (ms) before degradation
    pub max_latency_ms: u64,
}

impl Default for DegradationThresholds {
    fn default() -> Self {
        Self {
            min_visible_ratio: 0.3,
            max_failures: 10,
            max_latency_ms: 5000,
        }
    }
}

impl DegradationManager {
    pub fn new(thresholds: DegradationThresholds) -> Self {
        Self {
            degraded: Arc::new(RwLock::new(false)),
            reason: Arc::new(RwLock::new(None)),
            thresholds,
            event_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Check if system is in degraded mode
    pub async fn is_degraded(&self) -> bool {
        *self.degraded.read().await
    }

    /// Get degradation reason
    pub async fn degradation_reason(&self) -> Option<String> {
        self.reason.read().await.clone()
    }

    /// Enter degraded mode
    pub async fn enter_degraded_mode(&self, reason: String) {
        let mut degraded = self.degraded.write().await;
        if !*degraded {
            *degraded = true;
            let mut r = self.reason.write().await;
            *r = Some(reason.clone());

            warn!("Entering degraded mode: {}", reason);

            let handlers = self.event_handlers.read().await;
            for handler in handlers.iter() {
                handler(RecoveryEvent::DegradedModeEntered {
                    reason: reason.clone(),
                });
            }
        }
    }

    /// Exit degraded mode
    pub async fn exit_degraded_mode(&self) {
        let mut degraded = self.degraded.write().await;
        if *degraded {
            *degraded = false;
            let mut r = self.reason.write().await;
            *r = None;

            info!("Exiting degraded mode");

            let handlers = self.event_handlers.read().await;
            for handler in handlers.iter() {
                handler(RecoveryEvent::DegradedModeExited);
            }
        }
    }

    /// Check conditions and potentially enter/exit degraded mode
    pub async fn evaluate(&self, visible_ratio: f64, failure_count: u32, avg_latency_ms: u64) {
        let should_degrade = visible_ratio < self.thresholds.min_visible_ratio
            || failure_count > self.thresholds.max_failures
            || avg_latency_ms > self.thresholds.max_latency_ms;

        let is_degraded = self.is_degraded().await;

        if should_degrade && !is_degraded {
            let reason = if visible_ratio < self.thresholds.min_visible_ratio {
                format!("Low peer visibility: {:.1}%", visible_ratio * 100.0)
            } else if failure_count > self.thresholds.max_failures {
                format!("High failure rate: {} failures", failure_count)
            } else {
                format!("High latency: {}ms", avg_latency_ms)
            };
            self.enter_degraded_mode(reason).await;
        } else if !should_degrade && is_degraded {
            self.exit_degraded_mode().await;
        }
    }

    /// Register an event handler
    pub async fn on_event(&self, handler: impl Fn(RecoveryEvent) + Send + Sync + 'static) {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(Box::new(handler));
    }

    /// Get thresholds
    pub fn thresholds(&self) -> &DegradationThresholds {
        &self.thresholds
    }
}

/// State reconciliation coordinator
pub struct StateReconciler {
    /// Last reconciliation time
    last_reconciliation: Arc<RwLock<Option<Instant>>>,
    /// Event handlers
    event_handlers: EventHandlers,
}

impl StateReconciler {
    pub fn new() -> Self {
        Self {
            last_reconciliation: Arc::new(RwLock::new(None)),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start state reconciliation with peers
    pub async fn start_reconciliation(&self, peer_count: usize) {
        info!("Starting state reconciliation with {} peers", peer_count);

        let handlers = self.event_handlers.read().await;
        for handler in handlers.iter() {
            handler(RecoveryEvent::ReconciliationStarted { peer_count });
        }
    }

    /// Complete state reconciliation
    pub async fn complete_reconciliation(&self, duration: Duration) {
        let mut last = self.last_reconciliation.write().await;
        *last = Some(Instant::now());

        info!("State reconciliation completed in {:?}", duration);

        let handlers = self.event_handlers.read().await;
        for handler in handlers.iter() {
            handler(RecoveryEvent::ReconciliationCompleted { duration });
        }
    }

    /// Get time since last reconciliation
    pub async fn time_since_last(&self) -> Option<Duration> {
        let last = self.last_reconciliation.read().await;
        last.map(|t| t.elapsed())
    }

    /// Register an event handler
    pub async fn on_event(&self, handler: impl Fn(RecoveryEvent) + Send + Sync + 'static) {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(Box::new(handler));
    }
}

impl Default for StateReconciler {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry executor for generic operations
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Execute an operation with retry
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<T, RecoveryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut attempt = 0u32;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempt += 1;

                    if attempt >= self.config.max_retries {
                        return Err(RecoveryError::MaxRetriesExceeded(attempt));
                    }

                    let delay = self.config.delay_for_attempt(attempt);
                    debug!(
                        "Operation failed (attempt {}): {}. Retrying in {:?}",
                        attempt, e, delay
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Execute with timeout
    pub async fn execute_with_timeout<F, Fut, T, E>(
        &self,
        operation: F,
        timeout: Duration,
    ) -> Result<T, RecoveryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        tokio::time::timeout(timeout, self.execute(operation))
            .await
            .map_err(|_| RecoveryError::Timeout)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.strategy, BackoffStrategy::Exponential);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_aggressive() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_retries, 10);
        assert!(config.initial_delay < Duration::from_millis(100));
    }

    #[test]
    fn test_retry_config_conservative() {
        let config = RetryConfig::conservative();
        assert_eq!(config.max_retries, 3);
        assert!(config.initial_delay >= Duration::from_secs(1));
    }

    #[test]
    fn test_fixed_backoff_delay() {
        let config = RetryConfig {
            strategy: BackoffStrategy::Fixed,
            initial_delay: Duration::from_millis(100),
            jitter: false,
            ..Default::default()
        };

        // All delays should be the same for fixed strategy
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(5), Duration::from_millis(100));
    }

    #[test]
    fn test_exponential_backoff_delay() {
        let config = RetryConfig {
            strategy: BackoffStrategy::Exponential,
            initial_delay: Duration::from_millis(100),
            multiplier: 2.0,
            max_delay: Duration::from_secs(10),
            jitter: false,
            ..Default::default()
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn test_linear_backoff_delay() {
        let config = RetryConfig {
            strategy: BackoffStrategy::Linear,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: false,
            ..Default::default()
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(300));
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig {
            strategy: BackoffStrategy::Exponential,
            initial_delay: Duration::from_millis(100),
            multiplier: 10.0,
            max_delay: Duration::from_secs(1),
            jitter: false,
            ..Default::default()
        };

        // Should be capped at max_delay
        assert_eq!(config.delay_for_attempt(5), Duration::from_secs(1));
    }

    #[test]
    fn test_connection_recovery_state() {
        let node_id = NodeId::new_v4();
        let mut state = ConnectionRecoveryState::new(node_id);

        assert_eq!(state.attempt, 0);
        assert!(state.in_progress);
        assert!(state.last_error.is_none());

        state.record_failure("Connection refused".to_string());
        assert_eq!(state.attempt, 1);
        assert_eq!(state.last_error, Some("Connection refused".to_string()));

        state.mark_complete();
        assert!(!state.in_progress);
    }

    #[tokio::test]
    async fn test_connection_recovery_start() {
        let recovery = ConnectionRecovery::new(RetryConfig::default());
        let node_id = NodeId::new_v4();

        recovery.start_recovery(node_id).await;

        assert!(recovery.is_recovering(node_id).await);

        let recovering = recovery.recovering_nodes().await;
        assert!(recovering.contains(&node_id));
    }

    #[tokio::test]
    async fn test_connection_recovery_success() {
        let recovery = ConnectionRecovery::new(RetryConfig::default());
        let node_id = NodeId::new_v4();

        recovery.start_recovery(node_id).await;
        recovery.record_success(node_id).await;

        assert!(!recovery.is_recovering(node_id).await);
    }

    #[tokio::test]
    async fn test_connection_recovery_max_retries() {
        let config = RetryConfig {
            max_retries: 3,
            ..Default::default()
        };
        let recovery = ConnectionRecovery::new(config);
        let node_id = NodeId::new_v4();

        recovery.start_recovery(node_id).await;

        // First two failures should return Ok with delay
        assert!(recovery
            .record_failure(node_id, "error".to_string())
            .await
            .is_ok());
        assert!(recovery
            .record_failure(node_id, "error".to_string())
            .await
            .is_ok());

        // Third failure should exceed max retries
        let result = recovery.record_failure(node_id, "error".to_string()).await;
        assert!(matches!(result, Err(RecoveryError::MaxRetriesExceeded(3))));
    }

    #[tokio::test]
    async fn test_connection_recovery_cancel() {
        let recovery = ConnectionRecovery::new(RetryConfig::default());
        let node_id = NodeId::new_v4();

        recovery.start_recovery(node_id).await;
        assert!(recovery.is_recovering(node_id).await);

        recovery.cancel_recovery(node_id).await;
        assert!(!recovery.is_recovering(node_id).await);
    }

    #[tokio::test]
    async fn test_degradation_manager_not_degraded() {
        let manager = DegradationManager::new(DegradationThresholds::default());

        assert!(!manager.is_degraded().await);
        assert!(manager.degradation_reason().await.is_none());
    }

    #[tokio::test]
    async fn test_degradation_manager_enter_exit() {
        let manager = DegradationManager::new(DegradationThresholds::default());

        manager.enter_degraded_mode("Test reason".to_string()).await;
        assert!(manager.is_degraded().await);
        assert_eq!(
            manager.degradation_reason().await,
            Some("Test reason".to_string())
        );

        manager.exit_degraded_mode().await;
        assert!(!manager.is_degraded().await);
        assert!(manager.degradation_reason().await.is_none());
    }

    #[tokio::test]
    async fn test_degradation_evaluate_low_visibility() {
        let thresholds = DegradationThresholds {
            min_visible_ratio: 0.5,
            max_failures: 100,
            max_latency_ms: 10000,
        };
        let manager = DegradationManager::new(thresholds);

        // Low visibility should trigger degradation
        manager.evaluate(0.3, 0, 100).await;
        assert!(manager.is_degraded().await);

        // High visibility should exit degradation
        manager.evaluate(0.8, 0, 100).await;
        assert!(!manager.is_degraded().await);
    }

    #[tokio::test]
    async fn test_degradation_evaluate_high_failures() {
        let thresholds = DegradationThresholds {
            min_visible_ratio: 0.1,
            max_failures: 10,
            max_latency_ms: 10000,
        };
        let manager = DegradationManager::new(thresholds);

        // High failures should trigger degradation
        manager.evaluate(0.9, 15, 100).await;
        assert!(manager.is_degraded().await);
    }

    #[tokio::test]
    async fn test_degradation_evaluate_high_latency() {
        let thresholds = DegradationThresholds {
            min_visible_ratio: 0.1,
            max_failures: 100,
            max_latency_ms: 1000,
        };
        let manager = DegradationManager::new(thresholds);

        // High latency should trigger degradation
        manager.evaluate(0.9, 0, 2000).await;
        assert!(manager.is_degraded().await);
    }

    #[tokio::test]
    async fn test_state_reconciler() {
        let reconciler = StateReconciler::new();

        assert!(reconciler.time_since_last().await.is_none());

        reconciler.start_reconciliation(5).await;
        reconciler
            .complete_reconciliation(Duration::from_millis(100))
            .await;

        assert!(reconciler.time_since_last().await.is_some());
    }

    #[tokio::test]
    async fn test_retry_executor_success() {
        let executor = RetryExecutor::new(RetryConfig::default());
        let mut call_count = 0u32;

        let result: Result<u32, RecoveryError> = executor
            .execute(|| {
                call_count += 1;
                async move { Ok::<u32, &str>(42) }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count, 1);
    }

    #[tokio::test]
    async fn test_retry_executor_eventual_success() {
        let executor = RetryExecutor::new(RetryConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            jitter: false,
            ..Default::default()
        });

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count_clone = call_count.clone();

        let result: Result<u32, RecoveryError> = executor
            .execute(move || {
                let count = count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    if count < 2 {
                        Err("not yet")
                    } else {
                        Ok(42u32)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn test_recovery_stats_default() {
        let stats = RecoveryStats::default();
        assert_eq!(stats.nodes_recovering, 0);
        assert_eq!(stats.total_attempts, 0);
    }

    #[test]
    fn test_degradation_thresholds_default() {
        let thresholds = DegradationThresholds::default();
        assert!(thresholds.min_visible_ratio > 0.0);
        assert!(thresholds.max_failures > 0);
        assert!(thresholds.max_latency_ms > 0);
    }
}
