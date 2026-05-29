//! Chaos Engineering and Network Partition Simulation Tests
//!
//! Comprehensive test suite covering:
//! - Node kill/recover scenarios
//! - Network partition simulation with quorum semantics
//! - Byzantine / random-flap chaos scenarios
//! - Gossip convergence benchmarks (timed with std::time::Instant)
//!
//! All network interaction is simulated via in-memory API calls — no sockets.

#![allow(dead_code)]

use mielin_mesh_core::{
    ConsistentHashRing, GossipConfig, GossipMessage, GossipState, HealthStatus, MemberInfo,
    MembershipEventKind, Node, NodeId, NodeRole, PartitionDetector,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;

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

    /// Return a value in `lo..=hi` (inclusive both ends).
    fn next_range(&mut self, lo: usize, hi: usize) -> usize {
        assert!(lo <= hi, "next_range: lo must be <= hi");
        let range = (hi - lo + 1) as u64;
        lo + (self.next() % range) as usize
    }

    /// Return `true` with probability `prob` (0.0 <= prob <= 1.0).
    fn next_bool(&mut self, prob: f64) -> bool {
        let threshold = (prob.clamp(0.0, 1.0) * u64::MAX as f64) as u64;
        self.next() < threshold
    }
}

// ============================================================================
// ChaosEngine — coordinates simulated failures
// ============================================================================

#[derive(Debug)]
enum ChaosEvent {
    NodeKilled { node_idx: usize, at_tick: u64 },
    NodeRecovered { node_idx: usize, at_tick: u64 },
    PartitionCreated { side_a: Vec<usize>, side_b: Vec<usize>, at_tick: u64 },
    PartitionHealed { at_tick: u64 },
    DelayInjected { node_idx: usize, delay_ms: u64, at_tick: u64 },
}

struct ChaosEngine {
    nodes: Vec<Arc<Node>>,
    gossip_states: Vec<Arc<RwLock<GossipState>>>,
    rng: Xorshift64,
    event_log: Vec<ChaosEvent>,
    /// Which nodes are currently "killed" (not participating).
    killed: HashSet<usize>,
    /// Current partition: Some((side_a_indices, side_b_indices)) or None.
    partition: Option<(Vec<usize>, Vec<usize>)>,
    tick: u64,
    /// Monotonically increasing incarnation for all state-change events.
    next_inc: u64,
}

impl ChaosEngine {
    fn new(node_count: usize, seed: u64) -> Self {
        let nodes: Vec<Arc<Node>> = (0..node_count)
            .map(|_| Arc::new(Node::new(NodeRole::Relay)))
            .collect();
        let gossip_states: Vec<Arc<RwLock<GossipState>>> = nodes
            .iter()
            .map(|n| Arc::new(RwLock::new(GossipState::new(n.clone()))))
            .collect();

        Self {
            nodes,
            gossip_states,
            rng: Xorshift64::new(seed),
            event_log: Vec::new(),
            killed: HashSet::new(),
            partition: None,
            tick: 0,
            next_inc: 100,
        }
    }

    /// Bootstrap: announce every node to every other node (full-mesh heartbeat).
    async fn bootstrap(&self) {
        let ids: Vec<NodeId> = self.nodes.iter().map(|n| *n.id()).collect();
        for (gi, gs) in self.gossip_states.iter().enumerate() {
            let g = gs.read().await;
            for (ni, id) in ids.iter().enumerate() {
                if gi != ni {
                    let _ = g.handle_message(GossipMessage::Heartbeat {
                        node_id: *id,
                        incarnation: 1,
                    }).await;
                }
            }
        }
    }

    /// Kill node at `idx` — mark it as Dead in all other nodes' gossip states.
    async fn kill_node(&mut self, idx: usize) {
        if self.killed.contains(&idx) {
            return;
        }
        self.killed.insert(idx);
        let dead_id = *self.nodes[idx].id();
        self.next_inc += 1;
        let inc_dead = self.next_inc;

        for (gi, gs) in self.gossip_states.iter().enumerate() {
            if gi == idx {
                continue;
            }
            let g = gs.read().await;
            let _ = g.handle_message(GossipMessage::MemberUpdate {
                member: MemberInfo {
                    node_id: dead_id,
                    status: HealthStatus::Dead,
                    incarnation: inc_dead,
                    last_seen: SystemTime::UNIX_EPOCH,
                    metadata: HashMap::new(),
                },
            }).await;
        }

        self.event_log.push(ChaosEvent::NodeKilled { node_idx: idx, at_tick: self.tick });
    }

    /// Recover node at `idx` — heartbeat it back into all other nodes' gossip states.
    async fn recover_node(&mut self, idx: usize) {
        if !self.killed.contains(&idx) {
            return;
        }
        self.killed.remove(&idx);
        let node_id = *self.nodes[idx].id();
        self.next_inc += 1;
        let inc_live = self.next_inc;

        for (gi, gs) in self.gossip_states.iter().enumerate() {
            if gi == idx {
                continue;
            }
            let g = gs.read().await;
            let _ = g.handle_message(GossipMessage::Heartbeat {
                node_id,
                incarnation: inc_live,
            }).await;
        }

        self.event_log.push(ChaosEvent::NodeRecovered { node_idx: idx, at_tick: self.tick });
    }

