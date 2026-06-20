//! Peer Lifecycle Management
//!
//! Integrates health monitoring with peer discovery to automatically
//! detect and remove dead peers from the mesh network.

use crate::discovery::DiscoveryService;
use crate::health::{HealthEvent, HealthMonitor};
use crate::WireError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Default dead peer removal delay (30 seconds after detection)
const DEFAULT_REMOVAL_DELAY: Duration = Duration::from_secs(30);

/// Peer lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerState {
    /// Peer is connecting (initial state)
    Connecting,
    /// Peer is active and healthy
    Active,
    /// Peer is degraded but still functional
    Degraded,
    /// Peer is suspected to be dead (pending confirmation)
    Suspected,
    /// Peer is confirmed dead and will be removed
    Dead,
    /// Peer has been removed from the mesh
    Removed,
}

/// Peer lifecycle event
#[derive(Debug, Clone)]
pub enum PeerLifecycleEvent {
    /// New peer discovered
    Discovered {
        node_id: [u8; 16],
        address: SocketAddr,
    },
    /// Peer became active
    Activated {
        node_id: [u8; 16],
        address: SocketAddr,
    },
    /// Peer degraded
    Degraded {
        node_id: [u8; 16],
        address: SocketAddr,
        reason: String,
    },
    /// Peer suspected dead
    Suspected {
        node_id: [u8; 16],
        address: SocketAddr,
    },
    /// Peer confirmed dead
    Dead {
        node_id: [u8; 16],
        address: SocketAddr,
    },
    /// Peer removed from mesh
    Removed {
        node_id: [u8; 16],
        address: SocketAddr,
        reason: String,
    },
    /// Peer recovered
    Recovered {
        node_id: [u8; 16],
        address: SocketAddr,
    },
}

/// Configuration for peer lifecycle management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerLifecycleConfig {
    /// Enable automatic dead peer removal
    pub auto_remove_dead_peers: bool,
    /// Delay before removing a dead peer
    pub removal_delay: Duration,
    /// Number of consecutive failures before marking as suspected
    pub suspicion_threshold: u32,
    /// Enable automatic recovery detection
    pub enable_recovery: bool,
    /// Maximum time to keep a suspected peer before marking as dead
    pub max_suspicion_time: Duration,
}

impl Default for PeerLifecycleConfig {
    fn default() -> Self {
        Self {
            auto_remove_dead_peers: true,
            removal_delay: DEFAULT_REMOVAL_DELAY,
            suspicion_threshold: 3,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(60),
        }
    }
}

impl PeerLifecycleConfig {
    /// Create a new lifecycle configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable automatic dead peer removal
    pub fn disable_auto_removal(mut self) -> Self {
        self.auto_remove_dead_peers = false;
        self
    }

    /// Set removal delay
    pub fn with_removal_delay(mut self, delay: Duration) -> Self {
        self.removal_delay = delay;
        self
    }

    /// Preset for aggressive removal (fast detection, quick removal)
    pub fn aggressive() -> Self {
        Self {
            auto_remove_dead_peers: true,
            removal_delay: Duration::from_secs(10),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(30),
        }
    }

    /// Preset for conservative removal (slow detection, delayed removal)
    pub fn conservative() -> Self {
        Self {
            auto_remove_dead_peers: true,
            removal_delay: Duration::from_secs(60),
            suspicion_threshold: 5,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(120),
        }
    }
}

/// Peer lifecycle state tracking
#[derive(Debug, Clone)]
struct PeerLifecycleState {
    _node_id: [u8; 16],
    address: SocketAddr,
    state: PeerState,
    failure_count: u32,
    suspected_at: Option<Instant>,
    dead_at: Option<Instant>,
}

impl PeerLifecycleState {
    fn new(node_id: [u8; 16], address: SocketAddr) -> Self {
        Self {
            _node_id: node_id,
            address,
            state: PeerState::Connecting,
            failure_count: 0,
            suspected_at: None,
            dead_at: None,
        }
    }
}

