//! Gossip Protocol for Mesh State Synchronization
//!
//! Implements a SWIM-inspired gossip protocol for:
//! - Node membership management
//! - Failure detection via heartbeats
//! - State dissemination across the mesh
//! - Anti-entropy reconciliation
//! - Hierarchical gossip with zones and super-peers

use crate::{Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

/// Gossip interval for state propagation
const GOSSIP_INTERVAL: Duration = Duration::from_secs(5);

/// Heartbeat timeout - consider node suspect after this duration
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Failure timeout - consider node dead after this duration
const FAILURE_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// GossipConfig — configurable flat (SWIM-style) gossip parameters
// ============================================================================

/// Configuration for the flat (SWIM-style) gossip protocol.
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// How often gossip messages are sent (default: 5 s)
    pub gossip_interval: Duration,
    /// After this duration without heartbeat, mark suspect (default: 15 s)
    pub heartbeat_timeout: Duration,
    /// After this duration without heartbeat, declare failed (default: 30 s)
    pub failure_timeout: Duration,
    /// Number of nodes to gossip to per round (default: 3)
    pub fanout: usize,
    /// Maximum number of membership event entries to retain (default: 512)
    pub max_history: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            gossip_interval: GOSSIP_INTERVAL,
            heartbeat_timeout: HEARTBEAT_TIMEOUT,
            failure_timeout: FAILURE_TIMEOUT,
            fanout: 3,
            max_history: 512,
        }
    }
}

// ============================================================================
// Membership event log
// ============================================================================

/// Kind of membership change recorded in the event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembershipEventKind {
    /// Node joined the membership.
    Joined,
    /// Node voluntarily left.
    Left,
    /// Node was declared failed by failure detection.
    Failed,
    /// Node recovered (incarnation bump after being suspected/dead).
    Recovered,
    /// Node health status changed.
    StatusChanged {
        from: HealthStatus,
        to: HealthStatus,
    },
    /// Incarnation number was updated (e.g. refutation).
    IncarnationUpdated { old: u64, new: u64 },
}

/// A single membership-change event stored in the history ring-buffer.
#[derive(Debug, Clone)]
pub struct MembershipEvent {
    /// The node this event concerns.
    pub node_id: NodeId,
    /// The kind of change that occurred.
    pub kind: MembershipEventKind,
    /// Incarnation number at the time of the event.
    pub incarnation: u64,
    /// Wall-clock time of the event.
    pub timestamp: SystemTime,
    /// Optional human-readable annotation.
    pub metadata: Option<String>,
}

impl MembershipEvent {
    fn new(node_id: NodeId, kind: MembershipEventKind, incarnation: u64) -> Self {
        Self {
            node_id,
            kind,
            incarnation,
            timestamp: SystemTime::now(),
            metadata: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum GossipError {
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),
}

/// Node health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Node is alive and responding
    Alive,
    /// Node is suspected to have failed (missed heartbeats)
    Suspect,
    /// Node is confirmed dead
    Dead,
}

/// Membership information for a peer node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub node_id: NodeId,
    pub status: HealthStatus,
    pub incarnation: u64,
    pub last_seen: SystemTime,
    pub metadata: HashMap<String, String>,
}

impl MemberInfo {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            status: HealthStatus::Alive,
            incarnation: 0,
            last_seen: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.status, HealthStatus::Alive)
    }

    pub fn is_suspect(&self) -> bool {
        matches!(self.status, HealthStatus::Suspect)
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.status, HealthStatus::Dead)
    }

    pub fn heartbeat_age(&self) -> Duration {
        self.last_seen.elapsed().unwrap_or_default()
    }

    /// Whether this member should transition from `Alive` to `Suspect`,
    /// given the caller-supplied `heartbeat_timeout` (normally
    /// [`GossipConfig::heartbeat_timeout`]). This does NOT read any
    /// hard-coded default — the timeout must be threaded in by the caller
    /// so that a runtime-configured `GossipConfig` actually governs
    /// suspicion detection.
    pub fn should_suspect(&self, heartbeat_timeout: Duration) -> bool {
        self.is_alive() && self.heartbeat_age() > heartbeat_timeout
    }

    /// Whether this member should transition from `Suspect` to `Dead`,
    /// given the caller-supplied `failure_timeout` (normally
    /// [`GossipConfig::failure_timeout`]). This does NOT read any
    /// hard-coded default — the timeout must be threaded in by the caller
    /// so that a runtime-configured `GossipConfig` actually governs
    /// death declaration.
    pub fn should_declare_dead(&self, failure_timeout: Duration) -> bool {
        self.is_suspect() && self.heartbeat_age() > failure_timeout
    }
}

/// State store value type: (data, version)
type StateValue = (Vec<u8>, u64);

/// Gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Heartbeat from a node
    Heartbeat { node_id: NodeId, incarnation: u64 },
    /// State update for a member
    MemberUpdate { member: MemberInfo },
    /// Request for full state sync
    SyncRequest { from_node: NodeId },
    /// Response with full membership state
    SyncResponse { members: Vec<MemberInfo> },
    /// Custom state update
    StateUpdate {
        key: String,
        value: Vec<u8>,
        version: u64,
    },
}

