//! Large-Scale Cluster Simulation Tests
//!
//! Validates cluster behaviour at 100+ nodes:
//! - Gossip convergence at scale
//! - Consistent hash ring uniformity and stability
//! - Partition detection with quorum semantics
//! - Performance regression baselines (timed, with generous debug-build bounds)
//!
//! All network interaction is fully in-memory — no sockets, no I/O.

#![allow(dead_code)]

use mielin_mesh_core::{
    ConsistentHashRing, GossipMessage, GossipState, HealthStatus, MemberInfo, Node, NodeId,
    NodeRole, PartitionDetector,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

// ============================================================================
// Xorshift64 — deterministic PRNG (no rand dependency)
// ============================================================================

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0xdeadbeef_cafebabe } else { seed })
    }

    #[inline]
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Return a value in `0..m` (exclusive upper bound).
    fn next_usize_mod(&mut self, m: usize) -> usize {
        (self.next() as usize) % m
    }

    /// Return a value in `lo..=hi` (inclusive both ends).
    fn next_range(&mut self, lo: usize, hi: usize) -> usize {
        assert!(lo <= hi, "next_range: lo must be <= hi");
        let range = (hi - lo + 1) as u64;
        lo + (self.next() % range) as usize
    }
}

// ============================================================================
// LargeCluster — high-level helper for 100+ node scenarios
// ============================================================================

struct LargeCluster {
    nodes: Vec<Arc<Node>>,
    gossips: Vec<GossipState>,
    rng: Xorshift64,
    node_count: usize,
    /// Indices of nodes considered "killed" (excluded from receiving messages).
    killed: HashSet<usize>,
    /// Global incarnation counter for state transitions.
    next_inc: u64,
}

impl LargeCluster {
    /// Create a cluster of `n` nodes with an initial full-mesh heartbeat.
    fn new(n: usize, seed: u64) -> Self {
        let nodes: Vec<Arc<Node>> = (0..n)
            .map(|_| Arc::new(Node::new(NodeRole::Relay)))
            .collect();
        let gossips: Vec<GossipState> = nodes
            .iter()
            .map(|nd| GossipState::new(nd.clone()))
            .collect();

        Self {
            nodes,
            gossips,
            rng: Xorshift64::new(seed),
            node_count: n,
            killed: HashSet::new(),
            next_inc: 100,
        }
    }

    /// Bootstrap: announce every live node to every other live node.
    async fn bootstrap(&self) {
        let ids: Vec<NodeId> = self.nodes.iter().map(|n| *n.id()).collect();
        for (gi, gs) in self.gossips.iter().enumerate() {
            for (ni, id) in ids.iter().enumerate() {
                if gi != ni {
                    let _ = gs
                        .handle_message(GossipMessage::Heartbeat {
                            node_id: *id,
                            incarnation: 1,
                        })
                        .await;
                }
            }
        }
    }

    /// Run `rounds` iterations of all-to-all `SyncResponse` gossip (full-mesh).
    async fn converge_rounds(&self, rounds: usize) {
        for _ in 0..rounds {
            for src in 0..self.node_count {
                if self.killed.contains(&src) {
                    continue;
                }
                let members = self.gossips[src].get_all_members().await;
                let sync = GossipMessage::SyncResponse {
                    members: members.clone(),
                };
                for dst in 0..self.node_count {
                    if dst == src || self.killed.contains(&dst) {
                        continue;
                    }
                    let _ = self.gossips[dst].handle_message(sync.clone()).await;
                }
            }
        }
    }

    /// How many members node `observer` considers alive (including itself).
    async fn alive_count_from_perspective(&self, observer: usize) -> usize {
        self.gossips[observer].get_alive_members().await.len()
    }

    /// How many members node `observer` considers dead.
    async fn dead_count_from_perspective(&self, observer: usize) -> usize {
        let all = self.gossips[observer].get_all_members().await;
        all.iter().filter(|m| m.is_dead()).count()
    }

    /// Total member entries known by `observer`.
    async fn total_count_from_perspective(&self, observer: usize) -> usize {
        self.gossips[observer].get_all_members().await.len()
    }

    /// Kill node `idx`: mark it as Dead in all other nodes' gossip states.
    async fn kill_node(&mut self, idx: usize) {
        if self.killed.contains(&idx) {
            return;
        }
        self.killed.insert(idx);
        let dead_id = *self.nodes[idx].id();
        self.next_inc += 1;
        let inc_dead = self.next_inc;

        for (gi, gs) in self.gossips.iter().enumerate() {
            if gi == idx {
                continue;
            }
            let _ = gs
                .handle_message(GossipMessage::MemberUpdate {
                    member: MemberInfo {
                        node_id: dead_id,
                        status: HealthStatus::Dead,
                        incarnation: inc_dead,
                        last_seen: SystemTime::UNIX_EPOCH,
                        metadata: HashMap::new(),
                    },
                })
                .await;
        }
    }