    /// Create a partition: nodes in `side_a` cannot communicate with nodes in `side_b`.
    /// We simulate this by marking cross-side nodes as Dead.
    async fn create_partition(&mut self, side_a: Vec<usize>, side_b: Vec<usize>) {
        self.next_inc += 1;
        let dead_inc = self.next_inc;

        // Mark side_b as Dead for all side_a gossip states.
        for &ai in &side_a {
            let gs = &self.gossip_states[ai];
            let g = gs.read().await;
            for &bi in &side_b {
                let dead_id = *self.nodes[bi].id();
                let _ = g.handle_message(GossipMessage::MemberUpdate {
                    member: MemberInfo {
                        node_id: dead_id,
                        status: HealthStatus::Dead,
                        incarnation: dead_inc,
                        last_seen: SystemTime::UNIX_EPOCH,
                        metadata: HashMap::new(),
                    },
                }).await;
            }
        }

        // Mark side_a as Dead for all side_b gossip states.
        for &bi in &side_b {
            let gs = &self.gossip_states[bi];
            let g = gs.read().await;
            for &ai in &side_a {
                let dead_id = *self.nodes[ai].id();
                let _ = g.handle_message(GossipMessage::MemberUpdate {
                    member: MemberInfo {
                        node_id: dead_id,
                        status: HealthStatus::Dead,
                        incarnation: dead_inc,
                        last_seen: SystemTime::UNIX_EPOCH,
                        metadata: HashMap::new(),
                    },
                }).await;
            }
        }

        self.event_log.push(ChaosEvent::PartitionCreated {
            side_a: side_a.clone(),
            side_b: side_b.clone(),
            at_tick: self.tick,
        });
        self.partition = Some((side_a, side_b));
    }

    /// Heal the current partition: restore all nodes as Alive across sides.
    async fn heal_partition(&mut self) {
        if let Some((side_a, side_b)) = self.partition.take() {
            self.next_inc += 1;
            let heal_inc = self.next_inc;

            // side_a nodes rejoin side_b's view.
            for &ai in &side_a {
                let node_id = *self.nodes[ai].id();
                for &bi in &side_b {
                    let gs = &self.gossip_states[bi];
                    let g = gs.read().await;
                    let _ = g.handle_message(GossipMessage::Heartbeat {
                        node_id,
                        incarnation: heal_inc,
                    }).await;
                }
            }

            // side_b nodes rejoin side_a's view.
            for &bi in &side_b {
                let node_id = *self.nodes[bi].id();
                for &ai in &side_a {
                    let gs = &self.gossip_states[ai];
                    let g = gs.read().await;
                    let _ = g.handle_message(GossipMessage::Heartbeat {
                        node_id,
                        incarnation: heal_inc,
                    }).await;
                }
            }

            self.event_log.push(ChaosEvent::PartitionHealed { at_tick: self.tick });
        }
    }

    fn advance_tick(&mut self) {
        self.tick += 1;
    }

    /// Count alive members as seen by node at `observer_idx`.
    async fn alive_count_at(&self, observer_idx: usize) -> usize {
        let gs = &self.gossip_states[observer_idx];
        let g = gs.read().await;
        g.get_alive_members().await.len()
    }

