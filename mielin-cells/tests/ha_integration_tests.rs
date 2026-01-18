//! High Availability Integration Tests
//!
//! Tests the complete HA workflow including leader election,
//! failover, and state replication.

use mielin_cells::{
    Agent, ClusterHealthMonitor, FailoverConfig, FailoverCoordinator, HAHealthConfig, HealthMetric,
    LeaderElector, QuorumConfig, QuorumPolicy, QuorumVote, QuorumVoter, ReplicationConfig,
    ReplicationManager, ReplicationStrategy,
};
use std::time::{Duration, Instant};

#[test]
fn test_complete_ha_workflow() {
    // 1. Setup cluster with leader election
    let config = mielin_cells::ha::leader::ElectionConfig::default();
    let leader = LeaderElector::new("node1".to_string(), config.clone());
    let _follower1 = LeaderElector::new("node2".to_string(), config.clone());
    let _follower2 = LeaderElector::new("node3".to_string(), config);

    // Add nodes to cluster
    leader.add_node("node2".to_string()).expect("add node2");
    leader.add_node("node3".to_string()).expect("add node3");

    // 2. Elect a leader
    let election = leader.start_election().expect("start election");
    assert_eq!(election.leader_id, "node1");
    assert!(leader.is_leader().expect("is leader"));

    // 3. Setup failover coordination
    let failover_config = FailoverConfig::default();
    let failover = FailoverCoordinator::new(failover_config);

    let primary = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    failover
        .register_agent(primary.id(), vec![backup.id()])
        .expect("register agent");

    // 4. Setup state replication
    let replication_config = ReplicationConfig {
        strategy: ReplicationStrategy::Quorum { min_replicas: 2 },
        replication_factor: 3,
        timeout: Duration::from_secs(10),
        enable_compression: true,
        verify_checksum: true,
    };
    let replication = ReplicationManager::new(replication_config);

    let state_data = vec![1, 2, 3, 4, 5];
    let target_nodes = vec![
        "node1".to_string(),
        "node2".to_string(),
        "node3".to_string(),
    ];

    let log = replication
        .replicate_state(&primary.id(), state_data, target_nodes)
        .expect("replicate state");

    assert!(log.success_count >= 2);

    // 5. Setup health monitoring
    let health_config = HAHealthConfig::default();
    let health_monitor = ClusterHealthMonitor::new(health_config);

    health_monitor
        .register_node("node1".to_string())
        .expect("register node1");
    health_monitor
        .register_node("node2".to_string())
        .expect("register node2");
    health_monitor
        .register_node("node3".to_string())
        .expect("register node3");

    let metrics = vec![
        HealthMetric {
            name: "cpu".to_string(),
            value: 0.5,
            threshold: 0.8,
            healthy: true,
        },
        HealthMetric {
            name: "memory".to_string(),
            value: 0.6,
            threshold: 0.85,
            healthy: true,
        },
    ];

    health_monitor
        .check_node_health("node1", metrics)
        .expect("check health");

    let cluster_health = health_monitor
        .get_cluster_health()
        .expect("get cluster health");
    assert_eq!(cluster_health.total_nodes, 3);
}

#[test]
fn test_quorum_voting_integration() {
    let config = QuorumConfig {
        policy: QuorumPolicy::SimpleMajority,
        vote_timeout: Duration::from_secs(30),
        min_participants: 2,
    };

    let voter = QuorumVoter::new(config, 5);

    // Cast votes from different nodes
    for i in 0..3 {
        let vote = QuorumVote {
            node_id: format!("node{}", i),
            vote: true,
            weight: 1.0,
            timestamp: Instant::now(),
        };
        voter.cast_vote(vote).expect("cast vote");
    }

    // Decision should be made (3/5 = majority)
    let decision = voter.get_decision().expect("get decision");
    assert!(decision.is_some());
    let decision = decision.expect("decision");
    assert!(decision.passed);
    assert_eq!(decision.yes_votes, 3);
}

#[test]
fn test_failover_with_replication() {
    // Setup failover
    let failover_config = FailoverConfig::default();
    let coordinator = FailoverCoordinator::new(failover_config);

    let primary = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    coordinator
        .register_agent(primary.id(), vec![backup.id()])
        .expect("register");

    // Setup replication for both agents with BestEffort strategy
    let replication_config = ReplicationConfig {
        strategy: ReplicationStrategy::BestEffort,
        ..Default::default()
    };
    let replication = ReplicationManager::new(replication_config);

    let state = vec![1, 2, 3, 4, 5];
    replication
        .replicate_state(&primary.id(), state.clone(), vec!["node1".to_string()])
        .expect("replicate primary");

    replication
        .replicate_state(&backup.id(), state, vec!["node2".to_string()])
        .expect("replicate backup");

    // Trigger failover
    let decision = coordinator
        .report_failure(&primary.id(), "node failure")
        .expect("report failure");

    if decision.should_failover {
        let event = coordinator
            .execute_failover(&primary.id(), &backup.id())
            .expect("execute failover");
        assert!(event.success);
        assert_eq!(event.backup_id, Some(backup.id()));
    }
}

#[test]
fn test_health_monitoring_with_degradation() {
    let config = HAHealthConfig {
        check_interval: Duration::from_secs(5),
        thresholds: mielin_cells::HealthThreshold::default(),
        failure_threshold: 2,
        success_threshold: 2,
    };

    let monitor = ClusterHealthMonitor::new(config);

    monitor
        .register_node("node1".to_string())
        .expect("register");
    monitor
        .register_node("node2".to_string())
        .expect("register");

    // Node 1 is healthy
    let healthy_metrics = vec![HealthMetric {
        name: "cpu".to_string(),
        value: 0.5,
        threshold: 0.8,
        healthy: true,
    }];

    monitor
        .check_node_health("node1", healthy_metrics)
        .expect("check health");

    // Node 2 is degraded
    let degraded_metrics = vec![HealthMetric {
        name: "cpu".to_string(),
        value: 0.85,
        threshold: 0.8,
        healthy: false,
    }];

    monitor
        .check_node_health("node2", degraded_metrics.clone())
        .expect("check health");

    // Check again to mark as unhealthy
    monitor
        .check_node_health("node2", degraded_metrics)
        .expect("check health");

    let unhealthy = monitor.get_unhealthy_nodes().expect("get unhealthy");
    assert_eq!(unhealthy.len(), 1);
    assert_eq!(unhealthy[0].node_id, "node2");
}