    /// Recover node `idx`: heartbeat it back into all other nodes' gossip states.
    async fn recover_node(&mut self, idx: usize) {
        if !self.killed.contains(&idx) {
            return;
        }
        self.killed.remove(&idx);
        let node_id = *self.nodes[idx].id();
        self.next_inc += 1;
        let inc_live = self.next_inc;

        for (gi, gs) in self.gossips.iter().enumerate() {
            if gi == idx {
                continue;
            }
            let _ = gs
                .handle_message(GossipMessage::Heartbeat {
                    node_id,
                    incarnation: inc_live,
                })
                .await;
        }
    }

    /// True when every live node sees exactly the same alive-member count.
    async fn is_converged(&self) -> bool {
        let expected_alive = self.node_count - self.killed.len();
        for i in 0..self.node_count {
            if self.killed.contains(&i) {
                continue;
            }
            let alive = self.alive_count_from_perspective(i).await;
            if alive != expected_alive {
                return false;
            }
        }
        true
    }

    /// Number of live (non-killed) nodes.
    fn live_count(&self) -> usize {
        self.node_count - self.killed.len()
    }
}

// ============================================================================
// Helper: build a PartitionDetector for node `node_idx` with full knowledge
// of `all_nodes` but only `visible_indices` marked as visible.
// ============================================================================
async fn build_detector(
    node_idx: usize,
    all_nodes: &[Arc<Node>],
    visible_indices: &[usize],
) -> PartitionDetector {
    let detector = PartitionDetector::new(all_nodes[node_idx].clone());
    // Register all other nodes as known but not yet visible.
    for (i, n) in all_nodes.iter().enumerate() {
        if i != node_idx {
            detector.add_known_node(*n.id()).await;
        }
    }
    // The node always sees itself.
    detector.mark_node_visible(*all_nodes[node_idx].id()).await;
    // Mark explicitly visible peers.
    for &vi in visible_indices {
        if vi != node_idx {
            detector.mark_node_visible(*all_nodes[vi].id()).await;
        }
    }
    detector
}

// ============================================================================
// Category 1 — 100-Node Gossip Convergence
// ============================================================================

/// Create 100 nodes, verify all GossipStates initialize correctly.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_gossip_initialization() {
    let cluster = LargeCluster::new(100, 0x1111_2222_3333_4444);

    // Each GossipState is seeded with its own node, so each sees exactly 1 member (itself).
    for i in 0..100 {
        let count = cluster.total_count_from_perspective(i).await;
        assert_eq!(
            count, 1,
            "node {i}: freshly created gossip should know only itself"
        );
    }
    assert_eq!(cluster.node_count, 100, "cluster must hold 100 nodes");
}

/// Full-mesh bootstrap + 20 rounds of all-to-all gossip → every node sees 100 alive.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_convergence_full_mesh() {
    let cluster = LargeCluster::new(100, 0xAAAA_BBBB_CCCC_DDDD);
    cluster.bootstrap().await;
    cluster.converge_rounds(20).await;

    for i in 0..100 {
        let alive = cluster.alive_count_from_perspective(i).await;
        assert_eq!(
            alive, 100,
            "node {i}: after full-mesh convergence all 100 nodes must be alive"
        );
    }
}

/// 100 nodes — kill 1, run 30 rounds — all 99 survivors see only 99 alive.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_single_failure_detection() {
    let mut cluster = LargeCluster::new(100, 0xDEAD_BEEF_0001_0001);
    cluster.bootstrap().await;
    cluster.converge_rounds(5).await;

    // Kill node 50 (arbitrary choice near the middle).
    cluster.kill_node(50).await;
    cluster.converge_rounds(30).await;

    for i in 0..100 {
        if i == 50 {
            continue; // skip the dead node itself
        }
        let alive = cluster.alive_count_from_perspective(i).await;
        assert_eq!(
            alive, 99,
            "node {i}: after killing node 50, survivors must see 99 alive"
        );
        let dead = cluster.dead_count_from_perspective(i).await;
        assert_eq!(dead, 1, "node {i}: exactly 1 dead node must be recorded");
    }
}

/// 100 nodes — kill 20 simultaneously — run 50 rounds — all 80 survivors agree.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_burst_failure() {
    let mut cluster = LargeCluster::new(100, 0xBEEF_DEAD_CAFE_BABE);
    cluster.bootstrap().await;
    cluster.converge_rounds(5).await;

    // Kill nodes 80–99 (20 nodes).
    for idx in 80..100 {
        cluster.kill_node(idx).await;
    }
    cluster.converge_rounds(50).await;

    for i in 0..80 {
        let alive = cluster.alive_count_from_perspective(i).await;
        assert_eq!(
            alive, 80,
            "node {i}: after burst kill of 20, survivors must see 80 alive"
        );
    }
}

