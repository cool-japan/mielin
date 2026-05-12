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
use std::collections::{HashMap, HashSet};
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

    pub fn should_suspect(&self) -> bool {
        self.is_alive() && self.heartbeat_age() > HEARTBEAT_TIMEOUT
    }

    pub fn should_declare_dead(&self) -> bool {
        self.is_suspect() && self.heartbeat_age() > FAILURE_TIMEOUT
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
}

impl GossipState {
    pub fn new(node: Arc<Node>) -> Self {
        let node_id = *node.id();
        let mut members = HashMap::new();
        members.insert(node_id, MemberInfo::new(node_id));

        Self {
            local_node: node,
            members: Arc::new(RwLock::new(members)),
            local_incarnation: Arc::new(RwLock::new(0)),
            state_store: Arc::new(RwLock::new(HashMap::new())),
        }
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

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(GOSSIP_INTERVAL);
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

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let mut members = members.write().await;
                let mut updates = Vec::new();

                for (id, member) in members.iter() {
                    if *id == node_id {
                        continue; // Skip self
                    }

                    if member.should_declare_dead() {
                        updates.push((*id, HealthStatus::Dead));
                        warn!(
                            "Declaring node {} as dead (no heartbeat for {:?})",
                            id,
                            member.heartbeat_age()
                        );
                    } else if member.should_suspect() {
                        updates.push((*id, HealthStatus::Suspect));
                        debug!(
                            "Marking node {} as suspect (no heartbeat for {:?})",
                            id,
                            member.heartbeat_age()
                        );
                    }
                }

                for (id, status) in updates {
                    if let Some(member) = members.get_mut(&id) {
                        member.status = status;
                    }
                }
            }
        });
    }

    /// Spawn gossip propagation task
    fn spawn_gossip_task(&self) {
        let members = self.members.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(GOSSIP_INTERVAL);
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
                // 1. Select random peers to gossip with
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
        let mut members = self.members.write().await;

        if let Some(member) = members.get_mut(&node_id) {
            // Update if incarnation is newer
            if incarnation > member.incarnation {
                member.incarnation = incarnation;
                member.last_seen = SystemTime::now();
                member.status = HealthStatus::Alive;
                debug!(
                    "Updated heartbeat for node {} (incarnation: {})",
                    node_id, incarnation
                );
            }
        } else {
            // New member discovered via heartbeat
            let mut member = MemberInfo::new(node_id);
            member.incarnation = incarnation;
            members.insert(node_id, member);
            info!("Discovered new member {} via heartbeat", node_id);
        }

        Ok(())
    }

    /// Handle member update from gossip
    async fn handle_member_update(&self, new_info: MemberInfo) -> Result<(), GossipError> {
        let mut members = self.members.write().await;

        if let Some(existing) = members.get_mut(&new_info.node_id) {
            // Update if incarnation is newer
            if new_info.incarnation > existing.incarnation {
                *existing = new_info;
                debug!("Updated member info for {}", existing.node_id);
            }
        } else {
            // New member
            members.insert(new_info.node_id, new_info.clone());
            info!("Added new member {} to membership", new_info.node_id);
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
    /// Election state: (term, voted_for)
    election_state: Arc<RwLock<(u64, Option<NodeId>)>>,
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
            .expect("local_zone was just inserted above")
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
                voter: _,
                candidate,
                term,
                granted,
            } => {
                if zone_id == self.local_zone && candidate == self.local_node_id && granted {
                    self.record_vote(term).await;
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

    /// Record a vote received
    async fn record_vote(&self, term: u64) {
        let state = self.election_state.read().await;
        if term == state.0 {
            // In a full implementation, count votes and promote if majority
            debug!("Received vote for term {}", term);
        }
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
mod tests {
    use super::*;
    use crate::NodeRole;

    #[test]
    fn test_member_info_creation() {
        let node_id = NodeId::new_v4();
        let member = MemberInfo::new(node_id);

        assert_eq!(member.node_id, node_id);
        assert!(member.is_alive());
        assert!(!member.is_suspect());
        assert!(!member.is_dead());
    }

    #[test]
    fn test_heartbeat_timeout_detection() {
        let node_id = NodeId::new_v4();
        let mut member = MemberInfo::new(node_id);
        member.last_seen = SystemTime::now() - Duration::from_secs(20);

        assert!(member.should_suspect());
        assert!(!member.should_declare_dead());
    }

    #[test]
    fn test_failure_timeout_detection() {
        let node_id = NodeId::new_v4();
        let mut member = MemberInfo::new(node_id);
        member.status = HealthStatus::Suspect;
        member.last_seen = SystemTime::now() - Duration::from_secs(35);

        assert!(member.should_declare_dead());
    }

    #[tokio::test]
    async fn test_gossip_state_creation() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        let members = gossip.get_all_members().await;
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].node_id, *node.id());
    }

    #[tokio::test]
    async fn test_add_member() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        let new_node_id = NodeId::new_v4();
        gossip.add_member(new_node_id).await.unwrap();

        let members = gossip.get_all_members().await;
        assert_eq!(members.len(), 2);
    }

    #[tokio::test]
    async fn test_heartbeat_handling() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        let peer_id = NodeId::new_v4();
        gossip.handle_heartbeat(peer_id, 1).await.unwrap();

        let members = gossip.get_all_members().await;
        assert_eq!(members.len(), 2);

        let peer_member = members.iter().find(|m| m.node_id == peer_id).unwrap();
        assert_eq!(peer_member.incarnation, 1);
        assert!(peer_member.is_alive());
    }

    #[tokio::test]
    async fn test_state_updates() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        let key = "test_key".to_string();
        let value = vec![1, 2, 3, 4];

        gossip
            .publish_state(key.clone(), value.clone())
            .await
            .unwrap();

        let retrieved = gossip.get_state(&key).await;
        assert_eq!(retrieved, Some(value));
    }

    #[tokio::test]
    async fn test_sync_request_response() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        // Add some members
        gossip.add_member(NodeId::new_v4()).await.unwrap();
        gossip.add_member(NodeId::new_v4()).await.unwrap();

        let request = GossipMessage::SyncRequest {
            from_node: NodeId::new_v4(),
        };

        let response = gossip.handle_message(request).await.unwrap();
        assert!(response.is_some());

        if let Some(GossipMessage::SyncResponse { members }) = response {
            assert_eq!(members.len(), 3); // local + 2 added
        } else {
            panic!("Expected SyncResponse");
        }
    }

    #[tokio::test]
    async fn test_member_stats() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let gossip = GossipState::new(node.clone());

        gossip.add_member(NodeId::new_v4()).await.unwrap();
        gossip.add_member(NodeId::new_v4()).await.unwrap();

        let (alive, suspect, dead) = gossip.get_member_stats().await;
        assert_eq!(alive, 3);
        assert_eq!(suspect, 0);
        assert_eq!(dead, 0);
    }

    // ========================================================================
    // Hierarchical Gossip Tests
    // ========================================================================

    #[test]
    fn test_zone_id_from_node() {
        let node1 = NodeId::new_v4();
        let node2 = NodeId::new_v4();
        let num_zones = 4;

        let zone1 = ZoneId::from_node_id(&node1, num_zones);
        let zone2 = ZoneId::from_node_id(&node2, num_zones);

        // Both should be within valid range
        assert!(zone1.0 < num_zones);
        assert!(zone2.0 < num_zones);

        // Same node should always get same zone
        assert_eq!(zone1, ZoneId::from_node_id(&node1, num_zones));
    }

    #[test]
    fn test_zone_member_creation() {
        let node_id = NodeId::new_v4();
        let zone_id = ZoneId::new(1);
        let member = ZoneMember::new(node_id, zone_id);

        assert_eq!(member.node_id, node_id);
        assert_eq!(member.zone_id, zone_id);
        assert_eq!(member.role, GossipRole::Regular);
        assert!(member.reachable);
        assert!(!member.is_super_peer());
    }

    #[test]
    fn test_zone_member_with_role() {
        let node_id = NodeId::new_v4();
        let zone_id = ZoneId::new(2);
        let member = ZoneMember::new(node_id, zone_id).with_role(GossipRole::SuperPeer);

        assert!(member.is_super_peer());
        assert_eq!(member.role, GossipRole::SuperPeer);
    }

    #[tokio::test]
    async fn test_hierarchical_gossip_creation() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        assert_eq!(gossip.local_node_id(), node_id);
        assert!(gossip.local_zone().0 < 4); // Default is 4 zones
        assert_eq!(gossip.role().await, GossipRole::Regular);
    }

    #[tokio::test]
    async fn test_hierarchical_add_member() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let peer_id = NodeId::new_v4();
        let member = ZoneMember::new(peer_id, gossip.local_zone());
        gossip.add_member(member).await;

        let members = gossip.local_zone_members().await;
        assert_eq!(members.len(), 2); // Self + peer
    }

    #[tokio::test]
    async fn test_hierarchical_role_change() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        gossip.set_role(GossipRole::SuperPeer).await;
        assert_eq!(gossip.role().await, GossipRole::SuperPeer);

        gossip.set_role(GossipRole::ZoneLeader).await;
        assert_eq!(gossip.role().await, GossipRole::ZoneLeader);
    }

    #[tokio::test]
    async fn test_hierarchical_intra_zone_message() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let from = NodeId::new_v4();
        let message = HierarchicalMessage::IntraZone {
            zone_id: gossip.local_zone(),
            payload: GossipMessage::Heartbeat {
                node_id: from,
                incarnation: 1,
            },
        };

        let responses = gossip.handle_message(from, message).await;
        assert!(responses.is_empty()); // Heartbeat doesn't generate response
    }

    #[tokio::test]
    async fn test_hierarchical_zone_announce() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let peer_id = NodeId::new_v4();
        let member = ZoneMember::new(peer_id, gossip.local_zone());
        let message = HierarchicalMessage::ZoneAnnounce {
            member: member.clone(),
        };

        gossip.handle_message(peer_id, message).await;

        let members = gossip.local_zone_members().await;
        assert!(members.iter().any(|m| m.node_id == peer_id));
    }

    #[tokio::test]
    async fn test_hierarchical_election() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        // Add some peers to local zone
        for _ in 0..3 {
            let peer = ZoneMember::new(NodeId::new_v4(), gossip.local_zone());
            gossip.add_member(peer).await;
        }

        let election_messages = gossip.start_election().await;
        assert_eq!(election_messages.len(), 3); // 3 peers

        for (_, msg) in &election_messages {
            if let HierarchicalMessage::SuperPeerElection {
                zone_id,
                candidate,
                term,
            } = msg
            {
                assert_eq!(*zone_id, gossip.local_zone());
                assert_eq!(*candidate, node_id);
                assert_eq!(*term, 1);
            } else {
                panic!("Expected SuperPeerElection message");
            }
        }
    }

    #[tokio::test]
    async fn test_hierarchical_vote() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let candidate = NodeId::new_v4();
        let message = HierarchicalMessage::SuperPeerElection {
            zone_id: gossip.local_zone(),
            candidate,
            term: 1,
        };

        let responses = gossip.handle_message(candidate, message).await;
        assert_eq!(responses.len(), 1);

        if let HierarchicalMessage::SuperPeerVote {
            granted,
            term,
            candidate: vote_for,
            ..
        } = &responses[0].1
        {
            assert!(*granted);
            assert_eq!(*term, 1);
            assert_eq!(*vote_for, candidate);
        } else {
            panic!("Expected SuperPeerVote message");
        }
    }

    #[tokio::test]
    async fn test_hierarchical_vote_only_once_per_term() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let candidate1 = NodeId::new_v4();
        let candidate2 = NodeId::new_v4();

        // First vote should be granted
        let msg1 = HierarchicalMessage::SuperPeerElection {
            zone_id: gossip.local_zone(),
            candidate: candidate1,
            term: 1,
        };
        let responses1 = gossip.handle_message(candidate1, msg1).await;
        if let HierarchicalMessage::SuperPeerVote { granted, .. } = &responses1[0].1 {
            assert!(*granted);
        }

        // Second vote in same term should not be granted
        let msg2 = HierarchicalMessage::SuperPeerElection {
            zone_id: gossip.local_zone(),
            candidate: candidate2,
            term: 1,
        };
        let responses2 = gossip.handle_message(candidate2, msg2).await;
        if let HierarchicalMessage::SuperPeerVote { granted, .. } = &responses2[0].1 {
            assert!(!*granted);
        }
    }

    #[tokio::test]
    async fn test_hierarchical_zone_stats() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        // Add members and update stats
        for _ in 0..5 {
            let peer = ZoneMember::new(NodeId::new_v4(), gossip.local_zone());
            gossip.add_member(peer).await;
        }

        gossip.update_local_stats().await;

        let stats = gossip.get_zone_stats(gossip.local_zone()).await;
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().member_count, 6); // Self + 5 peers
    }

    #[tokio::test]
    async fn test_hierarchical_total_member_count() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig {
            num_zones: 2,
            ..Default::default()
        };
        let gossip = HierarchicalGossip::new(node_id, config);

        // Add members to different zones
        let zone0 = ZoneId::new(0);
        let zone1 = ZoneId::new(1);

        gossip
            .add_member(ZoneMember::new(NodeId::new_v4(), zone0))
            .await;
        gossip
            .add_member(ZoneMember::new(NodeId::new_v4(), zone1))
            .await;

        let total = gossip.total_member_count().await;
        // 1 (self) + 2 added (but one might be in same zone as self)
        assert!(total >= 2);
    }

    #[tokio::test]
    async fn test_hierarchical_message_queue() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let target = NodeId::new_v4();
        let message = HierarchicalMessage::ZoneAnnounce {
            member: ZoneMember::new(node_id, gossip.local_zone()),
        };

        gossip.queue_message(target, message).await;

        let outbox = gossip.drain_outbox().await;
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].0, target);

        // Queue should be empty after drain
        let empty = gossip.drain_outbox().await;
        assert!(empty.is_empty());
    }

    #[test]
    fn test_zone_id_display() {
        let zone = ZoneId::new(42);
        assert_eq!(format!("{}", zone), "zone-42");
    }

    #[test]
    fn test_hierarchical_config_default() {
        let config = HierarchicalGossipConfig::default();
        assert_eq!(config.num_zones, 4);
        assert_eq!(config.super_peers_per_zone, 3);
        assert_eq!(config.fanout, 3);
        assert_eq!(config.max_inter_zone_ttl, 4);
    }

    #[tokio::test]
    async fn test_hierarchical_remove_member() {
        let node_id = NodeId::new_v4();
        let config = HierarchicalGossipConfig::default();
        let gossip = HierarchicalGossip::new(node_id, config);

        let peer_id = NodeId::new_v4();
        let member = ZoneMember::new(peer_id, gossip.local_zone());
        gossip.add_member(member).await;

        let before = gossip.local_zone_members().await;
        assert!(before.iter().any(|m| m.node_id == peer_id));

        gossip.remove_member(&peer_id).await;

        let after = gossip.local_zone_members().await;
        assert!(!after.iter().any(|m| m.node_id == peer_id));
    }
}