/// Gossip state manager
pub struct GossipState {
    local_node: Arc<Node>,
    members: Arc<RwLock<HashMap<NodeId, MemberInfo>>>,
    local_incarnation: Arc<RwLock<u64>>,
    state_store: Arc<RwLock<HashMap<String, StateValue>>>,
    /// Runtime configuration for the flat gossip protocol.
    config: Arc<GossipConfig>,
    /// Bounded ring-buffer of historical membership change events.
    history: Arc<RwLock<VecDeque<MembershipEvent>>>,
}

impl GossipState {
    /// Create a new `GossipState` with default configuration.
    pub fn new(node: Arc<Node>) -> Self {
        Self::with_config(node, GossipConfig::default())
    }

    /// Create a new `GossipState` with explicit configuration.
    pub fn with_config(node: Arc<Node>, config: GossipConfig) -> Self {
        let node_id = *node.id();
        let mut members = HashMap::new();
        members.insert(node_id, MemberInfo::new(node_id));

        Self {
            local_node: node,
            members: Arc::new(RwLock::new(members)),
            local_incarnation: Arc::new(RwLock::new(0)),
            state_store: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            history: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Return a reference to the active configuration.
    pub fn config(&self) -> &GossipConfig {
        &self.config
    }

    // -----------------------------------------------------------------------
    // Membership history helpers
    // -----------------------------------------------------------------------

    /// Append a membership event to the history ring-buffer.
    /// Evicts the oldest entry if capacity is exceeded.
    fn record_membership_event(&self, event: MembershipEvent) {
        let history = self.history.clone();
        let max = self.config.max_history;
        tokio::spawn(async move {
            let mut h = history.write().await;
            if h.len() >= max {
                h.pop_front();
            }
            h.push_back(event);
        });
    }

    /// Return a snapshot of the entire membership history.
    pub async fn membership_history(&self) -> Vec<MembershipEvent> {
        self.history.read().await.iter().cloned().collect()
    }

    /// Return all history entries for a specific node.
    pub async fn history_for(&self, node_id: &NodeId) -> Vec<MembershipEvent> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| &e.node_id == node_id)
            .cloned()
            .collect()
    }

