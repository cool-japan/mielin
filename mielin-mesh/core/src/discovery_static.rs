//! Static Peer List Discovery
//!
//! Provides deterministic, configuration-driven peer discovery for cases where
//! mDNS is unavailable or undesired — typical for production deployments with
//! known topology (e.g., Kubernetes services, cloud load-balancers, VPN meshes).
//!
//! The list tracks live health state per peer via `PeerHealth`, supports
//! weighted random selection using a Xorshift64 PRNG (no external dep), and
//! prunes peers that have exceeded a configurable failure threshold.

use crate::{Node, NodeId, NodeRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// StaticPeer
// ---------------------------------------------------------------------------

/// A statically configured peer with optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticPeer {
    pub node_id: NodeId,
    pub address: SocketAddr,
    pub role: NodeRole,
    /// Arbitrary string labels (e.g. "edge", "us-east-1")
    pub tags: Vec<String>,
    /// Load-balancing weight; higher = more selection probability (default 1)
    pub weight: u32,
    pub enabled: bool,
}

impl StaticPeer {
    pub fn new(node_id: NodeId, address: SocketAddr) -> Self {
        Self {
            node_id,
            address,
            role: NodeRole::Relay,
            tags: Vec::new(),
            weight: 1,
            enabled: true,
        }
    }

    pub fn with_role(mut self, role: NodeRole) -> Self {
        self.role = role;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

// ---------------------------------------------------------------------------
// PeerHealth
// ---------------------------------------------------------------------------

/// Health state of a static peer.
///
/// The `Instant` fields are deliberately not serialisable — they are
/// runtime-only state, not configuration.
#[derive(Debug, Clone)]
pub enum PeerHealth {
    /// Health has never been probed.
    Unknown,
    /// Peer is reachable; optionally carries last observed latency.
    Reachable {
        last_seen: Instant,
        latency_ms: Option<u64>,
    },
    /// Peer is currently unreachable.
    Unreachable {
        last_attempt: Instant,
        failure_count: u32,
    },
    /// Peer has been administratively disabled.
    Disabled,
}

impl PartialEq for PeerHealth {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (PeerHealth::Unknown, PeerHealth::Unknown)
                | (PeerHealth::Reachable { .. }, PeerHealth::Reachable { .. })
                | (
                    PeerHealth::Unreachable { .. },
                    PeerHealth::Unreachable { .. }
                )
                | (PeerHealth::Disabled, PeerHealth::Disabled)
        )
    }
}

impl Eq for PeerHealth {}

impl PeerHealth {
    /// Returns `true` for `Reachable` and `Unknown` (benefit-of-the-doubt).
    pub fn is_healthy(&self) -> bool {
        matches!(self, PeerHealth::Reachable { .. } | PeerHealth::Unknown)
    }