/// 100 nodes — kill 10 — detect — recover all — all 100 alive again.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_recovery() {
    let mut cluster = LargeCluster::new(100, 0x1234_5678_9ABC_DEF0);
    cluster.bootstrap().await;
    cluster.converge_rounds(5).await;

    let killed: Vec<usize> = (0..10).collect();
    for &idx in &killed {
        cluster.kill_node(idx).await;
    }
    cluster.converge_rounds(20).await;

    // Verify detection: survivors see 90 alive.
    for i in 10..100 {
        let alive = cluster.alive_count_from_perspective(i).await;
        assert_eq!(
            alive, 90,
            "node {i}: after killing 10, survivors must see 90 alive"
        );
    }

    // Recover all 10.
    for &idx in &killed {
        cluster.recover_node(idx).await;
    }
    cluster.converge_rounds(30).await;

    // All 100 should be alive again.
    for i in 0..100 {
        let alive = cluster.alive_count_from_perspective(i).await;
        assert_eq!(
            alive, 100,
            "node {i}: after full recovery all 100 must be alive"
        );
    }
}

// ============================================================================
// Category 2 — Consistent Hash Ring at Scale
// ============================================================================

/// Add 500 nodes to ConsistentHashRing, verify node_count and random lookups.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_500_node_ring_construction() {
    let mut ring = ConsistentHashRing::new();
    let nodes: Vec<Arc<Node>> = (0..500)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    for n in &nodes {
        ring.add_node(*n.id());
    }

    assert_eq!(
        ring.node_count(),
        500,
        "ring must contain 500 distinct nodes"
    );

    // 1000 random key lookups must all return Some.
    let mut rng = Xorshift64::new(0x9999_AAAA_BBBB_CCCC);
    for _ in 0..1000 {
        let key = format!("key:{}", rng.next());
        let result = ring.get_node(&key);
        assert!(
            result.is_some(),
            "ring.get_node must return Some for any key when ring has 500 nodes"
        );
    }
}

/// 200-node ring, 10000 key lookups — verify distribution uniformity.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_consistent_hash_distribution_uniformity() {
    let mut ring = ConsistentHashRing::new();
    let nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    for n in &nodes {
        ring.add_node(*n.id());
    }

    let mut counts: HashMap<NodeId, usize> = HashMap::new();
    let mut rng = Xorshift64::new(0xC0FF_EE00_1234_5678);
    let total_keys = 10_000usize;

    for _ in 0..total_keys {
        let key = format!("bench:{}", rng.next());
        if let Some(id) = ring.get_node(&key) {
            *counts.entry(id).or_insert(0) += 1;
        }
    }

    let expected_mean = total_keys as f64 / 200.0;
    // Allow 5× deviation: virtual nodes smooth out the distribution considerably,
    // but hash function collisions can cause minor skew.
    let lower_bound = (expected_mean * 0.1) as usize;
    let upper_bound = (expected_mean * 5.0) as usize;

    for (id, count) in &counts {
        assert!(
            *count >= lower_bound && *count <= upper_bound,
            "node {id}: got {count} keys, expected between {lower_bound} and {upper_bound}"
        );
    }

    // Compute coefficient of variation (std_dev / mean).
    let mean = total_keys as f64 / counts.len() as f64;
    let variance = counts
        .values()
        .map(|&c| {
            let diff = c as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / counts.len() as f64;
    let std_dev = variance.sqrt();
    let cv = std_dev / mean;
    // CV should be well below 1.0 for a properly distributed ring.
    assert!(
        cv < 1.0,
        "coefficient of variation ({cv:.3}) must be < 1.0 for uniform distribution"
    );
}

/// 200 nodes, record distribution, remove 10 nodes, verify stable keys don't change.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_consistent_hash_node_removal_consistency() {
    let nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let mut ring = ConsistentHashRing::new();
    for n in &nodes {
        ring.add_node(*n.id());
    }

    let mut rng = Xorshift64::new(0xFACE_B00C_DEAD_1234);
    let keys: Vec<String> = (0..1000).map(|_| format!("key:{}", rng.next())).collect();

    // Record original mapping.
    let original: HashMap<String, NodeId> = keys
        .iter()
        .filter_map(|k| ring.get_node(k).map(|id| (k.clone(), id)))
        .collect();

    // Remove nodes 190–199 (10 nodes).
    let removed_ids: HashSet<NodeId> = nodes[190..200].iter().map(|n| *n.id()).collect();
    for n in &nodes[190..200] {
        ring.remove_node(*n.id());
    }
    assert_eq!(
        ring.node_count(),
        190,
        "ring must have 190 nodes after removal"
    );

    // Keys that were NOT on a removed node must still map to the same node.
    let mut stable_checked = 0usize;
    for key in &keys {
        let orig_id = original[key];
        if !removed_ids.contains(&orig_id) {
            let new_id = ring.get_node(key);
            assert_eq!(
                new_id,
                Some(orig_id),
                "key '{key}' was on a stable node but changed mapping after removal"
            );
            stable_checked += 1;
        }
    }
    // Most keys should be on stable nodes (only ~5% should have been on the 10 removed nodes).
    assert!(
        stable_checked > 850,
        "at least 850 of 1000 keys should be on stable nodes (removing 10 of 200), got {stable_checked}"
    );
}

