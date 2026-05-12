//! Connection Lifecycle Hooks
//!
//! Provides a callback-based system for reacting to connection lifecycle events.
//! Users can register hooks that execute custom logic when connections are
//! established, degraded, recovered, or removed.

use crate::peer_lifecycle::{PeerLifecycleEvent, PeerLifecycleManager};
use crate::WireError;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Hook function type for connection events
pub type HookFn = Arc<dyn Fn(HookContext) + Send + Sync>;

/// Context passed to hook functions
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Node ID of the peer
    pub node_id: [u8; 16],
    /// Socket address of the peer
    pub address: SocketAddr,
    /// Event type
    pub event_type: HookEventType,
    /// Additional context data
    pub metadata: Option<String>,
}

/// Types of lifecycle events that can trigger hooks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEventType {
    /// Connection established
    Connected,
    /// Connection activated (became healthy)
    Activated,
    /// Connection degraded (poor health)
    Degraded,
    /// Connection suspected dead
    Suspected,
    /// Connection confirmed dead
    Dead,
    /// Connection removed from mesh
    Removed,
    /// Connection recovered from degraded/suspected state
    Recovered,
}

impl HookEventType {
    /// Get a string representation of the event type
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Activated => "activated",
            Self::Degraded => "degraded",
            Self::Suspected => "suspected",
            Self::Dead => "dead",
            Self::Removed => "removed",
            Self::Recovered => "recovered",
        }
    }
}

/// Hook execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookExecutionMode {
    /// Execute hooks synchronously (blocks event processing)
    Sync,
    /// Execute hooks asynchronously (non-blocking)
    Async,
}

/// Hook registration entry
#[derive(Clone)]
struct Hook {
    id: usize,
    event_types: Vec<HookEventType>,
    callback: HookFn,
    execution_mode: HookExecutionMode,
}

/// Manager for connection lifecycle hooks
pub struct LifecycleHookManager {
    hooks: Arc<RwLock<Vec<Hook>>>,
    next_hook_id: Arc<RwLock<usize>>,
    lifecycle_manager: Arc<PeerLifecycleManager>,
}