    /// Returns the consecutive failure count, or 0 for healthy states.
    pub fn failure_count(&self) -> u32 {
        match self {
            PeerHealth::Unreachable { failure_count, .. } => *failure_count,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Runtime statistics snapshot for a `StaticPeerList`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StaticPeerStats {
    pub total_peers: usize,
    pub enabled_peers: usize,
    pub healthy_peers: usize,
    pub unreachable_peers: usize,
    pub refresh_count: u64,
    pub last_refresh: Option<std::time::SystemTime>,
}

// ---------------------------------------------------------------------------
// Xorshift64 — deterministic PRNG (no external dep)
// ---------------------------------------------------------------------------

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Guard against a zero seed which would produce an all-zero sequence.
        Self(if seed == 0 {
            0xDEAD_BEEF_CAFE_BABE
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

// ---------------------------------------------------------------------------
// StaticPeerList
// ---------------------------------------------------------------------------

/// Thread-safe static peer registry with health tracking.
pub struct StaticPeerList {
    /// Map from `NodeId` to `(peer config, health state)`.
    peers: Arc<RwLock<HashMap<NodeId, (StaticPeer, PeerHealth)>>>,
    local_node: Arc<Node>,
    refresh_interval: Duration,
    max_failures_before_remove: u32,
    stats: Arc<RwLock<StaticPeerStats>>,
}

impl StaticPeerList {
    pub fn new(local_node: Arc<Node>) -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            local_node,
            refresh_interval: Duration::from_secs(30),
            max_failures_before_remove: 5,
            stats: Arc::new(RwLock::new(StaticPeerStats::default())),
        }
    }

    pub fn with_refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = interval;
        self
    }

    pub fn with_max_failures(mut self, max: u32) -> Self {
        self.max_failures_before_remove = max;
        self
    }

    /// Add a peer to the list. Silently overwrites an existing entry.
    pub async fn add_peer(&self, peer: StaticPeer) {
        let health = if peer.enabled {
            PeerHealth::Unknown
        } else {
            PeerHealth::Disabled
        };
        let mut peers = self.peers.write().await;
        peers.insert(peer.node_id, (peer, health));
        self.update_stats_locked(&peers).await;
    }

    /// Remove a peer by its `NodeId`.
    pub async fn remove_peer(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        peers.remove(node_id);
        self.update_stats_locked(&peers).await;
    }

    /// Enable a previously disabled peer.
    pub async fn enable_peer(&self, node_id: &NodeId) -> Result<(), StaticDiscoveryError> {
        let mut peers = self.peers.write().await;
        match peers.get_mut(node_id) {
            Some((peer, health)) => {
                peer.enabled = true;
                *health = PeerHealth::Unknown;
                self.update_stats_locked(&peers).await;
                Ok(())
            }
            None => Err(StaticDiscoveryError::PeerNotFound {
                node_id: node_id.to_string(),
            }),
        }
    }

    /// Administratively disable a peer (it will be excluded from `healthy_peers`).
    pub async fn disable_peer(&self, node_id: &NodeId) -> Result<(), StaticDiscoveryError> {
        let mut peers = self.peers.write().await;
        match peers.get_mut(node_id) {
            Some((peer, health)) => {
                peer.enabled = false;
                *health = PeerHealth::Disabled;
                self.update_stats_locked(&peers).await;
                Ok(())
            }
            None => Err(StaticDiscoveryError::PeerNotFound {
                node_id: node_id.to_string(),
            }),
        }
    }

    /// Record a successful contact from a peer (updates health to `Reachable`).
    pub async fn mark_reachable(&self, node_id: &NodeId, latency_ms: Option<u64>) {
        let mut peers = self.peers.write().await;
        if let Some((_, health)) = peers.get_mut(node_id) {
            *health = PeerHealth::Reachable {
                last_seen: Instant::now(),
                latency_ms,
            };
            self.update_stats_locked(&peers).await;
        }
    }

    /// Record a failed connection attempt (increments the failure counter).
    pub async fn mark_unreachable(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some((_, health)) = peers.get_mut(node_id) {
            let prev_count = health.failure_count();
            *health = PeerHealth::Unreachable {
                last_attempt: Instant::now(),
                failure_count: prev_count.saturating_add(1),
            };
            self.update_stats_locked(&peers).await;
        }
    }

    /// Return all enabled, healthy peers.
    pub async fn healthy_peers(&self) -> Vec<StaticPeer> {
        let peers = self.peers.read().await;
        peers
            .values()
            .filter(|(p, h)| p.enabled && h.is_healthy())
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Return all enabled peers whose `tags` list contains `tag`.
    pub async fn peers_with_tag(&self, tag: &str) -> Vec<StaticPeer> {
        let peers = self.peers.read().await;
        peers
            .values()
            .filter(|(p, _)| p.enabled && p.tags.iter().any(|t| t == tag))
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Retrieve a single peer by `NodeId`, regardless of health state.
    pub async fn get_peer(&self, node_id: &NodeId) -> Option<StaticPeer> {
        let peers = self.peers.read().await;
        peers.get(node_id).map(|(p, _)| p.clone())
    }

    /// Weighted-random selection from healthy peers using a deterministic Xorshift64.
    ///
    /// The selection algorithm: compute total weight of all healthy peers, draw
    /// a random value in `[0, total_weight)`, then walk the peer list until the
    /// cumulative weight exceeds the draw. This is equivalent to a weighted
    /// roulette wheel with O(n) lookup.
    pub async fn select_peer(&self, seed: u64) -> Option<StaticPeer> {
        let candidates = self.healthy_peers().await;
        if candidates.is_empty() {
            return None;
        }
        let total_weight: u64 = candidates.iter().map(|p| p.weight as u64).sum();
        if total_weight == 0 {
            // Degenerate: all weights are zero; fall back to uniform selection.
            let mut rng = Xorshift64::new(seed);
            let idx = (rng.next() as usize) % candidates.len();
            return candidates.into_iter().nth(idx);
        }
        let mut rng = Xorshift64::new(seed);
        let draw = rng.next() % total_weight;
        let mut cumulative: u64 = 0;
        for peer in &candidates {
            cumulative += peer.weight as u64;
            if cumulative > draw {
                return Some(peer.clone());
            }
        }
        // Fallback — logically unreachable with positive weights.
        candidates.into_iter().next()
    }

    /// Remove peers whose failure count has exceeded `max_failures_before_remove`.
    ///
    /// Returns the number of pruned entries.
    pub async fn prune_unreachable(&self) -> usize {
        let max = self.max_failures_before_remove;
        let mut peers = self.peers.write().await;
        let before = peers.len();
        peers.retain(|_, (_, health)| match health {
            PeerHealth::Unreachable { failure_count, .. } => *failure_count <= max,
            _ => true,
        });
        let pruned = before - peers.len();
        if pruned > 0 {
            self.update_stats_locked(&peers).await;
        }
        pruned
    }

    /// Bulk-load a set of peers, replacing any with matching `NodeId`s.
    pub async fn load_from_config(&self, peers_cfg: Vec<StaticPeer>) {
        let mut peers = self.peers.write().await;
        for peer in peers_cfg {
            let health = if peer.enabled {
                PeerHealth::Unknown
            } else {
                PeerHealth::Disabled
            };
            peers.insert(peer.node_id, (peer, health));
        }
        self.update_stats_locked(&peers).await;
    }

    /// Get a snapshot of current statistics.
    pub async fn stats(&self) -> StaticPeerStats {
        self.stats.read().await.clone()
    }

    /// Current number of registered peers (regardless of health).
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Expose the local node for callers that need to identify themselves.
    pub fn local_node(&self) -> &Arc<Node> {
        &self.local_node
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Recompute and persist stats from a live peers map.
    ///
    /// Caller must hold the write lock on `self.peers`.
    async fn update_stats_locked(&self, peers: &HashMap<NodeId, (StaticPeer, PeerHealth)>) {
        let total_peers = peers.len();
        let enabled_peers = peers.values().filter(|(p, _)| p.enabled).count();
        let healthy_peers = peers
            .values()
            .filter(|(p, h)| p.enabled && h.is_healthy())
            .count();
        let unreachable_peers = peers
            .values()
            .filter(|(p, h)| p.enabled && matches!(h, PeerHealth::Unreachable { .. }))
            .count();

        let mut stats = self.stats.write().await;
        stats.total_peers = total_peers;
        stats.enabled_peers = enabled_peers;
        stats.healthy_peers = healthy_peers;
        stats.unreachable_peers = unreachable_peers;
        stats.refresh_count += 1;
        stats.last_refresh = Some(std::time::SystemTime::now());
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum StaticDiscoveryError {
    #[error("Peer not found: {node_id}")]
    PeerNotFound { node_id: String },

    #[error("Peer already exists: {node_id}")]
    PeerAlreadyExists { node_id: String },

    #[error("No healthy peers available")]
    NoHealthyPeers,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_node() -> Arc<Node> {
        Arc::new(Node::new(NodeRole::Relay))
    }

    fn make_addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{port}").parse().unwrap()
    }

    fn make_peer(port: u16) -> StaticPeer {
        StaticPeer::new(uuid::Uuid::new_v4(), make_addr(port))
    }

    // 1. Empty list on construction
    #[tokio::test]
    async fn test_static_peer_list_new() {
        let list = StaticPeerList::new(make_node());
        assert_eq!(list.peer_count().await, 0);
        let stats = list.stats().await;
        assert_eq!(stats.total_peers, 0);
    }

    // 2. Add and retrieve a peer
    #[tokio::test]
    async fn test_static_peer_add_get() {
        let list = StaticPeerList::new(make_node());
        let peer = make_peer(9001);
        let id = peer.node_id;
        list.add_peer(peer).await;
        assert_eq!(list.peer_count().await, 1);
        let found = list.get_peer(&id).await.expect("peer must exist");
        assert_eq!(found.node_id, id);
    }

    // 3. Mark a peer as reachable
    #[tokio::test]
    async fn test_static_peer_mark_reachable() {
        let list = StaticPeerList::new(make_node());
        let peer = make_peer(9002);
        let id = peer.node_id;
        list.add_peer(peer).await;
        list.mark_reachable(&id, Some(5)).await;
        let healthy = list.healthy_peers().await;
        assert_eq!(healthy.len(), 1);
        let stats = list.stats().await;
        assert_eq!(stats.healthy_peers, 1);
    }

    // 4. Mark a peer as unreachable
    #[tokio::test]
    async fn test_static_peer_mark_unreachable() {
        let list = StaticPeerList::new(make_node());
        let peer = make_peer(9003);
        let id = peer.node_id;
        list.add_peer(peer).await;
        list.mark_unreachable(&id).await;
        let healthy = list.healthy_peers().await;
        assert_eq!(healthy.len(), 0);
        let stats = list.stats().await;
        assert_eq!(stats.unreachable_peers, 1);
    }

    // 5. Disabled peer not in healthy_peers
    #[tokio::test]
    async fn test_static_peer_disable() {
        let list = StaticPeerList::new(make_node());
        let peer = make_peer(9004);
        let id = peer.node_id;
        list.add_peer(peer).await;
        list.disable_peer(&id).await.expect("disable ok");
        let healthy = list.healthy_peers().await;
        assert_eq!(healthy.len(), 0);
    }

    // 6. Tag filtering
    #[tokio::test]
    async fn test_static_peer_tags_filter() {
        let list = StaticPeerList::new(make_node());
        let edge_peer = make_peer(9010).with_tags(vec!["edge".into(), "us-east".into()]);
        let core_peer = make_peer(9011).with_tags(vec!["core".into()]);
        list.add_peer(edge_peer).await;
        list.add_peer(core_peer).await;

        let edge_found = list.peers_with_tag("edge").await;
        assert_eq!(edge_found.len(), 1);
        assert!(edge_found[0].tags.contains(&"edge".to_string()));

        let core_found = list.peers_with_tag("core").await;
        assert_eq!(core_found.len(), 1);

        let none_found = list.peers_with_tag("nonexistent").await;
        assert_eq!(none_found.len(), 0);
    }

    // 7. Select peer (basic — seed produces a peer)
    #[tokio::test]
    async fn test_static_peer_select_weighted() {
        let list = StaticPeerList::new(make_node());
        list.add_peer(make_peer(9020)).await;
        list.add_peer(make_peer(9021)).await;
        let selected = list.select_peer(42).await;
        assert!(selected.is_some());
    }

    // 8. Prune removes high-failure peers
    #[tokio::test]
    async fn test_static_peer_prune() {
        let list = StaticPeerList::new(make_node()).with_max_failures(2);
        let peer = make_peer(9030);
        let id = peer.node_id;
        list.add_peer(peer).await;
        // 3 failures — exceeds max of 2
        list.mark_unreachable(&id).await;
        list.mark_unreachable(&id).await;
        list.mark_unreachable(&id).await;
        let pruned = list.prune_unreachable().await;
        assert_eq!(pruned, 1);
        assert_eq!(list.peer_count().await, 0);
    }

    // 9. load_from_config adds all peers
    #[tokio::test]
    async fn test_static_peer_load_config() {
        let list = StaticPeerList::new(make_node());
        let peers = vec![make_peer(9040), make_peer(9041), make_peer(9042)];
        list.load_from_config(peers).await;
        assert_eq!(list.peer_count().await, 3);
    }

    // 10. Stats reflect health updates
    #[tokio::test]
    async fn test_static_peer_stats_track() {
        let list = StaticPeerList::new(make_node());
        let p1 = make_peer(9050);
        let p2 = make_peer(9051);
        let id1 = p1.node_id;
        let id2 = p2.node_id;
        list.add_peer(p1).await;
        list.add_peer(p2).await;

        // Both Unknown => healthy (benefit of doubt)
        let stats = list.stats().await;
        assert_eq!(stats.healthy_peers, 2);

        list.mark_unreachable(&id1).await;
        let stats = list.stats().await;
        assert_eq!(stats.healthy_peers, 1);
        assert_eq!(stats.unreachable_peers, 1);

        list.mark_reachable(&id2, None).await;
        let stats = list.stats().await;
        // id2 was already healthy (Unknown → Reachable), id1 still unreachable
        assert_eq!(stats.healthy_peers, 1);
    }

    // 11. Enable/disable cycle
    #[tokio::test]
    async fn test_static_peer_enable_disable_cycle() {
        let list = StaticPeerList::new(make_node());
        let peer = make_peer(9060);
        let id = peer.node_id;
        list.add_peer(peer).await;

        list.disable_peer(&id).await.expect("disable ok");
        assert_eq!(list.healthy_peers().await.len(), 0);

        list.enable_peer(&id).await.expect("enable ok");
        // After re-enable health is Unknown (healthy, benefit-of-doubt)
        assert_eq!(list.healthy_peers().await.len(), 1);
    }

    // 12. Higher-weight peers selected more often over many trials
    #[tokio::test]
    async fn test_static_peer_weighted_selection_bias() {
        let list = StaticPeerList::new(make_node());
        let light_peer = make_peer(9070).with_weight(1);
        let heavy_peer = make_peer(9071).with_weight(9);
        let heavy_id = heavy_peer.node_id;

        list.add_peer(light_peer).await;
        list.add_peer(heavy_peer).await;

        let trials = 200u64;
        let mut heavy_count = 0u64;
        for seed in 0..trials {
            if let Some(p) = list.select_peer(seed * 7 + 13).await {
                if p.node_id == heavy_id {
                    heavy_count += 1;
                }
            }
        }
        // Heavy peer has 90% probability; expect at least 60% in a fair sample
        let ratio = heavy_count as f64 / trials as f64;
        assert!(
            ratio >= 0.60,
            "Expected heavy peer selected >=60% but got {ratio:.2}"
        );
    }
}