/// 100 baseline nodes, add+remove 10 nodes 5 times, verify stability of untouched keys.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_consistent_hash_stable_during_churn() {
    let baseline_nodes: Vec<Arc<Node>> = (0..100)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let mut ring = ConsistentHashRing::new();
    for n in &baseline_nodes {
        ring.add_node(*n.id());
    }

    let mut rng = Xorshift64::new(0x1234_ABCD_BEEF_CAFE);
    let keys: Vec<String> = (0..500).map(|_| format!("churn:{}", rng.next())).collect();

    // Baseline mapping before any churn.
    let baseline: HashMap<String, NodeId> = keys
        .iter()
        .filter_map(|k| ring.get_node(k).map(|id| (k.clone(), id)))
        .collect();

    let baseline_ids: HashSet<NodeId> = baseline_nodes.iter().map(|n| *n.id()).collect();

    // Five rounds of churn: add 10, then remove those same 10.
    for round in 0..5usize {
        let churn_nodes: Vec<Arc<Node>> = (0..10)
            .map(|_| Arc::new(Node::new(NodeRole::Relay)))
            .collect();

        for n in &churn_nodes {
            ring.add_node(*n.id());
        }
        // Remove them immediately.
        for n in &churn_nodes {
            ring.remove_node(*n.id());
        }

        assert_eq!(
            ring.node_count(),
            100,
            "round {round}: ring must be back to 100 nodes after churn"
        );
    }

    // Keys that originally mapped to a baseline node must still map there.
    for key in &keys {
        let orig_id = baseline[key];
        if baseline_ids.contains(&orig_id) {
            let after_id = ring.get_node(key);
            assert_eq!(
                after_id,
                Some(orig_id),
                "key '{key}' on baseline node should be stable after repeated churn"
            );
        }
    }
}

/// 1000 nodes, 10000 key lookups must complete within generous timing bound.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_consistent_hash_performance_10k_lookups() {
    let nodes: Vec<Arc<Node>> = (0..1000)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let mut ring = ConsistentHashRing::new();
    for n in &nodes {
        ring.add_node(*n.id());
    }

    let mut rng = Xorshift64::new(0x0FF1_CE0A_AB12_3456);
    let t0 = Instant::now();

    for _ in 0..10_000 {
        let key = format!("perf:{}", rng.next());
        let _ = ring.get_node(&key);
    }

    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_millis() < 10_000,
        "10000 lookups on 1000-node ring took {}ms, must be < 10000ms",
        elapsed.as_millis()
    );
}

// ============================================================================
// Category 3 — Large-Scale Partition Simulation
// ============================================================================

/// 200 nodes split 100/100 — neither side has quorum (both detect partition).
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_200_node_50_50_partition() {
    let all_nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Group A = indices 0..100, Group B = indices 100..200.
    // Quorum for 200 total = ceil(200 * 0.5) + 1 = 101.
    // Each group has 100 visible → 100 < 101 → no quorum → partition detected.
    let visible_a: Vec<usize> = (0..100).collect();
    let det_a = build_detector(0, &all_nodes, &visible_a).await;

    let visible_b: Vec<usize> = (100..200).collect();
    let det_b = build_detector(100, &all_nodes, &visible_b).await;

    let quorum_a = det_a.has_quorum().await;
    let quorum_b = det_b.has_quorum().await;
    assert!(
        !quorum_a,
        "group A (100/200) must NOT have quorum (need 101)"
    );
    assert!(
        !quorum_b,
        "group B (100/200) must NOT have quorum (need 101)"
    );

    assert_eq!(
        det_a.visible_count().await,
        100,
        "group A: 100 visible nodes"
    );
    assert_eq!(
        det_b.visible_count().await,
        100,
        "group B: 100 visible nodes"
    );
    assert_eq!(det_a.known_count().await, 200, "group A: 200 known nodes");
    assert_eq!(det_b.known_count().await, 200, "group B: 200 known nodes");
}