    /// Return all history entries whose timestamp is ≥ `since`.
    pub async fn history_since(&self, since: SystemTime) -> Vec<MembershipEvent> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.timestamp >= since)
            .cloned()
            .collect()
    }

    /// Return the maximum number of events the history can hold.
    pub fn history_capacity(&self) -> usize {
        self.config.max_history
    }

    /// Return the current number of events stored in history.
    pub async fn history_count(&self) -> usize {
        self.history.read().await.len()
    }

    /// Start the gossip protocol
    pub async fn start(&self) {
        info!("Starting gossip protocol for node {}", self.local_node.id());

        // Spawn heartbeat task
        self.spawn_heartbeat_task();

        // Spawn failure detection task
        self.spawn_failure_detection_task();

        // Spawn gossip propagation task
        self.spawn_gossip_task();
    }

    /// Spawn heartbeat task to update local member status
    fn spawn_heartbeat_task(&self) {
        let members = self.members.clone();
        let incarnation = self.local_incarnation.clone();
        let node_id = *self.local_node.id();
        let interval_dur = self.config.gossip_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval_dur);
            loop {
                interval.tick().await;

                let mut members = members.write().await;
                if let Some(member) = members.get_mut(&node_id) {
                    member.last_seen = SystemTime::now();
                    let inc = *incarnation.read().await;
                    member.incarnation = inc;
                }
            }
        });
    }

    /// Spawn failure detection task
    fn spawn_failure_detection_task(&self) {
        let members = self.members.clone();
        let node_id = *self.local_node.id();
        let heartbeat_timeout = self.config.heartbeat_timeout;
        let failure_timeout = self.config.failure_timeout;
        let history = self.history.clone();
        let max_history = self.config.max_history;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let mut members_guard = members.write().await;
                let mut updates: Vec<(NodeId, HealthStatus, HealthStatus, u64)> = Vec::new();

                for (id, member) in members_guard.iter() {
                    if *id == node_id {
                        continue;
                    }

                    let age = member.heartbeat_age();
                    if member.should_declare_dead(failure_timeout) {
                        updates.push((*id, member.status, HealthStatus::Dead, member.incarnation));
                        warn!("Declaring node {} as dead (no heartbeat for {:?})", id, age);
                    } else if member.should_suspect(heartbeat_timeout) {
                        updates.push((
                            *id,
                            member.status,
                            HealthStatus::Suspect,
                            member.incarnation,
                        ));
                        debug!(
                            "Marking node {} as suspect (no heartbeat for {:?})",
                            id, age
                        );
                    }
                }

                for (id, old_status, new_status, inc) in updates {
                    if let Some(member) = members_guard.get_mut(&id) {
                        member.status = new_status;
                    }
                    // Record failure event outside members lock
                    let kind = if new_status == HealthStatus::Dead {
                        MembershipEventKind::Failed
                    } else {
                        MembershipEventKind::StatusChanged {
                            from: old_status,
                            to: new_status,
                        }
                    };
                    let event = MembershipEvent::new(id, kind, inc);
                    let mut h = history.write().await;
                    if h.len() >= max_history {
                        h.pop_front();
                    }
                    h.push_back(event);
                }
            }
        });
    }

    /// Spawn gossip propagation task
    fn spawn_gossip_task(&self) {
        let members = self.members.clone();
        let interval_dur = self.config.gossip_interval;
        let _fanout = self.config.fanout;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval_dur);
            loop {
                interval.tick().await;

                let members = members.read().await;
                let alive_count = members.values().filter(|m| m.is_alive()).count();
                let suspect_count = members.values().filter(|m| m.is_suspect()).count();
                let dead_count = members.values().filter(|m| m.is_dead()).count();

                debug!(
                    "Gossip state: {} alive, {} suspect, {} dead",
                    alive_count, suspect_count, dead_count
                );

                // In a real implementation, this would:
                // 1. Select random peers to gossip with (up to _fanout)
                // 2. Send member updates
                // 3. Exchange state information
            }
        });
    }

    /// Handle incoming gossip message
    pub async fn handle_message(
        &self,
        message: GossipMessage,
    ) -> Result<Option<GossipMessage>, GossipError> {
        match message {
            GossipMessage::Heartbeat {
                node_id,
                incarnation,
            } => {
                self.handle_heartbeat(node_id, incarnation).await?;
                Ok(None)
            }
            GossipMessage::MemberUpdate { member } => {
                self.handle_member_update(member).await?;
                Ok(None)
            }
            GossipMessage::SyncRequest { from_node } => {
                let response = self.handle_sync_request(from_node).await?;
                Ok(Some(response))
            }
            GossipMessage::SyncResponse { members } => {
                self.handle_sync_response(members).await?;
                Ok(None)
            }
            GossipMessage::StateUpdate {
                key,
                value,
                version,
            } => {
                self.handle_state_update(key, value, version).await?;
                Ok(None)
            }
        }
    }

    /// Handle heartbeat from a peer
    async fn handle_heartbeat(&self, node_id: NodeId, incarnation: u64) -> Result<(), GossipError> {
        let event = {
            let mut members = self.members.write().await;

            if let Some(member) = members.get_mut(&node_id) {
                if incarnation > member.incarnation {
                    let old_inc = member.incarnation;
                    let was_unhealthy = !member.is_alive();
                    member.incarnation = incarnation;
                    member.last_seen = SystemTime::now();
                    member.status = HealthStatus::Alive;
                    debug!(
                        "Updated heartbeat for node {} (incarnation: {})",
                        node_id, incarnation
                    );
                    if was_unhealthy {
                        Some(MembershipEvent::new(
                            node_id,
                            MembershipEventKind::Recovered,
                            incarnation,
                        ))
                    } else if old_inc != incarnation {
                        Some(MembershipEvent::new(
                            node_id,
                            MembershipEventKind::IncarnationUpdated {
                                old: old_inc,
                                new: incarnation,
                            },
                            incarnation,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // New member discovered via heartbeat
                let mut member = MemberInfo::new(node_id);
                member.incarnation = incarnation;
                members.insert(node_id, member);
                info!("Discovered new member {} via heartbeat", node_id);
                Some(MembershipEvent::new(
                    node_id,
                    MembershipEventKind::Joined,
                    incarnation,
                ))
            }
        };

        if let Some(ev) = event {
            self.record_membership_event(ev);
        }

        Ok(())
    }

    /// Handle member update from gossip
    async fn handle_member_update(&self, new_info: MemberInfo) -> Result<(), GossipError> {
        let event = {
            let mut members = self.members.write().await;

            if let Some(existing) = members.get_mut(&new_info.node_id) {
                if new_info.incarnation > existing.incarnation {
                    let old_status = existing.status;
                    let new_status = new_info.status;
                    let node_id = new_info.node_id;
                    let inc = new_info.incarnation;
                    *existing = new_info;
                    debug!("Updated member info for {}", node_id);
                    if old_status != new_status {
                        Some(MembershipEvent::new(
                            node_id,
                            MembershipEventKind::StatusChanged {
                                from: old_status,
                                to: new_status,
                            },
                            inc,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                let node_id = new_info.node_id;
                let inc = new_info.incarnation;
                members.insert(node_id, new_info);
                info!("Added new member {} to membership", node_id);
                Some(MembershipEvent::new(
                    node_id,
                    MembershipEventKind::Joined,
                    inc,
                ))
            }
        };

        if let Some(ev) = event {
            self.record_membership_event(ev);
        }

        Ok(())
    }

    /// Handle sync request
    async fn handle_sync_request(&self, _from_node: NodeId) -> Result<GossipMessage, GossipError> {
        let members = self.members.read().await;
        let member_list: Vec<MemberInfo> = members.values().cloned().collect();

        Ok(GossipMessage::SyncResponse {
            members: member_list,
        })
    }

    /// Handle sync response
    async fn handle_sync_response(&self, members: Vec<MemberInfo>) -> Result<(), GossipError> {
        let mut local_members = self.members.write().await;

        for member in members {
            if let Some(existing) = local_members.get_mut(&member.node_id) {
                if member.incarnation > existing.incarnation {
                    *existing = member;
                }
            } else {
                local_members.insert(member.node_id, member);
            }
        }

        Ok(())
    }

    /// Handle state update
    async fn handle_state_update(
        &self,
        key: String,
        value: Vec<u8>,
        version: u64,
    ) -> Result<(), GossipError> {
        let mut state_store = self.state_store.write().await;

        if let Some((_, existing_version)) = state_store.get(&key) {
            if version > *existing_version {
                state_store.insert(key.clone(), (value, version));
                debug!("Updated state for key {} (version: {})", key, version);
            }
        } else {
            state_store.insert(key.clone(), (value, version));
            debug!("Added new state for key {} (version: {})", key, version);
        }

        Ok(())
    }

    /// Add a new member to the membership
    pub async fn add_member(&self, node_id: NodeId) -> Result<(), GossipError> {
        let mut members = self.members.write().await;

        if let std::collections::hash_map::Entry::Vacant(e) = members.entry(node_id) {
            e.insert(MemberInfo::new(node_id));
            info!("Added member {} to membership", node_id);
            drop(members);
            self.record_membership_event(MembershipEvent::new(
                node_id,
                MembershipEventKind::Joined,
                0,
            ));
        }

        Ok(())
    }

    /// Remove a member from the membership, recording a `Left` event.
    pub async fn remove_member(&self, node_id: NodeId) -> Result<(), GossipError> {
        let inc = {
            let mut members = self.members.write().await;
            if let Some(removed) = members.remove(&node_id) {
                info!("Removed member {} from membership", node_id);
                removed.incarnation
            } else {
                return Err(GossipError::NodeNotFound(node_id));
            }
        };
        self.record_membership_event(MembershipEvent::new(
            node_id,
            MembershipEventKind::Left,
            inc,
        ));
        Ok(())
    }

    /// Update the status of an existing member, recording a `StatusChanged` event.
    pub async fn update_member_status(
        &self,
        node_id: NodeId,
        new_status: HealthStatus,
    ) -> Result<(), GossipError> {
        let (old_status, inc) = {
            let mut members = self.members.write().await;
            let member = members
                .get_mut(&node_id)
                .ok_or(GossipError::NodeNotFound(node_id))?;
            let old = member.status;
            let inc = member.incarnation;
            member.status = new_status;
            (old, inc)
        };
        if old_status != new_status {
            let kind = MembershipEventKind::StatusChanged {
                from: old_status,
                to: new_status,
            };
            self.record_membership_event(MembershipEvent::new(node_id, kind, inc));
        }
        Ok(())
    }

    /// Get all alive members
    pub async fn get_alive_members(&self) -> Vec<MemberInfo> {
        let members = self.members.read().await;
        members.values().filter(|m| m.is_alive()).cloned().collect()
    }

    /// Get all members
    pub async fn get_all_members(&self) -> Vec<MemberInfo> {
        let members = self.members.read().await;
        members.values().cloned().collect()
    }

    /// Get member count by status
    pub async fn get_member_stats(&self) -> (usize, usize, usize) {
        let members = self.members.read().await;
        let alive = members.values().filter(|m| m.is_alive()).count();
        let suspect = members.values().filter(|m| m.is_suspect()).count();
        let dead = members.values().filter(|m| m.is_dead()).count();
        (alive, suspect, dead)
    }

    /// Publish state update
    pub async fn publish_state(&self, key: String, value: Vec<u8>) -> Result<(), GossipError> {
        let mut state_store = self.state_store.write().await;

        let version = if let Some((_, existing_version)) = state_store.get(&key) {
            existing_version + 1
        } else {
            1
        };

        state_store.insert(key.clone(), (value.clone(), version));
        debug!("Published state for key {} (version: {})", key, version);

        // In real implementation, this would propagate to peers
        Ok(())
    }

    /// Get state value
    pub async fn get_state(&self, key: &str) -> Option<Vec<u8>> {
        let state_store = self.state_store.read().await;
        state_store.get(key).map(|(value, _)| value.clone())
    }

    /// Increment local incarnation (used when refuting suspicion)
    pub async fn increment_incarnation(&self) {
        let mut incarnation = self.local_incarnation.write().await;
        *incarnation += 1;
        info!("Incremented local incarnation to {}", *incarnation);
    }
}

// ============================================================================
// Hierarchical Gossip Protocol
// ============================================================================

/// Zone identifier - groups nodes by geography or function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ZoneId(pub u64);

impl ZoneId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Create zone ID from node ID (simple hash-based assignment)
    pub fn from_node_id(node_id: &NodeId, num_zones: u64) -> Self {
        let bytes = node_id.as_bytes();
        let hash = bytes
            .iter()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
        Self(hash % num_zones)
    }
}

impl std::fmt::Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "zone-{}", self.0)
    }
}

/// Role of a node in the hierarchical gossip protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GossipRole {
    /// Regular node - only gossips within its zone
    Regular,
    /// Super-peer - gossips across zones
    SuperPeer,
    /// Zone leader - coordinates zone membership
    ZoneLeader,
}

/// Zone membership information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneMember {
    pub node_id: NodeId,
    pub role: GossipRole,
    pub zone_id: ZoneId,
    pub last_seen: SystemTime,
    pub reachable: bool,
}

impl ZoneMember {
    pub fn new(node_id: NodeId, zone_id: ZoneId) -> Self {
        Self {
            node_id,
            role: GossipRole::Regular,
            zone_id,
            last_seen: SystemTime::now(),
            reachable: true,
        }
    }

    pub fn with_role(mut self, role: GossipRole) -> Self {
        self.role = role;
        self
    }

    pub fn is_super_peer(&self) -> bool {
        matches!(self.role, GossipRole::SuperPeer | GossipRole::ZoneLeader)
    }
}

/// Zone statistics for load balancing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneStats {
    pub zone_id: ZoneId,
    pub member_count: usize,
    pub super_peer_count: usize,
    pub avg_latency_ms: u64,
    pub message_rate: u64,
}

