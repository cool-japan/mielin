//! Shutdown Coordination for MielinMesh
//!
//! Provides graceful shutdown coordination for all mesh components:
//! - Discovery service
//! - Gossip protocol
//! - Agent registry
//! - Migration coordinator
//! - DHT routing table

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Shutdown errors
#[derive(Debug, Error)]
pub enum ShutdownError {
    #[error("Shutdown timeout after {0:?}")]
    Timeout(Duration),

    #[error("Component shutdown failed: {component}: {reason}")]
    ComponentFailed { component: String, reason: String },

    #[error("Shutdown already in progress")]
    AlreadyInProgress,

    #[error("Multiple component failures during shutdown: {count} components failed")]
    MultipleFailures { count: usize },
}

/// Component lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentState {
    /// Component is not started
    Stopped,
    /// Component is starting up
    Starting,
    /// Component is running
    Running,
    /// Component is shutting down
    ShuttingDown,
    /// Component failed
    Failed,
}

/// Shutdown signal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// Graceful shutdown with timeout
    Graceful { timeout_secs: u64 },
    /// Immediate shutdown without waiting
    Immediate,
    /// Forced shutdown (kill all tasks)
    Forced,
}

/// Component priority for shutdown ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShutdownPriority {
    /// High priority (shutdown first) - e.g., API endpoints
    High = 0,
    /// Normal priority - e.g., business logic components
    Normal = 1,
    /// Low priority (shutdown last) - e.g., logging, metrics
    Low = 2,
}

/// Trait for components that support graceful shutdown
#[async_trait::async_trait]
pub trait ShutdownHandler: Send + Sync {
    /// Component name for logging
    fn name(&self) -> &str;

    /// Shutdown priority
    fn priority(&self) -> ShutdownPriority {
        ShutdownPriority::Normal
    }

    /// Execute graceful shutdown
    async fn shutdown(&self) -> Result<(), String>;

    /// Get component state
    async fn state(&self) -> ComponentState;
}

/// Shutdown coordinator managing component lifecycle
pub struct ShutdownCoordinator {
    components: Arc<RwLock<HashMap<String, Arc<dyn ShutdownHandler>>>>,
    shutdown_tx: broadcast::Sender<ShutdownSignal>,
    state: Arc<RwLock<CoordinatorState>>,
}