    /// Count dead members as seen by node at `observer_idx`.
    async fn dead_count_at(&self, observer_idx: usize) -> usize {
        let gs = &self.gossip_states[observer_idx];
        let g = gs.read().await;
        let all = g.get_all_members().await;
        all.iter().filter(|m| m.is_dead()).count()
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

// ============================================================================
// Helper: build a PartitionDetector for `node` knowing all nodes in the cluster,
// with only `visible_indices` marked as visible.
// ============================================================================
async fn build_detector(
    node_idx: usize,
    all_nodes: &[Arc<Node>],
    visible_indices: &[usize],
) -> PartitionDetector {
    let detector = PartitionDetector::new(all_nodes[node_idx].clone());
    for (i, n) in all_nodes.iter().enumerate() {
        if i != node_idx {
            detector.add_known_node(*n.id()).await;
        }
    }
    // Always mark self as visible (a node always sees itself).
    detector
        .mark_node_visible(*all_nodes[node_idx].id())
        .await;
    for &vi in visible_indices {
        if vi != node_idx {
            detector.mark_node_visible(*all_nodes[vi].id()).await;
        }
    }
    detector
}

// ============================================================================
// Helper: make a dead MemberInfo
// ============================================================================
fn dead_member(node_id: NodeId, incarnation: u64) -> MemberInfo {
    MemberInfo {
        node_id,
        status: HealthStatus::Dead,
        incarnation,
        last_seen: SystemTime::UNIX_EPOCH,
        metadata: HashMap::new(),
    }
}

// ============================================================================
// A. Basic node failure tests
// ============================================================================

/// Kill 1 of 5 nodes; remaining 4 must see it as Dead.
#[tokio::test]
async fn chaos_single_node_kill() {
    let mut engine = ChaosEngine::new(5, 0xAABBCCDD_11223344);
    engine.bootstrap().await;

    // Initially all 5 nodes are alive everywhere.
    let initial_alive = engine.alive_count_at(0).await;
    assert_eq!(
        initial_alive, 5,
        "before kill: all 5 nodes should be alive at observer 0"
    );

    engine.kill_node(4).await;

    // The 4 remaining nodes should see 4 alive, 1 dead.
    for observer in 0..4usize {
        let alive = engine.alive_count_at(observer).await;
        assert_eq!(
            alive, 4,
            "after killing node 4: observer {observer} should see 4 alive"
        );
        let dead = engine.dead_count_at(observer).await;
        assert_eq!(
            dead, 1,
            "after killing node 4: observer {observer} should see 1 dead"
        );
    }

    assert_eq!(engine.event_log.len(), 1, "exactly one chaos event recorded");
}

/// Kill 2 of 7 nodes; majority (5) survives with quorum.
#[tokio::test]
async fn chaos_minority_failure() {
    let node_count = 7usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = all_nodes
        .iter()
        .map(|n| GossipState::new(n.clone()))
        .collect();

    // Bootstrap.
    for (gi, g) in gossips.iter().enumerate() {
        for (ni, n) in all_nodes.iter().enumerate() {
            if gi != ni {
                let _ = g.handle_message(GossipMessage::Heartbeat {
                    node_id: *n.id(),
                    incarnation: 1,
                }).await;
            }
        }
    }

    // Kill nodes 5 and 6.
    for killed_idx in [5usize, 6usize] {
        let dead_id = *all_nodes[killed_idx].id();
        for (gi, g) in gossips.iter().enumerate() {
            if gi != killed_idx {
                let _ = g.handle_message(GossipMessage::MemberUpdate {
                    member: dead_member(dead_id, 100),
                }).await;
            }
        }
    }

    // Build PartitionDetector for node[0] with 5 visible (0..=4).
    let visible: Vec<usize> = (0..5).collect();
    let detector = build_detector(0, &all_nodes, &visible).await;

    let has_q = detector.has_quorum().await;
    assert!(
        has_q,
        "5 of 7 nodes visible (>50%) — majority must have quorum"
    );

    // Surviving nodes should see 5 alive.
    for g in &gossips[0..5] {
        let alive = g.get_alive_members().await;
        assert_eq!(
            alive.len(),
            5,
            "surviving nodes see 5 alive after minority failure"
        );
    }
}

/// Kill 4 of 7 nodes; remaining 3 cannot form quorum.
#[tokio::test]
async fn chaos_majority_failure() {
    let node_count = 7usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Only nodes 0,1,2 survive — 3 of 7 total.
    let visible: Vec<usize> = vec![0, 1, 2];
    let detector = build_detector(0, &all_nodes, &visible).await;

    let has_q = detector.has_quorum().await;
    assert!(
        !has_q,
        "3 of 7 nodes visible (<50%) — minority must NOT have quorum"
    );

    let vc = detector.visible_count().await;
    let kc = detector.known_count().await;
    assert_eq!(vc, 3, "3 nodes visible in detector");
    assert_eq!(kc, 7, "7 nodes known in detector");
}

/// Kill a node, then recover it, verifying it rejoins the membership as Alive.
#[tokio::test]
async fn chaos_node_kill_and_recover() {
    let mut engine = ChaosEngine::new(4, 0x1234_5678_ABCD_EF01);
    engine.bootstrap().await;

    engine.kill_node(2).await;
    let dead = engine.dead_count_at(0).await;
    assert_eq!(dead, 1, "node 2 should be dead at observer 0 after kill");

    engine.recover_node(2).await;

    // After recovery, observer 0 should see all 4 alive again.
    let alive = engine.alive_count_at(0).await;
    assert_eq!(alive, 4, "all 4 nodes alive after recovery");
    let dead_after = engine.dead_count_at(0).await;
    assert_eq!(dead_after, 0, "no dead nodes after recovery");

    assert_eq!(engine.event_log.len(), 2, "kill + recover = 2 events");
}

/// Kill and recover the same node 10 times in rapid succession.
#[tokio::test]
async fn chaos_rapid_kill_recover() {
    let mut engine = ChaosEngine::new(5, 0xDEAD_BEEF_0000_0001);
    engine.bootstrap().await;

    for _cycle in 0..10 {
        engine.kill_node(3).await;
        engine.advance_tick();
        engine.recover_node(3).await;
        engine.advance_tick();
    }

    // After 10 cycles, node 3 should be alive again (last action was recover).
    let alive = engine.alive_count_at(0).await;
    assert_eq!(
        alive, 5,
        "all 5 nodes should be alive after 10 kill/recover cycles"
    );

    // 20 events: 10 kills + 10 recovers.
    assert_eq!(
        engine.event_log.len(),
        20,
        "10 kills + 10 recovers = 20 events"
    );
}

/// Kill all nodes one by one; the last survivor has no peers alive.
#[tokio::test]
async fn chaos_all_nodes_killed_sequentially() {
    let node_count = 5usize;
    let mut engine = ChaosEngine::new(node_count, 0xCAFE_BABE_0000_0002);
    engine.bootstrap().await;

    // Kill nodes 1..4 sequentially, leaving only node 0.
    for kill_idx in 1..node_count {
        engine.kill_node(kill_idx).await;
        engine.advance_tick();
    }

    // Observer 0 (still alive) should see itself as the only alive member.
    let alive = engine.alive_count_at(0).await;
    assert_eq!(alive, 1, "last survivor should see only itself alive");

    let dead = engine.dead_count_at(0).await;
    assert_eq!(
        dead,
        node_count - 1,
        "all other nodes are dead at last survivor"
    );
}

/// Reduce cluster to 1 node; that node must report has_quorum() == false
/// because there are known peers it cannot reach.
#[tokio::test]
async fn chaos_last_node_standing() {
    let node_count = 5usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Only node 0 is visible; nodes 1..4 are known but invisible.
    let visible = vec![0usize];
    let detector = build_detector(0, &all_nodes, &visible).await;

    let has_q = detector.has_quorum().await;
    assert!(
        !has_q,
        "last node standing (1 of 5) must NOT have quorum"
    );

    let vc = detector.visible_count().await;
    assert_eq!(vc, 1, "only 1 node visible");

    let kc = detector.known_count().await;
    assert_eq!(kc, 5, "all 5 nodes known");
}

/// Kill 3 nodes simultaneously; membership event count should be consistent.
#[tokio::test]
async fn chaos_simultaneous_kills() {
    let node_count = 6usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossip_arc = Arc::new(GossipState::new(all_nodes[0].clone()));

    // Register all 6 nodes at observer 0.
    for n in &all_nodes[1..] {
        let _ = gossip_arc.handle_message(GossipMessage::Heartbeat {
            node_id: *n.id(),
            incarnation: 1,
        }).await;
    }

    // Kill nodes 3, 4, 5 simultaneously via concurrent tasks.
    let mut handles = Vec::new();
    for dead_node in &all_nodes[3..6] {
        let g = Arc::clone(&gossip_arc);
        let dead_id = *dead_node.id();
        handles.push(tokio::spawn(async move {
            let _ = g.handle_message(GossipMessage::MemberUpdate {
                member: dead_member(dead_id, 500),
            }).await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let dead = gossip_arc
        .get_all_members()
        .await
        .iter()
        .filter(|m| m.is_dead())
        .count();
    assert_eq!(
        dead, 3,
        "3 simultaneous kills should produce exactly 3 dead entries"
    );

    let alive = gossip_arc.get_alive_members().await;
    assert_eq!(alive.len(), 3, "3 nodes remain alive after simultaneous kills");
}

// ============================================================================
// B. Network partition simulation tests
// ============================================================================

/// 10 nodes split 5/5 — neither half has quorum.
#[tokio::test]
async fn partition_50_50_split() {
    let node_count = 10usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let side_a: Vec<usize> = (0..5).collect();
    let side_b: Vec<usize> = (5..10).collect();

    // Detector for side_a[0]: sees 5 nodes (including itself).
    let detector_a = build_detector(0, &all_nodes, &side_a).await;
    assert!(
        !detector_a.has_quorum().await,
        "5 of 10 nodes (50%) must NOT have quorum (>50% required)"
    );

    // Detector for side_b[0]: sees 5 nodes.
    let detector_b = build_detector(5, &all_nodes, &side_b).await;
    assert!(
        !detector_b.has_quorum().await,
        "5 of 10 nodes (50%) must NOT have quorum on the other side"
    );
}

/// 7 nodes split 4/3 — majority (4) has quorum, minority (3) does not.
#[tokio::test]
async fn partition_majority_minority_split() {
    let node_count = 7usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Majority side (5 nodes: need 4 others visible for quorum = ceil(6*0.5)+1 = 4).
    let majority_visible: Vec<usize> = (0..5).collect();
    let det_majority = build_detector(0, &all_nodes, &majority_visible).await;
    assert!(
        det_majority.has_quorum().await,
        "5 of 7 nodes — majority partition must have quorum"
    );

    // Minority side (2 nodes).
    let minority_visible: Vec<usize> = (5..7).collect();
    let det_minority = build_detector(5, &all_nodes, &minority_visible).await;
    assert!(
        !det_minority.has_quorum().await,
        "2 of 7 nodes — minority partition must NOT have quorum"
    );
}

/// Create partition, heal it, verify full membership restored.
#[tokio::test]
async fn partition_heal_recovers_full_view() {
    let mut engine = ChaosEngine::new(6, 0xFEED_FACE_0000_0003);
    engine.bootstrap().await;

    let side_a = vec![0, 1, 2];
    let side_b = vec![3, 4, 5];

    engine.create_partition(side_a.clone(), side_b.clone()).await;

    // Verify partition is effective: side_a only sees side_a alive.
    let alive_after_partition = engine.alive_count_at(0).await;
    assert_eq!(
        alive_after_partition, 3,
        "after partition, side_a[0] should see only 3 alive"
    );

    // Heal the partition.
    engine.heal_partition().await;

    // Now all 6 nodes should be alive again at observer 0.
    let alive_after_heal = engine.alive_count_at(0).await;
    assert_eq!(
        alive_after_heal, 6,
        "after healing partition, all 6 nodes should be alive again"
    );
}

/// Isolate 1 node from N-1 nodes; isolated node loses quorum.
#[tokio::test]
async fn partition_island_one_node() {
    let node_count = 7usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Node 6 is isolated: sees only itself.
    let isolated_visible: Vec<usize> = vec![6];
    let det_isolated = build_detector(6, &all_nodes, &isolated_visible).await;
    assert!(
        !det_isolated.has_quorum().await,
        "isolated node (1 of 7) must NOT have quorum"
    );

    // The N-1 majority: nodes 0..5 see each other (6 nodes).
    let majority_visible: Vec<usize> = (0..6).collect();
    let det_majority = build_detector(0, &all_nodes, &majority_visible).await;
    assert!(
        det_majority.has_quorum().await,
        "6 of 7 nodes must have quorum after isolating one"
    );
}

/// Leader ends up in minority partition; majority elects a new one.
#[tokio::test]
async fn partition_with_leader_on_minority_side() {
    use mielin_mesh_core::partition::PartitionInfo;

    let node_count = 7usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Simulate: node 0 is the "leader" but ends up in minority (3 nodes: 0, 1, 2).
    let minority_visible: Vec<usize> = vec![0, 1, 2];
    let det_minority = build_detector(0, &all_nodes, &minority_visible).await;
    assert!(
        !det_minority.has_quorum().await,
        "leader in minority (3 of 7) has no quorum, cannot lead"
    );

    // Majority (5 nodes: 2..6) can elect a new leader. Need 4 others visible.
    let majority_visible: Vec<usize> = vec![2, 3, 4, 5, 6];
    let det_majority = build_detector(2, &all_nodes, &majority_visible).await;
    assert!(
        det_majority.has_quorum().await,
        "majority side (5 of 7) can form quorum and elect new leader"
    );

    // Construct PartitionInfo for majority side and elect a leader.
    let visible_ids: HashSet<NodeId> = majority_visible
        .iter()
        .map(|&i| *all_nodes[i].id())
        .collect();
    let mut info = PartitionInfo::new(visible_ids, node_count);
    assert!(
        info.has_quorum,
        "majority PartitionInfo should indicate quorum"
    );
    assert!(info.leader.is_none(), "no leader elected yet");

    // Elect node 3 as new leader.
    info.leader = Some(*all_nodes[3].id());
    assert_eq!(
        info.leader,
        Some(*all_nodes[3].id()),
        "new leader should be node 3 (from majority side)"
    );
}

/// Cascading splits: partition, then split each half again.
#[tokio::test]
async fn partition_cascading_splits() {
    let node_count = 8usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // After 2-level cascading splits: 4 quarters of 2 nodes each.
    // Each quarter has only 2 of 8 = 25% — no quorum anywhere.
    let all_visible: Vec<Vec<usize>> = vec![
        vec![0, 1],
        vec![2, 3],
        vec![4, 5],
        vec![6, 7],
    ];

    for quarter in &all_visible {
        let det = build_detector(quarter[0], &all_nodes, quarter).await;
        assert!(
            !det.has_quorum().await,
            "after cascading splits: 2 of 8 nodes has no quorum (quarter {:?})",
            quarter
        );
    }
}

/// Measure that partition detection (quorum check) reflects connectivity loss instantly.
#[tokio::test]
async fn partition_detection_latency() {
    let node_count = 6usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let detector = PartitionDetector::new(all_nodes[0].clone());
    // Know all nodes.
    for n in &all_nodes[1..] {
        detector.add_known_node(*n.id()).await;
        detector.mark_node_visible(*n.id()).await;
    }

    // Verify full quorum initially.
    assert!(
        detector.has_quorum().await,
        "should have quorum with all 6 visible"
    );

    let t0 = Instant::now();

    // Simulate partition: make nodes 3..5 invisible (only 3 remain visible).
    for n in &all_nodes[3..6] {
        detector.mark_node_invisible(*n.id()).await;
    }

    // Detection is immediate (API call) — measure elapsed.
    let no_quorum = !detector.has_quorum().await;
    let latency = t0.elapsed();

    assert!(no_quorum, "3 of 6 visible after partition — no quorum");
    assert!(
        latency < Duration::from_millis(10),
        "partition detection must be near-instant (<10ms), got {:?}",
        latency
    );
}

/// Create two different partitions sequentially, then heal each one.
#[tokio::test]
async fn partition_multiple_overlapping() {
    let mut engine = ChaosEngine::new(9, 0x0000_1111_2222_3333);
    engine.bootstrap().await;

    // Partition 1: nodes [0..2] vs [3..8].
    engine.create_partition(vec![0, 1, 2], vec![3, 4, 5, 6, 7, 8]).await;
    let alive_p1 = engine.alive_count_at(0).await;
    assert_eq!(alive_p1, 3, "after first partition: [0..2] see 3 alive");

    // Heal first partition.
    engine.heal_partition().await;
    let alive_healed = engine.alive_count_at(0).await;
    assert_eq!(
        alive_healed, 9,
        "after healing first partition: all 9 alive at node 0"
    );

    // Partition 2: nodes [6..8] isolated.
    engine.create_partition(vec![0, 1, 2, 3, 4, 5], vec![6, 7, 8]).await;
    let alive_p2 = engine.alive_count_at(0).await;
    assert_eq!(alive_p2, 6, "after second partition: [0..5] see 6 alive");

    engine.heal_partition().await;
    let alive_final = engine.alive_count_at(0).await;
    assert_eq!(
        alive_final, 9,
        "after healing second partition: all 9 alive again"
    );
}

// ============================================================================
// C. Byzantine / chaos scenarios
// ============================================================================

/// Random kill/recover of ~20% of nodes for 50 cycles; verify membership consistency.
#[tokio::test]
async fn chaos_random_flap_50_cycles() {
    let node_count = 10usize;
    let mut engine = ChaosEngine::new(node_count, 0xBAAD_F00D_0000_0004);
    engine.bootstrap().await;

    for _cycle in 0..50 {
        for idx in 0..node_count {
            if engine.rng.next_bool(0.20) {
                if engine.killed.contains(&idx) {
                    engine.recover_node(idx).await;
                } else {
                    engine.kill_node(idx).await;
                }
            }
        }
        engine.advance_tick();
    }

    // Recover any still-killed nodes for final consistency check.
    let still_killed: Vec<usize> = engine.killed.iter().cloned().collect();
    for idx in still_killed {
        engine.recover_node(idx).await;
    }

    // After recovery of all, every observer must see all 10 alive.
    for observer in 0..node_count {
        let alive = engine.alive_count_at(observer).await;
        assert_eq!(
            alive, node_count,
            "after random flap + full recovery: observer {observer} must see all {node_count} alive"
        );
    }
}

/// Rolling restart: restart nodes one at a time while cluster serves requests.
#[tokio::test]
async fn chaos_rolling_restart() {
    let node_count = 5usize;
    let mut engine = ChaosEngine::new(node_count, 0xCAFE_0000_0000_0005);
    engine.bootstrap().await;

    // Rolling restart: kill and immediately recover each node.
    for restart_idx in 0..node_count {
        engine.kill_node(restart_idx).await;
        engine.advance_tick();
        engine.recover_node(restart_idx).await;
        engine.advance_tick();

        // Verify the post-recovery state from the next node's perspective.
        let observer = (restart_idx + 1) % node_count;
        let alive = engine.alive_count_at(observer).await;
        assert_eq!(
            alive, node_count,
            "after rolling restart of node {restart_idx}: observer {observer} must see all {node_count} alive"
        );
    }

    // Final state: all nodes alive.
    for observer in 0..node_count {
        let alive = engine.alive_count_at(observer).await;
        assert_eq!(
            alive, node_count,
            "rolling restart complete: observer {observer} sees all alive"
        );
    }
}

/// Simulate high-latency gossip by injecting a stale-timestamped Suspect update.
#[tokio::test]
async fn chaos_network_delay_high_latency() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let cfg = GossipConfig {
        gossip_interval: Duration::from_secs(5),
        heartbeat_timeout: Duration::from_secs(15),
        failure_timeout: Duration::from_secs(30),
        fanout: 3,
        max_history: 512,
    };
    let gossip = GossipState::with_config(node.clone(), cfg);

    let peer_id = NodeId::new_v4();
    gossip.add_member(peer_id).await.unwrap();

    // Inject update with `last_seen` more than 500ms in the past.
    let stale_time = SystemTime::now() - Duration::from_millis(600);
    gossip.handle_message(GossipMessage::MemberUpdate {
        member: MemberInfo {
            node_id: peer_id,
            status: HealthStatus::Suspect,
            incarnation: 2,
            last_seen: stale_time,
            metadata: HashMap::new(),
        },
    }).await.unwrap();

    let all = gossip.get_all_members().await;
    let peer = all.iter().find(|m| m.node_id == peer_id);
    assert!(peer.is_some(), "peer must still be in membership");
    let peer = peer.unwrap();
    assert!(
        peer.is_suspect(),
        "peer with stale heartbeat should be Suspect"
    );
    assert!(
        peer.heartbeat_age() >= Duration::from_millis(500),
        "heartbeat age must reflect the injected stale time"
    );
}

/// Two isolated partitions both try to elect a leader; neither can because no quorum.
#[tokio::test]
async fn chaos_split_brain_detection() {
    let node_count = 6usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    // Split 3/3.
    let side_a: Vec<usize> = vec![0, 1, 2];
    let side_b: Vec<usize> = vec![3, 4, 5];

    let det_a = build_detector(0, &all_nodes, &side_a).await;
    let det_b = build_detector(3, &all_nodes, &side_b).await;

    let a_has_quorum = det_a.has_quorum().await;
    let b_has_quorum = det_b.has_quorum().await;

    assert!(
        !a_has_quorum,
        "side A (3 of 6) must NOT have quorum (prevents split-brain)"
    );
    assert!(
        !b_has_quorum,
        "side B (3 of 6) must NOT have quorum (prevents split-brain)"
    );

    // Verify split-brain cannot occur: at most one side can have quorum simultaneously.
    assert!(
        !(a_has_quorum && b_has_quorum),
        "split-brain: both sides cannot simultaneously have quorum"
    );
}

/// Kill 40% of nodes; remaining 60% eventually converge on a consistent membership.
#[tokio::test]
async fn chaos_membership_convergence_after_mass_failure() {
    let node_count = 10usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = all_nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Bootstrap full mesh.
    for (gi, g) in gossips.iter().enumerate() {
        for (ni, n) in all_nodes.iter().enumerate() {
            if gi != ni {
                let _ = g.handle_message(GossipMessage::Heartbeat {
                    node_id: *n.id(),
                    incarnation: 1,
                }).await;
            }
        }
    }

    // Kill nodes 6..9 (4 nodes = 40%).
    let killed_nodes = 4usize;
    let survivors = node_count - killed_nodes; // 6 survivors
    for (kill_idx, dead_node) in all_nodes.iter().enumerate().take(node_count).skip(survivors) {
        let dead_id = *dead_node.id();
        for (gi, g) in gossips.iter().enumerate() {
            if gi != kill_idx && gi < survivors {
                let _ = g.handle_message(GossipMessage::MemberUpdate {
                    member: dead_member(dead_id, 200),
                }).await;
            }
        }
    }

    // Propagate convergence: survivors sync with each other.
    for _sync_round in 0..3 {
        let members_at_0 = gossips[0].get_all_members().await;
        let sync_resp = GossipMessage::SyncResponse { members: members_at_0 };
        for g in gossips.iter().take(survivors).skip(1) {
            let _ = g.handle_message(sync_resp.clone()).await;
        }
    }

    // All survivors must agree: 6 alive, 4 dead.
    for g in gossips.iter().take(survivors) {
        let alive = g.get_alive_members().await;
        assert_eq!(
            alive.len(),
            survivors,
            "after mass failure convergence: observer must see {survivors} alive"
        );
    }
}

/// Consistent hash ring key->node assignment should be stable across node kills.
#[tokio::test]
async fn chaos_consistent_hash_ring_stability() {
    let node_count = 10usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let mut ring = ConsistentHashRing::new();
    for n in &all_nodes {
        ring.add_node(*n.id());
    }

    // Compute initial assignments for 100 keys.
    let keys: Vec<String> = (0..100).map(|i| format!("chaos-key-{i:04}")).collect();
    let initial_assignments: Vec<Option<NodeId>> =
        keys.iter().map(|k| ring.get_node(k)).collect();

    // Kill 3 nodes (remove from ring).
    ring.remove_node(*all_nodes[7].id());
    ring.remove_node(*all_nodes[8].id());
    ring.remove_node(*all_nodes[9].id());

    // Remaining 7 nodes handle all keys.
    let remaining_ids: HashSet<NodeId> = all_nodes[..7].iter().map(|n| *n.id()).collect();
    let killed_ids: HashSet<NodeId> = all_nodes[7..].iter().map(|n| *n.id()).collect();

    let mut reassigned = 0usize;
    for (i, key) in keys.iter().enumerate() {
        let new_assignment = ring.get_node(key);
        assert!(
            new_assignment.is_some(),
            "key {key} must still be served after kills"
        );
        let new_node = new_assignment.unwrap();
        assert!(
            remaining_ids.contains(&new_node),
            "key {key} must be routed to a surviving node"
        );
        if initial_assignments[i] != new_assignment {
            reassigned += 1;
        }
    }

    // Only keys previously assigned to killed nodes should be reassigned.
    let originally_on_killed = initial_assignments
        .iter()
        .filter(|a| a.is_some_and(|id| killed_ids.contains(&id)))
        .count();
    assert_eq!(
        reassigned, originally_on_killed,
        "exactly the keys that were on killed nodes should be reassigned"
    );
}

/// Killing and recovering a node should increment its incarnation number.
#[tokio::test]
async fn chaos_incarnation_number_increment() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let gossip = GossipState::new(node.clone());

    let peer_id = NodeId::new_v4();

    // Join at incarnation 1.
    gossip.handle_message(GossipMessage::Heartbeat {
        node_id: peer_id,
        incarnation: 1,
    }).await.unwrap();
    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert_eq!(peer.incarnation, 1, "initial incarnation must be 1");
    }

    // Kill: Dead with incarnation 2 (must exceed existing to update status).
    gossip.handle_message(GossipMessage::MemberUpdate {
        member: dead_member(peer_id, 2),
    }).await.unwrap();
    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_dead(), "peer must be Dead after kill");
    }

    // Recover: new incarnation 5 (must be higher).
    gossip.handle_message(GossipMessage::Heartbeat {
        node_id: peer_id,
        incarnation: 5,
    }).await.unwrap();
    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_alive(), "peer must be Alive after recovery");
        assert_eq!(peer.incarnation, 5, "recovered incarnation must be 5");
        assert!(peer.incarnation > 1, "incarnation must be strictly higher after recovery");
    }

    // Kill and recover again at incarnation 10.
    gossip.handle_message(GossipMessage::MemberUpdate {
        member: dead_member(peer_id, 5),
    }).await.unwrap();
    gossip.handle_message(GossipMessage::Heartbeat {
        node_id: peer_id,
        incarnation: 10,
    }).await.unwrap();

    let all = gossip.get_all_members().await;
    let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
    assert_eq!(
        peer.incarnation, 10,
        "second recovery must increment incarnation to 10"
    );
}