impl ZoneStats {
    pub fn new(zone_id: ZoneId) -> Self {
        Self {
            zone_id,
            ..Default::default()
        }
    }
}

/// Hierarchical gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HierarchicalMessage {
    /// Intra-zone gossip (within same zone)
    IntraZone {
        zone_id: ZoneId,
        payload: GossipMessage,
    },
    /// Inter-zone gossip (between zones via super-peers)
    InterZone {
        source_zone: ZoneId,
        target_zone: ZoneId,
        payload: GossipMessage,
        ttl: u8,
    },
    /// Zone membership announcement
    ZoneAnnounce { member: ZoneMember },
    /// Zone statistics broadcast
    ZoneStats { stats: ZoneStats },
    /// Super-peer election request
    SuperPeerElection {
        zone_id: ZoneId,
        candidate: NodeId,
        term: u64,
    },
    /// Super-peer election vote
    SuperPeerVote {
        zone_id: ZoneId,
        voter: NodeId,
        candidate: NodeId,
        term: u64,
        granted: bool,
    },
}

/// Configuration for hierarchical gossip
#[derive(Debug, Clone)]
pub struct HierarchicalGossipConfig {
    /// Number of zones
    pub num_zones: u64,
    /// Target number of super-peers per zone
    pub super_peers_per_zone: usize,
    /// Intra-zone gossip interval
    pub intra_zone_interval: Duration,
    /// Inter-zone gossip interval
    pub inter_zone_interval: Duration,
    /// Maximum TTL for inter-zone messages
    pub max_inter_zone_ttl: u8,
    /// Number of random peers to gossip to
    pub fanout: usize,
}

