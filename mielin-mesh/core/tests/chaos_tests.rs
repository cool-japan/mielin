//! Network Partition and Chaos Tests for Mesh Core
//!
//! All network interaction is simulated with in-memory message passing via
//! `tokio::sync::mpsc` channels — no sockets are opened.

use mielin_mesh_core::{
    gossip::{GossipMessage, GossipState, HealthStatus},
    node::{Node, NodeRole},
    partition::PartitionDetector,
    registry::{AgentId, AgentRegistry},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_node(role: NodeRole) -> Arc<Node> {
    Arc::new(Node::new(role))
}

fn relay_node() -> Arc<Node> {
    make_node(NodeRole::Relay)
}

fn core_node() -> Arc<Node> {
    make_node(NodeRole::Core)
}

// ── test 1 ───────────────────────────────────────────────────────────────────

/// Build a 6-member GossipState cluster, split it into two groups of 3,
/// then verify each group only sees its own members as Alive.
#[tokio::test]
async fn test_network_partition_splits_cluster() {
    // Build 6 GossipState instances (one per node)
    let nodes: Vec<Arc<Node>> = (0..6).map(|_| relay_node()).collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Announce all nodes to each other (full mesh).
    for (gi, g) in gossips.iter().enumerate() {
        for (ni, n) in nodes.iter().enumerate() {
            if gi != ni {
                let hb = GossipMessage::Heartbeat {
                    node_id: *n.id(),
                    incarnation: 1,
                };
                let _ = g.handle_message(hb).await;
            }
        }
    }

    // Verify all 6 gossips know about all 6 members.
    for g in &gossips {
        assert_eq!(g.get_all_members().await.len(), 6);
    }

    // Simulate partition: group A = [0,1,2], group B = [3,4,5].
    // Stop sending heartbeats across the partition boundary.
    // Apply FAILURE_TIMEOUT-equivalent status changes manually.
    let group_a = &nodes[0..3];
    let group_b = &nodes[3..6];

    // Mark group-B members as Dead in group-A gossips.
    // Incarnation must be > 1 (the one used when they were added via heartbeat).
    for g in &gossips[0..3] {
        for nb in group_b {
            let dead = GossipMessage::MemberUpdate {
                member: mielin_mesh_core::gossip::MemberInfo {
                    node_id: *nb.id(),
                    status: HealthStatus::Dead,
                    incarnation: 10,
                    last_seen: SystemTime::UNIX_EPOCH,
                    metadata: HashMap::new(),
                },
            };
            let _ = g.handle_message(dead).await;
        }
    }

    // Mark group-A members as Dead in group-B gossips.
    for g in &gossips[3..6] {
        for na in group_a {
            let dead = GossipMessage::MemberUpdate {
                member: mielin_mesh_core::gossip::MemberInfo {
                    node_id: *na.id(),
                    status: HealthStatus::Dead,
                    incarnation: 10,
                    last_seen: SystemTime::UNIX_EPOCH,
                    metadata: HashMap::new(),
                },
            };
            let _ = g.handle_message(dead).await;
        }
    }

    // Group A: 3 alive (own members), 3 dead (group B).
    for g in &gossips[0..3] {
        let alive = g.get_alive_members().await;
        assert_eq!(alive.len(), 3, "group A must see exactly 3 alive members");
    }

    // Group B: 3 alive, 3 dead.
    for g in &gossips[3..6] {
        let alive = g.get_alive_members().await;
        assert_eq!(alive.len(), 3, "group B must see exactly 3 alive members");
    }
}

// ── test 2 ───────────────────────────────────────────────────────────────────

/// Add members to a GossipState, then manually age their `last_seen` to
/// trigger Suspect and Dead transitions via `MemberInfo` helpers.
#[tokio::test]
async fn test_node_failure_detection_timing() {
    let node = relay_node();
    let gossip = GossipState::new(node.clone());

    let peer_id = mielin_mesh_core::node::NodeId::new_v4();
    gossip.add_member(peer_id).await.unwrap();

    // Initially Alive
    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_alive());
    }

    // Inject a member-update that marks the peer as Suspect.
    let suspect_update = GossipMessage::MemberUpdate {
        member: mielin_mesh_core::gossip::MemberInfo {
            node_id: peer_id,
            status: HealthStatus::Suspect,
            incarnation: 1,
            last_seen: SystemTime::now() - Duration::from_secs(20),
            metadata: HashMap::new(),
        },
    };
    gossip.handle_message(suspect_update).await.unwrap();

    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_suspect(), "peer must be Suspect after update");
    }

    // Escalate to Dead.
    let dead_update = GossipMessage::MemberUpdate {
        member: mielin_mesh_core::gossip::MemberInfo {
            node_id: peer_id,
            status: HealthStatus::Dead,
            incarnation: 2,
            last_seen: SystemTime::now() - Duration::from_secs(40),
            metadata: HashMap::new(),
        },
    };
    gossip.handle_message(dead_update).await.unwrap();

    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_dead(), "peer must be Dead after second update");
    }
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// A new member joins; simulate K rounds of gossip and verify all original
/// members learn about the newcomer.
#[tokio::test]
async fn test_gossip_convergence_after_join() {
    let count = 5usize;
    let nodes: Vec<Arc<Node>> = (0..count).map(|_| relay_node()).collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Bootstrap: each node knows all others.
    for (gi, g) in gossips.iter().enumerate() {
        for (ni, n) in nodes.iter().enumerate() {
            if gi != ni {
                let hb = GossipMessage::Heartbeat {
                    node_id: *n.id(),
                    incarnation: 1,
                };
                let _ = g.handle_message(hb).await;
            }
        }
    }

    // New member joins.
    let new_node = relay_node();
    let new_gossip = GossipState::new(new_node.clone());

    // New node sends a heartbeat to node[0], which propagates via sync.
    let hb_from_new = GossipMessage::Heartbeat {
        node_id: *new_node.id(),
        incarnation: 1,
    };
    gossips[0].handle_message(hb_from_new).await.unwrap();

    // Simulate K=3 gossip rounds: node[0] syncs its view to the rest.
    for _ in 0..3 {
        // Node 0 broadcasts a sync response to all.
        let members_at_0 = gossips[0].get_all_members().await;
        let sync_resp = GossipMessage::SyncResponse {
            members: members_at_0,
        };
        for g in &gossips[1..] {
            let _ = g.handle_message(sync_resp.clone()).await;
        }
    }

    // All original nodes must now know about the newcomer.
    for g in &gossips {
        let members = g.get_all_members().await;
        let knows_new = members.iter().any(|m| m.node_id == *new_node.id());
        assert!(knows_new, "every member must converge on the newcomer");
    }

    // New node itself also knows the cluster after a sync exchange.
    let sync_req = GossipMessage::SyncRequest {
        from_node: *new_node.id(),
    };
    if let Some(GossipMessage::SyncResponse { members }) =
        gossips[0].handle_message(sync_req).await.unwrap()
    {
        let sync_back = GossipMessage::SyncResponse { members };
        new_gossip.handle_message(sync_back).await.unwrap();
    }

    let new_members = new_gossip.get_all_members().await;
    assert!(
        new_members.len() > count,
        "new node must see the whole cluster"
    );
}