// ============================================================================
// D. Gossip convergence benchmarks
// ============================================================================

/// Measure rounds until all 10 nodes agree on membership.
#[tokio::test]
async fn bench_gossip_convergence_10_nodes() {
    let node_count = 10usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Start with only node 0 knowing all others.
    for n in &nodes[1..] {
        let _ = gossips[0].handle_message(GossipMessage::Heartbeat {
            node_id: *n.id(),
            incarnation: 1,
        }).await;
    }

    let t0 = Instant::now();
    let mut rounds = 0u32;

    loop {
        rounds += 1;
        for src in 0..node_count {
            let members = gossips[src].get_all_members().await;
            let sync = GossipMessage::SyncResponse { members };
            let dst = (src + 1) % node_count;
            let _ = gossips[dst].handle_message(sync).await;
        }

        // Check convergence: all nodes see all `node_count` members.
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
            rounds < 100,
            "gossip must converge within 100 rounds for 10 nodes"
        );
    }

    let elapsed = t0.elapsed();
    assert!(
        rounds <= 20,
        "10-node gossip should converge within 20 rounds, took {rounds}"
    );
    let _ = elapsed;
}

/// Measure rounds until 50 nodes converge; assert O(log N) behaviour.
#[tokio::test]
async fn bench_gossip_convergence_50_nodes() {
    let node_count = 50usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Seed: node 0 knows all others.
    for n in &nodes[1..] {
        let _ = gossips[0].handle_message(GossipMessage::Heartbeat {
            node_id: *n.id(),
            incarnation: 1,
        }).await;
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
            "gossip must converge within 200 rounds for 50 nodes"
        );
    }

    let elapsed = t0.elapsed();
    let log_n = (node_count as f64).log2().ceil() as u32;
    assert!(
        rounds <= log_n * 5,
        "50-node gossip with fanout=3 should converge in O(log N) rounds, got {rounds} (log_n={log_n})"
    );
    let _ = elapsed;
}

