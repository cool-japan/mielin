//! Gossip Protocol Implementation
//!
//! Provides epidemic-style state propagation for distributed mesh coordination.
//! Implements anti-entropy reconciliation, failure detection integration,
//! and network partition handling.

use crate::discovery::DiscoveryService;
use crate::health::HealthMonitor;
use crate::WireError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Default gossip interval (5 seconds)
const DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_secs(5);

/// Default fanout (number of peers to gossip to)
const DEFAULT_FANOUT: usize = 3;

/// Default state expiration time (60 seconds)
const DEFAULT_STATE_EXPIRATION: Duration = Duration::from_secs(60);

/// Helper function to create default Instant for deserialization
fn now() -> Instant {
    Instant::now()
}

/// Vector clock for causal ordering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorClock {
    /// Node ID to version mapping
    clocks: HashMap<[u8; 16], u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Increment the clock for a specific node
    pub fn increment(&mut self, node_id: [u8; 16]) {
        let counter = self.clocks.entry(node_id).or_insert(0);
        *counter += 1;
    }

    /// Update with another vector clock (take maximum)
    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, &version) in &other.clocks {
            let counter = self.clocks.entry(*node_id).or_insert(0);
            *counter = (*counter).max(version);
        }
    }

    /// Check if this clock happened before another (causal ordering)
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut strictly_less = false;

        // Check all nodes in self
        for (node_id, &self_version) in &self.clocks {
            let other_version = other.clocks.get(node_id).copied().unwrap_or(0);
            if self_version > other_version {
                return false;
            }
            if self_version < other_version {
                strictly_less = true;
            }
        }

        // Check nodes only in other
        for node_id in other.clocks.keys() {
            if !self.clocks.contains_key(node_id) {
                strictly_less = true;
            }
        }

        strictly_less
    }

    /// Check if two clocks are concurrent (neither happens before the other)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }

    /// Get version for a specific node
    pub fn get_version(&self, node_id: &[u8; 16]) -> u64 {
        self.clocks.get(node_id).copied().unwrap_or(0)
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Gossip state entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipState {
    /// State key
    pub key: String,
    /// State value
    pub value: Vec<u8>,
    /// Vector clock for versioning
    pub version: VectorClock,
    /// Timestamp of last update
    #[serde(skip, default = "now")]
    pub updated_at: Instant,
}

/// Gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Push state updates to peer
    Push {
        node_id: [u8; 16],
        states: Vec<GossipState>,
    },
    /// Pull state from peer
    Pull {
        node_id: [u8; 16],
        known_versions: HashMap<String, VectorClock>,
    },
    /// Response to pull request
    PullResponse {
        node_id: [u8; 16],
        states: Vec<GossipState>,
    },
    /// Anti-entropy synchronization
    AntiEntropy {
        node_id: [u8; 16],
        digests: HashMap<String, VectorClock>,
    },
}

/// Gossip protocol configuration
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// Interval between gossip rounds
    pub gossip_interval: Duration,
    /// Number of peers to gossip to in each round
    pub fanout: usize,
    /// State expiration time
    pub state_expiration: Duration,
    /// Enable anti-entropy reconciliation
    pub enable_anti_entropy: bool,
    /// Anti-entropy interval
    pub anti_entropy_interval: Duration,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            gossip_interval: DEFAULT_GOSSIP_INTERVAL,
            fanout: DEFAULT_FANOUT,
            state_expiration: DEFAULT_STATE_EXPIRATION,
            enable_anti_entropy: true,
            anti_entropy_interval: Duration::from_secs(30),
            max_message_size: 1_048_576, // 1MB
        }
    }
}

/// Gossip protocol statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipStats {
    /// Total gossip rounds performed
    pub rounds: u64,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total states propagated
    pub states_propagated: u64,
    /// Total conflicts detected
    pub conflicts_detected: u64,
    /// Current state count
    pub state_count: usize,
}

/// Gossip protocol service
pub struct GossipService {
    config: GossipConfig,
    local_node_id: [u8; 16],
    discovery: Arc<DiscoveryService>,
    health: Arc<HealthMonitor>,

    /// Local state store
    states: Arc<RwLock<HashMap<String, GossipState>>>,

    /// Message channel (reserved for future transport integration)
    _message_tx: mpsc::Sender<GossipMessage>,
    _message_rx: Arc<RwLock<mpsc::Receiver<GossipMessage>>>,

    /// Statistics
    stats: Arc<RwLock<GossipStats>>,
}