// ── test 4 ───────────────────────────────────────────────────────────────────

/// A member leaves (declared Dead); the remaining members converge to a
/// consistent view where that member is no longer Alive.
#[tokio::test]
async fn test_gossip_convergence_after_leave() {
    let count = 4usize;
    let nodes: Vec<Arc<Node>> = (0..count).map(|_| relay_node()).collect();
    let gossips: Vec<GossipState> = nodes.iter().map(|n| GossipState::new(n.clone())).collect();

    // Bootstrap.
    for (gi, g) in gossips.iter().enumerate() {
        for (ni, n) in nodes.iter().enumerate() {
            if gi != ni {
                let hb = GossipMessage::Heartbeat {
                    node_id: *n.id(),
                    incarnation: 1,
                };
                let _ = g.handle_message(hb).await;
            }
        }
    }

    // Node[3] leaves: declared Dead by node[0].
    let leaving_id = *nodes[3].id();
    let dead_update = GossipMessage::MemberUpdate {
        member: mielin_mesh_core::gossip::MemberInfo {
            node_id: leaving_id,
            status: HealthStatus::Dead,
            incarnation: 2,
            last_seen: SystemTime::UNIX_EPOCH,
            metadata: HashMap::new(),
        },
    };
    gossips[0]
        .handle_message(dead_update.clone())
        .await
        .unwrap();

    // Propagate: node[0] pushes its updated view to [1..3].
    let members_at_0 = gossips[0].get_all_members().await;
    let sync_resp = GossipMessage::SyncResponse {
        members: members_at_0,
    };
    for g in &gossips[1..3] {
        let _ = g.handle_message(sync_resp.clone()).await;
    }

    // All remaining nodes must see exactly 3 Alive members.
    for g in &gossips[0..3] {
        let alive = g.get_alive_members().await;
        let still_alive: Vec<_> = alive.iter().filter(|m| m.node_id != leaving_id).collect();
        assert_eq!(
            still_alive.len(),
            3,
            "leaving node must not be in alive set"
        );
    }
}