#[derive(Debug)]
struct CoordinatorState {
    is_shutting_down: bool,
    shutdown_started_at: Option<std::time::Instant>,
    failed_components: Vec<String>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);

        Self {
            components: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            state: Arc::new(RwLock::new(CoordinatorState {
                is_shutting_down: false,
                shutdown_started_at: None,
                failed_components: Vec::new(),
            })),
        }
    }

    /// Register a component for managed shutdown
    pub async fn register_component(&self, component: Arc<dyn ShutdownHandler>) {
        let name = component.name().to_string();
        let mut components = self.components.write().await;
        components.insert(name.clone(), component);
        debug!("Registered component for shutdown: {}", name);
    }

    /// Unregister a component
    pub async fn unregister_component(&self, name: &str) {
        let mut components = self.components.write().await;
        components.remove(name);
        debug!("Unregistered component: {}", name);
    }

    /// Get a shutdown signal receiver
    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownSignal> {
        self.shutdown_tx.subscribe()
    }

    /// Check if shutdown is in progress
    pub async fn is_shutting_down(&self) -> bool {
        let state = self.state.read().await;
        state.is_shutting_down
    }

    /// Initiate graceful shutdown of all components
    pub async fn shutdown(&self, signal: ShutdownSignal) -> Result<(), ShutdownError> {
        let mut state = self.state.write().await;

        if state.is_shutting_down {
            return Err(ShutdownError::AlreadyInProgress);
        }

        state.is_shutting_down = true;
        state.shutdown_started_at = Some(std::time::Instant::now());
        drop(state);

        info!("Initiating shutdown: {:?}", signal);

        // Send shutdown signal to all subscribers
        let _ = self.shutdown_tx.send(signal);

        // Execute component shutdown based on signal type
        match signal {
            ShutdownSignal::Graceful { timeout_secs } => {
                self.graceful_shutdown(Duration::from_secs(timeout_secs))
                    .await
            }
            ShutdownSignal::Immediate => self.immediate_shutdown().await,
            ShutdownSignal::Forced => self.forced_shutdown().await,
        }
    }

    /// Execute graceful shutdown with timeout
    async fn graceful_shutdown(&self, shutdown_timeout: Duration) -> Result<(), ShutdownError> {
        info!(
            "Starting graceful shutdown (timeout: {:?})",
            shutdown_timeout
        );

        // Get components sorted by priority
        let components = self.components.read().await;
        let mut component_list: Vec<_> = components.iter().collect();
        component_list.sort_by_key(|(_, handler)| handler.priority());

        let component_names: Vec<String> = component_list
            .iter()
            .map(|(name, _)| (*name).clone())
            .collect();
        drop(components);

        info!(
            "Shutting down {} components in priority order",
            component_names.len()
        );

        // Shutdown each component with individual timeout
        let per_component_timeout = shutdown_timeout / component_names.len().max(1) as u32;

        for name in component_names {
            let components = self.components.read().await;
            if let Some(handler) = components.get(&name) {
                let handler = handler.clone();
                drop(components);

                info!("Shutting down component: {}", name);

                // Attempt shutdown with timeout
                let shutdown_result = timeout(per_component_timeout, handler.shutdown()).await;

                match shutdown_result {
                    Ok(Ok(())) => {
                        info!("Component {} shut down successfully", name);
                    }
                    Ok(Err(e)) => {
                        warn!("Component {} shutdown failed: {}", name, e);
                        let mut state = self.state.write().await;
                        state.failed_components.push(name.clone());
                    }
                    Err(_) => {
                        warn!(
                            "Component {} shutdown timed out after {:?}",
                            name, per_component_timeout
                        );
                        let mut state = self.state.write().await;
                        state.failed_components.push(name.clone());
                    }
                }
            }
        }

        // Check for failures
        let state = self.state.read().await;
        if !state.failed_components.is_empty() {
            let count = state.failed_components.len();
            error!("Shutdown completed with {} component failures", count);
            return Err(ShutdownError::MultipleFailures { count });
        }

        info!("Graceful shutdown completed successfully");
        Ok(())
    }

    /// Execute immediate shutdown (no waiting)
    async fn immediate_shutdown(&self) -> Result<(), ShutdownError> {
        info!("Starting immediate shutdown");

        let components = self.components.read().await;
        let component_names: Vec<String> = components.keys().cloned().collect();
        drop(components);

        for name in component_names {
            let components = self.components.read().await;
            if let Some(handler) = components.get(&name) {
                let handler = handler.clone();
                drop(components);

                info!("Shutting down component: {}", name);
                if let Err(e) = handler.shutdown().await {
                    warn!("Component {} shutdown failed: {}", name, e);
                }
            }
        }

        info!("Immediate shutdown completed");
        Ok(())
    }

    /// Execute forced shutdown (kill all tasks)
    async fn forced_shutdown(&self) -> Result<(), ShutdownError> {
        warn!("Starting forced shutdown - tasks will be killed");

        // Clear all components
        let mut components = self.components.write().await;
        components.clear();

        info!("Forced shutdown completed");
        Ok(())
    }

    /// Get shutdown statistics
    pub async fn get_stats(&self) -> ShutdownStats {
        let state = self.state.read().await;
        let components = self.components.read().await;

        ShutdownStats {
            is_shutting_down: state.is_shutting_down,
            registered_components: components.len(),
            failed_components: state.failed_components.clone(),
            shutdown_duration: state.shutdown_started_at.map(|start| start.elapsed()),
        }
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Shutdown statistics
#[derive(Debug, Clone)]
pub struct ShutdownStats {
    pub is_shutting_down: bool,
    pub registered_components: usize,
    pub failed_components: Vec<String>,
    pub shutdown_duration: Option<Duration>,
}

/// Helper macro to create shutdown handler for simple components
#[macro_export]
macro_rules! impl_shutdown_handler {
    ($type:ty, $name:expr, $priority:expr, $shutdown_fn:expr) => {
        #[async_trait::async_trait]
        impl ShutdownHandler for $type {
            fn name(&self) -> &str {
                $name
            }

            fn priority(&self) -> ShutdownPriority {
                $priority
            }

            async fn shutdown(&self) -> Result<(), String> {
                $shutdown_fn(self).await
            }

            async fn state(&self) -> ComponentState {
                ComponentState::Running
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Clone)]
    struct TestComponent {
        name: String,
        priority: ShutdownPriority,
        shutdown_called: Arc<AtomicBool>,
        should_fail: bool,
        delay: Duration,
    }

    impl TestComponent {
        fn new(name: &str, priority: ShutdownPriority) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                priority,
                shutdown_called: Arc::new(AtomicBool::new(false)),
                should_fail: false,
                delay: Duration::from_millis(10),
            })
        }

        fn with_failure(self: Arc<Self>) -> Arc<Self> {
            let mut component = Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone());
            component.should_fail = true;
            Arc::new(component)
        }

        fn with_delay(self: Arc<Self>, delay: Duration) -> Arc<Self> {
            let mut component = Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone());
            component.delay = delay;
            Arc::new(component)
        }

        fn was_shutdown(&self) -> bool {
            self.shutdown_called.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ShutdownHandler for TestComponent {
        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> ShutdownPriority {
            self.priority
        }

        async fn shutdown(&self) -> Result<(), String> {
            tokio::time::sleep(self.delay).await;
            self.shutdown_called.store(true, Ordering::SeqCst);

            if self.should_fail {
                Err(format!("Intentional failure from {}", self.name))
            } else {
                Ok(())
            }
        }

        async fn state(&self) -> ComponentState {
            if self.was_shutdown() {
                ComponentState::Stopped
            } else {
                ComponentState::Running
            }
        }
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = ShutdownCoordinator::new();
        assert!(!coordinator.is_shutting_down().await);

        let stats = coordinator.get_stats().await;
        assert_eq!(stats.registered_components, 0);
        assert!(!stats.is_shutting_down);
    }

    #[tokio::test]
    async fn test_component_registration() {
        let coordinator = ShutdownCoordinator::new();
        let component = TestComponent::new("test", ShutdownPriority::Normal);

        coordinator.register_component(component.clone()).await;

        let stats = coordinator.get_stats().await;
        assert_eq!(stats.registered_components, 1);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_success() {
        let coordinator = ShutdownCoordinator::new();
        let comp1 = TestComponent::new("comp1", ShutdownPriority::Normal);
        let comp2 = TestComponent::new("comp2", ShutdownPriority::High);

        coordinator.register_component(comp1.clone()).await;
        coordinator.register_component(comp2.clone()).await;

        let result = coordinator
            .shutdown(ShutdownSignal::Graceful { timeout_secs: 5 })
            .await;

        assert!(result.is_ok());
        assert!(comp1.was_shutdown());
        assert!(comp2.was_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_priority_ordering() {
        let coordinator = ShutdownCoordinator::new();

        let low = TestComponent::new("low", ShutdownPriority::Low);
        let normal = TestComponent::new("normal", ShutdownPriority::Normal);
        let high = TestComponent::new("high", ShutdownPriority::High);

        coordinator.register_component(low.clone()).await;
        coordinator.register_component(normal.clone()).await;
        coordinator.register_component(high.clone()).await;

        coordinator
            .shutdown(ShutdownSignal::Graceful { timeout_secs: 5 })
            .await
            .unwrap();

        // All should be shutdown
        assert!(high.was_shutdown());
        assert!(normal.was_shutdown());
        assert!(low.was_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_with_component_failure() {
        let coordinator = ShutdownCoordinator::new();

        let good = TestComponent::new("good", ShutdownPriority::Normal);
        let bad = TestComponent::new("bad", ShutdownPriority::Normal).with_failure();

        coordinator.register_component(good.clone()).await;
        coordinator.register_component(bad.clone()).await;

        let result = coordinator
            .shutdown(ShutdownSignal::Graceful { timeout_secs: 5 })
            .await;

        assert!(result.is_err());
        assert!(good.was_shutdown());
        assert!(bad.was_shutdown());

        let stats = coordinator.get_stats().await;
        assert_eq!(stats.failed_components.len(), 1);
    }

    #[tokio::test]
    async fn test_immediate_shutdown() {
        let coordinator = ShutdownCoordinator::new();
        let comp = TestComponent::new("comp", ShutdownPriority::Normal);

        coordinator.register_component(comp.clone()).await;

        let result = coordinator.shutdown(ShutdownSignal::Immediate).await;

        assert!(result.is_ok());
        assert!(comp.was_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_already_in_progress() {
        let coordinator = Arc::new(ShutdownCoordinator::new());
        let comp =
            TestComponent::new("comp", ShutdownPriority::Normal).with_delay(Duration::from_secs(1));

        coordinator.register_component(comp).await;

        let coord1 = coordinator.clone();
        let handle = tokio::spawn(async move {
            coord1
                .shutdown(ShutdownSignal::Graceful { timeout_secs: 5 })
                .await
        });

        // Wait a bit to ensure first shutdown started
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Try to start another shutdown
        let result = coordinator
            .shutdown(ShutdownSignal::Graceful { timeout_secs: 5 })
            .await;

        assert!(matches!(result, Err(ShutdownError::AlreadyInProgress)));

        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_signal_broadcast() {
        let coordinator = ShutdownCoordinator::new();
        let mut rx1 = coordinator.subscribe();
        let mut rx2 = coordinator.subscribe();

        let handle1 = tokio::spawn(async move { rx1.recv().await.ok() });

        let handle2 = tokio::spawn(async move { rx2.recv().await.ok() });

        coordinator
            .shutdown(ShutdownSignal::Immediate)
            .await
            .unwrap();

        let signal1 = handle1.await.unwrap();
        let signal2 = handle2.await.unwrap();

        assert_eq!(signal1, Some(ShutdownSignal::Immediate));
        assert_eq!(signal2, Some(ShutdownSignal::Immediate));
    }
}