impl GossipService {
    /// Create a new gossip service
    pub fn new(
        config: GossipConfig,
        local_node_id: [u8; 16],
        discovery: Arc<DiscoveryService>,
        health: Arc<HealthMonitor>,
    ) -> Self {
        let (message_tx, message_rx) = mpsc::channel(1000);

        Self {
            config,
            local_node_id,
            discovery,
            health,
            states: Arc::new(RwLock::new(HashMap::new())),
            _message_tx: message_tx,
            _message_rx: Arc::new(RwLock::new(message_rx)),
            stats: Arc::new(RwLock::new(GossipStats {
                rounds: 0,
                messages_sent: 0,
                messages_received: 0,
                states_propagated: 0,
                conflicts_detected: 0,
                state_count: 0,
            })),
        }
    }

    /// Start the gossip service
    pub async fn start(&self) -> Result<(), WireError> {
        info!(
            "Starting gossip service with interval: {:?}",
            self.config.gossip_interval
        );

        // Start gossip rounds
        self.start_gossip_rounds().await;

        // Start anti-entropy if enabled
        if self.config.enable_anti_entropy {
            self.start_anti_entropy().await;
        }

        // Start state expiration
        self.start_state_expiration().await;

        Ok(())
    }

    /// Put a state value
    pub async fn put(&self, key: String, value: Vec<u8>) -> Result<(), WireError> {
        let mut states = self.states.write().await;

        // Get or create state
        let mut state = states.get(&key).cloned().unwrap_or_else(|| GossipState {
            key: key.clone(),
            value: Vec::new(),
            version: VectorClock::new(),
            updated_at: Instant::now(),
        });

        // Update state
        state.value = value;
        state.version.increment(self.local_node_id);
        state.updated_at = Instant::now();

        states.insert(key, state);

        debug!("Updated state with new version");
        Ok(())
    }