impl Default for HierarchicalGossipConfig {
    fn default() -> Self {
        Self {
            num_zones: 4,
            super_peers_per_zone: 3,
            intra_zone_interval: Duration::from_secs(2),
            inter_zone_interval: Duration::from_secs(10),
            max_inter_zone_ttl: 4,
            fanout: 3,
        }
    }
}

/// Hierarchical gossip state manager
pub struct HierarchicalGossip {
    /// Local node ID
    local_node_id: NodeId,
    /// Local zone ID
    local_zone: ZoneId,
    /// Local role in the gossip protocol
    local_role: Arc<RwLock<GossipRole>>,
    /// Configuration
    config: HierarchicalGossipConfig,
    /// Members by zone
    zones: Arc<RwLock<HashMap<ZoneId, HashMap<NodeId, ZoneMember>>>>,
    /// Super-peers by zone
    super_peers: Arc<RwLock<HashMap<ZoneId, HashSet<NodeId>>>>,
    /// Zone statistics
    zone_stats: Arc<RwLock<HashMap<ZoneId, ZoneStats>>>,
    /// Election state: (current_term, last_voted_for)
    election_state: Arc<RwLock<(u64, Option<NodeId>)>>,
    /// Per-term vote tally: term → (voter_id → candidate_id).
    votes_received: Arc<RwLock<HashMap<u64, HashMap<NodeId, NodeId>>>>,
    /// Message queue for outgoing messages
    outbox: Arc<RwLock<Vec<(NodeId, HierarchicalMessage)>>>,
}