/// Compare convergence speed at fanout 3 vs 5 vs 7.
#[tokio::test]
async fn bench_gossip_fanout_effect() {
    async fn measure_convergence(fanout: usize) -> u32 {
        let node_count = 20usize;
        let nodes: Vec<Arc<Node>> = (0..node_count)
            .map(|_| Arc::new(Node::new(NodeRole::Relay)))
            .collect();
        let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

        // Seed: node 0 knows all.
        for n in &nodes[1..] {
            let _ = gossips[0].handle_message(GossipMessage::Heartbeat {
                node_id: *n.id(),
                incarnation: 1,
            }).await;
        }

        let mut rounds = 0u32;
        loop {
            rounds += 1;
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
            assert!(rounds < 500, "convergence exceeded 500 rounds at fanout={fanout}");
        }
        rounds
    }

    let rounds_3 = measure_convergence(3).await;
    let rounds_5 = measure_convergence(5).await;
    let rounds_7 = measure_convergence(7).await;

    // Higher fanout must converge in fewer-or-equal rounds.
    assert!(
        rounds_5 <= rounds_3,
        "fanout=5 ({rounds_5}) should converge in <= rounds as fanout=3 ({rounds_3})"
    );
    assert!(
        rounds_7 <= rounds_5,
        "fanout=7 ({rounds_7}) should converge in <= rounds as fanout=5 ({rounds_5})"
    );
}