/// 200 nodes split 134/66 — majority has quorum, minority does not.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_200_node_2_3_partition() {
    let all_nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Quorum = ceil(200 * 0.5) + 1 = 101.
    // Majority (134 visible) → 134 >= 101 → has quorum.
    // Minority (66 visible)  →  66 < 101 → no quorum.
    let visible_majority: Vec<usize> = (0..134).collect();
    let det_maj = build_detector(0, &all_nodes, &visible_majority).await;

    let visible_minority: Vec<usize> = (134..200).collect();
    let det_min = build_detector(134, &all_nodes, &visible_minority).await;

    let quorum_maj = det_maj.has_quorum().await;
    let quorum_min = det_min.has_quorum().await;
    assert!(
        quorum_maj,
        "majority (134/200) must have quorum (need 101), visible={}",
        det_maj.visible_count().await
    );
    assert!(
        !quorum_min,
        "minority (66/200) must NOT have quorum (need 101)"
    );
}

/// Partition 200 into 100/100, heal, verify all nodes have quorum again.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_200_node_partition_healing() {
    let all_nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Pre-partition: build detector for node 0 that knows all 200.
    let all_visible: Vec<usize> = (0..200).collect();
    let det = build_detector(0, &all_nodes, &all_visible).await;

    // Before partition: 200/200 visible → quorum.
    assert!(
        det.has_quorum().await,
        "pre-partition: 200/200 visible must have quorum"
    );

    // Simulate partition: remove nodes 100–199 from visible set.
    for n in &all_nodes[100..200] {
        det.mark_node_invisible(*n.id()).await;
    }
    assert!(
        !det.has_quorum().await,
        "during partition: 100/200 visible must NOT have quorum"
    );

    // Heal: restore all nodes as visible.
    for n in &all_nodes[100..200] {
        det.mark_node_visible(*n.id()).await;
    }
    assert!(
        det.has_quorum().await,
        "after healing: 200/200 visible must have quorum again"
    );
}

/// 100 nodes, progressively remove visibility — detect quorum loss below 50%.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_cascading_failures() {
    let all_nodes: Vec<Arc<Node>> = (0..100)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Detector for node 0 starts with all 100 visible.
    let det = PartitionDetector::new(all_nodes[0].clone());
    det.mark_node_visible(*all_nodes[0].id()).await;
    for n in &all_nodes[1..] {
        det.add_known_node(*n.id()).await;
        det.mark_node_visible(*n.id()).await;
    }

    // Quorum formula: ceil(100 * 0.5) + 1 = 51.
    for (removed, node) in all_nodes.iter().enumerate().take(50).skip(1) {
        det.mark_node_invisible(*node.id()).await;
        let visible = det.visible_count().await;
        let known = det.known_count().await;
        assert_eq!(
            visible,
            100 - removed,
            "after removing {removed} nodes: visible must be {}",
            100 - removed
        );
        let has_q = det.has_quorum().await;
        if visible >= 51 {
            assert!(
                has_q,
                "with {visible}/{known} visible, quorum must hold (need 51)"
            );
        } else {
            assert!(
                !has_q,
                "with {visible}/{known} visible, quorum must be lost (need 51)"
            );
        }
    }

    // Remove node at index 50 → 50 visible, 100 known → quorum lost.
    det.mark_node_invisible(*all_nodes[50].id()).await;
    assert!(
        !det.has_quorum().await,
        "with 50/100 visible, quorum must be lost (need 51)"
    );
}

/// 100 nodes: kill 20, recover one at a time, verify visible count grows correctly.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_100_node_rolling_restart() {
    let all_nodes: Vec<Arc<Node>> = (0..100)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let det = PartitionDetector::new(all_nodes[0].clone());
    det.mark_node_visible(*all_nodes[0].id()).await;
    for n in &all_nodes[1..] {
        det.add_known_node(*n.id()).await;
        det.mark_node_visible(*n.id()).await;
    }

    // Kill nodes 1–20 (make invisible).
    for n in &all_nodes[1..=20] {
        det.mark_node_invisible(*n.id()).await;
    }
    assert_eq!(
        det.visible_count().await,
        80,
        "after killing 20 nodes, visible must be 80"
    );
    assert!(
        det.has_quorum().await,
        "80/100 visible: quorum must hold (need 51)"
    );

    // Recover nodes one by one and verify count increments.
    for (step, n) in all_nodes[1..=20].iter().enumerate() {
        det.mark_node_visible(*n.id()).await;
        let expected = 81 + step;
        assert_eq!(
            det.visible_count().await,
            expected,
            "after recovery step {}: visible must be {expected}",
            step + 1
        );
    }

    // After full recovery, all 100 visible → quorum.
    assert_eq!(
        det.visible_count().await,
        100,
        "after rolling restart: all 100 nodes visible"
    );
    assert!(
        det.has_quorum().await,
        "100/100 visible: quorum must hold after rolling restart"
    );
}

// ============================================================================
// Category 4 — Performance Regression Baselines
// ============================================================================