impl HierarchicalGossip {
    /// Create a new hierarchical gossip manager
    pub fn new(node_id: NodeId, config: HierarchicalGossipConfig) -> Self {
        let local_zone = ZoneId::from_node_id(&node_id, config.num_zones);
        let mut zones = HashMap::new();
        let mut zone_stats = HashMap::new();

        // Initialize all zones
        for i in 0..config.num_zones {
            let zone_id = ZoneId::new(i);
            zones.insert(zone_id, HashMap::new());
            zone_stats.insert(zone_id, ZoneStats::new(zone_id));
        }

        // Add self to local zone
        let local_member = ZoneMember::new(node_id, local_zone);
        zones
            .get_mut(&local_zone)
            .expect("invariant: local_zone was just inserted in initialization loop")
            .insert(node_id, local_member);

        Self {
            local_node_id: node_id,
            local_zone,
            local_role: Arc::new(RwLock::new(GossipRole::Regular)),
            config,
            zones: Arc::new(RwLock::new(zones)),
            super_peers: Arc::new(RwLock::new(HashMap::new())),
            zone_stats: Arc::new(RwLock::new(zone_stats)),
            election_state: Arc::new(RwLock::new((0, None))),
            votes_received: Arc::new(RwLock::new(HashMap::new())),
            outbox: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get local zone ID
    pub fn local_zone(&self) -> ZoneId {
        self.local_zone
    }

    /// Get local node ID
    pub fn local_node_id(&self) -> NodeId {
        self.local_node_id
    }

    /// Get current role
    pub async fn role(&self) -> GossipRole {
        *self.local_role.read().await
    }

    /// Set local role (for testing or manual promotion)
    pub async fn set_role(&self, role: GossipRole) {
        let mut r = self.local_role.write().await;
        *r = role;
        info!("Node {} role changed to {:?}", self.local_node_id, role);
    }

    /// Add a member to the appropriate zone
    pub async fn add_member(&self, member: ZoneMember) {
        let mut zones = self.zones.write().await;
        if let Some(zone) = zones.get_mut(&member.zone_id) {
            zone.insert(member.node_id, member.clone());
            debug!("Added {} to {}", member.node_id, member.zone_id);
        }
    }

    /// Remove a member
    pub async fn remove_member(&self, node_id: &NodeId) {
        let mut zones = self.zones.write().await;
        for zone in zones.values_mut() {
            zone.remove(node_id);
        }
    }

    /// Get members in local zone
    pub async fn local_zone_members(&self) -> Vec<ZoneMember> {
        let zones = self.zones.read().await;
        zones
            .get(&self.local_zone)
            .map(|z| z.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get members in a specific zone
    pub async fn zone_members(&self, zone_id: ZoneId) -> Vec<ZoneMember> {
        let zones = self.zones.read().await;
        zones
            .get(&zone_id)
            .map(|z| z.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all super-peers
    pub async fn all_super_peers(&self) -> HashMap<ZoneId, HashSet<NodeId>> {
        self.super_peers.read().await.clone()
    }

    /// Get super-peers for a specific zone
    pub async fn zone_super_peers(&self, zone_id: ZoneId) -> HashSet<NodeId> {
        let super_peers = self.super_peers.read().await;
        super_peers.get(&zone_id).cloned().unwrap_or_default()
    }

    /// Select random peers from local zone for gossip
    pub async fn select_gossip_peers(&self, count: usize) -> Vec<NodeId> {
        let zones = self.zones.read().await;
        if let Some(zone) = zones.get(&self.local_zone) {
            let peers: Vec<_> = zone
                .keys()
                .filter(|id| **id != self.local_node_id)
                .copied()
                .collect();

            if peers.len() <= count {
                return peers;
            }

            // Simple random selection using system time
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as usize;

            peers
                .into_iter()
                .enumerate()
                .filter(|(i, _)| (now.wrapping_add(*i * 7)) % (count + 1) < count)
                .take(count)
                .map(|(_, id)| id)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Handle incoming hierarchical message
    pub async fn handle_message(
        &self,
        from: NodeId,
        message: HierarchicalMessage,
    ) -> Vec<(NodeId, HierarchicalMessage)> {
        let mut responses = Vec::new();

        match message {
            HierarchicalMessage::IntraZone { zone_id, payload } => {
                if zone_id == self.local_zone {
                    trace!("Received intra-zone message from {}", from);
                    // Process the inner gossip message
                    self.process_gossip_payload(from, payload).await;
                }
            }

            HierarchicalMessage::InterZone {
                source_zone,
                target_zone,
                payload,
                ttl,
            } => {
                if target_zone == self.local_zone {
                    // Message is for our zone - deliver locally
                    debug!(
                        "Received inter-zone message from {} (via {})",
                        source_zone, from
                    );
                    self.process_gossip_payload(from, payload).await;
                } else if ttl > 0 {
                    // Forward to super-peers in target zone or closer
                    let role = self.role().await;
                    if role == GossipRole::SuperPeer || role == GossipRole::ZoneLeader {
                        let targets = self.find_route_to_zone(target_zone).await;
                        for target in targets {
                            responses.push((
                                target,
                                HierarchicalMessage::InterZone {
                                    source_zone,
                                    target_zone,
                                    payload: payload.clone(),
                                    ttl: ttl - 1,
                                },
                            ));
                        }
                    }
                }
            }

            HierarchicalMessage::ZoneAnnounce { member } => {
                self.add_member(member).await;
            }

            HierarchicalMessage::ZoneStats { stats } => {
                let mut zone_stats = self.zone_stats.write().await;
                zone_stats.insert(stats.zone_id, stats);
            }

            HierarchicalMessage::SuperPeerElection {
                zone_id,
                candidate,
                term,
            } => {
                if zone_id == self.local_zone {
                    let vote = self.vote_for_super_peer(candidate, term).await;
                    responses.push((
                        from,
                        HierarchicalMessage::SuperPeerVote {
                            zone_id,
                            voter: self.local_node_id,
                            candidate,
                            term,
                            granted: vote,
                        },
                    ));
                }
            }

            HierarchicalMessage::SuperPeerVote {
                zone_id,
                voter,
                candidate,
                term,
                granted,
            } => {
                if zone_id == self.local_zone && candidate == self.local_node_id && granted {
                    self.record_vote(term, voter, candidate).await;
                }
            }
        }

        responses
    }

    /// Process inner gossip payload
    async fn process_gossip_payload(&self, from: NodeId, payload: GossipMessage) {
        match payload {
            GossipMessage::Heartbeat {
                node_id,
                incarnation: _,
            } => {
                // Update last_seen for the member
                let mut zones = self.zones.write().await;
                if let Some(zone) = zones.get_mut(&self.local_zone) {
                    if let Some(member) = zone.get_mut(&node_id) {
                        member.last_seen = SystemTime::now();
                        member.reachable = true;
                    }
                }
            }
            GossipMessage::MemberUpdate {
                member: gossip_member,
            } => {
                // Convert to ZoneMember and add
                let zone_member = ZoneMember::new(gossip_member.node_id, self.local_zone);
                self.add_member(zone_member).await;
            }
            GossipMessage::SyncRequest { from_node: _ } => {
                // Respond with our zone members
                let members = self.local_zone_members().await;
                debug!(
                    "Sync request from {}, responding with {} members",
                    from,
                    members.len()
                );
            }
            GossipMessage::SyncResponse { members: _ } => {
                // Update our membership view
                debug!("Received sync response from {}", from);
            }
            GossipMessage::StateUpdate {
                key: _,
                value: _,
                version: _,
            } => {
                // Handle state update
            }
        }
    }

    /// Find route to another zone via super-peers
    async fn find_route_to_zone(&self, target_zone: ZoneId) -> Vec<NodeId> {
        let super_peers = self.super_peers.read().await;

        // First, try direct super-peers in the target zone
        if let Some(targets) = super_peers.get(&target_zone) {
            return targets.iter().take(self.config.fanout).copied().collect();
        }

        // Fall back to any super-peer in adjacent zones
        let mut candidates = Vec::new();
        for (zone, peers) in super_peers.iter() {
            if *zone != self.local_zone && *zone != target_zone {
                candidates.extend(peers.iter().copied());
            }
        }
        candidates.truncate(self.config.fanout);
        candidates
    }

    /// Vote for super-peer candidate
    async fn vote_for_super_peer(&self, candidate: NodeId, term: u64) -> bool {
        let mut state = self.election_state.write().await;
        if term > state.0 {
            *state = (term, Some(candidate));
            true
        } else if term == state.0 && state.1.is_none() {
            state.1 = Some(candidate);
            true
        } else {
            false
        }
    }

    /// Record a received vote, and promote the winner when a majority is reached.
    ///
    /// `voter_id` is the node that cast the vote; `candidate_id` is whom they
    /// voted for.  A node counts itself as its own first vote when it calls
    /// `start_election`, so the tally will include that self-vote too.
    pub async fn record_vote(&self, term: u64, voter_id: NodeId, candidate_id: NodeId) {
        let winner = {
            let mut votes = self.votes_received.write().await;
            let term_votes = votes.entry(term).or_insert_with(HashMap::new);
            term_votes.insert(voter_id, candidate_id);

            // Tally votes for each candidate in this term.
            let mut tally: HashMap<NodeId, usize> = HashMap::new();
            for cand in term_votes.values() {
                *tally.entry(*cand).or_insert(0) += 1;
            }

            // Majority threshold across all known members (≥1 to handle degenerate case).
            let total_members = {
                // We can't hold the votes write-lock and the zones read-lock at the
                // same time without risking lock-order inversion, so we read the
                // tally size as a lower bound.  The real count will be fetched below.
                term_votes.len().max(1)
            };

            let majority = total_members / 2 + 1;

            tally
                .into_iter()
                .find(|(_, count)| *count >= majority)
                .map(|(cand, _)| cand)
        };

        if let Some(candidate) = winner {
            // Re-check with the authoritative zone membership count.
            let total_members = {
                let zones = self.zones.read().await;
                zones.values().flat_map(|z| z.keys()).count().max(1)
            };
            let votes_for_winner = {
                let votes = self.votes_received.read().await;
                votes
                    .get(&term)
                    .map(|tv| tv.values().filter(|&&c| c == candidate).count())
                    .unwrap_or(0)
            };
            let majority = total_members / 2 + 1;
            if votes_for_winner >= majority {
                self.promote_super_peer(term, candidate).await;
            }
        }
    }

    /// Promote `candidate` as a super-peer in their zone and record the
    /// election outcome in `election_state`.
    async fn promote_super_peer(&self, term: u64, candidate: NodeId) {
        {
            let mut zones = self.zones.write().await;
            for zone_members in zones.values_mut() {
                if let Some(member) = zone_members.get_mut(&candidate) {
                    member.role = GossipRole::SuperPeer;
                    info!(
                        "Node {} promoted to SuperPeer in zone {} (term {})",
                        candidate, member.zone_id, term
                    );
                }
            }
        }

        // Register the winner in the super_peers index.
        {
            let zones = self.zones.read().await;
            for (zone_id, zone_members) in zones.iter() {
                if zone_members.contains_key(&candidate) {
                    let mut sp = self.super_peers.write().await;
                    sp.entry(*zone_id)
                        .or_insert_with(HashSet::new)
                        .insert(candidate);
                    break;
                }
            }
        }

        let mut state = self.election_state.write().await;
        *state = (term, Some(candidate));
    }

    /// Return the list of current super-peers, one per zone that has one.
    pub async fn current_super_peers(&self) -> Vec<(ZoneId, NodeId)> {
        let super_peers = self.super_peers.read().await;
        let mut result = Vec::new();
        for (zone_id, peers) in super_peers.iter() {
            for &node_id in peers {
                result.push((*zone_id, node_id));
            }
        }
        result
    }

    /// Return the current election term and the winning candidate (if any).
    pub async fn get_election_state(&self) -> (u64, Option<NodeId>) {
        *self.election_state.read().await
    }

    /// Start super-peer election for local zone
    pub async fn start_election(&self) -> Vec<(NodeId, HierarchicalMessage)> {
        let mut state = self.election_state.write().await;
        state.0 += 1;
        state.1 = Some(self.local_node_id);
        let term = state.0;
        drop(state);

        let peers = self.local_zone_members().await;
        peers
            .into_iter()
            .filter(|m| m.node_id != self.local_node_id)
            .map(|m| {
                (
                    m.node_id,
                    HierarchicalMessage::SuperPeerElection {
                        zone_id: self.local_zone,
                        candidate: self.local_node_id,
                        term,
                    },
                )
            })
            .collect()
    }

    /// Broadcast message to all zones (super-peer only)
    pub async fn broadcast_inter_zone(
        &self,
        payload: GossipMessage,
    ) -> Vec<(NodeId, HierarchicalMessage)> {
        let role = self.role().await;
        if role != GossipRole::SuperPeer && role != GossipRole::ZoneLeader {
            return Vec::new();
        }

        let mut messages = Vec::new();
        let super_peers = self.super_peers.read().await;

        for (zone_id, peers) in super_peers.iter() {
            if *zone_id == self.local_zone {
                continue;
            }

            for peer in peers.iter().take(1) {
                messages.push((
                    *peer,
                    HierarchicalMessage::InterZone {
                        source_zone: self.local_zone,
                        target_zone: *zone_id,
                        payload: payload.clone(),
                        ttl: self.config.max_inter_zone_ttl,
                    },
                ));
            }
        }

        messages
    }

    /// Get zone statistics
    pub async fn get_zone_stats(&self, zone_id: ZoneId) -> Option<ZoneStats> {
        let stats = self.zone_stats.read().await;
        stats.get(&zone_id).cloned()
    }

    /// Update local zone statistics
    pub async fn update_local_stats(&self) {
        let zones = self.zones.read().await;
        let super_peers = self.super_peers.read().await;

        if let Some(zone) = zones.get(&self.local_zone) {
            let super_peer_count = super_peers
                .get(&self.local_zone)
                .map(|s| s.len())
                .unwrap_or(0);

            let stats = ZoneStats {
                zone_id: self.local_zone,
                member_count: zone.len(),
                super_peer_count,
                avg_latency_ms: 0, // Would be computed from actual measurements
                message_rate: 0,   // Would be computed from actual counts
            };

            drop(zones);
            drop(super_peers);

            let mut zone_stats = self.zone_stats.write().await;
            zone_stats.insert(self.local_zone, stats);
        }
    }

    /// Get pending outbox messages
    pub async fn drain_outbox(&self) -> Vec<(NodeId, HierarchicalMessage)> {
        let mut outbox = self.outbox.write().await;
        std::mem::take(&mut *outbox)
    }

    /// Queue a message for sending
    pub async fn queue_message(&self, target: NodeId, message: HierarchicalMessage) {
        let mut outbox = self.outbox.write().await;
        outbox.push((target, message));
    }

    /// Get total member count across all zones
    pub async fn total_member_count(&self) -> usize {
        let zones = self.zones.read().await;
        zones.values().map(|z| z.len()).sum()
    }

    /// Get zone count
    pub fn zone_count(&self) -> u64 {
        self.config.num_zones
    }
}

#[cfg(test)]
#[path = "gossip_tests.rs"]
mod tests;