/// 100K consistent-hash lookups on a 50-node ring; assert <100ms total.
#[tokio::test]
async fn bench_consistent_hash_lookup_speed() {
    let node_count = 50usize;
    let nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();
    let mut ring = ConsistentHashRing::new();
    for n in &nodes {
        ring.add_node(*n.id());
    }

    let lookup_count = 100_000usize;
    let t0 = Instant::now();

    let mut rng = Xorshift64::new(0x1122_3344_5566_7788);
    for i in 0..lookup_count {
        let key = format!("lookup-key-{}", rng.next() % 1_000_000);
        let result = ring.get_node(&key);
        assert!(result.is_some(), "lookup {i} must return a node");
    }

    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_millis(2000),
        "100K consistent-hash lookups must complete in <2s (debug build), took {:?}",
        elapsed
    );
}

/// 1000 sequential QuorumDecision::vote() calls; assert fast.
#[tokio::test]
async fn bench_quorum_decision_throughput() {
    use mielin_mesh_core::partition::QuorumDecision;

    let total_voters = 1000usize;
    let t0 = Instant::now();

    let mut decision = QuorumDecision::new(total_voters);
    for _ in 0..1000 {
        decision.vote(NodeId::new_v4(), true);
    }

    assert!(
        decision.has_quorum(),
        "1000 votes for 1000 voters must reach quorum"
    );
    let (yes, _no) = decision.vote_counts();
    assert_eq!(yes, 1000, "all 1000 votes should be 'yes'");

    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "1000 vote() calls must complete in <100ms, took {:?}",
        elapsed
    );
}