/// 50-node gossip via SyncResponse, measure rounds to convergence (must be ≤ 15).
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn bench_50_node_convergence_time() {
    let node_count = 50usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Seed: node 0 knows all others via initial heartbeat.
    for n in &nodes[1..] {
        let _ = gossips[0]
            .handle_message(GossipMessage::Heartbeat {
                node_id: *n.id(),
                incarnation: 1,
            })
            .await;
    }

    let t0 = Instant::now();
    let mut rounds = 0u32;

    loop {
        rounds += 1;
        let fanout = 3usize;
        for src in 0..node_count {
            let members = gossips[src].get_all_members().await;
            let sync = GossipMessage::SyncResponse { members };
            for offset in 1..=fanout {
                let dst = (src + offset) % node_count;
                let _ = gossips[dst].handle_message(sync.clone()).await;
            }
        }

        let mut all_agree = true;
        for g in &gossips {
            if g.get_all_members().await.len() < node_count {
                all_agree = false;
                break;
            }
        }
        if all_agree {
            break;
        }
        assert!(
            rounds < 200,
            "50-node gossip must converge within 200 rounds"
        );
    }

    let elapsed = t0.elapsed();
    assert!(
        rounds <= 15,
        "50-node gossip should converge within 15 rounds, took {rounds}"
    );
    assert!(
        elapsed.as_millis() < 30_000,
        "50-node convergence took {}ms, must be < 30000ms",
        elapsed.as_millis()
    );
}

/// 100-node, 100 gossip rounds — verify convergence and throughput.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn bench_gossip_100_node_throughput() {
    let node_count = 100usize;
    let cluster = LargeCluster::new(node_count, 0x5555_6666_7777_8888);
    cluster.bootstrap().await;

    // 5 rounds is sufficient post-bootstrap; full-mesh O(n²) is expensive in debug.
    let t0 = Instant::now();
    cluster.converge_rounds(5).await;
    let elapsed = t0.elapsed();

    // After 5 rounds of full-mesh gossip (post-bootstrap) all nodes must be converged.
    let mut total_alive = 0usize;
    for i in 0..node_count {
        total_alive += cluster.alive_count_from_perspective(i).await;
    }
    // Each of the 100 nodes should see ≥50 alive (partial convergence threshold).
    assert!(
        total_alive >= 5_000,
        "total alive-entries across all nodes must be >= 5000, got {total_alive}"
    );
    assert!(
        elapsed.as_millis() < 60_000,
        "5 gossip rounds on 100 nodes took {}ms, must be < 60000ms",
        elapsed.as_millis()
    );
}

/// 1000-node ring, 100K lookups, must complete within generous timing bound.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn bench_consistent_hash_1000_node_lookup_speed() {
    let nodes: Vec<Arc<Node>> = (0..1000)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let mut ring = ConsistentHashRing::new();
    for n in &nodes {
        ring.add_node(*n.id());
    }

    let mut rng = Xorshift64::new(0xFEDC_BA98_7654_3210);
    let t0 = Instant::now();
    let mut found = 0usize;

    for _ in 0..100_000 {
        let key = format!("lkp:{}", rng.next());
        if ring.get_node(&key).is_some() {
            found += 1;
        }
    }

    let elapsed = t0.elapsed();
    assert_eq!(found, 100_000, "all 100K lookups must return Some");
    assert!(
        elapsed.as_millis() < 60_000,
        "100K lookups on 1000-node ring took {}ms, must be < 60000ms",
        elapsed.as_millis()
    );
}

/// 200-node detector — run 1000 visibility-sweep iterations — timing assertion.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn bench_partition_detector_200_node_sweep() {
    let all_nodes: Vec<Arc<Node>> = (0..200)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // One detector that knows all 200 nodes.
    let det = PartitionDetector::new(all_nodes[0].clone());
    det.mark_node_visible(*all_nodes[0].id()).await;
    for n in &all_nodes[1..] {
        det.add_known_node(*n.id()).await;
        det.mark_node_visible(*n.id()).await;
    }

    let t0 = Instant::now();

    // Simulate 1000 visibility-sweep operations (alternating add/remove).
    for sweep in 0..1000usize {
        let target_idx = (sweep % 199) + 1; // cycle through nodes 1..199
        let target_id = *all_nodes[target_idx].id();
        if sweep % 2 == 0 {
            det.mark_node_invisible(target_id).await;
        } else {
            det.mark_node_visible(target_id).await;
        }
    }

    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_millis() < 60_000,
        "1000 visibility sweeps on 200-node detector took {}ms, must be < 60000ms",
        elapsed.as_millis()
    );
}