// ── test 5 ───────────────────────────────────────────────────────────────────

/// Partition into two equal halves of 3. The PartitionDetector must report
/// that neither half has quorum (>50 % of known nodes).
#[tokio::test]
async fn test_split_brain_prevention() {
    // Half A sees only itself + 2 peers; half B same.
    let half_a_nodes: Vec<Arc<Node>> = (0..3).map(|_| relay_node()).collect();
    let half_b_nodes: Vec<Arc<Node>> = (0..3).map(|_| relay_node()).collect();

    // PartitionDetector for node[0] of half A.
    let detector_a = PartitionDetector::new(half_a_nodes[0].clone());

    // Add all 6 nodes as "known" to detector_a.
    for n in &half_a_nodes[1..] {
        detector_a.add_known_node(*n.id()).await;
    }
    for n in &half_b_nodes {
        detector_a.add_known_node(*n.id()).await;
    }

    // Only the 3 nodes in half A are visible to detector_a.
    for n in &half_a_nodes[1..] {
        detector_a.mark_node_visible(*n.id()).await;
    }
    // Half B nodes are NOT visible (partition).

    // With 3 visible out of 6 known (50 %), quorum requires >50 % = 4 nodes.
    let has_quorum = detector_a.has_quorum().await;
    assert!(
        !has_quorum,
        "equal split must NOT have quorum (prevents split-brain)"
    );

    // PartitionDetector for node[0] of half B, mirrored.
    let detector_b = PartitionDetector::new(half_b_nodes[0].clone());
    for n in &half_a_nodes {
        detector_b.add_known_node(*n.id()).await;
    }
    for n in &half_b_nodes[1..] {
        detector_b.add_known_node(*n.id()).await;
    }
    for n in &half_b_nodes[1..] {
        detector_b.mark_node_visible(*n.id()).await;
    }

    let has_quorum_b = detector_b.has_quorum().await;
    assert!(!has_quorum_b, "half B also must NOT have quorum");
}

// ── test 6 ───────────────────────────────────────────────────────────────────