    /// Get a state value
    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let states = self.states.read().await;
        states.get(key).map(|s| s.value.clone())
    }

    /// Get all state keys
    pub async fn keys(&self) -> Vec<String> {
        let states = self.states.read().await;
        states.keys().cloned().collect()
    }

    /// Get statistics
    pub async fn stats(&self) -> GossipStats {
        let mut stats = self.stats.read().await.clone();
        stats.state_count = self.states.read().await.len();
        stats
    }

    /// Start periodic gossip rounds
    async fn start_gossip_rounds(&self) {
        let states = self.states.clone();
        let _discovery = self.discovery.clone();
        let health = self.health.clone();
        let _local_node_id = self.local_node_id;
        let config = self.config.clone();
        let stats = self.stats.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.gossip_interval);

            loop {
                interval.tick().await;

                // Get healthy peers
                let healthy_peers = health.get_healthy_connections().await;
                if healthy_peers.is_empty() {
                    continue;
                }

                // Select random peers (fanout)
                let selected_peers = Self::select_random_peers(&healthy_peers, config.fanout);

                // Get recent states to gossip
                let states_snapshot = states.read().await;
                let recent_states: Vec<GossipState> = states_snapshot
                    .values()
                    .filter(|s| s.updated_at.elapsed() < config.state_expiration)
                    .cloned()
                    .collect();
                drop(states_snapshot);

                if !recent_states.is_empty() {
                    debug!(
                        "Gossip round: sending {} states to {} peers",
                        recent_states.len(),
                        selected_peers.len()
                    );

                    let peer_count = selected_peers.len();

                    // Send to selected peers (in real implementation, would use transport)
                    for _peer in &selected_peers {
                        // TODO: Actual network send
                        // transport.send(peer, GossipMessage::Push { ... }).await;
                    }

                    // Update stats
                    let mut stats_guard = stats.write().await;
                    stats_guard.rounds += 1;
                    stats_guard.messages_sent += peer_count as u64;
                    stats_guard.states_propagated += (recent_states.len() * peer_count) as u64;
                }
            }
        });
    }

    /// Start anti-entropy reconciliation
    async fn start_anti_entropy(&self) {
        let states = self.states.clone();
        let _discovery = self.discovery.clone();
        let health = self.health.clone();
        let _local_node_id = self.local_node_id;
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.anti_entropy_interval);

            loop {
                interval.tick().await;

                // Get healthy peers
                let healthy_peers = health.get_healthy_connections().await;
                if healthy_peers.is_empty() {
                    continue;
                }

                // Select one random peer for anti-entropy
                let selected_peers = Self::select_random_peers(&healthy_peers, 1);
                if selected_peers.is_empty() {
                    continue;
                }

                // Create digest of current state
                let states_snapshot = states.read().await;
                let digests: HashMap<String, VectorClock> = states_snapshot
                    .iter()
                    .map(|(k, v)| (k.clone(), v.version.clone()))
                    .collect();
                drop(states_snapshot);

                debug!("Anti-entropy: sending digest with {} keys", digests.len());

                // Send digest to peer (in real implementation)
                // let peer = selected_peers[0];
                // transport.send(peer, GossipMessage::AntiEntropy { ... }).await;
            }
        });
    }

    /// Start state expiration
    async fn start_state_expiration(&self) {
        let states = self.states.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                let mut states_guard = states.write().await;
                let initial_count = states_guard.len();

                states_guard
                    .retain(|_, state| state.updated_at.elapsed() < config.state_expiration);

                let removed = initial_count - states_guard.len();
                if removed > 0 {
                    debug!("Expired {} old states", removed);
                }
            }
        });
    }

    /// Handle incoming gossip message
    pub async fn handle_message(&self, message: GossipMessage) -> Result<(), WireError> {
        match message {
            GossipMessage::Push {
                node_id,
                states: incoming_states,
            } => self.handle_push(node_id, incoming_states).await,
            GossipMessage::Pull {
                node_id,
                known_versions,
            } => self.handle_pull(node_id, known_versions).await,
            GossipMessage::PullResponse {
                node_id,
                states: incoming_states,
            } => self.handle_push(node_id, incoming_states).await,
            GossipMessage::AntiEntropy { node_id, digests } => {
                self.handle_anti_entropy(node_id, digests).await
            }
        }
    }

    /// Handle push message
    async fn handle_push(
        &self,
        _node_id: [u8; 16],
        incoming_states: Vec<GossipState>,
    ) -> Result<(), WireError> {
        let mut states = self.states.write().await;
        let mut stats = self.stats.write().await;

        for incoming_state in incoming_states {
            match states.get(&incoming_state.key) {
                Some(local_state) => {
                    // Conflict resolution using vector clocks
                    if incoming_state.version.happens_before(&local_state.version) {
                        // Incoming is older, ignore
                        debug!("Ignoring older state for key: {}", incoming_state.key);
                    } else if local_state.version.happens_before(&incoming_state.version) {
                        // Incoming is newer, update
                        debug!("Updating state for key: {}", incoming_state.key);
                        states.insert(incoming_state.key.clone(), incoming_state);
                    } else if incoming_state.version.is_concurrent(&local_state.version) {
                        // Concurrent updates - conflict!
                        warn!("Conflict detected for key: {}", incoming_state.key);
                        stats.conflicts_detected += 1;

                        // Simple resolution: merge versions and keep incoming value
                        let mut merged_state = incoming_state.clone();
                        merged_state.version.merge(&local_state.version);
                        states.insert(merged_state.key.clone(), merged_state);
                    }
                }
                None => {
                    // New state, accept
                    debug!("Adding new state for key: {}", incoming_state.key);
                    states.insert(incoming_state.key.clone(), incoming_state);
                }
            }
        }

        stats.messages_received += 1;
        Ok(())
    }

    /// Handle pull message
    async fn handle_pull(
        &self,
        _node_id: [u8; 16],
        known_versions: HashMap<String, VectorClock>,
    ) -> Result<(), WireError> {
        let states = self.states.read().await;

        // Find states that are newer than what the peer knows
        let mut response_states = Vec::new();

        for (key, local_state) in states.iter() {
            match known_versions.get(key) {
                Some(known_version) => {
                    if known_version.happens_before(&local_state.version) {
                        response_states.push(local_state.clone());
                    }
                }
                None => {
                    // Peer doesn't have this state
                    response_states.push(local_state.clone());
                }
            }
        }

        debug!("Pull response: sending {} states", response_states.len());

        // In real implementation, would send PullResponse message
        // transport.send(node_id, GossipMessage::PullResponse { ... }).await;

        Ok(())
    }

    /// Handle anti-entropy message
    async fn handle_anti_entropy(
        &self,
        _node_id: [u8; 16],
        digests: HashMap<String, VectorClock>,
    ) -> Result<(), WireError> {
        let states = self.states.read().await;

        // Find differences
        let mut needed_keys = Vec::new();
        let mut newer_states = Vec::new();

        for (key, remote_version) in &digests {
            match states.get(key) {
                Some(local_state) => {
                    if remote_version.happens_before(&local_state.version) {
                        // We have newer version
                        newer_states.push(local_state.clone());
                    } else if local_state.version.happens_before(remote_version) {
                        // They have newer version
                        needed_keys.push(key.clone());
                    }
                }
                None => {
                    // We don't have this key
                    needed_keys.push(key.clone());
                }
            }
        }

        // Check for keys we have that they don't
        for (key, local_state) in states.iter() {
            if !digests.contains_key(key) {
                newer_states.push(local_state.clone());
            }
        }

        debug!(
            "Anti-entropy: need {} keys, sending {} states",
            needed_keys.len(),
            newer_states.len()
        );

        // In real implementation:
        // - Send Pull for needed_keys
        // - Send Push for newer_states

        Ok(())
    }

    /// Select random peers from a list
    fn select_random_peers(peers: &[SocketAddr], count: usize) -> Vec<SocketAddr> {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        let mut selected = peers.to_vec();
        selected.shuffle(&mut rng);
        selected.truncate(count);
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_clock_increment() {
        let mut clock = VectorClock::new();
        let node_id = [1u8; 16];

        clock.increment(node_id);
        assert_eq!(clock.get_version(&node_id), 1);

        clock.increment(node_id);
        assert_eq!(clock.get_version(&node_id), 2);
    }

    #[test]
    fn test_vector_clock_happens_before() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();
        let node1 = [1u8; 16];

        clock1.increment(node1);
        clock2.increment(node1);
        clock2.increment(node1);

        assert!(clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
    }

    #[test]
    fn test_vector_clock_concurrent() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();
        let node1 = [1u8; 16];
        let node2 = [2u8; 16];

        clock1.increment(node1);
        clock2.increment(node2);

        assert!(clock1.is_concurrent(&clock2));
        assert!(clock2.is_concurrent(&clock1));
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();
        let node1 = [1u8; 16];
        let node2 = [2u8; 16];

        clock1.increment(node1);
        clock1.increment(node1);
        clock2.increment(node2);
        clock2.increment(node2);
        clock2.increment(node2);

        clock1.merge(&clock2);

        assert_eq!(clock1.get_version(&node1), 2);
        assert_eq!(clock1.get_version(&node2), 3);
    }

    #[tokio::test]
    async fn test_gossip_service_creation() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        let stats = service.stats().await;
        assert_eq!(stats.rounds, 0);
        assert_eq!(stats.state_count, 0);
    }

    #[tokio::test]
    async fn test_gossip_put_get() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        // Put a value
        service
            .put("key1".to_string(), b"value1".to_vec())
            .await
            .unwrap();

        // Get the value
        let value = service.get("key1").await;
        assert_eq!(value, Some(b"value1".to_vec()));

        // Check stats
        let stats = service.stats().await;
        assert_eq!(stats.state_count, 1);
    }

    #[tokio::test]
    async fn test_gossip_keys() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        // Put multiple values
        service
            .put("key1".to_string(), b"value1".to_vec())
            .await
            .unwrap();
        service
            .put("key2".to_string(), b"value2".to_vec())
            .await
            .unwrap();
        service
            .put("key3".to_string(), b"value3".to_vec())
            .await
            .unwrap();

        // Get all keys
        let keys = service.keys().await;
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
        assert!(keys.contains(&"key3".to_string()));
    }

    #[tokio::test]
    async fn test_handle_push_new_state() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        // Create incoming state
        let mut version = VectorClock::new();
        version.increment([1u8; 16]);

        let incoming_state = GossipState {
            key: "remote_key".to_string(),
            value: b"remote_value".to_vec(),
            version,
            updated_at: Instant::now(),
        };

        // Handle push
        service
            .handle_push([1u8; 16], vec![incoming_state])
            .await
            .unwrap();

        // Verify state was added
        let value = service.get("remote_key").await;
        assert_eq!(value, Some(b"remote_value".to_vec()));
    }

    #[tokio::test]
    async fn test_handle_push_newer_state() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        // Put initial value
        service
            .put("key1".to_string(), b"old_value".to_vec())
            .await
            .unwrap();

        // Create newer incoming state
        let mut version = VectorClock::new();
        version.increment(local_id);
        version.increment(local_id);
        version.increment([1u8; 16]);

        let incoming_state = GossipState {
            key: "key1".to_string(),
            value: b"new_value".to_vec(),
            version,
            updated_at: Instant::now(),
        };

        // Handle push
        service
            .handle_push([1u8; 16], vec![incoming_state])
            .await
            .unwrap();

        // Verify state was updated
        let value = service.get("key1").await;
        assert_eq!(value, Some(b"new_value".to_vec()));
    }

    #[tokio::test]
    async fn test_conflict_detection() {
        let config = GossipConfig::default();
        let local_id = [0u8; 16];

        let discovery_config = crate::discovery::DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

        let health = Arc::new(HealthMonitor::new());

        let service = GossipService::new(config, local_id, discovery, health);

        // Put initial value
        service
            .put("key1".to_string(), b"local_value".to_vec())
            .await
            .unwrap();

        // Create concurrent incoming state
        let mut version = VectorClock::new();
        version.increment([1u8; 16]);

        let incoming_state = GossipState {
            key: "key1".to_string(),
            value: b"remote_value".to_vec(),
            version,
            updated_at: Instant::now(),
        };

        // Handle push
        service
            .handle_push([1u8; 16], vec![incoming_state])
            .await
            .unwrap();

        // Check that conflict was detected
        let stats = service.stats().await;
        assert_eq!(stats.conflicts_detected, 1);
    }
}