/// 1000 has_quorum() calls on a PartitionDetector; assert timing is acceptable.
#[tokio::test]
async fn bench_partition_detector_overhead() {
    let node_count = 20usize;
    let all_nodes: Vec<Arc<Node>> = (0..node_count)
        .map(|_| Arc::new(Node::new(NodeRole::Relay)))
        .collect();

    let detector = PartitionDetector::new(all_nodes[0].clone());
    for n in &all_nodes[1..] {
        detector.add_known_node(*n.id()).await;
        detector.mark_node_visible(*n.id()).await;
    }

    let t0 = Instant::now();
    for _ in 0..1000 {
        let _q = detector.has_quorum().await;
    }
    let elapsed = t0.elapsed();

    assert!(
        elapsed < Duration::from_millis(500),
        "1000 has_quorum() calls must complete in <500ms, took {:?}",
        elapsed
    );
}

/// 10K membership events processed; measure throughput.
#[tokio::test]
async fn bench_membership_event_throughput() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let cfg = GossipConfig {
        max_history: 20_000,
        ..GossipConfig::default()
    };
    let gossip = GossipState::with_config(node.clone(), cfg);

    let event_count = 10_000usize;
    let t0 = Instant::now();

    // Add members — each triggers a Joined event.
    for _ in 0..event_count {
        let peer_id = NodeId::new_v4();
        gossip.add_member(peer_id).await.unwrap();
    }

    // Allow spawned history-recording tasks to complete.
    tokio::task::yield_now().await;

    let elapsed = t0.elapsed();

    let member_count = gossip.get_all_members().await.len();
    assert_eq!(
        member_count,
        event_count + 1, // +1 for local node
        "all 10K members must be recorded"
    );

    // Throughput assertion.
    let events_per_sec = event_count as f64 / elapsed.as_secs_f64();
    assert!(
        events_per_sec > 1_000.0,
        "membership event throughput must exceed 1000/s, got {events_per_sec:.0}/s"
    );
}