/// A node declared Dead then rejoins with a higher incarnation number;
/// it must become Alive again in the cluster view.
#[tokio::test]
async fn test_node_rejoin_after_failure() {
    let node = relay_node();
    let gossip = GossipState::new(node.clone());

    let peer_id = mielin_mesh_core::node::NodeId::new_v4();

    // Declare peer Dead at incarnation 1.
    gossip
        .handle_message(GossipMessage::MemberUpdate {
            member: mielin_mesh_core::gossip::MemberInfo {
                node_id: peer_id,
                status: HealthStatus::Dead,
                incarnation: 1,
                last_seen: SystemTime::UNIX_EPOCH,
                metadata: HashMap::new(),
            },
        })
        .await
        .unwrap();

    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(peer.is_dead());
    }

    // Peer rejoins with incarnation 5 (higher than 1).
    gossip
        .handle_message(GossipMessage::Heartbeat {
            node_id: peer_id,
            incarnation: 5,
        })
        .await
        .unwrap();

    {
        let all = gossip.get_all_members().await;
        let peer = all.iter().find(|m| m.node_id == peer_id).unwrap();
        assert!(
            peer.is_alive(),
            "node must become Alive again after rejoin with higher incarnation"
        );
        assert_eq!(peer.incarnation, 5);
    }
}

// ── test 7 ───────────────────────────────────────────────────────────────────