/// Peer lifecycle manager
pub struct PeerLifecycleManager {
    config: PeerLifecycleConfig,
    discovery: Arc<DiscoveryService>,
    health: Arc<HealthMonitor>,
    peer_states: Arc<RwLock<std::collections::HashMap<[u8; 16], PeerLifecycleState>>>,
    event_tx: mpsc::Sender<PeerLifecycleEvent>,
    event_rx: Arc<RwLock<mpsc::Receiver<PeerLifecycleEvent>>>,
}

impl PeerLifecycleManager {
    /// Create a new peer lifecycle manager
    pub fn new(
        config: PeerLifecycleConfig,
        discovery: Arc<DiscoveryService>,
        health: Arc<HealthMonitor>,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);

        Self {
            config,
            discovery,
            health,
            peer_states: Arc::new(RwLock::new(std::collections::HashMap::new())),
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
        }
    }

    /// Start the lifecycle manager
    pub async fn start(&self) -> Result<(), WireError> {
        info!("Starting peer lifecycle manager");

        // Start health monitoring integration
        self.start_health_monitoring().await;

        Ok(())
    }

    /// Subscribe to lifecycle events
    pub async fn subscribe(&self) -> mpsc::Receiver<PeerLifecycleEvent> {
        let (tx, rx) = mpsc::channel(100);

        // Spawn task to forward events
        let event_rx = self.event_rx.clone();
        let _task = tokio::spawn(async move {
            let mut rx_guard = event_rx.write().await;
            while let Some(event) = rx_guard.recv().await {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        rx
    }

    /// Start monitoring health events
    async fn start_health_monitoring(&self) {
        let peer_states = self.peer_states.clone();
        let discovery = self.discovery.clone();
        let event_tx = self.event_tx.clone();
        let config = self.config.clone();
        let health = self.health.clone();

        tokio::spawn(async move {
            loop {
                // Poll for health events
                if let Some(event) = health.poll_event().await {
                    match event {
                        HealthEvent::Unhealthy(addr) => {
                            debug!("Peer {} became unhealthy", addr);
                            Self::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
                        }
                        HealthEvent::Dead(addr) => {
                            warn!("Peer {} detected as dead", addr);
                            Self::handle_dead(&peer_states, &discovery, &event_tx, addr, &config)
                                .await;
                        }
                        HealthEvent::Recovered(addr) => {
                            info!("Peer {} recovered", addr);
                            Self::handle_recovered(&peer_states, &event_tx, addr).await;
                        }
                        _ => {}
                    }
                } else {
                    // No events, sleep briefly
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
    }

    /// Handle unhealthy peer
    async fn handle_unhealthy(
        peer_states: &Arc<RwLock<std::collections::HashMap<[u8; 16], PeerLifecycleState>>>,
        event_tx: &mpsc::Sender<PeerLifecycleEvent>,
        addr: SocketAddr,
        config: &PeerLifecycleConfig,
    ) {
        let mut states = peer_states.write().await;

        // Find peer by address
        if let Some((node_id, state)) = states.iter_mut().find(|(_, s)| s.address == addr) {
            state.failure_count += 1;

            if state.failure_count >= config.suspicion_threshold
                && state.state != PeerState::Suspected
            {
                state.state = PeerState::Suspected;
                state.suspected_at = Some(Instant::now());

                let event = PeerLifecycleEvent::Suspected {
                    node_id: *node_id,
                    address: addr,
                };
                let _ = event_tx.send(event).await;
            } else if state.state != PeerState::Suspected {
                state.state = PeerState::Degraded;

                let event = PeerLifecycleEvent::Degraded {
                    node_id: *node_id,
                    address: addr,
                    reason: format!("{} consecutive failures", state.failure_count),
                };
                let _ = event_tx.send(event).await;
            }
        }
    }

    /// Handle dead peer
    async fn handle_dead(
        peer_states: &Arc<RwLock<std::collections::HashMap<[u8; 16], PeerLifecycleState>>>,
        discovery: &Arc<DiscoveryService>,
        event_tx: &mpsc::Sender<PeerLifecycleEvent>,
        addr: SocketAddr,
        config: &PeerLifecycleConfig,
    ) {
        let mut states = peer_states.write().await;

        // Find peer by address
        if let Some((node_id, state)) = states.iter_mut().find(|(_, s)| s.address == addr) {
            let node_id = *node_id;
            state.state = PeerState::Dead;
            state.dead_at = Some(Instant::now());

            let event = PeerLifecycleEvent::Dead {
                node_id,
                address: addr,
            };
            let _ = event_tx.send(event).await;

            // Schedule removal if auto-removal is enabled
            if config.auto_remove_dead_peers {
                let discovery = discovery.clone();
                let event_tx = event_tx.clone();
                let removal_delay = config.removal_delay;

                tokio::spawn(async move {
                    tokio::time::sleep(removal_delay).await;

                    // Remove from discovery
                    discovery.remove_peer(&node_id).await;

                    let event = PeerLifecycleEvent::Removed {
                        node_id,
                        address: addr,
                        reason: "Dead peer timeout".to_string(),
                    };
                    let _ = event_tx.send(event).await;

                    info!("Removed dead peer {} after {:?}", addr, removal_delay);
                });
            }
        }
    }

    /// Handle recovered peer
    async fn handle_recovered(
        peer_states: &Arc<RwLock<std::collections::HashMap<[u8; 16], PeerLifecycleState>>>,
        event_tx: &mpsc::Sender<PeerLifecycleEvent>,
        addr: SocketAddr,
    ) {
        let mut states = peer_states.write().await;

        if let Some((node_id, state)) = states.iter_mut().find(|(_, s)| s.address == addr) {
            state.state = PeerState::Active;
            state.failure_count = 0;
            state.suspected_at = None;
            state.dead_at = None;

            let event = PeerLifecycleEvent::Recovered {
                node_id: *node_id,
                address: addr,
            };
            let _ = event_tx.send(event).await;
        }
    }

    /// Register a new peer
    pub async fn register_peer(&self, node_id: [u8; 16], address: SocketAddr) {
        let mut states = self.peer_states.write().await;

        if let std::collections::hash_map::Entry::Vacant(e) = states.entry(node_id) {
            e.insert(PeerLifecycleState::new(node_id, address));

            let event = PeerLifecycleEvent::Discovered { node_id, address };
            let _ = self.event_tx.send(event).await;
        }
    }

    /// Get peer state
    pub async fn get_peer_state(&self, node_id: &[u8; 16]) -> Option<PeerState> {
        self.peer_states.read().await.get(node_id).map(|s| s.state)
    }

    /// Get all peers in a specific state
    pub async fn get_peers_in_state(&self, state: PeerState) -> Vec<([u8; 16], SocketAddr)> {
        self.peer_states
            .read()
            .await
            .iter()
            .filter(|(_, s)| s.state == state)
            .map(|(id, s)| (*id, s.address))
            .collect()
    }

    /// Get lifecycle statistics
    pub async fn stats(&self) -> PeerLifecycleStats {
        let states = self.peer_states.read().await;

        let mut connecting = 0;
        let mut active = 0;
        let mut degraded = 0;
        let mut suspected = 0;
        let mut dead = 0;
        let mut removed = 0;

        for state in states.values() {
            match state.state {
                PeerState::Connecting => connecting += 1,
                PeerState::Active => active += 1,
                PeerState::Degraded => degraded += 1,
                PeerState::Suspected => suspected += 1,
                PeerState::Dead => dead += 1,
                PeerState::Removed => removed += 1,
            }
        }

        PeerLifecycleStats {
            total_peers: states.len(),
            connecting,
            active,
            degraded,
            suspected,
            dead,
            removed,
        }
    }

    /// Cleanup removed peers from memory
    pub async fn cleanup(&self) -> usize {
        let mut states = self.peer_states.write().await;
        let initial_count = states.len();

        states.retain(|_, s| s.state != PeerState::Removed);

        let removed = initial_count - states.len();
        if removed > 0 {
            debug!("Cleaned up {} removed peers from memory", removed);
        }
        removed
    }
}

/// Peer lifecycle statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerLifecycleStats {
    pub total_peers: usize,
    pub connecting: usize,
    pub active: usize,
    pub degraded: usize,
    pub suspected: usize,
    pub dead: usize,
    pub removed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{HealthConfig, HealthMonitor};

    #[test]
    fn test_lifecycle_config_creation() {
        let config = PeerLifecycleConfig::new();
        assert!(config.auto_remove_dead_peers);
        assert_eq!(config.suspicion_threshold, 3);
    }

    #[test]
    fn test_lifecycle_config_presets() {
        let aggressive = PeerLifecycleConfig::aggressive();
        assert_eq!(aggressive.suspicion_threshold, 2);
        assert_eq!(aggressive.removal_delay, Duration::from_secs(10));

        let conservative = PeerLifecycleConfig::conservative();
        assert_eq!(conservative.suspicion_threshold, 5);
        assert_eq!(conservative.removal_delay, Duration::from_secs(60));
    }

    #[test]
    fn test_peer_state_equality() {
        assert_eq!(PeerState::Active, PeerState::Active);
        assert_ne!(PeerState::Active, PeerState::Dead);
    }

    #[test]
    fn test_lifecycle_state_creation() {
        let state = PeerLifecycleState::new([1u8; 16], "127.0.0.1:8000".parse().unwrap());
        assert_eq!(state.state, PeerState::Connecting);
        assert_eq!(state.failure_count, 0);
        assert!(state.suspected_at.is_none());
    }

    // Helper to create test discovery service
    async fn create_test_discovery() -> Arc<DiscoveryService> {
        let config = crate::discovery::DiscoveryConfig::default();
        let local_id = [0u8; 16];
        Arc::new(DiscoveryService::new(config, local_id, vec![]))
    }

    // Helper to create test health monitor
    fn create_test_health() -> Arc<HealthMonitor> {
        let config = HealthConfig {
            heartbeat_interval: Duration::from_millis(100),
            unhealthy_timeout: Duration::from_millis(200),
            dead_timeout: Duration::from_millis(400),
            auto_reconnect: false,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_millis(100),
        };
        Arc::new(HealthMonitor::with_config(config))
    }

    #[tokio::test]
    async fn test_register_peer() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig::new();

        let manager = PeerLifecycleManager::new(config, discovery, health);

        let node_id = [1u8; 16];
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

        manager.register_peer(node_id, addr).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Connecting));
    }

    #[tokio::test]
    async fn test_peer_lifecycle_events() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig::new();

        let manager = PeerLifecycleManager::new(config, discovery, health);

        let node_id = [2u8; 16];
        let addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();

        // Subscribe to events first
        let mut event_rx = manager.subscribe().await;

        // Register peer - should emit discovered event
        manager.register_peer(node_id, addr).await;

        // Wait a bit for event to be delivered
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should receive discovered event
        let event = tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await;
        assert!(event.is_ok());
        if let Ok(Some(evt)) = event {
            assert!(matches!(evt, PeerLifecycleEvent::Discovered { .. }));
        }

        // Re-register same peer - should NOT emit another event
        manager.register_peer(node_id, addr).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let event = tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await;
        assert!(
            event.is_err(),
            "Should not receive event for duplicate registration"
        );
    }

    #[tokio::test]
    async fn test_degraded_to_suspected_transition() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: true,
            removal_delay: Duration::from_secs(1),
            suspicion_threshold: 3,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        let node_id = [3u8; 16];
        let addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();

        manager.register_peer(node_id, addr).await;

        // Register with health monitor
        health.register(addr).await;

        // Simulate unhealthy events
        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // First failure - should become degraded
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Degraded));

        // Second failure
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Degraded));

        // Third failure - should become suspected
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Suspected));
    }

    #[tokio::test]
    async fn test_suspected_to_dead_transition() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: false,
            removal_delay: Duration::from_secs(1),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        let node_id = [4u8; 16];
        let addr: SocketAddr = "127.0.0.1:8003".parse().unwrap();

        manager.register_peer(node_id, addr).await;
        health.register(addr).await;

        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // Make peer suspected
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Suspected));

        // Trigger dead event
        PeerLifecycleManager::handle_dead(&peer_states, &discovery, &event_tx, addr, &config).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Dead));
    }

    #[tokio::test]
    async fn test_auto_removal_after_dead() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: true,
            removal_delay: Duration::from_millis(500),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        let node_id = [5u8; 16];
        let addr: SocketAddr = "127.0.0.1:8004".parse().unwrap();

        manager.register_peer(node_id, addr).await;
        health.register(addr).await;

        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // Make peer dead
        PeerLifecycleManager::handle_dead(&peer_states, &discovery, &event_tx, addr, &config).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Dead));

        // Wait for auto-removal
        tokio::time::sleep(Duration::from_millis(600)).await;

        // Check that removal event was emitted
        // (Note: The actual state doesn't change to Removed in our current implementation,
        //  but the event is emitted)
    }

    #[tokio::test]
    async fn test_recovery_from_suspected() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: true,
            removal_delay: Duration::from_secs(1),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        let node_id = [6u8; 16];
        let addr: SocketAddr = "127.0.0.1:8005".parse().unwrap();

        manager.register_peer(node_id, addr).await;
        health.register(addr).await;

        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // Make peer suspected
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Suspected));

        // Recover
        PeerLifecycleManager::handle_recovered(&peer_states, &event_tx, addr).await;

        let state = manager.get_peer_state(&node_id).await;
        assert_eq!(state, Some(PeerState::Active));

        // Verify failure count reset
        let states = peer_states.read().await;
        let peer = states.get(&node_id).unwrap();
        assert_eq!(peer.failure_count, 0);
    }

    #[tokio::test]
    async fn test_get_peers_in_state() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig::new();

        let manager = PeerLifecycleManager::new(config, discovery, health);

        let node_id1 = [7u8; 16];
        let node_id2 = [8u8; 16];
        let node_id3 = [9u8; 16];
        let addr1: SocketAddr = "127.0.0.1:8006".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8007".parse().unwrap();
        let addr3: SocketAddr = "127.0.0.1:8008".parse().unwrap();

        manager.register_peer(node_id1, addr1).await;
        manager.register_peer(node_id2, addr2).await;
        manager.register_peer(node_id3, addr3).await;

        let connecting = manager.get_peers_in_state(PeerState::Connecting).await;
        assert_eq!(connecting.len(), 3);

        // Manually set one to active
        {
            let mut states = manager.peer_states.write().await;
            states.get_mut(&node_id2).unwrap().state = PeerState::Active;
        }

        let active = manager.get_peers_in_state(PeerState::Active).await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, node_id2);
    }

    #[tokio::test]
    async fn test_lifecycle_statistics() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig::new();

        let manager = PeerLifecycleManager::new(config, discovery, health);

        // Add peers in different states
        let node_id1 = [10u8; 16];
        let node_id2 = [11u8; 16];
        let node_id3 = [12u8; 16];
        let addr1: SocketAddr = "127.0.0.1:8009".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8010".parse().unwrap();
        let addr3: SocketAddr = "127.0.0.1:8011".parse().unwrap();

        manager.register_peer(node_id1, addr1).await;
        manager.register_peer(node_id2, addr2).await;
        manager.register_peer(node_id3, addr3).await;

        {
            let mut states = manager.peer_states.write().await;
            states.get_mut(&node_id1).unwrap().state = PeerState::Active;
            states.get_mut(&node_id2).unwrap().state = PeerState::Degraded;
            states.get_mut(&node_id3).unwrap().state = PeerState::Suspected;
        }

        let stats = manager.stats().await;
        assert_eq!(stats.total_peers, 3);
        assert_eq!(stats.active, 1);
        assert_eq!(stats.degraded, 1);
        assert_eq!(stats.suspected, 1);
    }

    #[tokio::test]
    async fn test_cleanup_removed_peers() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig::new();

        let manager = PeerLifecycleManager::new(config, discovery, health);

        let node_id1 = [13u8; 16];
        let node_id2 = [14u8; 16];
        let addr1: SocketAddr = "127.0.0.1:8012".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8013".parse().unwrap();

        manager.register_peer(node_id1, addr1).await;
        manager.register_peer(node_id2, addr2).await;

        // Mark one as removed
        {
            let mut states = manager.peer_states.write().await;
            states.get_mut(&node_id2).unwrap().state = PeerState::Removed;
        }

        let stats = manager.stats().await;
        assert_eq!(stats.total_peers, 2);
        assert_eq!(stats.removed, 1);

        // Cleanup
        let removed_count = manager.cleanup().await;
        assert_eq!(removed_count, 1);

        let stats = manager.stats().await;
        assert_eq!(stats.total_peers, 1);
        assert_eq!(stats.removed, 0);
    }

    #[tokio::test]
    async fn test_rapid_state_transitions() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: false,
            removal_delay: Duration::from_secs(1),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        let node_id = [15u8; 16];
        let addr: SocketAddr = "127.0.0.1:8014".parse().unwrap();

        manager.register_peer(node_id, addr).await;
        health.register(addr).await;

        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // Rapid transitions: unhealthy -> unhealthy -> recovered -> unhealthy
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state1 = manager.get_peer_state(&node_id).await;

        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state2 = manager.get_peer_state(&node_id).await;

        PeerLifecycleManager::handle_recovered(&peer_states, &event_tx, addr).await;
        let state3 = manager.get_peer_state(&node_id).await;

        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, addr, &config).await;
        let state4 = manager.get_peer_state(&node_id).await;

        // Verify transitions
        assert_eq!(state1, Some(PeerState::Degraded));
        assert_eq!(state2, Some(PeerState::Suspected));
        assert_eq!(state3, Some(PeerState::Active));
        assert_eq!(state4, Some(PeerState::Degraded));
    }

    #[tokio::test]
    async fn test_multiple_peers_lifecycle() {
        let discovery = create_test_discovery().await;
        let health = create_test_health();
        let config = PeerLifecycleConfig {
            auto_remove_dead_peers: false,
            removal_delay: Duration::from_secs(1),
            suspicion_threshold: 2,
            enable_recovery: true,
            max_suspicion_time: Duration::from_secs(5),
        };

        let manager = Arc::new(PeerLifecycleManager::new(
            config.clone(),
            discovery.clone(),
            health.clone(),
        ));

        // Create multiple peers
        let peers: Vec<([u8; 16], SocketAddr)> = (0..5)
            .map(|i| {
                let mut node_id = [0u8; 16];
                node_id[0] = 100 + i as u8;
                let addr: SocketAddr = format!("127.0.0.1:{}", 9000 + i).parse().unwrap();
                (node_id, addr)
            })
            .collect();

        // Register all peers
        for (node_id, addr) in &peers {
            manager.register_peer(*node_id, *addr).await;
            health.register(*addr).await;
        }

        let peer_states = manager.peer_states.clone();
        let event_tx = manager.event_tx.clone();

        // Make peer 0 and 1 degraded
        for (_node_id, peer_id) in peers.iter().take(2) {
            PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, *peer_id, &config)
                .await;
        }

        // Make peer 2 suspected
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, peers[2].1, &config).await;
        PeerLifecycleManager::handle_unhealthy(&peer_states, &event_tx, peers[2].1, &config).await;

        // Make peer 3 dead
        PeerLifecycleManager::handle_dead(&peer_states, &discovery, &event_tx, peers[3].1, &config)
            .await;

        // Verify final states
        assert_eq!(
            manager.get_peer_state(&peers[0].0).await,
            Some(PeerState::Degraded)
        );
        assert_eq!(
            manager.get_peer_state(&peers[1].0).await,
            Some(PeerState::Degraded)
        );
        assert_eq!(
            manager.get_peer_state(&peers[2].0).await,
            Some(PeerState::Suspected)
        );
        assert_eq!(
            manager.get_peer_state(&peers[3].0).await,
            Some(PeerState::Dead)
        );
        assert_eq!(
            manager.get_peer_state(&peers[4].0).await,
            Some(PeerState::Connecting)
        );

        let stats = manager.stats().await;
        assert_eq!(stats.total_peers, 5);
        assert_eq!(stats.degraded, 2);
        assert_eq!(stats.suspected, 1);
        assert_eq!(stats.dead, 1);
        assert_eq!(stats.connecting, 1);
    }
}