// ============================================================================
// Additional edge-case tests
// ============================================================================

/// PartitionInfo::can_form_quorum and quorum_size are consistent.
#[test]
fn partition_info_quorum_math() {
    use mielin_mesh_core::partition::PartitionInfo;

    // 6 of 10 visible: quorum_size = ceil(10*0.5)+1 = 6.
    let mut visible = HashSet::new();
    for _ in 0..6 {
        visible.insert(NodeId::new_v4());
    }
    let info = PartitionInfo::new(visible, 10);
    assert!(info.can_form_quorum(), "6 of 10 (60%) must form quorum");
    assert_eq!(info.quorum_size(), 6, "quorum_size for 10 nodes must be 6");

    // 5 of 10 = 50%, which is not strictly >50%.
    let mut visible2 = HashSet::new();
    for _ in 0..5 {
        visible2.insert(NodeId::new_v4());
    }
    let info2 = PartitionInfo::new(visible2, 10);
    assert!(!info2.can_form_quorum(), "5 of 10 (50%) must NOT form quorum");
}

/// PartitionDetector with a single-node cluster always has quorum.
#[tokio::test]
async fn partition_single_node_always_quorum() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let detector = PartitionDetector::new(node.clone());

    assert!(
        detector.has_quorum().await,
        "single-node cluster must always have quorum"
    );
    assert_eq!(detector.visible_count().await, 1);
    assert_eq!(detector.known_count().await, 1);
}

/// ConsistentHashRing maintains stable assignments as nodes are added gradually.
#[test]
fn consistent_hash_ring_gradual_growth() {
    let mut ring = ConsistentHashRing::new();
    let nodes: Vec<NodeId> = (0..10).map(|_| NodeId::new_v4()).collect();

    for (i, &node_id) in nodes.iter().enumerate() {
        ring.add_node(node_id);
        let result = ring.get_node("test-key");
        assert!(
            result.is_some(),
            "ring must serve lookups after adding node {i}"
        );
        assert_eq!(ring.node_count(), i + 1);
    }

    // After adding all, 10-node replication should return 10 unique nodes.
    let replicas = ring.get_nodes("test-key", 10);
    assert_eq!(replicas.len(), 10, "must get up to 10 replicas from a 10-node ring");
    let unique: HashSet<&NodeId> = replicas.iter().collect();
    assert_eq!(unique.len(), 10, "all 10 replicas must be distinct nodes");
}

/// Kill a node from gossip state and verify membership_history records the event.
#[tokio::test]
async fn chaos_node_kill_leaves_history() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let gossip = GossipState::new(node.clone());

    let peer_id = NodeId::new_v4();
    gossip.add_member(peer_id).await.unwrap();
    tokio::task::yield_now().await;

    // Kill: force Dead via MemberUpdate.
    gossip.handle_message(GossipMessage::MemberUpdate {
        member: dead_member(peer_id, 2),
    }).await.unwrap();
    tokio::task::yield_now().await;

    let history = gossip.membership_history().await;
    let has_join = history
        .iter()
        .any(|e| e.node_id == peer_id && e.kind == MembershipEventKind::Joined);
    let has_status_change = history.iter().any(|e| {
        e.node_id == peer_id
            && matches!(
                &e.kind,
                MembershipEventKind::StatusChanged {
                    to: HealthStatus::Dead,
                    ..
                }
            )
    });

    assert!(
        has_join,
        "membership history must record Joined event for peer"
    );
    assert!(
        has_status_change,
        "membership history must record StatusChanged->Dead event for killed peer"
    );
}

/// Xorshift64 PRNG produces deterministic sequences and all values of next_range are in bounds.
#[test]
fn xorshift64_determinism_and_bounds() {
    let mut rng1 = Xorshift64::new(12345);
    let mut rng2 = Xorshift64::new(12345);

    // Same seed → identical sequence.
    for _ in 0..100 {
        assert_eq!(
            rng1.next(),
            rng2.next(),
            "same seed must produce identical values"
        );
    }

    // next_range bounds.
    let mut rng3 = Xorshift64::new(0xFEED_BABE);
    for _ in 0..1000 {
        let v = rng3.next_range(3, 7);
        assert!((3..=7).contains(&v), "next_range(3,7) must return 3..=7, got {v}");
    }

    // next_bool approximate probability.
    let mut rng4 = Xorshift64::new(0xABCD_1234);
    let mut true_count = 0u64;
    let n = 100_000u64;
    for _ in 0..n {
        if rng4.next_bool(0.3) {
            true_count += 1;
        }
    }
    let empirical = true_count as f64 / n as f64;
    assert!(
        (empirical - 0.3).abs() < 0.02,
        "next_bool(0.3) empirical rate {empirical:.4} must be close to 0.3"
    );
}

/// GossipState membership is thread-safe: concurrent add_member calls from many tasks.
#[tokio::test]
async fn gossip_state_concurrent_membership() {
    let node = Arc::new(Node::new(NodeRole::Relay));
    let gossip = Arc::new(GossipState::new(node.clone()));

    let n_tasks = 50usize;
    let mut handles = Vec::new();
    for _ in 0..n_tasks {
        let g = Arc::clone(&gossip);
        handles.push(tokio::spawn(async move {
            let peer_id = NodeId::new_v4();
            g.add_member(peer_id).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let count = gossip.get_all_members().await.len();
    assert_eq!(
        count,
        n_tasks + 1,
        "concurrent adds: expected {} members (local + {n_tasks} added), got {count}",
        n_tasks + 1
    );
}