impl LifecycleHookManager {
    /// Create a new lifecycle hook manager
    pub fn new(lifecycle_manager: Arc<PeerLifecycleManager>) -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
            next_hook_id: Arc::new(RwLock::new(0)),
            lifecycle_manager,
        }
    }

    /// Register a hook for specific event types
    pub async fn register_hook(
        &self,
        event_types: Vec<HookEventType>,
        callback: HookFn,
        execution_mode: HookExecutionMode,
    ) -> Result<usize, WireError> {
        if event_types.is_empty() {
            return Err(WireError::InvalidInput(
                "At least one event type must be specified".to_string(),
            ));
        }

        let mut hooks = self.hooks.write().await;
        let mut next_id = self.next_hook_id.write().await;

        let hook_id = *next_id;
        *next_id += 1;

        hooks.push(Hook {
            id: hook_id,
            event_types,
            callback,
            execution_mode,
        });

        debug!(
            "Registered hook {} for events: {:?}",
            hook_id,
            hooks.last().expect("hook was just pushed").event_types
        );

        Ok(hook_id)
    }

    /// Unregister a hook by ID
    pub async fn unregister_hook(&self, hook_id: usize) -> Result<(), WireError> {
        let mut hooks = self.hooks.write().await;

        let initial_len = hooks.len();
        hooks.retain(|h| h.id != hook_id);

        if hooks.len() == initial_len {
            return Err(WireError::InvalidInput(format!(
                "Hook {} not found",
                hook_id
            )));
        }

        debug!("Unregistered hook {}", hook_id);
        Ok(())
    }

    /// Start processing lifecycle events and executing hooks
    pub async fn start(&self) -> Result<(), WireError> {
        let mut event_rx = self.lifecycle_manager.subscribe().await;
        let hooks = self.hooks.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let (event_type, node_id, address, metadata) = match event {
                    PeerLifecycleEvent::Discovered { node_id, address } => (
                        HookEventType::Connected,
                        node_id,
                        address,
                        Some("peer discovered".to_string()),
                    ),
                    PeerLifecycleEvent::Activated { node_id, address } => {
                        (HookEventType::Activated, node_id, address, None)
                    }
                    PeerLifecycleEvent::Degraded {
                        node_id,
                        address,
                        reason,
                    } => (HookEventType::Degraded, node_id, address, Some(reason)),
                    PeerLifecycleEvent::Suspected { node_id, address } => {
                        (HookEventType::Suspected, node_id, address, None)
                    }
                    PeerLifecycleEvent::Dead { node_id, address } => {
                        (HookEventType::Dead, node_id, address, None)
                    }
                    PeerLifecycleEvent::Removed {
                        node_id,
                        address,
                        reason,
                    } => (HookEventType::Removed, node_id, address, Some(reason)),
                    PeerLifecycleEvent::Recovered { node_id, address } => {
                        (HookEventType::Recovered, node_id, address, None)
                    }
                };

                let context = HookContext {
                    node_id,
                    address,
                    event_type,
                    metadata,
                };

                // Execute matching hooks
                let hooks_snapshot = hooks.read().await.clone();
                for hook in hooks_snapshot {
                    if hook.event_types.contains(&event_type) {
                        let callback = hook.callback.clone();
                        let ctx = context.clone();

                        match hook.execution_mode {
                            HookExecutionMode::Sync => {
                                // Execute synchronously
                                callback(ctx);
                            }
                            HookExecutionMode::Async => {
                                // Execute asynchronously
                                tokio::spawn(async move {
                                    callback(ctx);
                                });
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Get the number of registered hooks
    pub async fn hook_count(&self) -> usize {
        self.hooks.read().await.len()
    }

    /// Clear all registered hooks
    pub async fn clear_hooks(&self) {
        let mut hooks = self.hooks.write().await;
        hooks.clear();
        debug!("Cleared all lifecycle hooks");
    }
}

/// Builder for convenient hook registration
pub struct HookBuilder {
    event_types: Vec<HookEventType>,
    execution_mode: HookExecutionMode,
}

impl HookBuilder {
    /// Create a new hook builder
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            execution_mode: HookExecutionMode::Async,
        }
    }

    /// Register for connection established events
    pub fn on_connected(mut self) -> Self {
        self.event_types.push(HookEventType::Connected);
        self
    }

    /// Register for activation events
    pub fn on_activated(mut self) -> Self {
        self.event_types.push(HookEventType::Activated);
        self
    }

    /// Register for degraded events
    pub fn on_degraded(mut self) -> Self {
        self.event_types.push(HookEventType::Degraded);
        self
    }

    /// Register for suspected events
    pub fn on_suspected(mut self) -> Self {
        self.event_types.push(HookEventType::Suspected);
        self
    }

    /// Register for dead events
    pub fn on_dead(mut self) -> Self {
        self.event_types.push(HookEventType::Dead);
        self
    }

    /// Register for removed events
    pub fn on_removed(mut self) -> Self {
        self.event_types.push(HookEventType::Removed);
        self
    }

    /// Register for recovered events
    pub fn on_recovered(mut self) -> Self {
        self.event_types.push(HookEventType::Recovered);
        self
    }

    /// Register for all events
    pub fn on_all_events(mut self) -> Self {
        self.event_types = vec![
            HookEventType::Connected,
            HookEventType::Activated,
            HookEventType::Degraded,
            HookEventType::Suspected,
            HookEventType::Dead,
            HookEventType::Removed,
            HookEventType::Recovered,
        ];
        self
    }

    /// Set execution mode to synchronous
    pub fn sync(mut self) -> Self {
        self.execution_mode = HookExecutionMode::Sync;
        self
    }

    /// Set execution mode to asynchronous (default)
    pub fn async_mode(mut self) -> Self {
        self.execution_mode = HookExecutionMode::Async;
        self
    }

    /// Build and register the hook
    pub async fn build<F>(
        self,
        manager: &LifecycleHookManager,
        callback: F,
    ) -> Result<usize, WireError>
    where
        F: Fn(HookContext) + Send + Sync + 'static,
    {
        manager
            .register_hook(self.event_types, Arc::new(callback), self.execution_mode)
            .await
    }
}

impl Default for HookBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper trait for creating hook contexts
pub trait IntoHookContext {
    fn into_hook_context(self, event_type: HookEventType) -> HookContext;
}

impl IntoHookContext for ([u8; 16], SocketAddr) {
    fn into_hook_context(self, event_type: HookEventType) -> HookContext {
        HookContext {
            node_id: self.0,
            address: self.1,
            event_type,
            metadata: None,
        }
    }
}

impl IntoHookContext for ([u8; 16], SocketAddr, String) {
    fn into_hook_context(self, event_type: HookEventType) -> HookContext {
        HookContext {
            node_id: self.0,
            address: self.1,
            event_type,
            metadata: Some(self.2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{DiscoveryConfig, DiscoveryService};
    use crate::health::{HealthConfig, HealthMonitor};
    use crate::peer_lifecycle::PeerLifecycleConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    async fn create_test_setup() -> (Arc<PeerLifecycleManager>, Arc<LifecycleHookManager>) {
        let discovery_config = DiscoveryConfig::default();
        let local_id = [0u8; 16];
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health_config = HealthConfig::default();
        let health = Arc::new(HealthMonitor::with_config(health_config));

        let lifecycle_config = PeerLifecycleConfig::new();
        let lifecycle_manager = Arc::new(PeerLifecycleManager::new(
            lifecycle_config,
            discovery,
            health,
        ));

        let hook_manager = Arc::new(LifecycleHookManager::new(lifecycle_manager.clone()));

        (lifecycle_manager, hook_manager)
    }

    #[tokio::test]
    async fn test_hook_registration() {
        let (_, hook_manager) = create_test_setup().await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let hook_id = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(move |_ctx| {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        assert_eq!(hook_manager.hook_count().await, 1);
        assert!(hook_id == 0);
    }

    #[tokio::test]
    async fn test_hook_unregistration() {
        let (_, hook_manager) = create_test_setup().await;

        let hook_id = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(|_ctx| {}),
                HookExecutionMode::Async,
            )
            .await
            .unwrap();

        assert_eq!(hook_manager.hook_count().await, 1);

        hook_manager.unregister_hook(hook_id).await.unwrap();
        assert_eq!(hook_manager.hook_count().await, 0);
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let (lifecycle_manager, hook_manager) = create_test_setup().await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _hook_id = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(move |_ctx| {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        // Start hook processing
        hook_manager.start().await.unwrap();

        // Wait a bit for event processing to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Register a peer to trigger event
        let node_id = [1u8; 16];
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        lifecycle_manager.register_peer(node_id, addr).await;

        // Wait for hook to execute
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_hook_builder() {
        let (_, hook_manager) = create_test_setup().await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _hook_id = HookBuilder::new()
            .on_connected()
            .on_degraded()
            .sync()
            .build(&hook_manager, move |_ctx| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .await
            .unwrap();

        assert_eq!(hook_manager.hook_count().await, 1);
    }

    #[tokio::test]
    async fn test_multiple_hooks() {
        let (lifecycle_manager, hook_manager) = create_test_setup().await;

        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        let c1 = counter1.clone();
        let c2 = counter2.clone();

        // Register two hooks for the same event
        let _hook1 = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(move |_ctx| {
                    c1.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        let _hook2 = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(move |_ctx| {
                    c2.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        hook_manager.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Register a peer
        let node_id = [2u8; 16];
        let addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        lifecycle_manager.register_peer(node_id, addr).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Both hooks should have been called
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_hook_event_filtering() {
        let (lifecycle_manager, hook_manager) = create_test_setup().await;

        let connected_count = Arc::new(AtomicUsize::new(0));
        let degraded_count = Arc::new(AtomicUsize::new(0));

        let c1 = connected_count.clone();
        let c2 = degraded_count.clone();

        // Hook only for connected events
        let _hook1 = hook_manager
            .register_hook(
                vec![HookEventType::Connected],
                Arc::new(move |_ctx| {
                    c1.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        // Hook only for degraded events
        let _hook2 = hook_manager
            .register_hook(
                vec![HookEventType::Degraded],
                Arc::new(move |_ctx| {
                    c2.fetch_add(1, Ordering::SeqCst);
                }),
                HookExecutionMode::Sync,
            )
            .await
            .unwrap();

        hook_manager.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Register a peer (triggers connected event)
        let node_id = [3u8; 16];
        let addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        lifecycle_manager.register_peer(node_id, addr).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Only connected hook should be called
        assert_eq!(connected_count.load(Ordering::SeqCst), 1);
        assert_eq!(degraded_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_clear_hooks() {
        let (_, hook_manager) = create_test_setup().await;

        // Register multiple hooks
        for _ in 0..5 {
            hook_manager
                .register_hook(
                    vec![HookEventType::Connected],
                    Arc::new(|_ctx| {}),
                    HookExecutionMode::Async,
                )
                .await
                .unwrap();
        }

        assert_eq!(hook_manager.hook_count().await, 5);

        hook_manager.clear_hooks().await;
        assert_eq!(hook_manager.hook_count().await, 0);
    }

    #[test]
    fn test_hook_event_type_as_str() {
        assert_eq!(HookEventType::Connected.as_str(), "connected");
        assert_eq!(HookEventType::Degraded.as_str(), "degraded");
        assert_eq!(HookEventType::Dead.as_str(), "dead");
    }

    #[test]
    fn test_into_hook_context() {
        let node_id = [1u8; 16];
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

        let ctx = (node_id, addr).into_hook_context(HookEventType::Connected);
        assert_eq!(ctx.node_id, node_id);
        assert_eq!(ctx.address, addr);
        assert_eq!(ctx.event_type, HookEventType::Connected);
        assert!(ctx.metadata.is_none());

        let ctx_with_meta =
            (node_id, addr, "test reason".to_string()).into_hook_context(HookEventType::Degraded);
        assert_eq!(ctx_with_meta.metadata, Some("test reason".to_string()));
    }
}