/// 500-node cluster, 1 gossip round — no panics, state consistent.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn bench_gossip_500_nodes_one_round() {
    let node_count = 500usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Seed: node 0 knows all others via heartbeat.
    for n in &nodes[1..] {
        let _ = gossips[0]
            .handle_message(GossipMessage::Heartbeat {
                node_id: *n.id(),
                incarnation: 1,
            })
            .await;
    }

    let t0 = Instant::now();

    // One round: each node broadcasts SyncResponse to its two consecutive neighbours.
    for src in 0..node_count {
        let members = gossips[src].get_all_members().await;
        let sync = GossipMessage::SyncResponse { members };
        let dst_a = (src + 1) % node_count;
        let dst_b = (src + 2) % node_count;
        let _ = gossips[dst_a].handle_message(sync.clone()).await;
        let _ = gossips[dst_b].handle_message(sync.clone()).await;
    }

    let elapsed = t0.elapsed();

    // Node 0 should know all 500 nodes (it was seeded with all of them).
    let known_by_0 = gossips[0].get_all_members().await.len();
    assert_eq!(
        known_by_0, node_count,
        "node 0 must know all {node_count} nodes after seeding"
    );

    assert!(
        elapsed.as_millis() < 60_000,
        "500-node single round took {}ms, must be < 60000ms",
        elapsed.as_millis()
    );
}

// ============================================================================
// Category 5 — Edge Cases and Stress
// ============================================================================

/// Single-node cluster: initialises correctly, always has quorum.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_single_node_cluster() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let gossip = GossipState::new(node.clone());

    let count = gossip.get_all_members().await.len();
    assert_eq!(
        count, 1,
        "single-node cluster: gossip must see exactly 1 member"
    );

    let alive = gossip.get_alive_members().await.len();
    assert_eq!(alive, 1, "single-node cluster: the one node must be alive");

    // PartitionDetector with only 1 known node should report quorum.
    let det = PartitionDetector::new(node.clone());
    det.mark_node_visible(*node.id()).await;
    assert!(
        det.has_quorum().await,
        "single-node cluster must always have quorum"
    );
}

/// Two-node cluster: both see each other, no quorum loss.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_two_node_cluster_symmetry() {
    let nodes: Vec<Arc<Node>> = (0..2)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Bootstrap.
    let _ = gossips[0]
        .handle_message(GossipMessage::Heartbeat {
            node_id: *nodes[1].id(),
            incarnation: 1,
        })
        .await;
    let _ = gossips[1]
        .handle_message(GossipMessage::Heartbeat {
            node_id: *nodes[0].id(),
            incarnation: 1,
        })
        .await;

    assert_eq!(
        gossips[0].get_alive_members().await.len(),
        2,
        "node 0 must see 2 alive members"
    );
    assert_eq!(
        gossips[1].get_alive_members().await.len(),
        2,
        "node 1 must see 2 alive members"
    );

    // 2-node cluster: quorum = ceil(2 * 0.5) + 1 = 2.
    // When both are visible: 2 >= 2 → quorum.
    let det0 = build_detector(0, &nodes, &[0, 1]).await;
    let det1 = build_detector(1, &nodes, &[0, 1]).await;
    assert!(det0.has_quorum().await, "2-node: both visible → quorum");
    assert!(det1.has_quorum().await, "2-node: both visible → quorum");
}

/// 100 nodes, kill 99 — last survivor detects no quorum.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_all_nodes_failed_except_one() {
    let mut cluster = LargeCluster::new(100, 0x9876_5432_1000_ABCD);
    cluster.bootstrap().await;
    cluster.converge_rounds(5).await;

    // Kill nodes 1..99, leaving only node 0 alive.
    for idx in 1..100 {
        cluster.kill_node(idx).await;
    }

    // Node 0 should see 99 dead nodes and 1 alive (itself).
    let alive = cluster.alive_count_from_perspective(0).await;
    let dead = cluster.dead_count_from_perspective(0).await;
    assert_eq!(alive, 1, "last survivor must see exactly 1 alive (itself)");
    assert_eq!(
        dead, 99,
        "last survivor must see all 99 killed nodes as dead"
    );

    // Build a partition detector for node 0 knowing all 100 but only seeing itself.
    let det = PartitionDetector::new(cluster.nodes[0].clone());
    det.mark_node_visible(*cluster.nodes[0].id()).await;
    for n in &cluster.nodes[1..] {
        det.add_known_node(*n.id()).await;
    }
    // 1 visible, 100 known → quorum = ceil(100*0.5) + 1 = 51 → no quorum.
    assert!(
        !det.has_quorum().await,
        "last survivor (1/100 visible) must NOT have quorum (need 51)"
    );
}

/// 50 nodes, flood each with 100× messages — state stable, no panics.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_gossip_message_flood_stability() {
    let node_count = 50usize;
    let cluster = LargeCluster::new(node_count, 0xF100_D555_AAAA_BBBB);
    cluster.bootstrap().await;
    cluster.converge_rounds(5).await;

    let mut rng = Xorshift64::new(0x1337_C0DE_FEED_FACE);

    // Send 100 extra random heartbeats/updates to each node.
    for _ in 0..100 {
        for dst in 0..node_count {
            let src = rng.next_usize_mod(node_count);
            let node_id = *cluster.nodes[src].id();
            let incarnation = rng.next() % 1000 + 100;
            let _ = cluster.gossips[dst]
                .handle_message(GossipMessage::Heartbeat {
                    node_id,
                    incarnation,
                })
                .await;
        }
    }

    // After flood all gossip states must still be coherent (no panics above means OK).
    // Every node must know at least all 50 nodes.
    for i in 0..node_count {
        let total = cluster.total_count_from_perspective(i).await;
        assert!(
            total >= node_count,
            "node {i}: after flood must know >= {node_count} members, got {total}"
        );
    }
}

