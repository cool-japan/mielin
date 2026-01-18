//! High Availability Example
//!
//! Demonstrates how to set up a highly available agent cluster with:
//! - Leader election
//! - Automatic failover
//! - State replication
//! - Health monitoring
//! - Quorum-based decisions

use mielin_cells::{
    Agent, ClusterHealthMonitor, FailoverConfig, FailoverCoordinator, FailoverStrategy,
    HAHealthConfig, HealthMetric, LeaderElector, QuorumConfig, QuorumPolicy, QuorumVote,
    QuorumVoter, ReplicationConfig, ReplicationManager, ReplicationStrategy,
};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== High Availability Example ===\n");

    // 1. Set up leader election
    println!("1. Setting up leader election...");
    let election_config = mielin_cells::ha::leader::ElectionConfig {
        election_timeout: Duration::from_millis(150),
        heartbeat_interval: Duration::from_millis(50),
        min_quorum_size: 2,
    };

    let leader = LeaderElector::new("node1".to_string(), election_config.clone());
    let _follower1 = LeaderElector::new("node2".to_string(), election_config.clone());
    let _follower2 = LeaderElector::new("node3".to_string(), election_config);

    leader.add_node("node2".to_string())?;
    leader.add_node("node3".to_string())?;

    println!("   Cluster nodes: node1, node2, node3");

    // Start election
    let election_result = leader.start_election()?;
    println!("   ✓ Leader elected: {}", election_result.leader_id);
    println!("   Term: {}", election_result.term);
    println!();

    // 2. Set up failover coordination
    println!("2. Configuring automatic failover...");
    let failover_config = FailoverConfig {
        policy: mielin_cells::FailoverPolicy {
            strategy: FailoverStrategy::ConsecutiveFailures { count: 3 },
            max_failover_time: Duration::from_secs(30),
            min_failover_interval: Duration::from_secs(60),
            auto_recover: true,
        },
        max_backups: 3,
        health_check_interval: Duration::from_secs(5),
    };

    let failover = FailoverCoordinator::new(failover_config);

    // Create primary and backup agents
    let primary = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let backup1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let backup2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    failover.register_agent(primary.id(), vec![backup1.id(), backup2.id()])?;

    println!("   ✓ Primary agent registered with 2 backups");
    println!("   Strategy: ConsecutiveFailures(3)");
    println!();

    // 3. Set up state replication
    println!("3. Configuring state replication...");
    let replication_config = ReplicationConfig {
        strategy: ReplicationStrategy::Quorum { min_replicas: 2 },
        replication_factor: 3,
        timeout: Duration::from_secs(10),
        enable_compression: true,
        verify_checksum: true,
    };

    let replication = ReplicationManager::new(replication_config);

    let agent_state = b"critical_agent_state_data".to_vec();
    let target_nodes = vec![
        "node1".to_string(),
        "node2".to_string(),
        "node3".to_string(),
    ];

    let replication_log = replication.replicate_state(&primary.id(), agent_state, target_nodes)?;

    println!(
        "   ✓ State replicated to {} nodes",
        replication_log.success_count
    );
    println!("   Replication time: {:?}", replication_log.duration);
    println!("   Strategy: Quorum (min 2 replicas)");
    println!();

    // 4. Set up health monitoring
    println!("4. Setting up health monitoring...");
    let health_config = HAHealthConfig {
        check_interval: Duration::from_secs(10),
        thresholds: mielin_cells::HealthThreshold {
            cpu_threshold: 0.8,
            memory_threshold: 0.85,
            disk_threshold: 0.9,
            latency_threshold: Duration::from_millis(100),
            error_rate_threshold: 0.05,
        },
        failure_threshold: 3,
        success_threshold: 2,
    };

    let health_monitor = ClusterHealthMonitor::new(health_config);

    health_monitor.register_node("node1".to_string())?;
    health_monitor.register_node("node2".to_string())?;
    health_monitor.register_node("node3".to_string())?;

    // Simulate health checks
    let healthy_metrics = vec![
        HealthMetric {
            name: "cpu".to_string(),
            value: 0.45,
            threshold: 0.8,
            healthy: true,
        },
        HealthMetric {
            name: "memory".to_string(),
            value: 0.62,
            threshold: 0.85,
            healthy: true,
        },
    ];

    health_monitor.check_node_health("node1", healthy_metrics.clone())?;
    health_monitor.check_node_health("node2", healthy_metrics.clone())?;
    health_monitor.check_node_health("node3", healthy_metrics)?;

    let cluster_health = health_monitor.get_cluster_health()?;
    println!("   ✓ Cluster health status: {:?}", cluster_health.status);
    println!(
        "   Healthy nodes: {}/{}",
        cluster_health.healthy_nodes, cluster_health.total_nodes
    );
    println!();

    // 5. Demonstrate quorum-based decision
    println!("5. Making quorum-based decision...");
    let quorum_config = QuorumConfig {
        policy: QuorumPolicy::SimpleMajority,
        vote_timeout: Duration::from_secs(30),
        min_participants: 2,
    };

    let quorum = QuorumVoter::new(quorum_config, 3);

    // Cast votes
    for i in 0..2 {
        let vote = QuorumVote {
            node_id: format!("node{}", i + 1),
            vote: true,
            weight: 1.0,
            timestamp: Instant::now(),
        };
        quorum.cast_vote(vote)?;
    }

    if let Some(decision) = quorum.get_decision()? {
        println!(
            "   ✓ Quorum decision: {}",
            if decision.passed {
                "APPROVED"
            } else {
                "REJECTED"
            }
        );
        println!(
            "   Votes: {} yes, {} no (required: {})",
            decision.yes_votes, decision.no_votes, decision.required_votes
        );
    }
    println!();

    // 6. Simulate failover scenario
    println!("6. Simulating failover scenario...");

    // Report failures
    for i in 0..3 {
        let decision = failover.report_failure(&primary.id(), "node unresponsive")?;
        println!(
            "   Failure {} reported: should_failover={}",
            i + 1,
            decision.should_failover
        );

        if decision.should_failover {
            if let Some(backup_id) = decision.backup_id {
                let event = failover.execute_failover(&primary.id(), &backup_id)?;
                println!("   ✓ Failover executed successfully!");
                println!("   Primary: {} -> Backup: {}", primary.id(), backup_id);
                println!("   Success: {}", event.success);
                break;
            }
        }
    }

    println!("\n=== High Availability Setup Complete ===");
    Ok(())
}