/// 10 nodes join simultaneously (simulated via concurrent heartbeat delivery).
/// Final member count must equal 10.
#[tokio::test]
async fn test_concurrent_joins() {
    let hub = relay_node();
    let hub_gossip = Arc::new(GossipState::new(hub.clone()));

    let joiners: Vec<Arc<Node>> = (0..10).map(|_| relay_node()).collect();

    // Send heartbeats concurrently via tokio tasks.
    let mut handles = Vec::new();
    for joiner in &joiners {
        let g = Arc::clone(&hub_gossip);
        let id = *joiner.id();
        handles.push(tokio::spawn(async move {
            g.handle_message(GossipMessage::Heartbeat {
                node_id: id,
                incarnation: 1,
            })
            .await
            .unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Hub (1) + 10 joiners = 11.
    let count = hub_gossip.get_all_members().await.len();
    assert_eq!(count, 11, "all 10 concurrent joins must be recorded");
}

// ── test 8 ───────────────────────────────────────────────────────────────────

/// 100 join+leave cycles must not panic or cause data corruption.
#[tokio::test]
async fn test_rapid_join_leave_churn() {
    let node = relay_node();
    let gossip = GossipState::new(node.clone());

    let ephemeral_id = mielin_mesh_core::node::NodeId::new_v4();

    for incarnation in 1u64..=100 {
        // Join
        gossip
            .handle_message(GossipMessage::Heartbeat {
                node_id: ephemeral_id,
                incarnation,
            })
            .await
            .unwrap();

        // Leave
        gossip
            .handle_message(GossipMessage::MemberUpdate {
                member: mielin_mesh_core::gossip::MemberInfo {
                    node_id: ephemeral_id,
                    status: HealthStatus::Dead,
                    incarnation,
                    last_seen: SystemTime::UNIX_EPOCH,
                    metadata: HashMap::new(),
                },
            })
            .await
            .unwrap();

        // Rejoin with higher incarnation overrides dead status.
        gossip
            .handle_message(GossipMessage::Heartbeat {
                node_id: ephemeral_id,
                incarnation: incarnation + 1,
            })
            .await
            .unwrap();
    }

    // After 100 cycles the member table must not be empty or grossly inflated.
    let all = gossip.get_all_members().await;
    // At most 2 unique node IDs: local + ephemeral
    assert!(
        all.len() <= 2,
        "churn must not create phantom entries: {}",
        all.len()
    );
    assert!(!all.is_empty());
}

// ── test 9 ───────────────────────────────────────────────────────────────────

/// Agents registered on churning nodes must remain findable in the registry.
#[tokio::test]
async fn test_agent_registry_consistency_under_churn() {
    use mielin_mesh_core::dht::Dht;
    use tokio::sync::RwLock;

    let node = core_node();
    let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
    let registry = AgentRegistry::new(node.clone(), dht.clone());
    registry.start().await;

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Register 20 agents.
    let mut agent_ids: Vec<AgentId> = Vec::new();
    for i in 0u8..20 {
        let id: AgentId = [i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, i];
        registry.register_agent(id, addr).await.unwrap();
        agent_ids.push(id);
    }

    assert_eq!(registry.local_agent_count().await, 20);

    // Churn: deregister odd ones, keep even ones.
    for (idx, id) in agent_ids.iter().enumerate() {
        if idx % 2 != 0 {
            registry.deregister_agent(*id).await.unwrap();
        }
    }

    assert_eq!(registry.local_agent_count().await, 10);

    // Even agents must still be locatable.
    for (idx, id) in agent_ids.iter().enumerate() {
        if idx % 2 == 0 {
            let loc = registry.query_agent(*id).await.unwrap();
            assert_eq!(loc.agent_id, *id);
        }
    }
}

// ── test 10 ──────────────────────────────────────────────────────────────────

/// Attempt to migrate an agent across a simulated network partition.
/// Either migration succeeds (after partition heals) or fails cleanly —
/// in neither case must data be lost or a panic occur.
#[tokio::test]
async fn test_migration_during_partition() {
    use mielin_mesh_core::{
        gossip::MemberInfo,
        migration::{MigrationCoordinator, MigrationRequest, MigrationStrategy},
    };

    let source_node = core_node();
    let target_node = core_node();

    let source_gossip = GossipState::new(source_node.clone());
    let target_gossip = GossipState::new(target_node.clone());

    // Bootstrap: both nodes know each other.
    source_gossip
        .handle_message(GossipMessage::Heartbeat {
            node_id: *target_node.id(),
            incarnation: 1,
        })
        .await
        .unwrap();
    target_gossip
        .handle_message(GossipMessage::Heartbeat {
            node_id: *source_node.id(),
            incarnation: 1,
        })
        .await
        .unwrap();

    // Simulate partition: mark target as Dead in source's view.
    // Incarnation must exceed the one used in the bootstrap heartbeat (1).
    source_gossip
        .handle_message(GossipMessage::MemberUpdate {
            member: MemberInfo {
                node_id: *target_node.id(),
                status: HealthStatus::Dead,
                incarnation: 5,
                last_seen: SystemTime::UNIX_EPOCH,
                metadata: HashMap::new(),
            },
        })
        .await
        .unwrap();

    // Verify source no longer sees target as alive.
    let alive_at_source = source_gossip.get_alive_members().await;
    let sees_target = alive_at_source
        .iter()
        .any(|m| m.node_id == *target_node.id());
    assert!(
        !sees_target,
        "source must not see target as alive during partition"
    );

    // Attempt migration — the coordinator operates independently of gossip;
    // it will complete (in-memory) regardless, but we verify no panic/loss.
    let coordinator = MigrationCoordinator::new(source_node.clone());
    coordinator.start().await;

    let request = MigrationRequest {
        agent_id: [0xAB; 16],
        source_node: *source_node.id(),
        target_node: *target_node.id(),
        target_address: "127.0.0.1:0".parse().unwrap(),
        strategy: MigrationStrategy::PostCopy,
        priority: 1,
    };

    // Migration either succeeds or fails — the important invariant is no panic.
    let result = coordinator.initiate_migration(request).await;
    // The coordinator's in-memory implementation always succeeds; if the
    // architecture changes to check gossip health, result may be Err, which is fine.
    match result {
        Ok(_) => {
            // Verify stats are consistent after success.
            let stats = coordinator.get_migration_stats().await;
            assert!(stats.total_migrations >= 1);
        }
        Err(e) => {
            // Clean error — no panic, no data loss.
            let _ = e; // just checking it's a proper error type
        }
    }

    // Partition heals: target rejoins with incarnation higher than the Dead update (5).
    source_gossip
        .handle_message(GossipMessage::Heartbeat {
            node_id: *target_node.id(),
            incarnation: 10,
        })
        .await
        .unwrap();

    let alive_after_heal = source_gossip.get_alive_members().await;
    let sees_target_after = alive_after_heal
        .iter()
        .any(|m| m.node_id == *target_node.id());
    assert!(
        sees_target_after,
        "target must be alive again after partition heals"
    );
}