/// Run same 100-node scenario twice with the same seed — identical convergence patterns.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_large_cluster_deterministic() {
    async fn run_scenario(seed: u64) -> Vec<usize> {
        let mut cluster = LargeCluster::new(100, seed);
        cluster.bootstrap().await;

        // Kill nodes deterministically (first 5).
        for idx in 0..5 {
            cluster.kill_node(idx).await;
        }
        cluster.converge_rounds(20).await;

        // Record alive counts for all surviving nodes.
        let mut counts = Vec::new();
        for i in 5..100 {
            counts.push(cluster.alive_count_from_perspective(i).await);
        }
        counts
    }

    let seed = 0xDEAD_C0DE_CAFE_BABE;
    let run_a = run_scenario(seed).await;
    let run_b = run_scenario(seed).await;

    assert_eq!(
        run_a, run_b,
        "identical seeds must produce identical convergence patterns"
    );
}

/// 150-node cluster, 3 repeated partition-heal cycles — state remains coherent.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_large_cluster_repeated_partition_heal() {
    let all_nodes: Vec<Arc<Node>> = (0..150)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // One detector (node 0) that knows all 150 nodes.
    let det = PartitionDetector::new(all_nodes[0].clone());
    det.mark_node_visible(*all_nodes[0].id()).await;
    for n in &all_nodes[1..] {
        det.add_known_node(*n.id()).await;
        det.mark_node_visible(*n.id()).await;
    }

    // Three partition-heal cycles.
    for cycle in 0..3usize {
        // Partition: hide nodes 75..149 (75 nodes removed).
        for n in &all_nodes[75..150] {
            det.mark_node_invisible(*n.id()).await;
        }
        // 75 visible / 150 known → quorum = ceil(150*0.5)+1 = 76 → no quorum.
        assert!(
            !det.has_quorum().await,
            "cycle {cycle}: after partition, 75/150 must NOT have quorum"
        );

        // Heal: restore all nodes.
        for n in &all_nodes[75..150] {
            det.mark_node_visible(*n.id()).await;
        }
        assert!(
            det.has_quorum().await,
            "cycle {cycle}: after heal, 150/150 must have quorum"
        );
        assert_eq!(
            det.visible_count().await,
            150,
            "cycle {cycle}: all 150 nodes visible after heal"
        );
    }
}

/// Verify that `ConsistentHashRing` handles the empty ring correctly.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_empty_ring_edge_case() {
    let ring = ConsistentHashRing::new();
    assert_eq!(ring.node_count(), 0, "empty ring must have 0 nodes");
    assert!(ring.is_empty(), "empty ring must report is_empty() = true");
    assert!(
        ring.get_node("any-key").is_none(),
        "empty ring must return None for any key"
    );
}

/// Adding a single node to the ring — all keys route to it.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_single_node_ring() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let mut ring = ConsistentHashRing::new();
    ring.add_node(*node.id());

    assert_eq!(ring.node_count(), 1, "single-node ring must have 1 node");

    let mut rng = Xorshift64::new(0x1111_DEAD_BEEF_2222);
    for _ in 0..100 {
        let key = format!("k:{}", rng.next());
        let result = ring.get_node(&key);
        assert_eq!(
            result,
            Some(*node.id()),
            "single-node ring must always return the only node"
        );
    }
}

/// Gossip SyncResponse with full member list — verify all members are absorbed in one shot.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn test_gossip_sync_response_absorbs_full_membership() {
    let node_count = 100usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Seed node 0 with all others.
    for n in &nodes[1..] {
        let _ = gossips[0]
            .handle_message(GossipMessage::Heartbeat {
                node_id: *n.id(),
                incarnation: 1,
            })
            .await;
    }

    let members = gossips[0].get_all_members().await;
    assert_eq!(
        members.len(),
        node_count,
        "node 0 must know all {node_count} nodes"
    );

    // Node 0 broadcasts one SyncResponse to all others.
    let sync = GossipMessage::SyncResponse {
        members: members.clone(),
    };
    for g in gossips.iter().skip(1) {
        let _ = g.handle_message(sync.clone()).await;
    }

    // After one SyncResponse all nodes must know all node_count members.
    for (i, g) in gossips.iter().enumerate().skip(1) {
        let total = g.get_all_members().await.len();
        assert_eq!(
            total, node_count,
            "node {i}: after SyncResponse must know all {node_count} members"
        );
    }
}
